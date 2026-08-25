//! CLI capability probe + Skills/MCP readonly scan (v0.3)

use crate::paths;
use serde::Serialize;
use std::fs;
use std::path::Path;

/// What Desktop may honestly expose in Settings / spawn paths.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliCapabilities {
    /// `grok agent` accepts `--sandbox` (currently false — global flag only).
    pub agent_sandbox_flag: bool,
    /// Desktop can pass permission mode via spawn / session meta.
    pub permission_modes: bool,
    /// Desktop can pass model override on spawn.
    pub model_override: bool,
    /// ACP terminal host is implemented in-process.
    pub terminal_host: bool,
    /// Session resume via session/load.
    pub session_resume: bool,
    /// Skills/MCP are loaded by the agent from ~/.grok; Desktop only lists them.
    pub skills_mcp_readonly: bool,
    /// Notes for UI help text.
    pub notes: Vec<String>,
}

impl Default for CliCapabilities {
    fn default() -> Self {
        Self {
            // Documented constraint: `grok agent` rejects `--sandbox`.
            agent_sandbox_flag: false,
            permission_modes: true,
            model_override: true,
            terminal_host: true,
            session_resume: true,
            skills_mcp_readonly: true,
            notes: vec![
                "沙箱请在 CLI 全局配置或 `grok --sandbox …` 中设置；Desktop 的沙箱选项仅作说明。".into(),
                "Skills / MCP 由 agent 从 ~/.grok 加载；此处为只读列表。".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    pub name: String,
    pub path: String,
    pub source: String, // "skills" | "bundled" | "config"
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInfo {
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsMcpSnapshot {
    pub skills: Vec<SkillInfo>,
    pub mcp_servers: Vec<McpServerInfo>,
    pub config_path: String,
    pub skills_dir: String,
    pub notes: Vec<String>,
}

/// Semver-ish compare: returns true if `installed >= required`.
/// Accepts strings like "0.2.1", "grok 0.2.1", "v0.3.0-beta".
pub fn version_meets_min(installed: Option<&str>, min_required: &str) -> bool {
    let Some(inst) = installed else {
        return false;
    };
    let Some(a) = parse_semver(inst) else {
        // Unknown format — don't block the user.
        return true;
    };
    let Some(b) = parse_semver(min_required) else {
        return true;
    };
    a >= b
}

/// Extract (major, minor, patch) from a free-form version string.
pub fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let mut digits = String::new();
    let mut found_digit = false;
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
            found_digit = true;
        } else if found_digit && (ch == '.' || ch == '-') {
            if ch == '.' {
                digits.push('.');
            } else {
                break; // stop at pre-release separator after digits started
            }
        } else if found_digit {
            break;
        }
    }
    if digits.is_empty() {
        return None;
    }
    // trim trailing dots
    let digits = digits.trim_matches('.');
    let mut parts = digits.split('.').filter(|p| !p.is_empty());
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let patch: u64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

pub fn capabilities() -> CliCapabilities {
    CliCapabilities::default()
}

/// Scan ~/.grok for skills directories and MCP entries in config.toml (lightweight TOML scan).
pub fn list_skills_and_mcp() -> SkillsMcpSnapshot {
    let config_path = paths::grok_config_toml();
    let skills_dir = paths::grok_home().join("skills");
    let mut skills: Vec<SkillInfo> = Vec::new();
    let mut mcp_servers: Vec<McpServerInfo> = Vec::new();
    let mut notes = Vec::new();

    // Directory skills: ~/.grok/skills/<name>/
    collect_skill_dirs(&skills_dir, "skills", &mut skills);
    // Common alternate layouts
    collect_skill_dirs(&paths::grok_home().join("skill"), "skills", &mut skills);
    collect_skill_dirs(
        &paths::grok_home().join("plugins").join("skills"),
        "plugins",
        &mut skills,
    );

    // Parse config.toml with a tiny line scanner (avoid heavy toml dep for readonly UI).
    if config_path.is_file() {
        match fs::read_to_string(&config_path) {
            Ok(text) => {
                parse_config_skills_mcp(&text, &mut skills, &mut mcp_servers);
            }
            Err(e) => notes.push(format!("无法读取 config.toml：{e}")),
        }
    } else {
        notes.push("尚未找到 config.toml（将在首次 CLI 使用时创建）".into());
    }

    // Dedupe skills by name
    skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    skills.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));

    if skills.is_empty() {
        notes.push("未发现已安装 Skills。可通过 CLI 或编辑 config.toml 添加。".into());
    }
    if mcp_servers.is_empty() {
        notes.push("未在 config.toml 中发现 MCP 服务器配置。".into());
    }

    SkillsMcpSnapshot {
        skills,
        mcp_servers,
        config_path: config_path.display().to_string(),
        skills_dir: skills_dir.display().to_string(),
        notes,
    }
}

fn collect_skill_dirs(dir: &Path, source: &str, out: &mut Vec<SkillInfo>) {
    if !dir.is_dir() {
        return;
    }
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            // Prefer folders that look like skills (SKILL.md or any content)
            let has_marker = path.join("SKILL.md").is_file()
                || path.join("skill.md").is_file()
                || path.join("package.json").is_file()
                || true;
            if has_marker {
                out.push(SkillInfo {
                    name,
                    path: path.display().to_string(),
                    source: source.into(),
                });
            }
        }
    }
}

