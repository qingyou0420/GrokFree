//! One-click cloud update via GitHub Releases.

use crate::cli_caps::parse_semver;
use crate::paths;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

pub const UPDATE_OWNER: &str = "qingyou0420";
pub const UPDATE_REPO: &str = "GrokFree";

fn releases_latest_url() -> String {
    format!("https://api.github.com/repos/{UPDATE_OWNER}/{UPDATE_REPO}/releases/latest")
}

fn user_agent() -> String {
    format!("GrokFree/{}", env!("CARGO_PKG_VERSION"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudUpdateInfo {
    pub version: String,
    pub current_version: String,
    pub is_newer: bool,
    pub can_install: bool,
    pub download_url: String,
    pub file_name: String,
    pub html_url: String,
    pub notes: Option<String>,
    pub size_bytes: u64,
    pub published_at: Option<String>,
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub phase: String,
    pub received: u64,
    pub total: u64,
    pub percent: u8,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

pub fn current_app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn tag_to_version(tag: &str) -> String {
    tag.trim().trim_start_matches('v').trim().to_string()
}

fn is_our_setup_asset(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if !lower.ends_with("-setup.exe") {
        return false;
    }
    lower.starts_with("grokfree_") || lower.starts_with("grok free_")
}

fn version_is_newer(remote: &str, current: &str) -> bool {
    let Some(a) = parse_semver(remote) else {
        return false;
    };
    let Some(b) = parse_semver(current) else {
        return true;
    };
    a > b
}

fn pick_setup_asset(release: &GhRelease) -> Option<&GhAsset> {
    release.assets.iter().find(|a| is_our_setup_asset(&a.name))
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(user_agent())
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("HTTP 客户端失败：{e}"))
}

pub async fn fetch_latest_release() -> Result<Option<CloudUpdateInfo>, String> {
    let client = http_client()?;
    let resp = client
        .get(releases_latest_url())
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| format!("无法连接 GitHub：{e}"))?;

    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if status.as_u16() == 403 {
        return Err("GitHub API 限流，请稍后再试".into());
    }
    if !status.is_success() {
        return Err(format!("GitHub 返回 HTTP {status}"));
    }

    let release: GhRelease = resp
        .json()
        .await
        .map_err(|e| format!("解析 GitHub 发行说明失败：{e}"))?;

    if release.draft || release.prerelease {
        return Ok(None);
    }

    let Some(asset) = pick_setup_asset(&release) else {
        return Ok(None);
    };
    let file_name = asset.name.clone();
    let download_url = asset.browser_download_url.clone();
    let size_bytes = asset.size;

    let version = tag_to_version(&release.tag_name);
    if parse_semver(&version).is_none() {
        return Err(format!("无法识别版本号：{}", release.tag_name));
    }
    let current = current_app_version().to_string();
    let is_newer = version_is_newer(&version, &current);
    let staged = staged_installer_path(&file_name);
    let local_path = if staged.is_file() {
        Some(staged.display().to_string())
    } else {
        None
    };

    Ok(Some(CloudUpdateInfo {
        version,
        current_version: current,
        is_newer,
        can_install: is_newer,
        download_url,
        file_name,
        html_url: release.html_url,
        notes: release.body.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        size_bytes,
        published_at: release.published_at,
        local_path,
    }))
}

fn staged_installer_path(file_name: &str) -> PathBuf {
    paths::desktop_data_dir().join("installers").join(file_name)
}

fn emit_progress(app: &AppHandle, phase: &str, received: u64, total: u64) {
    let percent = if total > 0 {
        ((received.saturating_mul(100)) / total).min(100) as u8
    } else {
        0
    };
    let _ = app.emit(
        "app://update-progress",
        UpdateProgress {
            phase: phase.into(),
            received,
            total,
            percent,
        },
    );
}

async fn download_to(app: &AppHandle, url: &str, dest: &Path, expected_size: u64) -> Result<u64, String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("无法创建下载目录：{e}"))?;
    }
    let tmp = dest.with_extension("exe.partial");
    let _ = fs::remove_file(&tmp);

    let client = http_client()?;
    emit_progress(app, "download", 0, expected_size);
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败 HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(expected_size);
    let mut file = fs::File::create(&tmp).map_err(|e| format!("无法写入临时文件：{e}"))?;
    let mut received: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载中断：{e}"))?;
        file.write_all(&chunk)
            .map_err(|e| format!("写入安装包失败：{e}"))?;
        received += chunk.len() as u64;
        emit_progress(app, "download", received, total);
    }
    file.flush().map_err(|e| format!("落盘失败：{e}"))?;
    drop(file);

    if dest.exists() {
        let _ = fs::remove_file(dest);
    }
    fs::rename(&tmp, dest).map_err(|e| format!("无法完成下载：{e}"))?;
    emit_progress(app, "download", received.max(total), total.max(1));
    Ok(received)
}

