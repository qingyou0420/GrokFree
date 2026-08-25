//! Apply / reject unified diffs for Review pane (design D11 / v0.2)

use crate::process_util::silent_command;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyDiffResult {
    pub ok: bool,
    pub path: String,
    pub method: String, // "git_apply" | "patch" | "write"
    pub message: String,
}

/// Apply a unified diff for a single file relative to `cwd`.
pub fn apply_diff(cwd: &str, file_path: &str, patch: &str) -> Result<ApplyDiffResult, String> {
    let cwd_path = PathBuf::from(cwd);
    if !cwd_path.is_dir() {
        return Err(format!("工作目录不存在：{cwd}"));
    }

    let abs = crate::workspace::resolve_inside(&cwd_path, file_path)?;

    // 1) Prefer git apply when repo
    if is_git_repo(&cwd_path) {
        match git_apply(&cwd_path, patch) {
            Ok(()) => {
                return Ok(ApplyDiffResult {
                    ok: true,
                    path: abs.display().to_string(),
                    method: "git_apply".into(),
                    message: "已通过 git apply 应用变更".into(),
                });
            }
            Err(e) => {
                tracing::warn!("git apply failed: {e}; falling back to manual patch");
            }
        }
    }

    // 2) Manual unified-diff application for single file
    match apply_unified_patch(&abs, patch) {
        Ok(()) => Ok(ApplyDiffResult {
            ok: true,
            path: abs.display().to_string(),
            method: "patch".into(),
            message: "已应用 unified diff".into(),
        }),
        Err(e) => Err(e),
    }
}

/// Reject: no-op on disk; returns path for UI bookkeeping.
pub fn reject_diff(file_path: &str) -> ApplyDiffResult {
    ApplyDiffResult {
        ok: true,
        path: file_path.to_string(),
        method: "reject".into(),
        message: "已忽略此变更（未写入磁盘）".into(),
    }
}

