//! ACP Client terminal host (terminal/create|output|wait_for_exit|kill|release)
//!
//! Desktop advertises `clientCapabilities.terminal = true`, so the agent delegates
//! shell execution to us. Without this host every shell tool fails with
//! "未处理的客户端方法：terminal/create".
//!
//! On Windows the agent is told `Shell: powershell` and emits PowerShell syntax
//! (`Set-Location`, `;` chaining, cmdlets). Free-form / script lines therefore
//! MUST run under PowerShell by default — not `cmd.exe` (which was the old bug).

use anyhow::{anyhow, Result};
use parking_lot::Mutex as StdMutex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, Notify, RwLock};
use uuid::Uuid;

/// Preferred interpreter for free-form agent shell lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    PowerShell,
    Pwsh,
    Cmd,
}

impl ShellKind {
    pub fn from_prefs(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "cmd" | "cmd.exe" | "command" => Self::Cmd,
            "pwsh" | "pwsh.exe" | "powershell-core" => Self::Pwsh,
            // gitbash / wsl still use PowerShell for ACP one-shot tools;
            // interactive external terminal is handled elsewhere.
            _ => Self::PowerShell,
        }
    }
}

#[derive(Clone)]
pub struct TerminalHost {
    inner: Arc<RwLock<HashMap<String, Arc<TerminalState>>>>,
    /// Default shell for free-form / builtin lines (from Desktop prefs).
    default_shell: Arc<StdMutex<ShellKind>>,
}

struct TerminalState {
    session_id: String,
    /// Combined stdout+stderr text buffer (UTF-8 lossy).
    output: StdMutex<String>,
    truncated: AtomicBool,
    output_byte_limit: Option<usize>,
    child: Mutex<Option<Child>>,
    /// Set when process exits: (exit_code, signal_name)
    exit: StdMutex<Option<(Option<i64>, Option<String>)>>,
    done: Notify,
}

