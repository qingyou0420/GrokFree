//! Disk session history IPC
use super::AppState;
use crate::config::DesktopState;
use crate::sessions_disk;
use chrono::Utc;
use tauri::State;

#[tauri::command]
pub async fn list_disk_sessions(
    limit: Option<usize>,
    cwd: Option<String>,
) -> Result<Vec<sessions_disk::DiskSession>, String> {
    // Scans ~/.grok/sessions and reads chat_history.jsonl heads: keep this
    // off the IPC thread so a large history can't freeze other commands.
    tokio::task::spawn_blocking(move || {
        sessions_disk::list_disk_sessions(limit.unwrap_or(80), cwd.as_deref())
    })
    .await
    .map_err(|e| format!("list join: {e}"))
}

/// Resolve on-disk session directory path for a grok session id (if present).
#[tauri::command]
pub async fn resolve_disk_session_path(session_id: String) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || {
        sessions_disk::resolve_session_dir(&session_id, None).map(|p| p.display().to_string())
    })
    .await
    .map_err(|e| format!("resolve join: {e}"))
}

/// Load chat transcript from ~/.grok/sessions for UI restore.
#[tauri::command]
pub async fn load_disk_transcript(
    session_id: String,
    path: Option<String>,
) -> Result<Vec<sessions_disk::TranscriptBlock>, String> {
    tokio::task::spawn_blocking(move || {
        sessions_disk::load_disk_transcript(&session_id, path.as_deref())
    })
    .await
    .map_err(|e| format!("transcript join: {e}"))?
}

/// Permanently delete a session folder under ~/.grok/sessions.
#[tauri::command]
pub async fn delete_disk_session(
    session_id: String,
    path: Option<String>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        sessions_disk::delete_disk_session(&session_id, path.as_deref())
    })
    .await
    .map_err(|e| format!("delete join: {e}"))?
}

// 同步 command 会在主线程执行；这些都做 `st.save()` 写盘，必须 async。
#[tauri::command]
pub async fn rename_session(
    state: State<'_, AppState>,
    session_id: String,
    title: String,
) -> Result<DesktopState, String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("标题不能为空".into());
    }
    let mut st = state.desktop.lock();
    if let Some(s) = st.sessions.iter_mut().find(|s| s.id == session_id) {
        s.title = title.clone();
        s.last_active_at = Utc::now().to_rfc3339();
    } else {
        return Err("会话不存在".into());
    }
    st.save()?;
    Ok(st.clone())
}

#[tauri::command]
pub async fn remove_session_meta(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DesktopState, String> {
    let mut st = state.desktop.lock();
    st.sessions.retain(|s| s.id != session_id);
    st.save()?;
    Ok(st.clone())
}

/// Remove project-list meta entries that look like empty / placeholder sessions
/// (uuid-only titles, "新会话", missing real title). Does not touch disk history.
#[tauri::command]
pub async fn purge_stale_session_meta(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> Result<DesktopState, String> {
    let mut st = state.desktop.lock();
    let before = st.sessions.len();
    st.sessions.retain(|s| {
        if let Some(ref pid) = project_id {
            if &s.project_id != pid {
                return true; // keep other projects
            }
        }
        !sessions_disk::is_placeholder_title(&s.title)
    });
    let removed = before.saturating_sub(st.sessions.len());
    st.save()?;
    tracing::info!("清理占位会话 meta：移除 {removed} 条");
    Ok(st.clone())
}

