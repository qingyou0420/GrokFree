//! Scan ~/.grok/sessions for historical sessions (design PR-12 / v0.2)
//!
//! On-disk layout used by Grok CLI / Build:
//! ```text
//! ~/.grok/sessions/
//!   <url-encoded-cwd>/
//!     <session-uuid>/
//!       summary.json
//!       chat_history.jsonl
//!       ...
//! ```

use crate::paths;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskSession {
    pub id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub path: String,
    pub source: String, // "index" | "directory" | "json" | "summary"
    /// Best-effort message count from summary.json (when present).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u32>,
}

/// UI chat block for replaying disk history into the frontend transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TranscriptBlock {
    #[serde(rename = "user")]
    User { id: String, text: String },
    #[serde(rename = "assistant")]
    Assistant { id: String, text: String },
    #[serde(rename = "thought")]
    Thought { id: String, text: String },
    #[serde(rename = "tool")]
    Tool {
        id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        title: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        input: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<Value>,
    },
    #[serde(rename = "system")]
    System { id: String, text: String },
    #[serde(rename = "plan")]
    Plan { id: String, text: String },
    #[serde(rename = "diff")]
    Diff {
        id: String,
        path: String,
        patch: String,
    },
}

/// List sessions from `~/.grok/sessions` (flexible layout).
///
/// Ghost / empty `session/new` folders are skipped. When `cwd_filter` is set,
/// only sessions whose cwd matches (path-normalized) are returned.
pub fn list_disk_sessions(limit: usize, cwd_filter: Option<&str>) -> Vec<DiskSession> {
    let root = paths::grok_sessions_dir();
    if !root.exists() {
        return vec![];
    }

    let mut out: Vec<DiskSession> = Vec::new();

    // Prefer index files if present
    for name in ["index.json", "sessions.json", "session-index.json"] {
        let p = root.join(name);
        if p.is_file() {
            if let Ok(list) = parse_index_file(&p) {
                out.extend(list);
            }
        }
    }

    // Walk: either nested <encoded-cwd>/<session-id>/ or flat session dirs / json
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            // Skip sqlite / non-session artifacts at root
            if path.is_file() {
                if path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("json"))
                    .unwrap_or(false)
                    && !matches!(
                        name.as_str(),
                        "index.json" | "sessions.json" | "session-index.json"
                    )
                {
                    if let Some(s) = session_from_json_file(&path) {
                        if !out.iter().any(|x| x.id == s.id) {
                            out.push(s);
                        }
                    }
                }
                continue;
            }

            if !path.is_dir() {
                continue;
            }

            // Nested layout: cwd bucket contains session uuid folders
            if is_cwd_bucket(&path) {
                let cwd_decoded = percent_decode(&name);
                if let Ok(children) = fs::read_dir(&path) {
                    for child in children.flatten() {
                        let child_path = child.path();
                        if !child_path.is_dir() {
                            continue;
                        }
                        if let Some(mut s) = session_from_session_dir(&child_path) {
                            if s.cwd.is_none() && !cwd_decoded.is_empty() {
                                s.cwd = Some(cwd_decoded.clone());
                            }
                            if !out.iter().any(|x| x.id == s.id) {
                                out.push(s);
                            }
                        }
                    }
                }
                continue;
            }

            // Flat: directory itself is a session (has summary.json / meta)
            if let Some(s) = session_from_session_dir(&path) {
                if !out.iter().any(|x| x.id == s.id) {
                    out.push(s);
                }
            }
        }
    }

    // Drop leftover uuid-only / placeholder titles that slipped past summary gate
    out.retain(|s| !is_placeholder_title(&s.title));

    if let Some(filter) = cwd_filter.map(str::trim).filter(|s| !s.is_empty()) {
        let want = normalize_path_key(filter);
        out.retain(|s| {
            s.cwd
                .as_deref()
                .map(|c| normalize_path_key(c) == want)
                .unwrap_or(false)
        });
    }

    // Sort by updated_at / created_at descending (string RFC3339-ish or mtime fallback)
    out.sort_by(|a, b| {
        let ka = a
            .updated_at
            .as_ref()
            .or(a.created_at.as_ref())
            .cloned()
            .unwrap_or_default();
        let kb = b
            .updated_at
            .as_ref()
            .or(b.created_at.as_ref())
            .cloned()
            .unwrap_or_default();
        kb.cmp(&ka)
    });

    if out.len() > limit {
        out.truncate(limit);
    }
    out
}