pub fn launch_installer(path: &str) -> Result<(), String> {
    let p = PathBuf::from(path);
    let p = fs::canonicalize(&p).unwrap_or(p);
    let path_display = p.display().to_string();
    if !p.is_file() {
        return Err(format!("安装包不存在：{path_display}"));
    }
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if !is_our_setup_asset(&name) {
        return Err(format!("拒绝启动：不是可识别的 GrokFree 安装包（{path_display}）"));
    }

    #[cfg(windows)]
    {
        let mut errors: Vec<String> = Vec::new();
        let escaped = path_display.replace('\'', "''");
        let script = format!("Start-Process -FilePath '{escaped}'");
        match std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &script,
            ])
            .spawn()
        {
            Ok(_) => {
                tracing::info!("started installer via PowerShell: {path_display}");
                return Ok(());
            }
            Err(e) => errors.push(format!("PowerShell: {e}")),
        }

        let script_as = format!("Start-Process -FilePath '{escaped}' -Verb RunAs");
        match std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &script_as,
            ])
            .spawn()
        {
            Ok(_) => {
                tracing::info!("started installer via PowerShell RunAs: {path_display}");
                return Ok(());
            }
            Err(e) => errors.push(format!("PowerShell RunAs: {e}")),
        }

        let quoted = path_display.replace('"', "");
        let cmdline = format!("start \"\" \"{quoted}\"");
        match std::process::Command::new("cmd").args(["/C", &cmdline]).spawn() {
            Ok(_) => {
                tracing::info!("started installer via cmd start: {path_display}");
                return Ok(());
            }
            Err(e) => errors.push(format!("cmd start: {e}")),
        }

        return Err(format!(
            "无法启动安装程序（{path_display}）。尝试结果：{}。可手动双击该 setup.exe 完成更新。",
            errors.join("；")
        ));
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new(&p)
            .spawn()
            .map_err(|e| format!("无法启动安装程序：{e}（{path_display}）"))?;
        Ok(())
    }
}

/// Download the latest NSIS installer (if needed) and launch it.
pub async fn download_and_launch(app: AppHandle) -> Result<CloudUpdateInfo, String> {
    emit_progress(&app, "check", 0, 0);
    let mut info = fetch_latest_release()
        .await?
        .ok_or_else(|| "GitHub 上还没有可用的 GrokFree 安装包".to_string())?;
    if !info.is_newer {
        return Err(format!(
            "已是最新版本 v{}，无需更新",
            info.current_version
        ));
    }

    let dest = staged_installer_path(&info.file_name);
    let reuse = dest.is_file()
        && fs::metadata(&dest)
            .map(|m| m.len() == info.size_bytes && info.size_bytes > 0)
            .unwrap_or(false);
    if !reuse {
        download_to(&app, &info.download_url, &dest, info.size_bytes).await?;
    }
    info.local_path = Some(dest.display().to_string());
    emit_progress(&app, "launch", 1, 1);
    launch_installer(&dest.display().to_string())?;
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_strips_v() {
        assert_eq!(tag_to_version("v0.9.0"), "0.9.0");
        assert_eq!(tag_to_version("0.9.0"), "0.9.0");
    }

    #[test]
    fn setup_asset_shape() {
        assert!(is_our_setup_asset("GrokFree_0.9.0_x64-setup.exe"));
        assert!(!is_our_setup_asset("Grok-Build-Desktop-Setup-v0.9.0-x64.exe"));
        assert!(!is_our_setup_asset("notes.md"));
    }

    #[test]
    fn newer_detect() {
        assert!(version_is_newer("0.9.1", "0.9.0"));
        assert!(!version_is_newer("0.9.0", "0.9.0"));
        assert!(!version_is_newer("0.8.1", "0.9.0"));
    }
}