fn is_git_repo(cwd: &Path) -> bool {
    cwd.join(".git").exists()
        || silent_command("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(cwd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

fn git_apply(cwd: &Path, patch: &str) -> Result<(), String> {
    let tmp = std::env::temp_dir().join(format!(
        "grok-desktop-diff-{}.patch",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    fs::write(&tmp, patch).map_err(|e| e.to_string())?;
    let out = silent_command("git")
        .args(["apply", "--whitespace=nowarn"])
        .arg(&tmp)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("无法运行 git：{e}"))?;
    let _ = fs::remove_file(&tmp);
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        Err(format!("git apply 失败：{stderr}{stdout}"))
    }
}

struct HunkOp {
    op: char,
    text: String,
}

struct Hunk {
    old_start: i64,
    ops: Vec<HunkOp>,
}

/// Does the old side of `hunk` (context + deletions) match `lines` at `start`?
/// `loose` compares with trailing whitespace trimmed, as a fallback for noisy diffs.
fn hunk_matches_at(lines: &[String], start: usize, hunk: &Hunk, loose: bool) -> bool {
    let mut idx = start;
    for op in &hunk.ops {
        match op.op {
            ' ' | '-' => {
                let Some(cur) = lines.get(idx) else {
                    return false;
                };
                let ok = if loose {
                    cur.trim_end() == op.text.trim_end()
                } else {
                    cur == &op.text
                };
                if !ok {
                    return false;
                }
                idx += 1;
            }
            _ => {}
        }
    }
    true
}

/// Very small unified-diff applier for single-file patches (hunks with @@).
///
/// Safety rule: a hunk whose old side (context + deleted lines) cannot be
/// located in the file fails loudly instead of being written at the stated
/// line number — a mismatched patch must never silently corrupt the file.
fn apply_unified_patch(target: &Path, patch: &str) -> Result<(), String> {
    let original = if target.exists() {
        fs::read_to_string(target).map_err(|e| format!("读取 {target:?} 失败：{e}"))?
    } else {
        String::new()
    };

    let mut lines: Vec<String> = if original.is_empty() {
        Vec::new()
    } else {
        // Preserve trailing newline semantics loosely
        original.lines().map(|s| s.to_string()).collect()
    };

    let patch_lines: Vec<&str> = patch.lines().collect();
    let mut hunks: Vec<Hunk> = Vec::new();

    let mut i = 0usize;
    while i < patch_lines.len() {
        let line = patch_lines[i];
        if line.starts_with("@@") {
            let (old_start, _old_count, _new_start, _new_count) = parse_hunk_header(line)?;
            i += 1;
            let mut ops: Vec<HunkOp> = Vec::new();
            while i < patch_lines.len() {
                let l = patch_lines[i];
                if l.starts_with("@@") || l.starts_with("diff ") || l.starts_with("---") {
                    break;
                }
                if l.starts_with("+++") || l.starts_with("index ") || l.starts_with("new file") {
                    i += 1;
                    continue;
                }
                if l.is_empty() {
                    ops.push(HunkOp {
                        op: ' ',
                        text: String::new(),
                    });
                    i += 1;
                    continue;
                }
                let ch = l.chars().next().unwrap_or(' ');
                let rest: String = l.chars().skip(1).collect();
                match ch {
                    ' ' | '+' | '-' | '\\' => {
                        if ch != '\\' {
                            ops.push(HunkOp { op: ch, text: rest });
                        }
                        i += 1;
                    }
                    _ => break,
                }
            }
            hunks.push(Hunk { old_start, ops });
        } else {
            i += 1;
        }
    }

    if hunks.is_empty() {
        // No hunks: only ever accept as full content of a brand-new file.
        let plus: Vec<String> = patch
            .lines()
            .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
            .map(|l| l.chars().skip(1).collect())
            .collect();
        if plus.is_empty() {
            return Err("无法解析 diff：未找到 hunk".into());
        }
        if !lines.is_empty() {
            return Err(
                "补丁不含 hunk 但目标文件已有内容；为避免整文件覆盖已拒绝应用".into(),
            );
        }
        lines = plus;
    } else {
        // Hunks must apply against the file in order; track how far we consumed
        let mut search_from = 0usize;
        for hunk in &hunks {
            let old_len = hunk.ops.iter().filter(|o| matches!(o.op, ' ' | '-')).count();
            // old_start is 1-based; 0 means "no old lines" (new file)
            let preferred = if hunk.old_start <= 0 {
                0
            } else {
                (hunk.old_start as usize).saturating_sub(1)
            };

            let mut pos: Option<usize> = None;
            if preferred <= lines.len() {
                if hunk_matches_at(&lines, preferred, hunk, false)
                    || hunk_matches_at(&lines, preferred, hunk, true)
                {
                    pos = Some(preferred);
                }
            }
            if pos.is_none() {
                // Line numbers drift when earlier hunks already applied or the
                // diff was taken against a slightly different base: locate the
                // old-side sequence by content, from where the last hunk ended.
                let scan_start = search_from;
                let scan_end = lines.len().saturating_sub(old_len);
                for cand in scan_start..=scan_end {
                    if hunk_matches_at(&lines, cand, hunk, false)
                        || hunk_matches_at(&lines, cand, hunk, true)
                    {
                        pos = Some(cand);
                        break;
                    }
                }
            }
            let Some(pos) = pos else {
                let first_ctx = hunk
                    .ops
                    .iter()
                    .find(|o| matches!(o.op, ' ' | '-'))
                    .map(|o| o.text.as_str())
                    .unwrap_or("");
                return Err(format!(
                    "补丁上下文不匹配（hunk 声明第 {} 行，期望「{}」），已拒绝写入以免损坏文件",
                    hunk.old_start.max(1),
                    first_ctx.chars().take(48).collect::<String>()
                ));
            };

            let mut out_segment: Vec<String> = Vec::new();
            let mut src_idx = pos;
            for op in &hunk.ops {
                match op.op {
                    ' ' => {
                        // context — keep the original line as-is
                        if src_idx < lines.len() {
                            out_segment.push(lines[src_idx].clone());
                            src_idx += 1;
                        } else {
                            return Err("补丁上下文超出文件末尾，已拒绝应用".into());
                        }
                    }
                    '-' => {
                        if src_idx < lines.len() {
                            src_idx += 1;
                        } else {
                            return Err("删除行超出文件末尾，已拒绝应用".into());
                        }
                    }
                    '+' => out_segment.push(op.text.clone()),
                    _ => {}
                }
            }
            let end = src_idx.min(lines.len());
            lines.splice(pos..end, out_segment.clone());
            search_from = pos + out_segment.len();
        }
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut content = lines.join("\n");
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    fs::write(target, content).map_err(|e| format!("写入失败：{e}"))?;
    Ok(())
}

fn parse_hunk_header(line: &str) -> Result<(i64, i64, i64, i64), String> {
    // @@ -l,s +l,s @@
    let rest = line.trim_start_matches("@@").trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(format!("非法 hunk 头：{line}"));
    }
    let (o_start, o_count) = parse_range(parts[0].trim_start_matches('-'))?;
    let (n_start, n_count) = parse_range(parts[1].trim_start_matches('+'))?;
    Ok((o_start, o_count, n_start, n_count))
}

fn parse_range(s: &str) -> Result<(i64, i64), String> {
    if let Some((a, b)) = s.split_once(',') {
        Ok((
            a.parse().map_err(|_| format!("range {s}"))?,
            b.parse().map_err(|_| format!("range {s}"))?,
        ))
    } else {
        let n: i64 = s.parse().map_err(|_| format!("range {s}"))?;
        Ok((n, 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, content: Option<&str>) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "grok-diff-test-{}-{name}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("target.txt");
        if let Some(c) = content {
            fs::write(&p, c).unwrap();
        }
        p
    }

    fn read(p: &Path) -> String {
        fs::read_to_string(p).unwrap()
    }

    #[test]
    fn simple_modification() {
        let p = temp_file("simple", Some("a\nb\nc\n"));
        let patch = "--- a/t.txt\n+++ b/t.txt\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n";
        apply_unified_patch(&p, patch).unwrap();
        assert_eq!(read(&p), "a\nB\nc\n");
    }

    #[test]
    fn cjk_lines_apply_and_survive_multibyte_leading_char() {
        let p = temp_file("cjk", Some("第一行\n第二行\n第三行\n"));
        let patch = "@@ -1,3 +1,3 @@\n 第一行\n-第二行\n+改成这样\n 第三行\n中文打头的行不是合法 diff 正文，解析器应跳过而非 panic\n";
        apply_unified_patch(&p, patch).unwrap();
        assert_eq!(read(&p), "第一行\n改成这样\n第三行\n");
    }

    #[test]
    fn context_mismatch_is_rejected_and_file_untouched() {
        let p = temp_file("mismatch", Some("keep me\n"));
        let patch = "@@ -1,1 +1,1 @@\n-not this line\n+new\n";
        let err = apply_unified_patch(&p, patch).unwrap_err();
        assert!(err.contains("上下文不匹配"), "unexpected err: {err}");
        assert_eq!(read(&p), "keep me\n");
    }

    #[test]
    fn new_file_hunk_creates_file() {
        let p = temp_file("newfile", None);
        let patch = "@@ -0,0 +1,2 @@\n+hello\n+world\n";
        apply_unified_patch(&p, patch).unwrap();
        assert_eq!(read(&p), "hello\nworld\n");
    }

    #[test]
    fn no_hunk_patch_refused_on_existing_content() {
        let p = temp_file("refuse-overwrite", Some("precious data\n"));
        let err = apply_unified_patch(&p, "+evil\n+replacement\n").unwrap_err();
        assert!(err.contains("拒绝"), "unexpected err: {err}");
        assert_eq!(read(&p), "precious data\n");
    }

    #[test]
    fn no_hunk_patch_allowed_for_missing_file() {
        let p = temp_file("allow-new", None);
        apply_unified_patch(&p, "+brand\n+new file\n").unwrap();
        assert_eq!(read(&p), "brand\nnew file\n");
    }

    #[test]
    fn drifted_line_numbers_still_apply_by_content() {
        let p = temp_file("drift", Some("l1\nl2\nl3\nl4\nl5\n"));
        // Header says line 1 but the content actually lives at line 4
        let patch = "@@ -1,2 +1,2 @@\n-l4\n+L4\n l5\n";
        apply_unified_patch(&p, patch).unwrap();
        assert_eq!(read(&p), "l1\nl2\nl3\nL4\nl5\n");
    }

    #[test]
    fn multiple_hunks_in_order() {
        let p = temp_file("multi", Some("a\nb\nc\nd\ne\nf\n"));
        let patch = "@@ -1,2 +1,2 @@\n-a\n+A\n b\n@@ -5,2 +5,2 @@\n-e\n+E\n f\n";
        apply_unified_patch(&p, patch).unwrap();
        assert_eq!(read(&p), "A\nb\nc\nd\nE\nf\n");
    }

    #[test]
    fn trailing_whitespace_noise_matches_loosely() {
        let p = temp_file("loose", Some("ctx   \n"));
        let patch = "@@ -1,1 +1,2 @@\n-ctx\n+ctx\n+added\n";
        apply_unified_patch(&p, patch).unwrap();
        assert_eq!(read(&p), "ctx\nadded\n");
    }
}
