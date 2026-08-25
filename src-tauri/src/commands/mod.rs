//! Tauri commands (design §7.3)

use crate::config::{self, DesktopPrefs, DesktopState, Project};
use crate::diagnostics;
use crate::diff_ops;
use crate::git_ops;
use crate::paths;
use crate::supervisor::Supervisor;
use chrono::Utc;
use parking_lot::Mutex as StdMutex;
use serde_json::Value;
use std::fs;
use std::sync::Arc;
use tauri::{AppHandle, State};
use uuid::Uuid;

pub struct AppState {
    pub desktop: Arc<StdMutex<DesktopState>>,
    pub supervisor: Arc<Supervisor>,
    /// Session to focus when user clicks tray / second-instance (permission, error, toast).
    pub focus_session: Arc<StdMutex<Option<String>>>,
}

// NOTE: Tauri 把**同步** command 放在主线程执行。这里所有做文件 IO / 抢
// desktop 锁 / 起子进程的 command 一律 async（跑在 async runtime 上），
// 否则一次慢写盘 / 锁等待就会冻住整个窗口（连拖动都不行）。

#[tauri::command]
pub async fn get_app_state(state: State<'_, AppState>) -> Result<DesktopState, String> {
    // 返回内存态；勿每次磁盘重载，否则会与前端轮询/聚焦形成闪烁与状态抖动
    Ok(state.desktop.lock().clone())
}

#[tauri::command]
pub async fn reload_state(state: State<'_, AppState>) -> Result<DesktopState, String> {
    DesktopState::reload_into(&state.desktop);
    Ok(state.desktop.lock().clone())
}

#[tauri::command]
pub async fn probe_environment(state: State<'_, AppState>) -> Result<config::GrokEnvironment, String> {
    let prefs = state.desktop.lock().prefs.clone();
    tokio::task::spawn_blocking(move || config::probe_environment(&prefs))
        .await
        .map_err(|e| format!("probe join: {e}"))
}

#[tauri::command]
pub async fn update_prefs(state: State<'_, AppState>, prefs: DesktopPrefs) -> Result<DesktopState, String> {
    {
        let mut st = state.desktop.lock();
        st.prefs = prefs;
        st.save()?;
    }
    // grok 路径可能变了：作废版本探测缓存
    config::invalidate_version_cache();
    // Apply shell preference to ACP TerminalHost immediately
    state.supervisor.sync_shell_from_prefs();
    {
        let grok_path = state.desktop.lock().prefs.grok_path.clone();
        paths::apply_resolved_grok_home(if grok_path.trim().is_empty() {
            None
        } else {
            Some(grok_path.as_str())
        });
    }
    Ok(state.desktop.lock().clone())
}

#[tauri::command]
pub async fn set_onboarding_done(state: State<'_, AppState>, done: bool) -> Result<DesktopState, String> {
    let mut st = state.desktop.lock();
    st.onboarding_done = done;
    st.save()?;
    Ok(st.clone())
}

#[tauri::command]
pub async fn get_default_projects_dir(state: State<'_, AppState>) -> Result<String, String> {
    let dir = state.desktop.lock().prefs.default_projects_dir.clone();
    Ok(if dir.trim().is_empty() {
        r"D:\Grok Build".into()
    } else {
        dir
    })
}

#[tauri::command]
pub async fn add_project(state: State<'_, AppState>, cwd: String) -> Result<DesktopState, String> {
    let name = std::path::Path::new(&cwd)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| cwd.clone());
    let now = Utc::now().to_rfc3339();
    let project = Project {
        id: Uuid::new_v4().to_string(),
        name,
        cwd: cwd.clone(),
        created_at: now.clone(),
        last_opened_at: now,
    };
    let mut st = state.desktop.lock();
    st.projects.retain(|p| p.cwd != cwd);
    st.projects.insert(0, project);
    if st.projects.len() > 50 {
        st.projects.truncate(50);
    }
    st.save()?;
    Ok(st.clone())
}

#[tauri::command]
pub async fn remove_project(state: State<'_, AppState>, project_id: String) -> Result<DesktopState, String> {
    let mut st = state.desktop.lock();
    st.projects.retain(|p| p.id != project_id);
    st.sessions.retain(|s| s.project_id != project_id);
    st.save()?;
    Ok(st.clone())
}

