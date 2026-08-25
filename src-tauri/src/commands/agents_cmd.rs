//! Agent profile registry IPC（agents.json 读写，供 Settings 智能体页）

use crate::agents::AgentProfile;

#[tauri::command]
pub fn list_agents() -> Result<Vec<AgentProfile>, String> {
    Ok(crate::agents::load())
}

#[tauri::command]
pub fn save_agents(profiles: Vec<AgentProfile>) -> Result<Vec<AgentProfile>, String> {
    if profiles.is_empty() {
        return Err("至少保留一个小精灵档案".into());
    }
    let mut ids = std::collections::HashSet::new();
    for p in &profiles {
        let id = p.id.trim();
        if id.is_empty() {
            return Err("档案 id 不能为空".into());
        }
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!("档案 id「{id}」只能含字母/数字/-/_"));
        }
        if !ids.insert(id.to_string()) {
            return Err(format!("档案 id 重复：{id}"));
        }
    }
    // 保存后重新 load：把 grok 兜底补回等合并逻辑统一走读路径
    crate::agents::save(&profiles)?;
    Ok(crate::agents::load())
}