/// Minimal TOML section scanner for [[mcp]] / [mcp.servers.NAME] / skills = [...]
fn parse_config_skills_mcp(
    text: &str,
    skills: &mut Vec<SkillInfo>,
    mcp: &mut Vec<McpServerInfo>,
) {
    let mut section = String::new();
    let mut current_mcp_name: Option<String> = None;
    let mut current_mcp_detail = String::new();

    let flush_mcp = |name: &Option<String>, detail: &str, mcp: &mut Vec<McpServerInfo>| {
        if let Some(n) = name {
            if !n.is_empty() {
                mcp.push(McpServerInfo {
                    name: n.clone(),
                    detail: detail.trim().to_string(),
                });
            }
        }
    };

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            flush_mcp(&current_mcp_name, &current_mcp_detail, mcp);
            current_mcp_name = None;
            current_mcp_detail.clear();
            section = line.to_string();

            // [mcp.servers.foo] or [mcp_servers.foo]
            if let Some(rest) = line
                .strip_prefix("[mcp.servers.")
                .or_else(|| line.strip_prefix("[mcp_servers."))
            {
                let name = rest.trim_end_matches(']').trim().to_string();
                if !name.is_empty() {
                    current_mcp_name = Some(name);
                }
            }
            // [[mcp.servers]] table array — name comes from next `name =`
            if line == "[[mcp.servers]]" || line == "[[mcp]]" || line == "[[mcp_servers]]" {
                current_mcp_name = Some(String::new());
            }
            continue;
        }

        // skills = ["a", "b"] or skills = [{ name = "x" }]
        if line.starts_with("skills") && line.contains('=') {
            if let Some(rhs) = line.split_once('=').map(|(_, r)| r.trim()) {
                for name in extract_quoted_strings(rhs) {
                    if !skills.iter().any(|s| s.name == name) {
                        skills.push(SkillInfo {
                            name: name.clone(),
                            path: format!("config:{name}"),
                            source: "config".into(),
                        });
                    }
                }
            }
        }

        // Inside an MCP section
        if section.contains("mcp") {
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim();
                let val = v.trim().trim_matches('"').trim_matches('\'').to_string();
                if key == "name" || key == "id" {
                    if current_mcp_name.as_deref() == Some("") || current_mcp_name.is_none() {
                        current_mcp_name = Some(val.clone());
                    }
                }
                if !current_mcp_detail.is_empty() {
                    current_mcp_detail.push_str("; ");
                }
                current_mcp_detail.push_str(&format!("{key}={val}"));
            }
        }
    }
    flush_mcp(&current_mcp_name, &current_mcp_detail, mcp);
}

fn extract_quoted_strings(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' || c == '\'' {
            let quote = c;
            let mut buf = String::new();
            while let Some(ch) = chars.next() {
                if ch == quote {
                    break;
                }
                if ch == '\\' {
                    if let Some(n) = chars.next() {
                        buf.push(n);
                    }
                } else {
                    buf.push(ch);
                }
            }
            if !buf.is_empty() {
                out.push(buf);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_basic() {
        assert_eq!(parse_semver("0.2.1"), Some((0, 2, 1)));
        assert_eq!(parse_semver("v0.3.0"), Some((0, 3, 0)));
        assert_eq!(parse_semver("grok 1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("0.2"), Some((0, 2, 0)));
    }

    #[test]
    fn version_gate() {
        assert!(version_meets_min(Some("0.2.1"), "0.2.0"));
        assert!(version_meets_min(Some("0.3.0"), "0.2.1"));
        assert!(!version_meets_min(Some("0.1.9"), "0.2.0"));
        assert!(!version_meets_min(None, "0.2.0"));
        assert!(version_meets_min(Some("weird-build"), "0.2.0")); // unknown → allow
    }

    #[test]
    fn parse_mcp_section() {
        let toml = r#"
[mcp.servers.filesystem]
command = "npx"
args = ["-y", "server"]

[[mcp.servers]]
name = "browser"
command = "uvx"
"#;
        let mut skills = vec![];
        let mut mcp = vec![];
        parse_config_skills_mcp(toml, &mut skills, &mut mcp);
        assert!(mcp.iter().any(|m| m.name == "filesystem"));
        assert!(mcp.iter().any(|m| m.name == "browser"));
    }

    #[test]
    fn parse_skills_array() {
        let toml = r#"
skills = ["code-review", "docs"]
"#;
        let mut skills = vec![];
        let mut mcp = vec![];
        parse_config_skills_mcp(toml, &mut skills, &mut mcp);
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "code-review");
    }
}
