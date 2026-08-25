//! 自有会话日志（借鉴 grok-app store/journal_throttle 思路）。
//!
//! 实时 ACP 事件在前端折叠为 ChatBlock[]，前端每 ≥500ms 把变化的 transcript
//! 快照写回这里（`save_journal`）。打开会话时**日志优先**水合，读不到再退回
//! CLI 的 `~/.grok/sessions/**/chat_history.jsonl` 解析（导入/兜底路径）——
//! CLI 换历史格式不再影响已有会话的展示。
//!
//! 文件：`%LOCALAPPDATA%\GrokFree\journals\<desktopSessionId>.json`
//! （整块 ChatBlock JSON 数组；原子写 tmp+rename；仅在 async command 的
//! `spawn_blocking` 里触碰磁盘，不上主线程。）

use crate::paths;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

/// 单份日志的块数上限：超出截断头部（UI 本就分页尾部渲染）。
pub const MAX_JOURNAL_BLOCKS: usize = 2000;

pub fn journals_dir() -> PathBuf {
    paths::desktop_data_dir().join("journals")
}

/// 会话 id 只允许安全字符（uuid / `desk_*`），拒绝路径穿越。
fn sanitize_id(session_id: &str) -> Result<String, String> {
    let t = session_id.trim();
    if t.is_empty() || t.len() > 128 {
        return Err("非法会话 id".into());
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("非法会话 id：{t}"));
    }
    Ok(t.to_string())
}

pub fn journal_path(session_id: &str) -> Result<PathBuf, String> {
    let id = sanitize_id(session_id)?;
    Ok(journals_dir().join(format!("{id}.json")))
}

/// 保存 transcript 快照（原子写；超长截断头部保留尾部）。
pub fn save_journal(session_id: &str, blocks: &Value) -> Result<(), String> {
    let path = journal_path(session_id)?;
    let arr = blocks
        .as_array()
        .ok_or_else(|| "日志内容必须是块数组".to_string())?;
    let slice: Vec<Value> = if arr.len() > MAX_JOURNAL_BLOCKS {
        arr[arr.len() - MAX_JOURNAL_BLOCKS..].to_vec()
    } else {
        arr.clone()
    };
    fs::create_dir_all(journals_dir()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string(&slice).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &text).map_err(|e| format!("写日志临时文件失败：{e}"))?;
    // Windows rename 不覆盖：先挪走旧文件
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    fs::rename(&tmp, &path).map_err(|e| format!("日志落盘失败：{e}"))
}

/// 读取日志快照；不存在 / 解析失败返回 None（调用方退回 CLI 历史）。
pub fn load_journal(session_id: &str) -> Option<Vec<Value>> {
    let path = journal_path(session_id).ok()?;
    let text = fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<Vec<Value>>(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("日志解析失败（{session_id}），退回 CLI 历史：{e}");
            None
        }
    }
}

/// 往日志尾部追加一条 system 块（turn-lease 启动修复用）。
/// 日志不存在时创建只含该块的日志。
pub fn append_system_note(session_id: &str, text: &str) -> Result<(), String> {
    let mut blocks = load_journal(session_id).unwrap_or_default();
    blocks.push(serde_json::json!({
        "kind": "system",
        "id": format!(
            "sys_repair_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ),
        "text": text
    }));
    save_journal(session_id, &Value::Array(blocks))
}

/// 会话 meta 删除时连带清理日志。
pub fn delete_journal(session_id: &str) {
    if let Ok(path) = journal_path(session_id) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn uid() -> String {
        format!(
            "test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )
    }

    #[test]
    fn roundtrip_save_load() {
        let id = uid();
        let blocks = json!([
            { "kind": "user", "id": "u1", "text": "你好" },
            { "kind": "assistant", "id": "a1", "text": "hi" }
        ]);
        save_journal(&id, &blocks).unwrap();
        let loaded = load_journal(&id).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[1]["text"], "hi");
        delete_journal(&id);
        assert!(load_journal(&id).is_none());
    }

    #[test]
    fn append_note_creates_and_appends() {
        let id = uid();
        append_system_note(&id, "中断修复").unwrap();
        append_system_note(&id, "第二条").unwrap();
        let loaded = load_journal(&id).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0]["kind"], "system");
        assert!(loaded[1]["text"].as_str().unwrap().contains("第二条"));
        delete_journal(&id);
    }

    #[test]
    fn truncates_to_cap() {
        let id = uid();
        let blocks: Vec<Value> = (0..MAX_JOURNAL_BLOCKS + 50)
            .map(|i| json!({ "kind": "system", "id": format!("s{i}"), "text": i.to_string() }))
            .collect();
        save_journal(&id, &Value::Array(blocks)).unwrap();
        let loaded = load_journal(&id).unwrap();
        assert_eq!(loaded.len(), MAX_JOURNAL_BLOCKS);
        // 保尾不保头
        assert_eq!(loaded.last().unwrap()["id"], format!("s{}", MAX_JOURNAL_BLOCKS + 49));
        delete_journal(&id);
    }

    #[test]
    fn rejects_bad_ids() {
        assert!(journal_path("../evil").is_err());
        assert!(journal_path("a/b").is_err());
        assert!(journal_path("").is_err());
        assert!(journal_path("019fb6e2-ok_ID").is_ok());
    }
}
