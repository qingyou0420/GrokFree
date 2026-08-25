//! Workspace filesystem boundary (path sandbox).

use std::path::{Component, Path, PathBuf};

/// Resolve `candidate` against `root`. Rejects `..` escape and absolute paths outside root.
/// Returns canonical (or best-effort absolute) PathBuf inside root.
pub fn resolve_inside(root: &Path, candidate: &str) -> Result<PathBuf, String> {
    let candidate = candidate.trim();
    if candidate.is_empty() {
        return Err("路径为空".into());
    }
    let candidate = strip_file_url(candidate)?;
    if candidate.trim().is_empty() {
        return Err("路径为空".into());
    }

    if !root.exists() {
        return Err(format!("工作区不存在：{}", root.display()));
    }

    let root = canonicalize_best_effort(root)?;
    let cand = Path::new(&candidate);

    let joined = if is_absolute_path(cand) {
        normalize(cand)
    } else {
        join_under_root(&root, cand)?
    };

    if !is_inside(&root, &joined) {
        return Err(format!("路径超出工作区：{}", joined.display()));
    }

    finish_resolve(&root, &joined)
}

/// True if `path` is `root` or a descendant of `root`.
pub fn is_inside(root: &Path, path: &Path) -> bool {
    // Prefer canonical forms so Windows short (8.3) vs long paths compare equal.
    let root_c = std::fs::canonicalize(root)
        .map(|p| strip_verbatim(&p))
        .unwrap_or_else(|_| strip_verbatim(root));
    let path_c = if path.exists() {
        std::fs::canonicalize(path)
            .map(|p| strip_verbatim(&p))
            .unwrap_or_else(|_| strip_verbatim(path))
    } else {
        strip_verbatim(path)
    };
    let root_n = normalize(&root_c);
    let path_n = normalize(&path_c);
    let root_comps: Vec<Component<'_>> = root_n.components().collect();
    let path_comps: Vec<Component<'_>> = path_n.components().collect();
    if path_comps.len() < root_comps.len() {
        return false;
    }
    root_comps
        .iter()
        .zip(path_comps.iter())
        .all(|(a, b)| component_eq(a, b))
}

/// Resolve `candidate` against the first matching existing root.
/// On failure (no root matches), returns `路径超出允许的工作区`.
pub fn resolve_inside_any(roots: &[impl AsRef<Path>], candidate: &str) -> Result<PathBuf, String> {
    for root in roots {
        let root = root.as_ref();
        if !root.exists() {
            continue;
        }
        if let Ok(p) = resolve_inside(root, candidate) {
            return Ok(p);
        }
    }
    Err("路径超出允许的工作区".into())
}

fn finish_resolve(root: &Path, path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if !is_inside(root, &canon) {
            return Err(format!("路径超出工作区：{}", canon.display()));
        }
        return Ok(canon);
    }

    // New file: canonicalize the nearest existing ancestor; do not create directories.
    let mut cur = path.to_path_buf();
    let mut missing: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if cur.exists() {
            let canon = std::fs::canonicalize(&cur).unwrap_or_else(|_| abs_best_effort(&cur));
            if !is_inside(root, &canon) {
                return Err(format!("路径超出工作区：{}", path.display()));
            }
            let mut result = canon;
            for part in missing.iter().rev() {
                result.push(part);
            }
            if !is_inside(root, &result) {
                return Err(format!("路径超出工作区：{}", result.display()));
            }
            return Ok(result);
        }

        if !is_inside(root, &cur) && cur != root {
            return Err(format!("路径超出工作区：{}", path.display()));
        }

        match cur.file_name() {
            Some(name) => {
                missing.push(name.to_os_string());
                if !cur.pop() {
                    return Err(format!("路径超出工作区：{}", path.display()));
                }
            }
            None => return Err(format!("路径超出工作区：{}", path.display())),
        }
        if missing.len() > 64 {
            return Err(format!("路径超出工作区：{}", path.display()));
        }
    }
}

fn join_under_root(root: &Path, candidate: &Path) -> Result<PathBuf, String> {
    let mut out = root.to_path_buf();
    for c in candidate.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                let mut next = out.clone();
                next.pop();
                if !is_inside(root, &next) {
                    return Err(format!("路径超出工作区：{}", candidate.display()));
                }
                out.pop();
            }
            Component::Normal(s) => out.push(s),
            Component::Prefix(_) | Component::RootDir => {
                return Err(format!("路径超出工作区：{}", candidate.display()));
            }
        }
    }
    Ok(out)
}

