use super::types::*;
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;

/// Monotonic id so the supervisor can tell whether an `Exited` event comes
/// from the current client of a session or a stale one that was replaced.
static INSTANCE_SEQ: AtomicU64 = AtomicU64::new(1);

pub struct AcpClient {
    instance: u64,
    child: Mutex<Child>,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: PendingMap,
    next_id: Arc<AtomicU64>,
    agent_info: Mutex<Option<Value>>,
    killed: AtomicBool,
    /// Kept alive so JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE fires on drop.
    #[allow(dead_code)]
    job: Option<crate::job_object::KillOnCloseJob>,
}

impl AcpClient {
    /// Spawn an ACP agent process (grok / claude-agent-acp / qwen …) and run
    /// reader loop; events go to `event_tx`. Command shape comes from the
    /// agent profile registry (src/agents.rs) — this layer is provider-neutral.
    pub async fn spawn(
        spec: &crate::agents::SpawnSpec,
        cwd: &Path,
        event_tx: mpsc::UnboundedSender<AcpEvent>,
    ) -> Result<Arc<Self>> {
        let program = &spec.command;
        let mut cmd = Command::new(program);
        cmd.args(&spec.args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // 子进程默认继承本进程环境（PATH 等，npx/node 适配器必需），
        // 档案 env 在其上覆盖（NO_COLOR / GROK_HOME / 各家 API 端点与密钥）
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }

        #[cfg(windows)]
        {
            // Avoid flashing console windows for child
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().with_context(|| {
            format!(
                "无法启动「{}」（工作目录：{}），参数 {:?}",
                program,
                cwd.display(),
                spec.args
            )
        })?;

        let job = crate::job_object::KillOnCloseJob::new();
        if let Some(ref job) = job {
            // tokio::process::Child has no AsRawHandle; assign by PID.
            let assigned = match child.id() {
                Some(pid) => job.assign_pid(pid),
                None => Err("child has no pid yet".into()),
            };
            if let Err(e) = assigned {
                tracing::warn!("未能将 Agent 加入 Job Object：{e}");
            }
        }

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("缺少标准输入管道"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("缺少标准输出管道"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("缺少标准错误管道"))?;

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let next_id = Arc::new(AtomicU64::new(1));

        // stdout reader
        {
            let pending = pending.clone();
            let event_tx = event_tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Err(e) = handle_line(trimmed, &pending, &event_tx).await {
                        let _ = event_tx.send(AcpEvent::ParseError {
                            line: trimmed.to_string(),
                            error: e.to_string(),
                        });
                    }
                }
            });
        }

