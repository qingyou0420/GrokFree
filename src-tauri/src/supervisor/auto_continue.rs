//! 进程意外退出后的自动恢复。最多 2 次，且可由偏好关掉。
//!
//! 从 Supervisor 拆出，避免每加一种「进程死了之后怎么办」都继续堆在 mod.rs。

use super::Supervisor;
use super::LiveSession;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

/// 进程意外退出后自动恢复并补发「继续」的次数上限（防崩溃死循环）。
pub(super) const MAX_AUTO_CONTINUES: u8 = 2;
pub(super) const AUTO_CONTINUE_PROMPT: &str =
    "继续。上一轮因超时或进程退出中断。请从中断处接着完成，不要重复已经做过的修改。";

impl Supervisor {
    /// 偏好开着，且该会话还没到次数上限。
    pub(super) fn auto_continue_allowed(&self, session_id: &str) -> bool {
        if !self.state.lock().prefs.auto_continue {
            return false;
        }
        let n = self
            .auto_continues
            .lock()
            .get(session_id)
            .copied()
            .unwrap_or(0);
        n < MAX_AUTO_CONTINUES
    }

    pub(super) fn schedule_auto_continue(&self, app: AppHandle, meta: LiveSession) {
        if !self.state.lock().prefs.auto_continue {
            let _ = app.emit(
                "agent://autoContinue",
                json!({
                    "sessionId": meta.id,
                    "ok": false,
                    "error": "已关闭自动续跑",
                }),
            );
            return;
        }
        let n = {
            let mut map = self.auto_continues.lock();
            let e = map.entry(meta.id.clone()).or_insert(0);
            if *e >= MAX_AUTO_CONTINUES {
                tracing::warn!(
                    "会话 {} 自动续跑已达 {MAX_AUTO_CONTINUES} 次，不再重试",
                    meta.id
                );
                let _ = app.emit(
                    "agent://autoContinue",
                    json!({
                        "sessionId": meta.id,
                        "ok": false,
                        "error": "已达自动续跑上限",
                    }),
                );
                return;
            }
            *e += 1;
            *e
        };
        tracing::info!("会话 {} 进程意外退出，将自动恢复并继续（第 {n} 次）", meta.id);
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
            let Some(state) = app.try_state::<crate::commands::AppState>() else {
                return;
            };
            let Some(gsid) = meta.grok_session_id.clone() else {
                return;
            };
            if let Err(e) = state
                .supervisor
                .resume_session(
                    app.clone(),
                    meta.id.clone(),
                    gsid,
                    meta.project_id.clone(),
                    meta.cwd.clone(),
                    meta.title.clone(),
                    Some(meta.agent_id.clone()),
                )
                .await
            {
                tracing::warn!("自动恢复会话 {} 失败：{e}", meta.id);
                let _ = app.emit(
                    "agent://autoContinue",
                    json!({
                        "sessionId": meta.id,
                        "ok": false,
                        "error": e.to_string(),
                    }),
                );
                return;
            }
            if let Err(e) = state
                .supervisor
                .send_prompt(app.clone(), &meta.id, AUTO_CONTINUE_PROMPT)
                .await
            {
                tracing::warn!("自动继续发送失败（{}）：{e}", meta.id);
                let _ = app.emit(
                    "agent://autoContinue",
                    json!({
                        "sessionId": meta.id,
                        "ok": false,
                        "error": e.to_string(),
                    }),
                );
                return;
            }
            let _ = app.emit(
                "agent://autoContinue",
                json!({
                    "sessionId": meta.id,
                    "ok": true,
                    "text": AUTO_CONTINUE_PROMPT,
                    "attempt": n,
                }),
            );
        });
    }
}
