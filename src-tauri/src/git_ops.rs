//! Git status hints for Review pane (v0.2)
//!
//! IMPORTANT (Windows): every `git` spawn must go through `silent_command`.
//! Sidebar project switches refresh this status; without CREATE_NO_WINDOW the
//! user sees a console window flash open and close.

use crate::process_util::silent_command;
use serde::Serialize;
use std::path::Path;
use std::process::Stdio;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub is_repo: bool,
    pub branch: Option<String>,
    /// Commits ahead of upstream (from `## branch...upstream [ahead N, behind M]`).
    pub ahead: Option<u32>,
    /// Commits behind upstream.
    pub behind: Option<u32>,
    pub dirty: bool,
    pub entries: Vec<GitEntry>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitEntry {
    pub path: String,
    pub status: String, // M | A | D | ?? | R | ...
}

pub fn git_status(cwd: &str) -> GitStatus {
    let path = Path::new(cwd);
    if !path.is_dir() {
        return GitStatus {
            is_repo: false,
            branch: None,
            ahead: None,
            behind: None,
            dirty: false,
            entries: vec![],
            message: format!("目录不存在：{cwd}"),
        };
    }

    // Single process: branch header (`##`) + porcelain entries.
    // Avoids two console-spawn attempts on Windows.
    let output = match run_git(
        path,
        &["status", "--porcelain", "-b", "-uall"],
    ) {
        Ok(s) => s,
        Err(e) => {
            return GitStatus {
                is_repo: false,
                branch: None,
                ahead: None,
                behind: None,
                dirty: false,
                entries: vec![],
                message: e,
            };
        }
    };

    let mut branch: Option<String> = None;
    let mut ahead: Option<u32> = None;
    let mut behind: Option<u32> = None;
    let mut entries = Vec::new();

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            // Formats:
            //   ## main
            //   ## main...origin/main [ahead 1]
            //   ## main...origin/main [ahead 1, behind 2]
            //   ## HEAD (no branch)
            let name = rest
                .split(['.', ' ', '\t'])
                .next()
                .unwrap_or(rest)
                .trim();
            if !name.is_empty() {
                branch = Some(name.to_string());
            }
            ahead = parse_bracket_count(rest, "ahead");
            behind = parse_bracket_count(rest, "behind");
            continue;
        }
        if line.len() < 4 {
            continue;
        }
        let st = line[..2].trim().to_string();
        let file = line[3..].trim().to_string();
        if file.is_empty() {
            continue;
        }
        // handle "R old -> new"
        let path = if let Some((_, new)) = file.split_once(" -> ") {
            new.to_string()
        } else {
            file
        };
        entries.push(GitEntry {
            path,
            status: if st.is_empty() { "M".into() } else { st },
        });
    }

    let dirty = !entries.is_empty();
    let entry_count = entries.len();
    let mut message = if dirty {
        format!("{entry_count} 个文件有变更")
    } else {
        "工作区干净".into()
    };
    match (ahead, behind) {
        (Some(a), Some(b)) if a > 0 || b > 0 => {
            message = format!("{message} · ↑{a} ↓{b}");
        }
        (Some(a), _) if a > 0 => {
            message = format!("{message} · ↑{a}");
        }
        (_, Some(b)) if b > 0 => {
            message = format!("{message} · ↓{b}");
        }
        _ => {}
    }
    GitStatus {
        is_repo: true,
        branch,
        ahead,
        behind,
        dirty,
        entries,
        message,
    }
}

/// Parse `ahead N` / `behind N` from a git porcelain branch header.
fn parse_bracket_count(header: &str, key: &str) -> Option<u32> {
    let needle = format!("{key} ");
    let idx = header.find(&needle)?;
    let rest = &header[idx + needle.len()..];
    let num: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if num.is_empty() {
        None
    } else {
        num.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ahead_behind() {
        assert_eq!(
            parse_bracket_count("main...origin/main [ahead 2, behind 1]", "ahead"),
            Some(2)
        );
        assert_eq!(
            parse_bracket_count("main...origin/main [ahead 2, behind 1]", "behind"),
            Some(1)
        );
        assert_eq!(
            parse_bracket_count("main...origin/main [ahead 3]", "ahead"),
            Some(3)
        );
        assert_eq!(parse_bracket_count("main", "ahead"), None);
    }
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let out = silent_command("git")
        // Raw UTF-8 paths instead of octal escapes for CJK filenames
        .args(["-c", "core.quotepath=false"])
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("无法运行 git：{e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(if err.trim().is_empty() {
            "不是 git 仓库或 git 命令失败".into()
        } else {
            err.trim().to_string()
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
