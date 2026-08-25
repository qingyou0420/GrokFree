//! Live agent session IPC
use super::AppState;
use serde_json::Value;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn create_session(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    cwd: String,
    title: Option<String>,
    agent_id: Option<String>,
    delegated_by: Option<String>,
    job_id: Option<String>,
) -> Result<crate::supervisor::LiveSession, String> {
    state
        .supervisor
        .create_session(
            app,
            project_id,
            cwd,
            title,
            agent_id,
            delegated_by,
            job_id,
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resume_session(
    app: AppHandle,
    state: State<'_, AppState>,
    desktop_session_id: String,
    grok_session_id: String,
    project_id: String,
    cwd: String,
    title: String,
    agent_id: Option<String>,
) -> Result<crate::supervisor::LiveSession, String> {
    state
        .supervisor
        .resume_session(
            app,
            desktop_session_id,
            grok_session_id,
            project_id,
            cwd,
            title,
            agent_id,
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_live_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<crate::supervisor::LiveSession>, String> {
    Ok(state.supervisor.list_live().await)
}

#[tauri::command]
pub async fn send_prompt(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    text: String,
) -> Result<(), String> {
    state
        .supervisor
        .send_prompt(app, &session_id, &text)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cancel_prompt(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    state
        .supervisor
        .cancel(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn respond_permission(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    request_id: Value,
    allow: bool,
    option_id: Option<String>,
    remember_scope: Option<String>,
) -> Result<(), String> {
    state
        .supervisor
        .respond_permission(app, &session_id, request_id, allow, option_id, remember_scope)
        .await
        .map_err(|e| e.to_string())
}

/// 静默提示的「继续等待」：重置该会话的静默计时。
#[tauri::command]
pub async fn stall_keep_waiting(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state.supervisor.stall_keep_waiting(&session_id).await;
    Ok(())
}

/// 取消一次在途的会话启动/恢复（杀掉 initialize/load 中的 Agent 进程）。
#[tauri::command]
pub async fn cancel_start(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state
        .supervisor
        .cancel_start(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn handle_server_request(
    state: State<'_, AppState>,
    session_id: String,
    request_id: Value,
    method: String,
    params: Value,
) -> Result<(), String> {
    // Prefer the unified client-request path (fs + terminal). Terminal is also
    // handled directly in the ACP event forwarder; this remains for UI fallbacks.
    let handled = state
        .supervisor
        .handle_client_request(&session_id, request_id.clone(), &method, &params)
        .await
        .map_err(|e| e.to_string())?;
    if !handled {
        state
            .supervisor
            .respond_server_request(
                &session_id,
                request_id,
                None,
                Some(format!("未处理的客户端方法：{method}")),
            )
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 项目切换钩子：休眠其他项目里空闲/出错的会话，回收其 grok 进程。
/// 运行中 / 等待授权的会话不动。返回回收数量。
#[tauri::command]
pub async fn set_active_project(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
) -> Result<usize, String> {
    Ok(state
        .supervisor
        .hibernate_other_projects_idle(app, &project_id)
        .await)
}

#[tauri::command]
pub async fn hibernate_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state
        .supervisor
        .hibernate(app, &session_id)
        .await
        .map_err(|e| e.to_string())
}