/// Permanently delete a session directory under `~/.grok/sessions`.
/// Refuses paths outside the sessions root.
pub fn delete_disk_session(session_id: &str, path_hint: Option<&str>) -> Result<(), String> {
    let dir = resolve_session_dir(session_id, path_hint)
        .ok_or_else(|| format!("未找到会话目录：{session_id}"))?;

    let root = paths::grok_sessions_dir();
    let root_canon = fs::canonicalize(&root).unwrap_or(root.clone());
    let dir_canon = fs::canonicalize(&dir).map_err(|e| format!("无法解析会话路径：{e}"))?;

    if !dir_canon.starts_with(&root_canon) {
        return Err("拒绝删除 sessions 目录之外的路径".into());
    }
    // Never delete the sessions root itself
    if dir_canon == root_canon {
        return Err("非法会话路径".into());
    }

    fs::remove_dir_all(&dir_canon).map_err(|e| format!("删除失败：{e}"))?;
    tracing::info!("已删除磁盘会话 {} · {}", session_id, dir_canon.display());
    Ok(())
}

/// True when a list title is only a UUID / truncated id (not a real summary).
pub fn is_placeholder_title(title: &str) -> bool {
    let t = title.trim();
    if t.is_empty() {
        return true;
    }
    if matches!(
        t,
        "新会话" | "New session" | "session" | "Session" | "untitled" | "Untitled"
    ) {
        return true;
    }
    // truncate_title style: "019fb6e2…"
    let base = t
        .trim_end_matches('…')
        .trim_end_matches("...")
        .trim_end_matches('…');
    if base.len() >= 6
        && base.len() <= 36
        && base
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-')
        && (t.ends_with('…') || t.ends_with("...") || base.len() >= 20)
    {
        return true;
    }
    // Full UUID-ish only
    if t.len() >= 20
        && t.len() <= 40
        && t.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
    {
        return true;
    }
    false
}