impl TerminalHost {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            default_shell: Arc::new(StdMutex::new(ShellKind::PowerShell)),
        }
    }

    /// Update the preferred shell (call when prefs load / change).
    pub fn set_default_shell(&self, shell: &str) {
        *self.default_shell.lock() = ShellKind::from_prefs(shell);
    }

    pub fn default_shell(&self) -> ShellKind {
        *self.default_shell.lock()
    }

    /// Handle an ACP client method. Returns `Ok(None)` if not a terminal method.
    pub async fn handle(&self, method: &str, params: &Value) -> Result<Option<Value>> {
        let m = method.trim();
        match m {
            "terminal/create" | "terminal/create_terminal" => {
                Ok(Some(self.create(params).await?))
            }
            "terminal/output" => Ok(Some(self.output(params).await?)),
            "terminal/wait_for_exit" | "terminal/waitForExit" => {
                Ok(Some(self.wait_for_exit(params).await?))
            }
            "terminal/kill" => Ok(Some(self.kill(params).await?)),
            "terminal/release" => Ok(Some(self.release(params).await?)),
            _ if m.starts_with("terminal/") => Err(anyhow!("不支持的终端方法：{method}")),
            _ => Ok(None),
        }
    }

    async fn create(&self, params: &Value) -> Result<Value> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("terminal/create 缺少 command"))?
            .to_string();

        let args: Vec<String> = params
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Prefer desktop session id (stamped by Supervisor) for cleanup grouping.
        let session_id = params
            .get("_desktopSessionId")
            .or_else(|| params.get("sessionId"))
            .or_else(|| params.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let cwd = params
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let output_byte_limit = params
            .get("outputByteLimit")
            .or_else(|| params.get("output_byte_limit"))
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);

        let env_pairs: Vec<(String, String)> = params
            .get("env")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let name = item
                            .get("name")
                            .or_else(|| item.get("key"))
                            .and_then(|v| v.as_str())?;
                        let value = item.get("value").and_then(|v| v.as_str())?;
                        Some((name.to_string(), value.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Optional per-call shell override from agent params
        let shell_override = params
            .get("shell")
            .or_else(|| params.get("interpreter"))
            .and_then(|v| v.as_str())
            .map(ShellKind::from_prefs);

        let shell = shell_override.unwrap_or_else(|| self.default_shell());

        let terminal_id = Uuid::new_v4().to_string();
        let mut cmd = build_command(&command, &args, shell);

        if let Some(ref dir) = cwd {
            if !dir.is_empty() {
                cmd.current_dir(PathBuf::from(dir));
            }
        }

        for (k, v) in &env_pairs {
            cmd.env(k, v);
        }

        // Ensure PowerShell-friendly UTF-8 console I/O for captured output
        #[cfg(windows)]
        {
            cmd.env("PYTHONIOENCODING", "utf-8");
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().map_err(|e| {
            anyhow!(
                "无法创建终端进程「{command}」 {args:?} (shell={shell:?}): {e}"
            )
        })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("缺少 stdout 管道"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("缺少 stderr 管道"))?;

        let state = Arc::new(TerminalState {
            session_id,
            output: StdMutex::new(String::new()),
            truncated: AtomicBool::new(false),
            output_byte_limit,
            child: Mutex::new(Some(child)),
            exit: StdMutex::new(None),
            done: Notify::new(),
        });

        {
            let mut map = self.inner.write().await;
            map.insert(terminal_id.clone(), state.clone());
        }

        // stdout pump
        {
            let state = state.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => append_output(&state, &buf[..n]),
                        Err(_) => break,
                    }
                }
            });
        }

        // stderr pump
        {
            let state = state.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => append_output(&state, &buf[..n]),
                        Err(_) => break,
                    }
                }
            });
        }

        // exit poller
        {
            let state = state.clone();
            tokio::spawn(async move {
                loop {
                    let status = {
                        let mut guard = state.child.lock().await;
                        match guard.as_mut() {
                            Some(child) => match child.try_wait() {
                                Ok(Some(status)) => {
                                    *guard = None;
                                    Some(Ok(status))
                                }
                                Ok(None) => None,
                                Err(e) => {
                                    *guard = None;
                                    Some(Err(e))
                                }
                            },
                            None => break,
                        }
                    };

                    match status {
                        Some(Ok(status)) => {
                            let code = status.code().map(|c| c as i64);
                            let signal = {
                                #[cfg(unix)]
                                {
                                    use std::os::unix::process::ExitStatusExt;
                                    status.signal().map(|s| s.to_string())
                                }
                                #[cfg(not(unix))]
                                {
                                    None::<String>
                                }
                            };
                            *state.exit.lock() = Some((code, signal));
                            state.done.notify_waiters();
                            return;
                        }
                        Some(Err(e)) => {
                            *state.exit.lock() =
                                Some((None, Some(format!("wait error: {e}"))));
                            state.done.notify_waiters();
                            return;
                        }
                        None => {
                            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
                        }
                    }
                }

                if state.exit.lock().is_none() {
                    *state.exit.lock() = Some((None, Some("terminated".into())));
                }
                state.done.notify_waiters();
            });
        }

        tracing::info!(%terminal_id, %command, ?shell, "terminal/create ok");
        Ok(json!({ "terminalId": terminal_id }))
    }

    async fn output(&self, params: &Value) -> Result<Value> {
        let id = terminal_id_from(params)?;
        let state = self.get(&id).await?;
        let output = state.output.lock().clone();
        let truncated = state.truncated.load(Ordering::Relaxed);
        let exit_status = state.exit.lock().as_ref().map(|(code, signal)| {
            json!({
                "exitCode": code,
                "signal": signal,
            })
        });

        Ok(json!({
            "output": output,
            "truncated": truncated,
            "exitStatus": exit_status,
        }))
    }

    async fn wait_for_exit(&self, params: &Value) -> Result<Value> {
        let id = terminal_id_from(params)?;
        let state = self.get(&id).await?;

        loop {
            if let Some((code, signal)) = state.exit.lock().clone() {
                return Ok(json!({
                    "exitCode": code,
                    "signal": signal,
                }));
            }

            let notified = state.done.notified();
            tokio::select! {
                _ = notified => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                    if self.get(&id).await.is_err() {
                        return Err(anyhow!("终端已释放：{id}"));
                    }
                }
            }
        }
    }

    async fn kill(&self, params: &Value) -> Result<Value> {
        let id = terminal_id_from(params)?;
        let state = self.get(&id).await?;

        if state.exit.lock().is_some() {
            return Ok(json!({}));
        }

        {
            let mut guard = state.child.lock().await;
            if let Some(child) = guard.as_mut() {
                let _ = child.start_kill();
            }
        }

        let wait = async {
            loop {
                if state.exit.lock().is_some() {
                    return;
                }
                let n = state.done.notified();
                tokio::select! {
                    _ = n => {}
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                }
            }
        };
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), wait).await;

        if state.exit.lock().is_none() {
            let mut guard = state.child.lock().await;
            if let Some(mut child) = guard.take() {
                let _ = child.start_kill();
                match child.wait().await {
                    Ok(status) => {
                        *state.exit.lock() =
                            Some((status.code().map(|c| c as i64), Some("killed".into())));
                    }
                    Err(_) => {
                        *state.exit.lock() = Some((None, Some("killed".into())));
                    }
                }
            } else {
                *state.exit.lock() = Some((None, Some("killed".into())));
            }
            state.done.notify_waiters();
        }

        Ok(json!({}))
    }

    async fn release(&self, params: &Value) -> Result<Value> {
        let id = terminal_id_from(params)?;
        let state = {
            let mut map = self.inner.write().await;
            map.remove(&id)
        };

        if let Some(state) = state {
            {
                let mut guard = state.child.lock().await;
                if let Some(mut child) = guard.take() {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                }
            }
            if state.exit.lock().is_none() {
                *state.exit.lock() = Some((None, Some("released".into())));
            }
            state.done.notify_waiters();
        }
        Ok(json!({}))
    }

    async fn get(&self, id: &str) -> Result<Arc<TerminalState>> {
        let map = self.inner.read().await;
        map.get(id)
            .cloned()
            .ok_or_else(|| anyhow!("未知终端：{id}"))
    }

    /// True when the session still has a live (un-exited) terminal.
    /// 静默看门狗把它当作「长工具心跳」：终端在跑 = 不算失速。
    pub async fn session_has_running(&self, session_id: &str) -> bool {
        let map = self.inner.read().await;
        map.values()
            .any(|t| t.session_id == session_id && t.exit.lock().is_none())
    }

    /// Drop all terminals belonging to a desktop session.
    pub async fn release_session(&self, session_id: &str) {
        let ids: Vec<String> = {
            let map = self.inner.read().await;
            map.iter()
                .filter(|(_, t)| t.session_id == session_id)
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in ids {
            let _ = self.release(&json!({ "terminalId": id })).await;
        }
    }

    pub async fn release_all(&self) {
        let ids: Vec<String> = {
            let map = self.inner.read().await;
            map.keys().cloned().collect()
        };
        for id in ids {
            let _ = self.release(&json!({ "terminalId": id })).await;
        }
    }
}