#[tauri::command]
pub async fn open_config_file(state: State<'_, AppState>) -> Result<(), String> {
    let path = paths::grok_config_toml();
    if !path.exists() {
        // create empty so editor opens
        paths::ensure_desktop_dirs().ok();
        let _ = std::fs::create_dir_all(paths::grok_home());
        let _ = std::fs::write(&path, "# Grok Build config\n");
    }
    let editor = state.desktop.lock().prefs.default_editor.clone();
    config::open_in_editor(&editor, &path.display().to_string())
}

#[tauri::command]
pub async fn open_path(path: String) -> Result<(), String> {
    config::open_path(&path)
}

#[tauri::command]
pub async fn open_in_editor(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let editor = state.desktop.lock().prefs.default_editor.clone();
    config::open_in_editor(&editor, &path)
}

#[tauri::command]
pub async fn reveal_logs() -> Result<(), String> {
    paths::ensure_desktop_dirs().map_err(|e| e.to_string())?;
    config::open_path(&paths::desktop_logs_dir().display().to_string())
}

#[tauri::command]
pub async fn read_file(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let roots = {
        let st = state.desktop.lock();
        let mut roots: Vec<std::path::PathBuf> = Vec::new();
        for p in &st.projects {
            if !p.cwd.trim().is_empty() {
                roots.push(std::path::PathBuf::from(&p.cwd));
            }
        }
        for s in &st.sessions {
            if !s.cwd.trim().is_empty() {
                roots.push(std::path::PathBuf::from(&s.cwd));
            }
        }
        roots.push(paths::desktop_data_dir());
        roots.push(paths::grok_home());
        roots
    };
    tokio::task::spawn_blocking(move || {
        let resolved = crate::workspace::resolve_inside_any(&roots, &path)?;
        std::fs::read_to_string(resolved).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("read join: {e}"))?
}

#[tauri::command]
pub async fn app_info() -> Value {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let install_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.display().to_string()))
        .unwrap_or_default();
    serde_json::json!({
        "name": "GrokFree",
        "version": env!("CARGO_PKG_VERSION"),
        "identifier": "app.grokfree.desktop",
        "desktopDataDir": paths::desktop_data_dir().display().to_string(),
        "grokHome": paths::grok_home().display().to_string(),
        "executablePath": exe,
        "installDir": install_dir,
        "installersDir": paths::desktop_data_dir().join("installers").display().to_string(),
    })
}

/// Open `%LOCALAPPDATA%\\GrokBuild\\installers` for manual NSIS installs.
#[tauri::command]
pub async fn open_installers_dir() -> Result<String, String> {
    let dir = paths::desktop_data_dir().join("installers");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let s = dir.display().to_string();
    config::open_path(&s)?;
    Ok(s)
}

#[tauri::command]
pub async fn git_status(cwd: String) -> Result<git_ops::GitStatus, String> {
    tokio::task::spawn_blocking(move || git_ops::git_status(&cwd))
        .await
        .map_err(|e| format!("git join: {e}"))
}

#[tauri::command]
pub async fn apply_diff(
    cwd: String,
    path: String,
    patch: String,
) -> Result<diff_ops::ApplyDiffResult, String> {
    tokio::task::spawn_blocking(move || diff_ops::apply_diff(&cwd, &path, &patch))
        .await
        .map_err(|e| format!("diff join: {e}"))?
}

#[tauri::command]
pub async fn reject_diff(path: String) -> Result<diff_ops::ApplyDiffResult, String> {
    tokio::task::spawn_blocking(move || diff_ops::reject_diff(&path))
        .await
        .map_err(|e| format!("reject join: {e}"))
}

#[tauri::command]
pub async fn export_diagnostics(state: State<'_, AppState>) -> Result<String, String> {
    let st = state.desktop.lock().clone();
    let dir = tokio::task::spawn_blocking(move || diagnostics::export_diagnostics(&st))
        .await
        .map_err(|e| format!("diag join: {e}"))??;
    // Open the folder for the user
    let _ = config::open_path(&dir);
    Ok(dir)
}

#[tauri::command]
pub async fn open_external_terminal(state: State<'_, AppState>, cwd: String) -> Result<(), String> {
    let shell = state.desktop.lock().prefs.default_shell.clone();
    tokio::task::spawn_blocking(move || open_terminal(&shell, &cwd))
        .await
        .map_err(|e| format!("terminal join: {e}"))?
}

/// Readonly Skills / MCP snapshot from ~/.grok (v0.3).
#[tauri::command]
pub async fn list_skills_mcp() -> Result<crate::cli_caps::SkillsMcpSnapshot, String> {
    tokio::task::spawn_blocking(crate::cli_caps::list_skills_and_mcp)
        .await
        .map_err(|e| format!("skills join: {e}"))
}

