//! Composer 提示词润色（0.8.x `polish_prompt`，不依赖霜月陪伴）。
//! 走 api.x.ai Chat Completions，密钥来自环境变量或 ~/.grok/auth.json。

use crate::paths;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::time::Duration;

pub const MAX_CHARS: usize = 12_000;
pub const DEFAULT_MODEL: &str = "grok-4.6";

const SYSTEM_PROMPT: &str = "\
你是专业的 AI 编程 Agent 提示词润色助手，类似 Workbuddy 的提示强化。

任务：把用户草稿改写成更清晰、专业、可执行的提示词，便于编码 agent 准确完成任务。

要求：
1) 保留原意、约束、路径、技术栈与验收标准，不要删掉关键细节。
2) 用专业术语与结构化表述加强可执行性：目标 / 上下文 / 步骤或范围 / 约束 / 验收。
3) 不要回答问题本身，不要解释你的修改，不要加前后缀说明。
4) 只输出润色后的完整提示词正文，纯文本。";

pub fn validate_draft(text: &str) -> Result<&str, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("输入为空，无法润色".into());
    }
    if t.chars().count() > MAX_CHARS {
        return Err("文本过长，请小于约 12000 字".into());
    }
    Ok(t)
}

/// 去掉模型偶尔包上的 markdown 围栏 / 引号。
pub fn sanitize_polished(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if s.starts_with("```") {
        let mut lines: Vec<&str> = s.lines().collect();
        if !lines.is_empty() {
            lines.remove(0);
        }
        if lines
            .last()
            .map(|l| l.trim().starts_with("```"))
            .unwrap_or(false)
        {
            lines.pop();
        }
        s = lines.join("\n").trim().to_string();
    }
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('“') && s.ends_with('”') && s.chars().count() >= 2)
    {
        let mut chars: Vec<char> = s.chars().collect();
        chars.pop();
        chars.remove(0);
        s = chars.into_iter().collect::<String>().trim().to_string();
    }
    s
}

fn read_api_key() -> Result<String, String> {
    for var in ["XAI_API_KEY", "GROK_API_KEY"] {
        if let Ok(v) = std::env::var(var) {
            let t = v.trim();
            if !t.is_empty() {
                return Ok(t.to_string());
            }
        }
    }
    let path = paths::grok_home().join("auth.json");
    let raw = fs::read_to_string(&path).map_err(|_| {
        "未找到 API 密钥。请设置用户级 XAI_API_KEY，或先完成 grok CLI 登录（~/.grok/auth.json）"
            .to_string()
    })?;
    let v: Value = serde_json::from_str(&raw).map_err(|_| "auth.json 无法解析".to_string())?;
    for key in ["api_key", "access_token", "token"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            let t = s.trim();
            if !t.is_empty() {
                return Ok(t.to_string());
            }
        }
    }
    Err("auth.json 中未找到可用 token。请设置 XAI_API_KEY 或重新登录 grok CLI".into())
}

fn resolve_model(prefs_model: &str) -> String {
    if let Ok(v) = std::env::var("POLISH_MODEL") {
        let t = v.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    let m = prefs_model.trim();
    if m.is_empty() {
        DEFAULT_MODEL.to_string()
    } else {
        m.to_string()
    }
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Option<Vec<ChatChoice>>,
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: Option<ChatMessage>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    message: Option<String>,
}

pub async fn polish_prompt(text: &str, prefs_model: &str) -> Result<String, String> {
    let draft = validate_draft(text)?;
    let key = read_api_key()?;
    let model = resolve_model(prefs_model);

    let body = json!({
        "model": model,
        "temperature": 0.3,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            {
                "role": "user",
                "content": format!("请润色并强化以下 agent 提示词草稿：\n\n{draft}")
            }
        ]
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(format!("GrokFree/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("HTTP 客户端错误：{e}"))?;

    let res = client
        .post("https://api.x.ai/v1/chat/completions")
        .header("Content-Type", "application/json")
        .bearer_auth(&key)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            format!("请求 api.x.ai 失败：{e}。若持续失败，请检查 XAI_API_KEY 与网络（Agent 编程会话不受影响，走 CLI）")
        })?;

    let status = res.status();
    let raw = res
        .text()
        .await
        .map_err(|e| format!("读取响应失败：{e}"))?;

    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err("鉴权失败。请设置用户级 XAI_API_KEY，或确认 ~/.grok/auth.json 仍有效后重启".into());
    }
    if status.as_u16() == 404 {
        return Err(format!(
            "模型不可用：请求的 {model} 在 api.x.ai 上可能不存在或无权。可在设置改模型，或设 POLISH_MODEL"
        ));
    }
    if !status.is_success() {
        return Err(format!("Grok API HTTP {}：{}", status.as_u16(), raw.chars().take(240).collect::<String>()));
    }

    let parsed: ChatResponse =
        serde_json::from_str(&raw).map_err(|e| format!("解析响应 JSON 失败：{e}"))?;
    if let Some(err) = parsed.error.and_then(|e| e.message) {
        return Err(err);
    }
    let content = parsed
        .choices
        .as_ref()
        .and_then(|c| c.first())
        .and_then(|c| c.message.as_ref())
        .and_then(|m| m.content.as_ref())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "模型返回为空".to_string())?;

    let out = sanitize_polished(&content);
    if out.is_empty() {
        return Err("模型返回为空".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert!(validate_draft("   ").is_err());
        assert!(validate_draft("").is_err());
    }

    #[test]
    fn rejects_too_long() {
        let s = "啊".repeat(MAX_CHARS + 1);
        assert!(validate_draft(&s).is_err());
        let s = "a".repeat(MAX_CHARS);
        assert!(validate_draft(&s).is_ok());
    }

    #[test]
    fn strips_fence() {
        let raw = "```text\n目标：修 bug\n验收：测试通过\n```";
        assert_eq!(sanitize_polished(raw), "目标：修 bug\n验收：测试通过");
    }

    #[test]
    fn keeps_plain() {
        assert_eq!(sanitize_polished("  修好登录  "), "修好登录");
    }
}