        // stderr reader
        {
            let event_tx = event_tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = event_tx.send(AcpEvent::Stderr { text: line });
                }
            });
        }

        let client = Arc::new(Self {
            instance: INSTANCE_SEQ.fetch_add(1, Ordering::SeqCst),
            child: Mutex::new(child),
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            next_id,
            agent_info: Mutex::new(None),
            killed: AtomicBool::new(false),
            job,
        });

        // Detect child exit; skip Exited when kill() (hibernate) set `killed`.
        // Holds only a Weak handle: a strong one would keep `child` alive and
        // defeat kill_on_drop, orphaning the agent process on error paths.
        {
            let client = Arc::downgrade(&client);
            let event_tx = event_tx.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    let Some(client) = client.upgrade() else {
                        break; // last owner dropped → kill_on_drop fired
                    };
                    // kill() may hold the child lock through process death
                    let Some(mut guard) = client.child.try_lock().ok() else {
                        continue;
                    };
                    match guard.try_wait() {
                        Ok(Some(status)) => {
                            drop(guard);
                            // Fail in-flight requests instead of letting them
                            // hang until the 600s timeout.
                            client
                                .fail_all_pending("Agent 进程已退出，请求中止")
                                .await;
                            if !client.killed.load(Ordering::SeqCst) {
                                tracing::warn!(
                                    "Agent 进程意外退出：code={:?}",
                                    status.code()
                                );
                                let _ = event_tx.send(AcpEvent::Exited {
                                    code: status.code(),
                                });
                            } else {
                                tracing::info!("Agent 进程已结束（主动终止）");
                            }
                            break;
                        }
                        Err(e) => {
                            drop(guard);
                            client
                                .fail_all_pending("Agent 进程等待失败，请求中止")
                                .await;
                            if !client.killed.load(Ordering::SeqCst) {
                                tracing::warn!("Agent 进程等待失败：{e}");
                                let _ = event_tx.send(AcpEvent::Exited { code: None });
                            }
                            break;
                        }
                        Ok(None) => {
                            drop(guard);
                            continue;
                        }
                    }
                }
            });
        }

        Ok(client)
    }

    /// Identity of this client instance (spawn), see `INSTANCE_SEQ`.
    pub fn instance_id(&self) -> u64 {
        self.instance
    }

    /// Fail every in-flight request with `reason`; used on process exit/kill.
    async fn fail_all_pending(&self, reason: &str) {
        let mut map = self.pending.lock().await;
        for (_, tx) in map.drain() {
            let _ = tx.send(Err(anyhow!("{reason}")));
        }
    }

    pub async fn initialize(&self) -> Result<Value> {
        let result = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientInfo": {
                        "name": "grokfree",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "clientCapabilities": {
                        "fs": { "readTextFile": true, "writeTextFile": true },
                        "terminal": true
                    }
                }),
            )
            .await?;
        *self.agent_info.lock().await = Some(result.clone());
        Ok(result)
    }

    pub async fn session_new(&self, cwd: &Path, meta: Value) -> Result<Value> {
        self.request(
            "session/new",
            json!({
                "cwd": cwd.to_string_lossy(),
                "mcpServers": [],
                "_meta": meta
            }),
        )
        .await
    }

    pub async fn session_load(&self, session_id: &str, cwd: &Path) -> Result<Value> {
        // CLI ≥0.2 requires `mcpServers` on session/load (same as session/new).
        // Omitting it yields: Invalid params — missing field `mcpServers`.
        self.request(
            "session/load",
            json!({
                "sessionId": session_id,
                "cwd": cwd.to_string_lossy(),
                "mcpServers": []
            }),
        )
        .await
    }

    pub async fn prompt(&self, session_id: &str, text: &str) -> Result<Value> {
        self.request(
            "session/prompt",
            json!({
                "sessionId": session_id,
                "prompt": [{ "type": "text", "text": text }]
            }),
        )
        .await
    }

    pub async fn cancel(&self, session_id: &str) -> Result<Value> {
        match self
            .request("session/cancel", json!({ "sessionId": session_id }))
            .await
        {
            Ok(v) => Ok(v),
            Err(_) => {
                self.notify("session/cancel", json!({ "sessionId": session_id }))
                    .await?;
                Ok(json!({ "ok": true }))
            }
        }
    }

    pub async fn respond(&self, id: Value, result: Value) -> Result<()> {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        });
        self.write_json(&msg).await
    }

    pub async fn respond_error(&self, id: Value, code: i64, message: &str) -> Result<()> {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        });
        self.write_json(&msg).await
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending.lock().await;
            map.insert(id, tx);
        }
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        if let Err(e) = self.write_json(&msg).await {
            let mut map = self.pending.lock().await;
            map.remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(std::time::Duration::from_secs(600), rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err(anyhow!("请求通道已关闭：{method}")),
            Err(_) => {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                Err(anyhow!("请求超时：{method}"))
            }
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        self.write_json(&msg).await
    }

    async fn write_json(&self, value: &Value) -> Result<()> {
        let mut line = serde_json::to_string(value)?;
        line.push('\n');
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn kill(&self) -> Result<()> {
        self.killed.store(true, Ordering::SeqCst);
        // Unblock callers waiting on this client (prompt/permission) before
        // waiting for the process to die.
        self.fail_all_pending("Agent 已被终止（休眠/停止），请求中止")
            .await;
        let mut child = self.child.lock().await;
        let _ = child.start_kill();
        let _ = child.wait().await;
        Ok(())
    }
}

async fn handle_line(
    line: &str,
    pending: &PendingMap,
    event_tx: &mpsc::UnboundedSender<AcpEvent>,
) -> Result<()> {
    let v: Value = serde_json::from_str(line)?;

    // Response: has id + (result|error), no method
    if v.get("id").is_some() && v.get("method").is_none() {
        let id_num = match &v["id"] {
            Value::Number(n) => n.as_u64(),
            Value::String(s) => s.parse().ok(),
            _ => None,
        };
        if let Some(id) = id_num {
            let mut map = pending.lock().await;
            if let Some(tx) = map.remove(&id) {
                if let Some(err) = v.get("error") {
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("ACP 错误")
                        .to_string();
                    let _ = tx.send(Err(anyhow!(msg)));
                } else {
                    let result = v.get("result").cloned().unwrap_or(Value::Null);
                    let _ = tx.send(Ok(result));
                }
                return Ok(());
            }
        }
        return Ok(());
    }

    // Server request: method + id
    if v.get("method").is_some() && v.get("id").is_some() {
        let _ = event_tx.send(AcpEvent::ServerRequest {
            id: v.get("id").cloned().unwrap_or(Value::Null),
            method: v["method"].as_str().unwrap_or("").to_string(),
            params: v.get("params").cloned().unwrap_or(Value::Null),
        });
        return Ok(());
    }

    // Notification: method, no id
    if let Some(method) = v.get("method").and_then(|m| m.as_str()) {
        let _ = event_tx.send(AcpEvent::Notification {
            method: method.to_string(),
            params: v.get("params").cloned().unwrap_or(Value::Null),
        });
    }

    Ok(())
}