fn canonicalize_best_effort(p: &Path) -> Result<PathBuf, String> {
    if let Ok(c) = std::fs::canonicalize(p) {
        return Ok(c);
    }
    Ok(abs_best_effort(p))
}

fn abs_best_effort(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(p))
            .unwrap_or_else(|_| p.to_path_buf())
    }
}

fn is_absolute_path(p: &Path) -> bool {
    p.is_absolute()
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => out.push(c.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            Component::Normal(s) => out.push(s),
        }
    }
    out
}

fn strip_verbatim(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if let Some(unc) = rest.strip_prefix(r"UNC\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        return PathBuf::from(rest);
    }
    if let Some(rest) = s.strip_prefix("//?/") {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

fn component_eq(a: &Component<'_>, b: &Component<'_>) -> bool {
    match (a, b) {
        (Component::Normal(x), Component::Normal(y)) => {
            if cfg!(windows) {
                x.to_string_lossy()
                    .eq_ignore_ascii_case(&y.to_string_lossy())
            } else {
                x == y
            }
        }
        (Component::Prefix(x), Component::Prefix(y)) => {
            x.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&y.as_os_str().to_string_lossy())
        }
        _ => a == b,
    }
}

/// Strip `file://` when the remainder is a usable path; reject other `file:` forms.
fn strip_file_url(s: &str) -> Result<String, String> {
    if s.len() >= 7 && s[..7].eq_ignore_ascii_case("file://") {
        let mut rest = &s[7..];
        if rest.len() >= 9 && rest[..9].eq_ignore_ascii_case("localhost") {
            rest = &rest[9..];
        }
        let bytes = rest.as_bytes();
        // file:///C:/Users → C:/Users
        if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':'
        {
            rest = &rest[1..];
        }
        let decoded = percent_decode(rest);
        if decoded.trim().is_empty() {
            return Err("路径为空".into());
        }
        Ok(decoded)
    } else if s.len() >= 5 && s[..5].eq_ignore_ascii_case("file:") {
        Err(format!("路径超出工作区：{s}"))
    } else {
        Ok(s.to_string())
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TmpRoot(PathBuf);

    impl TmpRoot {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "gbd-ws-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }

    impl Drop for TmpRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn relative_path_inside() {
        let root = TmpRoot::new();
        let p = resolve_inside(&root.0, "foo/bar.txt").expect("inside");
        assert!(is_inside(&root.0, &p));
        assert!(p.ends_with("bar.txt"));
    }

    #[test]
    fn parent_dir_escape() {
        let root = TmpRoot::new();
        assert!(resolve_inside(&root.0, "../secret").is_err());
        assert!(resolve_inside(&root.0, "foo/../../secret").is_err());
        assert!(resolve_inside(&root.0, "..").is_err());
    }

    #[test]
    fn absolute_path_outside() {
        let root = TmpRoot::new();
        #[cfg(windows)]
        let outside = r"C:\Windows\System32\drivers\etc\hosts";
        #[cfg(not(windows))]
        let outside = "/etc/passwd";
        let err = resolve_inside(&root.0, outside).unwrap_err();
        assert!(
            err.contains("路径超出工作区") || err.contains("工作区不存在"),
            "unexpected error: {err}"
        );
        assert!(!is_inside(&root.0, Path::new(outside)));
    }

    #[test]
    fn nested_file_inside() {
        let root = TmpRoot::new();
        let nested = root.0.join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("c.txt"), "hi").unwrap();
        let p = resolve_inside(&root.0, "a/b/c.txt").expect("nested");
        assert!(is_inside(&root.0, &p));
        assert!(p.ends_with("c.txt"));
        let content = fs::read_to_string(&p).unwrap();
        assert_eq!(content, "hi");
    }

    #[test]
    fn empty_and_file_url() {
        let root = TmpRoot::new();
        assert!(resolve_inside(&root.0, "").is_err());
        assert!(resolve_inside(&root.0, "   ").is_err());
        // file:// to a path inside the root should work after stripping
        let inner = root.0.join("x.txt");
        fs::write(&inner, "ok").unwrap();
        let url = format!("file://{}", inner.display());
        let p = resolve_inside(&root.0, &url).expect("file url");
        assert!(is_inside(&root.0, &p));
    }

    #[test]
    fn is_inside_prefix_not_enough() {
        let a = Path::new("/tmp/ws");
        let b = Path::new("/tmp/ws-evil/file");
        assert!(!is_inside(a, b));
        assert!(is_inside(a, Path::new("/tmp/ws")));
        assert!(is_inside(a, Path::new("/tmp/ws/file")));
    }
}
