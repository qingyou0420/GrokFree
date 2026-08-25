//! One-click diagnostics bundle export (design PR-15 / v0.2)

use crate::config::{self, DesktopPrefs, DesktopState};
use crate::paths;
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn export_diagnostics(state: &DesktopState) -> Result<String, String> {
    paths::ensure_desktop_dirs().map_err(|e| e.to_string())?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let out_dir = paths::desktop_data_dir()
        .join("diagnostics")
        .join(format!("bundle-{ts}"));
    fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    let env = config::probe_environment(&state.prefs);

    // summary.json (no secrets)
    let summary = json!({
        "name": "GrokFree",
        "version": env!("CARGO_PKG_VERSION"),
        "exportedAt": chrono::Utc::now().to_rfc3339(),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "environment": {
            "grokHome": env.grok_home,
            "grokPath": env.grok_path,
            "grokExists": env.grok_exists,
            "grokVersion": env.grok_version,
            "authExists": env.auth_exists,
            "authLoggedIn": env.auth_logged_in,
            "configExists": env.config_exists,
            "sessionsDir": env.sessions_dir,
            "desktopDataDir": env.desktop_data_dir,
        },
        "prefs": redact_prefs(&state.prefs),
        "capabilities": crate::cli_caps::capabilities(),
        "cliVersionOk": env.cli_version_ok,
        "minCliVersion": env.min_cli_version,
        "projectCount": state.projects.len(),
        "sessionMetaCount": state.sessions.len(),
        "projects": state.projects.iter().map(|p| json!({
            "name": p.name,
            "cwd": p.cwd,
        })).collect::<Vec<_>>(),
        "sessions": state.sessions.iter().take(30).map(|s| json!({
            "id": s.id,
            "title": s.title,
            "cwd": s.cwd,
            "status": s.status,
            "grokSessionId": s.grok_session_id,
            "lastActiveAt": s.last_active_at,
        })).collect::<Vec<_>>(),
    });

    write_json(&out_dir.join("summary.json"), &summary)?;

    // Copy recent log files (last ~512KB total)
    let logs_src = paths::desktop_logs_dir();
    let logs_dst = out_dir.join("logs");
    let _ = fs::create_dir_all(&logs_dst);
    if logs_src.is_dir() {
        if let Ok(rd) = fs::read_dir(&logs_src) {
            let mut files: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            files.sort();
            for path in files.into_iter().rev().take(5) {
                if path.is_file() {
                    if let Some(name) = path.file_name() {
                        let _ = fs::copy(&path, logs_dst.join(name));
                    }
                }
            }
        }
    }

    // README
    let readme = format!(
        "GrokFree diagnostics\n\
         ==============================\n\
         Exported: {}\n\
         Open summary.json for environment and session metadata.\n\
         Secrets (API keys, tokens) are NOT included.\n\
         Share this folder when reporting issues.\n",
        chrono::Utc::now().to_rfc3339()
    );
    fs::write(out_dir.join("README.txt"), readme).map_err(|e| e.to_string())?;

    // Also write a pointer file at diagnostics/latest.txt
    let _ = fs::write(
        paths::desktop_data_dir()
            .join("diagnostics")
            .join("latest.txt"),
        out_dir.display().to_string(),
    );

    Ok(out_dir.display().to_string())
}

fn redact_prefs(p: &DesktopPrefs) -> serde_json::Value {
    json!({
        "permissionMode": p.permission_mode,
        "sandboxMode": p.sandbox_mode,
        "model": p.model,
        "theme": p.theme,
        "defaultEditor": p.default_editor,
        "defaultShell": p.default_shell,
        "minCliVersion": p.min_cli_version,
        "defaultProjectsDir": p.default_projects_dir,
        "grokPathSet": !p.grok_path.trim().is_empty(),
        "fsScope": p.fs_scope,
        "historyInitialVisible": p.history_initial_visible,
        "chatMaskQuiet": p.chat_mask_quiet,
        "agentChannel": "grok agent stdio (CLI)",
    })
}

fn write_json(path: &PathBuf, v: &serde_json::Value) -> Result<(), String> {
    let s = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    let mut f = fs::File::create(path).map_err(|e| e.to_string())?;
    f.write_all(s.as_bytes()).map_err(|e| e.to_string())
}
