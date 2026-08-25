//! Read ~/.grok config/auth and Desktop state (design §12)

use crate::paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPrefs {
    pub grok_path: String,
    pub permission_mode: String, // ask | auto | always-approve
    pub sandbox_mode: String,    // off | workspace | read-only | strict
    pub model: String,
    pub theme: String,
    pub default_editor: String,
    pub default_shell: String,
    pub min_cli_version: String,
    /// 新建/添加项目时文件夹选择器的默认起始目录
    #[serde(default = "default_projects_dir")]
    pub default_projects_dir: String,
    /// When false (default), unknown ACP events are hidden/collapsed in transcript.
    #[serde(default)]
    pub show_raw_acp_events: bool,
    /// ACP fs scope: "workspace" (default) | "unrestricted"
    #[serde(default = "default_fs_scope")]
    pub fs_scope: String,
    /// Transcript first-screen visible message count (30/50/100).
    #[serde(default = "default_history_initial")]
    pub history_initial_visible: u32,
    /// Hide “整理对话…” text on session paint mask (spinner/blank only).
    #[serde(default)]
    pub chat_mask_quiet: bool,
}

fn default_fs_scope() -> String {
    "workspace".into()
}

fn default_projects_dir() -> String {
    r"D:\Grok Build".into()
}

fn default_history_initial() -> u32 {
    50
}