fn normalize_path_key(p: &str) -> String {
    p.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// Load UI transcript from `chat_history.jsonl` for a session id or explicit path.
pub fn load_disk_transcript(
    session_id: &str,
    path_hint: Option<&str>,
) -> Result<Vec<TranscriptBlock>, String> {
    let dir = resolve_session_dir(session_id, path_hint)
        .ok_or_else(|| format!("未找到会话目录：{session_id}"))?;

    let history = dir.join("chat_history.jsonl");
    if !history.is_file() {
        // Fallback: empty transcript with a system note
        return Ok(vec![TranscriptBlock::System {
            id: "sys_empty".into(),
            text: format!(
                "已定位会话目录，但未找到 chat_history.jsonl：{}",
                dir.display()
            ),
        }]);
    }

    parse_chat_history(&history)
}

fn parse_chat_history(path: &Path) -> Result<Vec<TranscriptBlock>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut out: Vec<TranscriptBlock> = Vec::new();
    let mut n: usize = 0;

    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let msg_type = v.get("type").and_then(|x| x.as_str()).unwrap_or("");

        match msg_type {
            "system" => {
                // Agent system prompt — not useful in the chat UI
            }
            "user" => {
                // Skip injected / synthetic context messages
                if v.get("synthetic_reason").and_then(|x| x.as_str()).is_some() {
                    continue;
                }
                let text = content_to_text(v.get("content"));
                if text.is_empty() {
                    continue;
                }
                // Prefer the actual user query body when wrapped
                let display = extract_user_query(&text).unwrap_or_else(|| text.clone());
                // Skip pure environment/bootstrap payloads
                if is_bootstrap_user_text(&display) {
                    continue;
                }
                n += 1;
                out.push(TranscriptBlock::User {
                    id: format!("hist_u_{n}"),
                    text: display,
                });
            }
            "assistant" => {
                let text = content_to_text(v.get("content"));
                if !text.is_empty() {
                    n += 1;
                    out.push(TranscriptBlock::Assistant {
                        id: format!("hist_a_{n}"),
                        text,
                    });
                }
                if let Some(calls) = v.get("tool_calls").and_then(|x| x.as_array()) {
                    for call in calls {
                        let tool_call_id = call
                            .get("id")
                            .or_else(|| call.get("tool_call_id"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        if tool_call_id.is_empty() {
                            continue;
                        }
                        let title = call
                            .get("name")
                            .or_else(|| call.get("title"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("tool")
                            .to_string();
                        let input = call
                            .get("arguments")
                            .cloned()
                            .or_else(|| call.get("input").cloned())
                            .map(|args| {
                                if let Some(s) = args.as_str() {
                                    serde_json::from_str::<Value>(s).unwrap_or(Value::String(s.to_string()))
                                } else {
                                    args
                                }
                            });
                        n += 1;
                        out.push(TranscriptBlock::Tool {
                            id: format!("hist_t_{n}"),
                            tool_call_id,
                            title,
                            status: "completed".into(),
                            input,
                            output: None,
                        });
                    }
                }
            }
            "tool_result" => {
                let tool_call_id = v
                    .get("tool_call_id")
                    .or_else(|| v.get("toolCallId"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("");
                if tool_call_id.is_empty() {
                    continue;
                }
                let output = v.get("content").cloned().or_else(|| {
                    v.get("output").cloned()
                });
                // Attach to the matching tool block if present
                if let Some(block) = out.iter_mut().rev().find(|b| match b {
                    TranscriptBlock::Tool {
                        tool_call_id: id, ..
                    } => id == tool_call_id,
                    _ => false,
                }) {
                    if let TranscriptBlock::Tool {
                        output: slot,
                        status,
                        ..
                    } = block
                    {
                        *slot = output;
                        *status = "completed".into();
                    }
                }
            }
            "reasoning" | "agent_thought" => {
                // Prefer human-readable summary; skip encrypted blobs
                let text = v
                    .get("summary")
                    .and_then(|x| {
                        if let Some(s) = x.as_str() {
                            Some(s.to_string())
                        } else if let Some(arr) = x.as_array() {
                            let joined = arr
                                .iter()
                                .filter_map(|i| {
                                    i.get("text")
                                        .and_then(|t| t.as_str())
                                        .or_else(|| i.as_str())
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            if joined.is_empty() {
                                None
                            } else {
                                Some(joined)
                            }
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        let t = content_to_text(v.get("content"));
                        if t.is_empty() {
                            None
                        } else {
                            Some(t)
                        }
                    });
                if let Some(text) = text {
                    if !text.trim().is_empty() {
                        n += 1;
                        out.push(TranscriptBlock::Thought {
                            id: format!("hist_r_{n}"),
                            text,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    if out.is_empty() {
        out.push(TranscriptBlock::System {
            id: "sys_empty_hist".into(),
            text: "历史文件已读取，但没有可展示的用户/助手消息。可能是空会话、侧栏 meta 指向了错误 ID，或 session/load 曾失败。请从「磁盘会话历史」重新选择该会话查看完整记录。".into(),
        });
    }

    Ok(out)
}

fn content_to_text(content: Option<&Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        let mut parts = Vec::new();
        for item in arr {
            if let Some(t) = item.get("text").and_then(|x| x.as_str()) {
                parts.push(t.to_string());
            } else if let Some(t) = item.as_str() {
                parts.push(t.to_string());
            } else if item.get("type").and_then(|x| x.as_str()) == Some("text") {
                if let Some(t) = item.get("text").and_then(|x| x.as_str()) {
                    parts.push(t.to_string());
                }
            }
        }
        return parts.join("\n");
    }
    if let Some(t) = content.get("text").and_then(|x| x.as_str()) {
        return t.to_string();
    }
    String::new()
}

fn extract_user_query(text: &str) -> Option<String> {
    const OPEN: &str = "<user_query>";
    const CLOSE: &str = "</user_query>";
    let start = text.find(OPEN)? + OPEN.len();
    let end = text.find(CLOSE)?;
    if end <= start {
        return None;
    }
    let inner = text[start..end].trim();
    if inner.is_empty() {
        None
    } else {
        Some(inner.to_string())
    }
}

fn is_bootstrap_user_text(text: &str) -> bool {
    let t = text.trim();
    if t.starts_with("<user_info>") {
        return true;
    }
    if t.starts_with("<system-reminder>") {
        return true;
    }
    // Very long injected context without a real query
    if t.len() > 4000 && !t.contains("<user_query>") {
        return true;
    }
    false
}

/// A cwd bucket is a top-level dir that holds session uuid children (not a session itself).
fn is_cwd_bucket(dir: &Path) -> bool {
    // Heuristic 1: name looks percent-encoded (Grok stores cwd as URL-encoded path)
    let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if name.contains('%') && (name.contains("%3A") || name.contains("%3a") || name.contains("%5C") || name.contains("%5c") || name.contains("%2F") || name.contains("%2f")) {
        return true;
    }

    // Heuristic 2: no summary/chat_history here, but children look like sessions
    let has_own = dir.join("summary.json").is_file()
        || dir.join("chat_history.jsonl").is_file()
        || dir.join("meta.json").is_file()
        || dir.join("session.json").is_file();
    if has_own {
        return false;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        let mut child_sessions = 0usize;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir()
                && (p.join("summary.json").is_file()
                    || p.join("chat_history.jsonl").is_file())
            {
                child_sessions += 1;
            }
        }
        return child_sessions > 0;
    }
    false
}

fn session_from_session_dir(dir: &Path) -> Option<DiskSession> {
    // Prefer summary.json (Grok CLI truth)
    let summary_path = dir.join("summary.json");
    if summary_path.is_file() {
        if let Ok(text) = fs::read_to_string(&summary_path) {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                // Ghost sessions: Desktop/CLI session/new with no real user turns
                // (empty title + num_messages=0) — hide from the history list.
                if is_ghost_session(&v, dir) {
                    return None;
                }
                if let Some(mut s) = disk_session_from_summary(&v, dir) {
                    s.path = dir.display().to_string();
                    s.source = "summary".into();
                    return Some(s);
                }
            }
        }
    }

    // Legacy meta files
    for name in ["meta.json", "session.json", "manifest.json", "info.json"] {
        let p = dir.join(name);
        if p.is_file() {
            if let Ok(text) = fs::read_to_string(&p) {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    if let Some(mut s) = disk_session_from_value(&v, &p, "directory") {
                        if s.id.is_empty() {
                            s.id = dir
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "unknown".into());
                        }
                        s.path = dir.display().to_string();
                        return Some(s);
                    }
                }
            }
        }
    }

    // Directory with chat_history but no summary
    if dir.join("chat_history.jsonl").is_file() {
        let id = dir.file_name()?.to_string_lossy().to_string();
        if id.len() < 8 {
            return None;
        }
        // No summary and no visible user/assistant content → skip
        if !session_has_visible_content(dir) {
            return None;
        }
        let mtime = file_mtime_rfc3339(dir);
        // Try first real user query as title; never fall back to raw UUID in the list
        let title = first_user_message_title(dir)?;
        if is_placeholder_title(&title) {
            return None;
        }
        return Some(DiskSession {
            id: id.clone(),
            title,
            cwd: None,
            created_at: mtime.clone(),
            updated_at: mtime,
            path: dir.display().to_string(),
            source: "directory".into(),
            message_count: None,
        });
    }

    None
}

/// True when CLI wrote a session dir that never received a real user turn.
/// Typical Desktop noise: every `session/new` creates `019…` folders with only
/// system + synthetic bootstrap lines (`num_messages: 0`, empty title).
fn is_ghost_session(v: &Value, dir: &Path) -> bool {
    let title = v
        .get("generated_title")
        .or_else(|| v.get("session_summary"))
        .or_else(|| v.get("title"))
        .or_else(|| v.get("name"))
        .and_then(|x| x.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    if title.is_some() {
        return false;
    }

    let num_messages = v.get("num_messages").and_then(|x| x.as_u64());
    if num_messages == Some(0) {
        // Confirm no extractable user query (field can lag, but empty+0 is the common case)
        return first_user_message_title(dir).is_none();
    }

    // Missing num_messages or non-zero: still ghost if history has no visible chat
    if num_messages.is_none() && !session_has_visible_content(dir) {
        return true;
    }

    false
}

fn session_has_visible_content(dir: &Path) -> bool {
    first_user_message_title(dir).is_some() || has_assistant_text(dir)
}

fn has_assistant_text(dir: &Path) -> bool {
    let history = dir.join("chat_history.jsonl");
    let file = match fs::File::open(history) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let reader = BufReader::new(file);
    for line in reader.lines().flatten().take(120) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(|x| x.as_str()) != Some("assistant") {
            continue;
        }
        let text = content_to_text(v.get("content"));
        if !text.trim().is_empty() {
            return true;
        }
    }
    false
}

fn disk_session_from_summary(v: &Value, dir: &Path) -> Option<DiskSession> {
    let info = v.get("info").cloned().unwrap_or(Value::Null);

    let id = v
        .get("id")
        .or_else(|| info.get("id"))
        .or_else(|| v.pointer("/info/id"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            dir.file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "unknown".into());

    let mut title = v
        .get("generated_title")
        .or_else(|| v.get("session_summary"))
        .or_else(|| v.get("title"))
        .or_else(|| v.get("name"))
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default();

    // Empty summary → first real user message; still empty → skip (ghost)
    if title.is_empty() {
        title = first_user_message_title(dir).unwrap_or_default();
    }
    if title.is_empty() || is_placeholder_title(&title) {
        // Keep only if we already rejected via is_ghost; double-guard for list quality
        if !session_has_visible_content(dir) {
            return None;
        }
        if title.is_empty() {
            title = first_user_message_title(dir).unwrap_or_else(|| "（无标题会话）".into());
        }
    }

    let cwd = v
        .get("cwd")
        .or_else(|| info.get("cwd"))
        .or_else(|| v.get("working_directory"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());

    let created_at = v
        .get("created_at")
        .or_else(|| v.get("createdAt"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| file_mtime_rfc3339(dir));

    let updated_at = v
        .get("updated_at")
        .or_else(|| v.get("last_active_at"))
        .or_else(|| v.get("updatedAt"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| created_at.clone());

    let message_count = v
        .get("num_messages")
        .or_else(|| v.get("num_chat_messages"))
        .and_then(|x| x.as_u64())
        .map(|n| n as u32);

    Some(DiskSession {
        id,
        title,
        cwd,
        created_at,
        updated_at,
        path: dir.display().to_string(),
        source: "summary".into(),
        message_count,
    })
}

/// First real user-facing message suitable as a list title.
/// Prefers `<user_query>` body; falls back to plain non-bootstrap user text.
fn first_user_message_title(dir: &Path) -> Option<String> {
    let history = dir.join("chat_history.jsonl");
    let file = fs::File::open(history).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines().flatten().take(80) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(|x| x.as_str()) != Some("user") {
            continue;
        }
        if v.get("synthetic_reason").is_some() {
            continue;
        }
        let text = content_to_text(v.get("content"));
        if text.trim().is_empty() {
            continue;
        }
        let display = extract_user_query(&text).unwrap_or(text);
        if is_bootstrap_user_text(&display) {
            continue;
        }
        let t = display.lines().next().unwrap_or(&display).trim();
        if !t.is_empty() {
            return Some(truncate_display(t, 48));
        }
    }
    None
}

fn parse_index_file(path: &Path) -> Result<Vec<DiskSession>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut out = Vec::new();

    let items = if let Some(arr) = v.as_array() {
        arr.clone()
    } else if let Some(arr) = v.get("sessions").and_then(|x| x.as_array()) {
        arr.clone()
    } else if let Some(arr) = v.get("items").and_then(|x| x.as_array()) {
        arr.clone()
    } else {
        return Ok(out);
    };

    for item in items {
        if let Some(s) = disk_session_from_value(&item, path, "index") {
            out.push(s);
        }
    }
    Ok(out)
}

fn session_from_json_file(path: &Path) -> Option<DiskSession> {
    let text = fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let mut s = disk_session_from_value(&v, path, "json")?;
    if s.id.is_empty() {
        s.id = path
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());
    }
    s.path = path.display().to_string();
    Some(s)
}

fn disk_session_from_value(
    v: &Value,
    path: &Path,
    source: &str,
) -> Option<DiskSession> {
    let id = v
        .get("id")
        .or_else(|| v.get("sessionId"))
        .or_else(|| v.get("session_id"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();

    let title = v
        .get("title")
        .or_else(|| v.get("generated_title"))
        .or_else(|| v.get("session_summary"))
        .or_else(|| v.get("name"))
        .or_else(|| v.get("summary"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if id.is_empty() {
                path.file_stem()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "session".into())
            } else {
                truncate_title(&id)
            }
        });

    let cwd = v
        .get("cwd")
        .or_else(|| v.get("workingDirectory"))
        .or_else(|| v.get("working_directory"))
        .or_else(|| v.get("workspace"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());

    let created_at = v
        .get("createdAt")
        .or_else(|| v.get("created_at"))
        .or_else(|| v.get("created"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| file_mtime_rfc3339(path));

    let updated_at = v
        .get("updatedAt")
        .or_else(|| v.get("updated_at"))
        .or_else(|| v.get("lastActiveAt"))
        .or_else(|| v.get("last_active_at"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| created_at.clone());

    // Skip empty junk
    if id.is_empty() && title == "session" && cwd.is_none() {
        return None;
    }

    if is_placeholder_title(&title) && cwd.is_none() {
        return None;
    }

    Some(DiskSession {
        id: if id.is_empty() {
            path.file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| UuidLite::new())
        } else {
            id
        },
        title,
        cwd,
        created_at,
        updated_at,
        path: path.display().to_string(),
        source: source.into(),
        message_count: None,
    })
}

fn truncate_title(id: &str) -> String {
    if id.chars().count() <= 12 {
        id.to_string()
    } else {
        format!("{}…", id.chars().take(8).collect::<String>())
    }
}

fn truncate_display(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

fn file_mtime_rfc3339(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let datetime: chrono::DateTime<chrono::Utc> = modified.into();
    Some(datetime.to_rfc3339())
}

/// Percent-decode a path segment (Grok stores cwd as URL-encoded folder names).
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Minimal unique id without pulling uuid in this module's call sites for empty cases
struct UuidLite;
impl UuidLite {
    fn new() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("disk-{t}")
    }
}

/// Resolve a session directory by id (walk nested layout) or path hint.
pub fn resolve_session_dir(session_id: &str, path_hint: Option<&str>) -> Option<PathBuf> {
    if let Some(hint) = path_hint {
        let p = PathBuf::from(hint);
        if p.is_dir()
            && (p.join("chat_history.jsonl").is_file()
                || p.join("summary.json").is_file()
                || p.file_name().map(|n| n.to_string_lossy() == session_id).unwrap_or(false))
        {
            return Some(p);
        }
        // hint may point at a json file
        if p.is_file() {
            if let Some(parent) = p.parent() {
                if parent.join("chat_history.jsonl").is_file() || parent.join("summary.json").is_file()
                {
                    return Some(parent.to_path_buf());
                }
            }
        }
    }

    let root = paths::grok_sessions_dir();
    let direct = root.join(session_id);
    if direct.is_dir() {
        return Some(direct);
    }

    // Nested: */session_id/
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let nested = path.join(session_id);
            if nested.is_dir() {
                return Some(nested);
            }
            // One more level if needed
            if let Ok(children) = fs::read_dir(&path) {
                for child in children.flatten() {
                    let cp = child.path();
                    if cp.is_dir() && cp.file_name().map(|n| n.to_string_lossy() == session_id).unwrap_or(false)
                    {
                        return Some(cp);
                    }
                }
            }
        }
    }

    None
}

#[allow(dead_code)]
pub fn resolve_session_path(session_id: &str) -> Option<PathBuf> {
    resolve_session_dir(session_id, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_value() {
        let v = serde_json::json!({
            "sessionId": "abc-123",
            "title": "Fix tests",
            "cwd": "D:\\\\proj"
        });
        let s = disk_session_from_value(&v, Path::new("x.json"), "json").unwrap();
        assert_eq!(s.id, "abc-123");
        assert_eq!(s.title, "Fix tests");
    }

    #[test]
    fn percent_decode_cwd() {
        let enc = "D%3A%5CGrok%20Build%5C%E6%83%85%E8%89%B2%E5%B0%8F%E8%AF%B4%E5%B7%A5%E4%BD%9C%E5%AE%A4";
        let dec = percent_decode(enc);
        assert!(dec.contains("Grok Build"));
        assert!(dec.contains("小说") || dec.contains("工作"));
    }

    #[test]
    fn extract_query() {
        let t = "<user_query>\n你好世界\n</user_query>";
        assert_eq!(extract_user_query(t).as_deref(), Some("你好世界"));
    }

    #[test]
    fn truncate_title_cjk_no_panic() {
        // A CJK id longer than 12 chars must truncate on char boundaries
        let cjk = "会话编号超长中文字符串超啊";
        let t = truncate_title(cjk);
        assert!(t.ends_with('…'));
        assert_eq!(t.chars().count(), 9);
        // Short ids pass through untouched
        assert_eq!(truncate_title("019fb6e2"), "019fb6e2");
    }

    #[test]
    fn summary_title() {
        let v = serde_json::json!({
            "info": { "id": "019fb22a-2fc3-7c81-84f0-96842ea1c98a", "cwd": "D:\\\\proj" },
            "generated_title": "Migrate Project",
            "session_summary": "Migrate Project",
            "created_at": "2026-07-30T08:35:37Z",
            "updated_at": "2026-07-30T08:38:14Z"
        });
        let s = disk_session_from_summary(&v, Path::new("sess")).unwrap();
        assert_eq!(s.title, "Migrate Project");
        assert_eq!(s.id, "019fb22a-2fc3-7c81-84f0-96842ea1c98a");
        assert_eq!(s.cwd.as_deref(), Some("D:\\\\proj"));
    }

    #[test]
    fn ghost_session_when_zero_messages_and_empty_title() {
        let v = serde_json::json!({
            "info": { "id": "019fb6e2-5c22-7671-815b-cf490202a778", "cwd": "D:\\\\proj" },
            "session_summary": "",
            "num_messages": 0,
            "num_chat_messages": 2
        });
        // No chat_history on this path → no user title → ghost
        assert!(is_ghost_session(&v, Path::new("missing-sess-dir")));
    }

    #[test]
    fn not_ghost_when_titled() {
        let v = serde_json::json!({
            "info": { "id": "019fb6e2-5c22-7671-815b-cf490202a778" },
            "generated_title": "Real Work",
            "num_messages": 0
        });
        assert!(!is_ghost_session(&v, Path::new("missing-sess-dir")));
    }

    #[test]
    fn placeholder_title_detects_uuid() {
        assert!(is_placeholder_title("019fb6e2…"));
        assert!(is_placeholder_title("019fb6e2-5c22-7671-815b-cf490202a778"));
        assert!(is_placeholder_title(""));
        assert!(is_placeholder_title("新会话"));
        assert!(!is_placeholder_title("修复删除会话"));
        assert!(!is_placeholder_title("Migrate Project"));
    }

    #[test]
    fn list_real_sessions_if_present() {
        let list = list_disk_sessions(80, None);
        // If the developer machine has ~/.grok/sessions, titles must not be raw %XX paths
        for s in &list {
            assert!(
                !s.title.contains("%3A") && !s.title.contains("%5C") && !s.title.contains("%E6"),
                "garbled title: {}",
                s.title
            );
            // Ghost sessions (empty title → truncated uuid like "019fb6e2…") should be filtered
            assert!(
                !(s.title.ends_with('…') && s.title.len() <= 10 && s.title.chars().all(|c| c.is_ascii_hexdigit() || c == '…')),
                "ghost/uuid-only title leaked: {}",
                s.title
            );
            if let Some(cwd) = &s.cwd {
                assert!(!cwd.contains("%3A"), "cwd still encoded: {cwd}");
            }
        }
        if let Some(first) = list.first() {
            let blocks = load_disk_transcript(&first.id, Some(&first.path)).unwrap();
            assert!(!blocks.is_empty(), "transcript should not be empty for {}", first.id);
        }
    }
}
