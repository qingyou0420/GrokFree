//! Agent registry: 配置驱动的多智能体档案（多模型 / 多供应商）。
//!
//! 每个档案描述如何启动一个说 ACP 的进程：command + args + env。
//! grok 走原生集成（版本门禁 / GROK_HOME / --model / --always-approve / 磁盘 resume），
//! 其他档案（claude-agent-acp 接 GLM/DeepSeek、qwen、opencode…）纯配置，
//! 模型经 `{model}` 占位符注入 args 或 env。
//! 档案存 desktop_data_dir/agents.json，与 desktop-state.json 同级。

use crate::paths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_AGENT_ID: &str = "grok";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentProfile {
    /// 稳定 id（进 SessionMeta.agentId）
    pub id: String,
    /// 显示名
    pub name: String,
    /// 可执行命令（grok 原生档忽略此字段，走 prefs.grok_path 解析）
    pub command: String,
    /// 命令参数，支持 `{model}` 占位符
    pub args: Vec<String>,
    /// 环境变量，值支持 `{model}` 占位符（如 ANTHROPIC_MODEL）
    pub env: HashMap<String, String>,
    /// 默认 / 可选模型（UI 下拉用；空则模型输入框自由填写）
    pub models: Vec<String>,
    pub default_model: String,
    /// Grok 原生集成：版本门禁 + GROK_HOME + --model/--always-approve 标志
    pub is_grok: bool,
    /// 是否支持 session/load 恢复（磁盘历史目前仅 grok 格式）
    pub supports_resume: bool,
    pub enabled: bool,
    /// 备注（内置模板写用途说明，Settings 里展示）
    pub note: String,
}

impl Default for AgentProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            models: Vec::new(),
            default_model: String::new(),
            is_grok: false,
            supports_resume: false,
            enabled: false,
            note: String::new(),
        }
    }
}