/// Static capability flags for Settings UI.
#[tauri::command]
pub async fn cli_capabilities() -> crate::cli_caps::CliCapabilities {
    crate::cli_caps::capabilities()
}

/// Update system tray tooltip / title from aggregate agent status (v0.3).
/// `level`: idle | running | needs_attention
/// `focus_session_id`: optional session to open when user clicks the tray.
#[tauri::command]
pub async fn update_tray_status(
    app: AppHandle,
    state: State<'_, AppState>,
    level: String,
    detail: String,
    focus_session_id: Option<String>,
) -> Result<(), String> {
    {
        let mut slot = state.focus_session.lock();
        match level.as_str() {
            "needs_attention" | "waiting_permission" | "error" => {
                if let Some(id) = focus_session_id.filter(|s| !s.is_empty()) {
                    *slot = Some(id);
                }
            }
            "running" => {
                if let Some(id) = focus_session_id.filter(|s| !s.is_empty()) {
                    *slot = Some(id);
                }
            }
            _ => {
                // idle — clear only if no explicit target
                if focus_session_id.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
                    *slot = None;
                }
            }
        }
    }

    let tooltip = match level.as_str() {
        "running" => {
            if detail.is_empty() {
                "GrokFree · 运行中".into()
            } else {
                format!("GrokFree · 运行中 · {detail}")
            }
        }
        "needs_attention" | "waiting_permission" => {
            if detail.is_empty() {
                "GrokFree · 需要授权".into()
            } else {
                format!("GrokFree · 需要授权 · {detail}")
            }
        }
        "error" => {
            if detail.is_empty() {
                "GrokFree · 出错".into()
            } else {
                format!("GrokFree · 出错 · {detail}")
            }
        }
        _ => {
            if detail.is_empty() {
                "GrokFree".into()
            } else {
                format!("GrokFree · {detail}")
            }
        }
    };
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_tooltip(Some(&tooltip))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Focus main window (e.g. from toast / tray). Optionally emit session focus for UI.
#[tauri::command]
pub async fn focus_main_window(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let id = session_id
        .filter(|s| !s.is_empty())
        .or_else(|| state.focus_session.lock().clone());
    if let Some(id) = id {
        let _ = app.emit("app://focus-session", id);
    }
    Ok(())
}

/// Show main window and emit pending focus session (tray / second-instance helper).
pub fn show_and_focus_pending(app: &AppHandle) {
    use tauri::{Emitter, Manager};
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    if let Some(state) = app.try_state::<AppState>() {
        if let Some(id) = state.focus_session.lock().clone() {
            let _ = app.emit("app://focus-session", id);
        }
    }
}

fn open_terminal(shell: &str, cwd: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let status = match shell {
            "cmd" => {
                use std::os::windows::process::CommandExt;
                // raw_arg avoids Rust's arg quoting fighting cmd's own quote
                // rules; the empty "" is start's window-title placeholder.
                std::process::Command::new("cmd")
                    .raw_arg(format!("/c start \"\" cmd /k \"cd /d \"{cwd}\"\""))
                    .spawn()
            }
            "gitbash" => std::process::Command::new("bash")
                .args(["-lc", &format!("cd '{}' && exec bash", cwd.replace('\'', "'\\''"))])
                .spawn(),
            "wsl" => std::process::Command::new("wsl")
                .args(["--cd", cwd])
                .spawn(),
            "pwsh" => {
                if std::process::Command::new("wt")
                    .args(["-d", cwd, "pwsh", "-NoExit"])
                    .spawn()
                    .is_ok()
                {
                    return Ok(());
                }
                std::process::Command::new("pwsh")
                    .args([
                        "-NoExit",
                        "-Command",
                        &format!("Set-Location -LiteralPath '{}'", cwd.replace('\'', "''")),
                    ])
                    .spawn()
            }
            _ => {
                // powershell / default — prefer Windows Terminal
                if std::process::Command::new("wt")
                    .args(["-d", cwd])
                    .spawn()
                    .is_ok()
                {
                    return Ok(());
                }
                std::process::Command::new("powershell")
                    .args([
                        "-NoExit",
                        "-Command",
                        &format!("Set-Location -LiteralPath '{}'", cwd.replace('\'', "''")),
                    ])
                    .spawn()
            }
        };
        status.map_err(|e| format!("无法打开终端：{e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (shell, cwd);
        Err("仅 Windows 支持外部终端".into())
    }
}

pub mod agents_cmd;
pub mod disk;
pub mod update;
pub mod session;
