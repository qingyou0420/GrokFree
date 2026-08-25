//! Cloud update IPC (GitHub Releases)

use tauri::AppHandle;

#[tauri::command]
pub async fn check_cloud_update() -> Result<Option<crate::cloud_update::CloudUpdateInfo>, String> {
    crate::cloud_update::fetch_latest_release().await
}

/// Download the newer installer from GitHub and launch NSIS.
#[tauri::command]
pub async fn launch_cloud_update(
    app: AppHandle,
) -> Result<crate::cloud_update::CloudUpdateInfo, String> {
    crate::cloud_update::download_and_launch(app).await
}