fn terminal_id_from(params: &Value) -> Result<String> {
    params
        .get("terminalId")
        .or_else(|| params.get("terminal_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("缺少 terminalId"))
}

fn append_output(state: &TerminalState, bytes: &[u8]) {
    let chunk = String::from_utf8_lossy(bytes);
    let mut out = state.output.lock();
    out.push_str(&chunk);
    if let Some(limit) = state.output_byte_limit {
        let byte_len = out.len();
        if byte_len > limit {
            let overflow = byte_len - limit;
            let mut cut = overflow.min(out.len());
            while cut < out.len() && !out.is_char_boundary(cut) {
                cut += 1;
            }
            if cut > 0 && cut <= out.len() {
                out.replace_range(0..cut, "");
                state.truncated.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// Join command + args into a single shell line with basic quoting.
fn join_cmdline(command: &str, args: &[String]) -> String {
    let mut line = quote_if_needed(command);
    for a in args {
        line.push(' ');
        line.push_str(&quote_if_needed(a));
    }
    line
}

fn quote_if_needed(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".into();
    }
    if s.chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\'' || c == '&' || c == '|' || c == '>')
    {
        // PowerShell single-quote with doubled internal quotes
        format!("'{}'", s.replace('\'', "''"))
    } else {
        s.to_string()
    }
}

fn is_shell_binary(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    let base = Path::new(&lower)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&lower);
    matches!(
        base,
        "cmd"
            | "cmd.exe"
            | "powershell"
            | "powershell.exe"
            | "pwsh"
            | "pwsh.exe"
            | "bash"
            | "bash.exe"
            | "sh"
            | "zsh"
            | "wsl"
            | "wsl.exe"
    )
}

/// True if this looks like a real executable path / name rather than a
/// shell script line or PowerShell cmdlet.
fn looks_like_direct_executable(command: &str) -> bool {
    let t = command.trim();
    if t.is_empty() || t.contains(char::is_whitespace) {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    if lower.ends_with(".exe")
        || lower.ends_with(".cmd")
        || lower.ends_with(".bat")
        || lower.ends_with(".com")
        || lower.ends_with(".ps1")
        || lower.contains('/')
        || lower.contains('\\')
        || Path::new(t).is_absolute()
    {
        return true;
    }
    // Common tools spawned directly
    const KNOWN: &[&str] = &[
        "npm", "npx", "node", "pnpm", "yarn", "bun", "cargo", "rustc", "git",
        "python", "python3", "py", "pip", "pip3", "go", "dotnet", "java",
        "mvn", "gradle", "cmake", "make", "curl", "wget", "rg", "fd", "docker",
        "kubectl", "code", "cursor", "where", "which", "taskkill", "tasklist",
    ];
    KNOWN.iter().any(|k| lower == *k)
}

fn wrap_with_shell(shell: ShellKind, script: &str) -> Command {
    // Piped output on a Chinese-locale Windows defaults to the OEM codepage
    // (GBK) — force UTF-8 so captured Chinese text survives round-trip.
    match shell {
        ShellKind::Cmd => {
            let mut c = Command::new("cmd.exe");
            // /d = skip AutoRun, /s = quote semantics, /c = run and exit
            c.args(["/d", "/s", "/c", &format!("chcp 65001>nul & {script}")]);
            c
        }
        ShellKind::Pwsh => {
            let mut c = Command::new("pwsh.exe");
            c.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &format!("{PS_UTF8_PREFIX}{script}"),
            ]);
            c
        }
        ShellKind::PowerShell => {
            let mut c = Command::new("powershell.exe");
            c.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &format!("{PS_UTF8_PREFIX}{script}"),
            ]);
            c
        }
    }
}

