//! Path resolution for ~/.grok and Desktop LocalAppData (design §11.2)

use std::path::{Path, PathBuf};

pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn grok_exe_name() -> &'static str {
    if cfg!(windows) {
        "grok.exe"
    } else {
        "grok"
    }
}

/// A grok home is usable when it actually has the CLI binary or credentials.
/// An empty first-run stub (config.toml only) is *not* enough — after a
/// relocate, a stale `GROK_HOME` often points at just such a leftover.
pub fn grok_home_looks_usable(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    dir.join("bin").join(grok_exe_name()).is_file() || dir.join("auth.json").is_file()
}

/// `…/bin/grok.exe` → `…`  (the grok home). Anything else → None.
pub fn home_from_grok_exe(exe: &Path) -> Option<PathBuf> {
    let parent = exe.parent()?;
    if parent
        .file_name()
        .map(|n| n == "bin")
        .unwrap_or(false)
    {
        parent.parent().map(|p| p.to_path_buf())
    } else {
        None
    }
}

/// Pick a usable grok home.
///
/// Order: `GROK_HOME` if usable → override exe's parent home if usable →
/// `%USERPROFILE%\.grok` if usable → first existing candidate → default.
/// A leftover empty `GROK_HOME` after moving the real install must not win.
pub fn resolve_grok_home(override_exe: Option<&str>) -> PathBuf {
    let default = home_dir().join(".grok");
    let from_env = std::env::var("GROK_HOME")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let from_override = override_exe
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(Path::new)
        .and_then(home_from_grok_exe);

    let mut candidates: Vec<PathBuf> = Vec::new();
    for p in [from_env, from_override, Some(default.clone())]
        .into_iter()
        .flatten()
    {
        if !candidates.iter().any(|c| c == &p) {
            candidates.push(p);
        }
    }

    if let Some(p) = candidates.iter().find(|p| grok_home_looks_usable(p)) {
        return p.clone();
    }
    candidates
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or(default)
}

/// `%USERPROFILE%\.grok`, or a usable `GROK_HOME` / override-derived home.
pub fn grok_home() -> PathBuf {
    resolve_grok_home(None)
}

/// Pin the resolved home on this process so child `grok` inherits it.
/// Safe to call at startup and again after the user changes `grokPath`.
pub fn apply_resolved_grok_home(override_exe: Option<&str>) -> PathBuf {
    let home = resolve_grok_home(override_exe);
    std::env::set_var("GROK_HOME", &home);
    home
}

pub fn grok_bin() -> PathBuf {
    grok_home().join("bin").join(grok_exe_name())
}

pub fn grok_config_toml() -> PathBuf {
    grok_home().join("config.toml")
}

pub fn grok_auth_json() -> PathBuf {
    grok_home().join("auth.json")
}

pub fn grok_sessions_dir() -> PathBuf {
    grok_home().join("sessions")
}

/// `%LOCALAPPDATA%\GrokFree`（若尚不存在则从旧目录 GrokBuild 迁移）
pub fn desktop_data_dir() -> PathBuf {
    let local = dirs::data_local_dir()
        .unwrap_or_else(|| home_dir().join("AppData").join("Local"));
    let neu = local.join("GrokFree");
    if neu.exists() {
        return neu;
    }
    let legacy = local.join("GrokBuild");
    if legacy.exists() {
        match std::fs::rename(&legacy, &neu) {
            Ok(()) => {
                tracing::info!(
                    "migrated desktop data {} → {}",
                    legacy.display(),
                    neu.display()
                );
                return neu;
            }
            Err(e) => {
                tracing::warn!(
                    "could not migrate {} → {} ({e}); using legacy path",
                    legacy.display(),
                    neu.display()
                );
                return legacy;
            }
        }
    }
    neu
}

pub fn desktop_logs_dir() -> PathBuf {
    desktop_data_dir().join("logs")
}

pub fn desktop_state_path() -> PathBuf {
    desktop_data_dir().join("desktop-state.json")
}

pub fn desktop_session_map_path() -> PathBuf {
    desktop_data_dir().join("desktop-session-map.json")
}

pub fn ensure_desktop_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(desktop_data_dir())?;
    std::fs::create_dir_all(desktop_logs_dir())?;
    Ok(())
}

/// Resolve grok executable: settings override → GROK_PATH → ~/.grok/bin → PATH "grok"
/// Missing override / GROK_PATH entries are skipped instead of blocking fallback.
pub fn resolve_grok_executable(override_path: Option<&str>) -> PathBuf {
    if let Some(p) = override_path {
        let t = p.trim();
        if !t.is_empty() {
            let pb = PathBuf::from(t);
            if pb.exists() {
                return pb;
            }
        }
    }
    if let Ok(p) = std::env::var("GROK_PATH") {
        let t = p.trim();
        if !t.is_empty() {
            let pb = PathBuf::from(t);
            if pb.exists() {
                return pb;
            }
        }
    }
    let candidate = grok_bin();
    if candidate.exists() {
        return candidate;
    }
    PathBuf::from(grok_exe_name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn unique_temp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gbd-paths-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn home_from_exe_requires_bin_parent() {
        assert_eq!(
            home_from_grok_exe(Path::new(r"D:\data\.grok\bin\grok.exe")),
            Some(PathBuf::from(r"D:\data\.grok"))
        );
        assert_eq!(
            home_from_grok_exe(Path::new(r"C:\tools\grok.exe")),
            None
        );
    }

    #[test]
    fn empty_first_run_home_is_not_usable() {
        let dir = unique_temp("empty-home");
        fs::write(dir.join("config.toml"), "[marketplace]\n").unwrap();
        assert!(!grok_home_looks_usable(&dir));
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(dir.join("bin").join(grok_exe_name()), b"x").unwrap();
        assert!(grok_home_looks_usable(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn auth_only_home_is_usable() {
        let dir = unique_temp("auth-home");
        fs::write(dir.join("auth.json"), "{}").unwrap();
        assert!(grok_home_looks_usable(&dir));
        let _ = fs::remove_dir_all(&dir);
    }
}
