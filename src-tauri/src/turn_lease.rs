//! 轮次磁盘租约（借鉴 grok-app turn_lease / turn_interrupt 思路）。
//!
//! `session/prompt` 发出时落一个租约文件，轮次结束（成败皆然）删除。
//! 宿主进程崩溃 / 中途退出时租约残留：下次启动据此把该会话标记为
//! `interrupted`（而不是看起来"干净结束"），并往自有日志追加一条
//! system 说明。配套：启动时把 meta 里残留的 running / waiting_permission
//! 归一化为 hibernated——重启后进程必然已死，`running` 是谎言。
//!
//! 文件：`%LOCALAPPDATA%\GrokFree\turn-leases\<desktopSessionId>.json`

use crate::config::DesktopState;
use crate::paths;
use crate::session_fsm;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 记录进租约的用户指令头部长度（诊断用途，不是完整留档）。
pub const COMMAND_HEAD_CHARS: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TurnLease {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grok_session_id: Option<String>,
    pub title: String,
    /// 本轮用户指令的前若干字符
    pub command_head: String,
    pub started_at_ms: u128,
}

pub fn leases_dir() -> PathBuf {
    paths::desktop_data_dir().join("turn-leases")
}

fn sanitize_id(session_id: &str) -> Option<String> {
    let t = session_id.trim();
    if t.is_empty()
        || t.len() > 128
        || !t
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(t.to_string())
}

fn lease_path(session_id: &str) -> Option<PathBuf> {
    sanitize_id(session_id).map(|id| leases_dir().join(format!("{id}.json")))
}

/// 开轮：写租约。失败只记日志（租约是保险，不该挡住正常发送）。
pub fn write_lease(
    session_id: &str,
    grok_session_id: Option<&str>,
    title: &str,
    command: &str,
) {
    let Some(path) = lease_path(session_id) else {
        return;
    };
    let lease = TurnLease {
        session_id: session_id.to_string(),
        grok_session_id: grok_session_id.map(|s| s.to_string()),
        title: title.to_string(),
        command_head: command.chars().take(COMMAND_HEAD_CHARS).collect(),
        started_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    };
    if let Err(e) = fs::create_dir_all(leases_dir())
        .map_err(|e| e.to_string())
        .and_then(|_| serde_json::to_string(&lease).map_err(|e| e.to_string()))
        .and_then(|text| fs::write(&path, text).map_err(|e| e.to_string()))
    {
        tracing::warn!("写轮次租约失败（{session_id}）：{e}");
    }
}

/// 轮次结束（成功 / 失败 / 被停止）：清租约。
pub fn clear_lease(session_id: &str) {
    if let Some(path) = lease_path(session_id) {
        let _ = fs::remove_file(path);
    }
}

/// 列出残留租约（宿主上次没能正常收尾的轮次）。
pub fn list_leases() -> Vec<TurnLease> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(leases_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<TurnLease>(&t).ok())
        {
            Some(lease) => out.push(lease),
            None => {
                // 解析不了的残骸直接清掉，避免每次启动都报
                let _ = fs::remove_file(&path);
            }
        }
    }
    out
}

/// 启动修复：
/// 1. 残留租约 → 会话 meta 标 `interrupted` + 自有日志追加 system 说明 + 清租约；
/// 2. meta 里残留的 running / waiting_permission / starting → 归一化 hibernated。
///
/// 返回被标记为 interrupted 的会话数。在事件循环启动前调用（文件都很小）。
pub fn repair_on_startup(state: &mut DesktopState) -> usize {
    let leases = list_leases();
    let mut repaired = 0usize;

    for lease in &leases {
        let note = format!(
            "⚠ 上次运行中，桌面进程在本轮进行中中断，该轮未正常结束。\n中断时间线索：{}\n本轮指令开头：{}",
            format_ms(lease.started_at_ms),
            if lease.command_head.trim().is_empty() {
                "（空）".to_string()
            } else {
                lease.command_head.clone()
            }
        );
        if let Err(e) = crate::journal::append_system_note(&lease.session_id, &note) {
            tracing::warn!("中断修复写日志失败（{}）：{e}", lease.session_id);
        }
        if let Some(meta) = state
            .sessions
            .iter_mut()
            .find(|s| s.id == lease.session_id)
        {
            meta.status = session_fsm::status::INTERRUPTED.into();
            repaired += 1;
            tracing::info!(
                "中断修复：会话「{}」上轮未完成，已标记 interrupted",
                meta.title
            );
        }
        clear_lease(&lease.session_id);
    }

    // 无租约但状态残留「活着」：重启后进程必死，统一归为 hibernated
    for meta in state.sessions.iter_mut() {
        if session_fsm::is_stale_live_status(&meta.status) {
            meta.status = session_fsm::status::HIBERNATED.into();
        }
    }

    repaired
}