/// No-op on pwsh (already UTF-8); switches PS 5.1 piped output to UTF-8.
const PS_UTF8_PREFIX: &str =
    "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8;";

/// Build a process command.
///
/// Windows policy (v0.2.1 fix):
/// - Explicit shell binaries → spawn as-is with their args
/// - Free-form lines / scripts / cmdlets → preferred shell (default PowerShell)
/// - Known executables (`npm`, `git`, paths with `.exe`) → direct spawn
/// - Everything else (e.g. `pwd`, `Get-ChildItem`) → preferred shell
fn build_command(command: &str, args: &[String], shell: ShellKind) -> Command {
    #[cfg(windows)]
    {
        if is_shell_binary(command) {
            let mut c = Command::new(command);
            c.args(args);
            return c;
        }

        // Free-form script line (spaces, no separate argv) → shell -Command
        if args.is_empty() && command.contains(char::is_whitespace) {
            // If the line itself starts with a shell binary, still use preferred
            // wrap so quoting is consistent — agent already wrote full PS syntax.
            return wrap_with_shell(shell, command);
        }

        // Direct executable + argv
        if looks_like_direct_executable(command) {
            let mut c = Command::new(command);
            c.args(args);
            return c;
        }

        // Cmdlet / alias / unknown single token (pwd, Get-ChildItem, Set-Location, …)
        let script = join_cmdline(command, args);
        return wrap_with_shell(shell, &script);
    }

    #[cfg(not(windows))]
    {
        let _ = shell;
        if args.is_empty() && command.contains(char::is_whitespace) {
            let mut c = Command::new("sh");
            c.args(["-c", command]);
            return c;
        }
        if is_shell_binary(command) {
            let mut c = Command::new(command);
            c.args(args);
            return c;
        }
        // Prefer direct exec for known paths; else sh -c
        if looks_like_direct_executable(command) {
            let mut c = Command::new(command);
            c.args(args);
            return c;
        }
        let script = join_cmdline(command, args);
        let mut c = Command::new("sh");
        c.args(["-c", &script]);
        c
    }
}

impl Default for TerminalHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_kind_from_prefs() {
        assert_eq!(ShellKind::from_prefs("powershell"), ShellKind::PowerShell);
        assert_eq!(ShellKind::from_prefs("pwsh"), ShellKind::Pwsh);
        assert_eq!(ShellKind::from_prefs("cmd"), ShellKind::Cmd);
        assert_eq!(ShellKind::from_prefs("gitbash"), ShellKind::PowerShell);
    }

    #[test]
    fn direct_exec_detection() {
        assert!(looks_like_direct_executable("npm"));
        assert!(looks_like_direct_executable(r"C:\tools\foo.exe"));
        assert!(!looks_like_direct_executable("Get-ChildItem"));
        assert!(!looks_like_direct_executable("pwd"));
        assert!(!looks_like_direct_executable("Set-Location -LiteralPath x"));
    }
}