impl DesktopPrefs {
    pub fn defaults() -> Self {
        Self {
            grok_path: String::new(),
            // Safe default: ask each time (D7). Settings can enable always-approve.
            permission_mode: "ask".into(),
            // agent 子命令不支持 --sandbox；UI 仍保留选项供说明，默认关闭
            sandbox_mode: "off".into(),
            model: String::new(),
            theme: "light".into(),
            default_editor: "code".into(),
            default_shell: "powershell".into(),
            min_cli_version: "0.2.0".into(),
            default_projects_dir: default_projects_dir(),
            show_raw_acp_events: false,
            fs_scope: default_fs_scope(),
            history_initial_visible: default_history_initial(),
            chat_mask_quiet: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub created_at: String,
    pub last_opened_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub cwd: String,
    pub grok_session_id: Option<String>,
    /// 该会话使用的小精灵档案 id（agents.json，默认 grok）
    #[serde(default = "default_agent_id")]
    pub agent_id: String,
    pub status: String,
    pub created_at: String,
    pub last_active_at: String,
    /// 历史委托标记（兼容旧 desktop-state.json）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegated_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

fn default_agent_id() -> String {
    "grok".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DesktopState {
    pub prefs: DesktopPrefs,
    pub projects: Vec<Project>,
    pub sessions: Vec<SessionMeta>,
    pub onboarding_done: bool,
}

impl DesktopState {
    pub fn load() -> Self {
        let path = paths::desktop_state_path();
        match fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str::<DesktopState>(&s) {
                Ok(mut st) => {
                    tracing::info!(
                        "loaded desktop state: {} projects · {}",
                        st.projects.len(),
                        path.display()
                    );
                    // Ensure prefs fields are complete
                    if st.prefs.permission_mode.is_empty() {
                        st.prefs = DesktopPrefs::defaults();
                    }
                    st
                }
                Err(e) => {
                    tracing::error!(
                        "failed to parse desktop state ({}): {} — using empty state, original backed up",
                        path.display(),
                        e
                    );
                    let bak = path.with_extension("json.bak");
                    let _ = fs::copy(&path, &bak);
                    Self::fresh()
                }
            },
            Err(e) => {
                tracing::warn!("cannot read desktop state {}: {}", path.display(), e);
                Self::fresh()
            }
        }
    }

    fn fresh() -> Self {
        Self {
            prefs: DesktopPrefs::defaults(),
            projects: vec![],
            sessions: vec![],
            onboarding_done: false,
        }
    }

    pub fn save(&self) -> Result<(), String> {
        paths::ensure_desktop_dirs().map_err(|e| e.to_string())?;
        let path = paths::desktop_state_path();
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        // Atomic-ish write: temp in same directory then rename (avoids half-written JSON on crash).
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, &s).map_err(|e| format!("写入临时状态失败：{e}"))?;
        // On Windows, replace target if needed.
        if path.exists() {
            let bak = path.with_extension("json.prev");
            let _ = fs::remove_file(&bak);
            let _ = fs::rename(&path, &bak);
        }
        fs::rename(&tmp, &path).map_err(|e| {
            // Best-effort restore
            let bak = path.with_extension("json.prev");
            if bak.exists() && !path.exists() {
                let _ = fs::rename(&bak, &path);
            }
            format!("保存状态失败：{e}")
        })?;
        let bak = path.with_extension("json.prev");
        let _ = fs::remove_file(&bak);
        tracing::info!(
            "saved desktop state: {} projects · {}",
            self.projects.len(),
            path.display()
        );
        Ok(())
    }

    /// Reload from disk into the shared mutex (single-instance re-launch).
    pub fn reload_into(target: &parking_lot::Mutex<DesktopState>) {
        let fresh = Self::load();
        let mut guard = target.lock();
        *guard = fresh;
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrokEnvironment {
    pub grok_home: String,
    pub grok_path: String,
    pub grok_exists: bool,
    pub grok_version: Option<String>,
    /// Whether installed CLI version satisfies prefs.min_cli_version.
    pub cli_version_ok: bool,
    pub min_cli_version: String,
    pub config_path: String,
    pub config_exists: bool,
    pub auth_path: String,
    pub auth_exists: bool,
    pub auth_logged_in: bool,
    pub sessions_dir: String,
    pub desktop_data_dir: String,
    /// Honest capability flags for Settings UI (v0.3).
    pub capabilities: crate::cli_caps::CliCapabilities,
}

pub fn probe_environment(prefs: &DesktopPrefs) -> GrokEnvironment {
    let grok_path = paths::resolve_grok_executable(if prefs.grok_path.is_empty() {
        None
    } else {
        Some(&prefs.grok_path)
    });
    let grok_exists = grok_path.exists()
        || which_on_path(grok_path.file_name().and_then(|s| s.to_str()).unwrap_or("grok"));

    let version = if grok_exists {
        probe_grok_version_cached(&grok_path)
    } else {
        None
    };

    let min = if prefs.min_cli_version.is_empty() {
        "0.2.0".to_string()
    } else {
        prefs.min_cli_version.clone()
    };
    let cli_version_ok =
        grok_exists && crate::cli_caps::version_meets_min(version.as_deref(), &min);

    let auth_path = paths::grok_auth_json();
    let auth_exists = auth_path.exists();
    let auth_logged_in = auth_exists && auth_looks_valid(&auth_path);

    GrokEnvironment {
        grok_home: paths::grok_home().display().to_string(),
        grok_path: grok_path.display().to_string(),
        grok_exists,
        grok_version: version,
        cli_version_ok,
        min_cli_version: min,
        config_path: paths::grok_config_toml().display().to_string(),
        config_exists: paths::grok_config_toml().exists(),
        auth_path: auth_path.display().to_string(),
        auth_exists,
        auth_logged_in,
        sessions_dir: paths::grok_sessions_dir().display().to_string(),
        desktop_data_dir: paths::desktop_data_dir().display().to_string(),
        capabilities: crate::cli_caps::capabilities(),
    }
}

fn auth_looks_valid(path: &PathBuf) -> bool {
    match fs::read_to_string(path) {
        Ok(s) => {
            if s.trim().is_empty() {
                return false;
            }
            // Accept any non-empty JSON object with token-like keys
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                if let Some(obj) = v.as_object() {
                    return obj.contains_key("access_token")
                        || obj.contains_key("token")
                        || obj.contains_key("api_key")
                        || obj.contains_key("refresh_token")
                        || !obj.is_empty();
                }
            }
            // env-style or other formats
            true
        }
        Err(_) => false,
    }
}

/// Hard deadline for `grok --version`. The CLI has been observed to hang on
/// startup (network update check / AV scan); without a deadline every session
/// spawn that probes the version leaks one hung `grok` process and never
/// returns to the caller.
const VERSION_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const WHICH_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
const VERSION_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(60);

#[allow(clippy::type_complexity)]
static VERSION_CACHE: std::sync::Mutex<
    Option<(PathBuf, std::time::Instant, Option<String>)>,
> = std::sync::Mutex::new(None);

/// Cached `grok --version` (60s TTL). Every session spawn gates on the CLI
/// version; re-spawning the probe per session multiplied processes during
/// spawn storms. Cache is invalidated on prefs save (grok path may change).
pub fn probe_grok_version_cached(exe: &std::path::Path) -> Option<String> {
    {
        let cache = VERSION_CACHE.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((path, at, ver)) = cache.as_ref() {
            if path == exe && at.elapsed() < VERSION_CACHE_TTL {
                return ver.clone();
            }
        }
    }
    let ver = run_version(&exe.to_path_buf());
    let mut cache = VERSION_CACHE.lock().unwrap_or_else(|p| p.into_inner());
    *cache = Some((exe.to_path_buf(), std::time::Instant::now(), ver.clone()));
    ver
}

pub fn invalidate_version_cache() {
    let mut cache = VERSION_CACHE.lock().unwrap_or_else(|p| p.into_inner());
    *cache = None;
}

fn run_version(exe: &PathBuf) -> Option<String> {
    let mut cmd = crate::process_util::silent_command(exe);
    cmd.arg("--version")
        .env("GROK_HOME", crate::paths::grok_home());
    let out = crate::process_util::output_with_timeout(cmd, VERSION_PROBE_TIMEOUT)?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn which_on_path(name: &str) -> bool {
    let mut cmd = crate::process_util::silent_command(if cfg!(windows) {
        "where"
    } else {
        "which"
    });
    cmd.arg(name);
    crate::process_util::output_with_timeout(cmd, WHICH_PROBE_TIMEOUT)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn open_in_editor(editor: &str, path: &str) -> Result<(), String> {
    let status = std::process::Command::new(editor)
        .arg(path)
        .spawn()
        .map_err(|e| format!("无法启动编辑器「{editor}」：{e}"))?;
    // detach
    drop(status);
    Ok(())
}

pub fn open_path(path: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