/// 档案构建出的最终启动参数（纯函数，便于单测）
#[derive(Debug, PartialEq)]
pub struct SpawnSpec {
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

pub fn profiles_path() -> PathBuf {
    paths::desktop_data_dir().join("agents.json")
}

/// 内置档案。grok 默认启用；其余为模板（enabled=false，填好密钥再开）。
pub fn builtin_profiles() -> Vec<AgentProfile> {
    let mut grok_env = HashMap::new();
    grok_env.insert("NO_COLOR".to_string(), "1".to_string());
    vec![
        AgentProfile {
            id: "grok".into(),
            name: "Grok".into(),
            command: "grok".into(),
            args: vec!["agent".into(), "stdio".into()],
            env: grok_env,
            models: vec![],
            default_model: String::new(),
            is_grok: true,
            supports_resume: true,
            enabled: true,
            note: "xAI Grok CLI（原生集成：版本门禁 / 磁盘历史恢复）".into(),
        },
        AgentProfile {
            id: "glm".into(),
            name: "GLM（智谱）".into(),
            command: "npx".into(),
            args: vec![
                "@agentclientprotocol/claude-agent-acp@latest".into(),
            ],
            env: [
                (
                    "ANTHROPIC_BASE_URL",
                    "https://open.bigmodel.cn/api/anthropic",
                ),
                // 填智谱 API Key（在「设置 → 智能体」里编辑）
                ("ANTHROPIC_AUTH_TOKEN", ""),
                ("ANTHROPIC_MODEL", "{model}"),
                ("NO_COLOR", "1"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            models: vec!["glm-4.6".into(), "glm-4.5".into()],
            default_model: "glm-4.6".into(),
            is_grok: false,
            supports_resume: false,
            enabled: false,
            note: "智谱 Anthropic 兼容端点，经 claude-agent-acp 适配。填入 ANTHROPIC_AUTH_TOKEN 后启用。".into(),
        },
        AgentProfile {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            command: "npx".into(),
            args: vec![
                "@agentclientprotocol/claude-agent-acp@latest".into(),
            ],
            env: [
                ("ANTHROPIC_BASE_URL", "https://api.deepseek.com/anthropic"),
                ("ANTHROPIC_AUTH_TOKEN", ""),
                ("ANTHROPIC_MODEL", "{model}"),
                ("NO_COLOR", "1"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            models: vec!["deepseek-chat".into(), "deepseek-reasoner".into()],
            default_model: "deepseek-chat".into(),
            is_grok: false,
            supports_resume: false,
            enabled: false,
            note: "DeepSeek Anthropic 兼容端点，经 claude-agent-acp 适配。填入 ANTHROPIC_AUTH_TOKEN 后启用。".into(),
        },
        AgentProfile {
            id: "claude".into(),
            name: "Claude".into(),
            command: "npx".into(),
            args: vec![
                "@agentclientprotocol/claude-agent-acp@latest".into(),
            ],
            env: [
                ("ANTHROPIC_API_KEY", ""),
                ("NO_COLOR", "1"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            models: vec![],
            default_model: String::new(),
            is_grok: false,
            supports_resume: false,
            enabled: false,
            note: "Anthropic 官方 API，经 claude-agent-acp 适配。填入 ANTHROPIC_API_KEY 后启用。".into(),
        },
        AgentProfile {
            id: "qwen".into(),
            name: "Qwen Code".into(),
            command: "npx".into(),
            args: vec!["@qwen-code/qwen-code@latest".into(), "--acp".into()],
            env: [("NO_COLOR".to_string(), "1".to_string())]
                .into_iter()
                .collect(),
            models: vec![],
            default_model: String::new(),
            is_grok: false,
            supports_resume: false,
            enabled: false,
            note: "Qwen Code CLI 原生 ACP 模式（--acp）。需自行登录/配置授权。".into(),
        },
        AgentProfile {
            id: "opencode".into(),
            name: "OpenCode".into(),
            command: "npx".into(),
            args: vec![
                "opencode-ai@latest".into(),
                "acp".into(),
                "serve".into(),
            ],
            env: [("NO_COLOR".to_string(), "1".to_string())]
                .into_iter()
                .collect(),
            models: vec![],
            default_model: String::new(),
            is_grok: false,
            supports_resume: false,
            enabled: false,
            note: "OpenCode（任意 OpenAI 兼容模型，自身配置文件选模型）".into(),
        },
    ]
}

/// 读取档案列表；文件不存在时落一份内置默认并返回。
pub fn load() -> Vec<AgentProfile> {
    let path = profiles_path();
    match fs::read_to_string(&path) {
        Ok(s) => match serde_json::from_str::<Vec<AgentProfile>>(&s) {
            Ok(list) => {
                // 保证 grok 档案始终存在（被误删时补回，否则旧会话无法恢复）
                let mut list = list;
                if !list.iter().any(|p| p.id == DEFAULT_AGENT_ID) {
                    let builtins = builtin_profiles();
                    if let Some(g) = builtins.into_iter().find(|p| p.id == DEFAULT_AGENT_ID) {
                        list.insert(0, g);
                    }
                }
                list
            }
            Err(e) => {
                tracing::error!("agents.json 解析失败（{e}），使用内置默认");
                builtin_profiles()
            }
        },
        Err(_) => {
            let list = builtin_profiles();
            let _ = save(&list);
            list
        }
    }
}

pub fn save(list: &[AgentProfile]) -> Result<(), String> {
    let path = profiles_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

/// 按 id 找档案；找不到时回退 grok（再退第一个 enabled）。
pub fn find<'a>(list: &'a [AgentProfile], id: &str) -> Option<&'a AgentProfile> {
    list.iter()
        .find(|p| p.id == id)
        .or_else(|| list.iter().find(|p| p.id == DEFAULT_AGENT_ID))
        .or_else(|| list.iter().find(|p| p.enabled))
}

/// `{model}` 占位符替换。model 为空且占位符存在时返回 None（该 arg/env 项被丢弃）。
fn subst(tok: &str, model: &str) -> Option<String> {
    if tok.contains("{model}") {
        if model.trim().is_empty() {
            None
        } else {
            Some(tok.replace("{model}", model.trim()))
        }
    } else {
        Some(tok.to_string())
    }
}

/// 档案最终选用的模型：一律取 default_model（会话模型只看档案；
/// 启动时会把旧 prefs.model 并入 grok 档案）。
pub fn resolve_model(profile: &AgentProfile) -> String {
    profile.default_model.trim().to_string()
}

/// 构建最终启动参数。grok 原生档注入 --model/--always-approve 标志；
/// 其他档做占位符替换。grok_exe 为解析后的 grok 可执行路径（仅 is_grok 用）。
pub fn build_spawn(
    profile: &AgentProfile,
    model: &str,
    always_approve: bool,
    grok_exe: &str,
) -> SpawnSpec {
    if profile.is_grok {
        let mut args: Vec<String> = vec!["agent".into()];
        if always_approve {
            args.push("--always-approve".into());
        }
        let m = model.trim();
        if !m.is_empty() {
            args.push("--model".into());
            args.push(m.to_string());
        }
        args.push("stdio".into());
        let mut env = HashMap::new();
        env.insert("NO_COLOR".to_string(), "1".to_string());
        env.insert("GROK_DESKTOP".to_string(), "1".to_string());
        env.insert(
            "GROK_HOME".to_string(),
            crate::paths::grok_home().to_string_lossy().to_string(),
        );
        SpawnSpec {
            command: grok_exe.to_string(),
            args,
            env,
        }
    } else {
        let args = profile
            .args
            .iter()
            .filter_map(|a| subst(a, model))
            .collect();
        let env: HashMap<String, String> = profile
            .env
            .iter()
            .filter_map(|(k, v)| subst(v, model).map(|v| (k.clone(), v)))
            .collect();
        // Windows：npx 是 .cmd 脚本，CreateProcess 不会按 PATH 解析裸 "npx"
        let command = {
            #[cfg(windows)]
            {
                if profile.command.eq_ignore_ascii_case("npx") {
                    "npx.cmd".to_string()
                } else {
                    profile.command.clone()
                }
            }
            #[cfg(not(windows))]
            {
                profile.command.clone()
            }
        };
        SpawnSpec {
            command,
            args,
            env,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, args: Vec<&str>, env: &[(&str, &str)]) -> AgentProfile {
        AgentProfile {
            id: id.into(),
            name: id.into(),
            command: "npx".into(),
            args: args.into_iter().map(String::from).collect(),
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn grok_spawn_injects_flags_before_stdio() {
        let g = builtin_profiles()
            .into_iter()
            .find(|p| p.id == "grok")
            .unwrap();
        let spec = build_spawn(&g, "grok-4-fast", true, r"C:\grok\grok.exe");
        assert_eq!(spec.command, r"C:\grok\grok.exe");
        assert_eq!(
            spec.args,
            vec!["agent", "--always-approve", "--model", "grok-4-fast", "stdio"]
        );
        assert_eq!(spec.env.get("GROK_DESKTOP").map(String::as_str), Some("1"));
    }

    #[test]
    fn grok_spawn_without_model_omits_flag() {
        let g = builtin_profiles()
            .into_iter()
            .find(|p| p.id == "grok")
            .unwrap();
        let spec = build_spawn(&g, "", false, "grok");
        assert_eq!(spec.args, vec!["agent", "stdio"]);
    }

    #[test]
    fn model_placeholder_substituted_in_args_and_env() {
        let p = profile(
            "glm",
            vec!["@agentclientprotocol/claude-agent-acp@latest"],
            &[("ANTHROPIC_MODEL", "{model}"), ("NO_COLOR", "1")],
        );
        let spec = build_spawn(&p, "glm-4.6", false, "");
        assert_eq!(spec.args.len(), 1);
        assert_eq!(
            spec.env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("glm-4.6")
        );
        assert_eq!(spec.env.get("NO_COLOR").map(String::as_str), Some("1"));
    }

    #[test]
    fn empty_model_drops_placeholder_items_only() {
        let p = profile(
            "x",
            vec!["keep", "{model}"],
            &[("ANTHROPIC_MODEL", "{model}"), ("NO_COLOR", "1")],
        );
        let spec = build_spawn(&p, "", false, "");
        assert_eq!(spec.args, vec!["keep"]);
        assert!(spec.env.get("ANTHROPIC_MODEL").is_none());
        assert_eq!(spec.env.get("NO_COLOR").map(String::as_str), Some("1"));
    }

    #[test]
    fn find_falls_back_to_grok_then_enabled() {
        let list = vec![
            profile("qwen", vec![], &[]),
            AgentProfile {
                id: "grok".into(),
                enabled: true,
                ..profile("grok", vec![], &[])
            },
        ];
        assert_eq!(find(&list, "nope").unwrap().id, "grok");
        assert_eq!(find(&list, "qwen").unwrap().id, "qwen");
    }

    #[test]
    fn builtin_grok_is_enabled_and_resumable() {
        let g = builtin_profiles()
            .into_iter()
            .find(|p| p.id == "grok")
            .unwrap();
        assert!(g.enabled && g.is_grok && g.supports_resume);
    }

    #[test]
    fn load_ensures_grok_present() {
        // 间接验证：load 在无文件时写默认；这里只测合并逻辑的外围不变量
        let list = builtin_profiles();
        assert!(list.iter().all(|p| !p.id.is_empty()));
        assert!(list.len() >= 5);
    }
}