fn format_ms(ms: u128) -> String {
    use chrono::TimeZone;
    let secs = (ms / 1000) as i64;
    match chrono::Utc.timestamp_opt(secs, 0) {
        chrono::LocalResult::Single(t) => t.to_rfc3339(),
        _ => format!("{ms}ms"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> String {
        format!(
            "lease-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )
    }

    #[test]
    fn write_list_clear_roundtrip() {
        let id = uid();
        write_lease(&id, Some("grok-1"), "标题", "帮我改这个 bug，然后跑测试");
        let found = list_leases().into_iter().find(|l| l.session_id == id);
        let lease = found.expect("lease should be listed");
        assert_eq!(lease.grok_session_id.as_deref(), Some("grok-1"));
        assert!(lease.command_head.starts_with("帮我改"));
        clear_lease(&id);
        assert!(list_leases().iter().all(|l| l.session_id != id));
    }

    #[test]
    fn command_head_truncated() {
        let id = uid();
        let long: String = "长".repeat(COMMAND_HEAD_CHARS + 100);
        write_lease(&id, None, "t", &long);
        let lease = list_leases()
            .into_iter()
            .find(|l| l.session_id == id)
            .unwrap();
        assert_eq!(lease.command_head.chars().count(), COMMAND_HEAD_CHARS);
        clear_lease(&id);
    }

    #[test]
    fn bad_ids_are_ignored() {
        // 不应 panic，也不应写出文件
        write_lease("../evil", None, "t", "c");
        clear_lease("../evil");
        assert!(list_leases().iter().all(|l| l.session_id != "../evil"));
    }

    #[test]
    fn repair_marks_interrupted_and_normalizes() {
        use crate::config::{DesktopPrefs, SessionMeta};
        let id = uid();
        write_lease(&id, Some("g"), "测试会话", "cmd");
        let mk = |sid: &str, status: &str| SessionMeta {
            id: sid.into(),
            project_id: "p".into(),
            title: "t".into(),
            cwd: "c".into(),
            grok_session_id: Some("g".into()),
            agent_id: "grok".into(),
            status: status.into(),
            created_at: String::new(),
            last_active_at: String::new(),
            delegated_by: None,
            job_id: None,
        };
        let mut state = DesktopState {
            prefs: DesktopPrefs::defaults(),
            projects: vec![],
            sessions: vec![mk(&id, "running"), mk("other-1", "waiting_permission"), mk("other-2", "idle")],
            onboarding_done: true,
        };
        let repaired = repair_on_startup(&mut state);
        assert_eq!(repaired, 1);
        assert_eq!(state.sessions[0].status, "interrupted");
        // 无租约的残留活状态 → hibernated；idle 不动
        assert_eq!(state.sessions[1].status, "hibernated");
        assert_eq!(state.sessions[2].status, "idle");
        // 租约已清 + 日志有说明
        assert!(list_leases().iter().all(|l| l.session_id != id));
        let journal = crate::journal::load_journal(&id).unwrap();
        assert!(journal.last().unwrap()["text"]
            .as_str()
            .unwrap()
            .contains("中断"));
        crate::journal::delete_journal(&id);
    }
}
