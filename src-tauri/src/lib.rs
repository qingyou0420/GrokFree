mod acp;
mod agents;
mod cli_caps;
mod cloud_update;
mod commands;
mod config;
mod diagnostics;
mod diff_ops;
mod git_ops;
mod job_object;
mod paths;
mod process_util;
mod sessions_disk;
mod supervisor;
mod terminal;
mod workspace;

use commands::AppState;
use config::DesktopState;
use parking_lot::Mutex as StdMutex;
use std::sync::Arc;
use supervisor::Supervisor;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use tracing_subscriber::EnvFilter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = paths::ensure_desktop_dirs();
    init_logging();

    let desktop = Arc::new(StdMutex::new(DesktopState::load()));
    // 旧 prefs.model 曾驱动 grok 会话；现在会话模型只看档案 defaultModel。
    // 首次启动把旧值并入 grok 档案。
    {
        let m = desktop.lock().prefs.model.trim().to_string();
        if !m.is_empty() {
            let mut profiles = agents::load();
            let needs = profiles
                .iter()
                .any(|p| p.id == agents::DEFAULT_AGENT_ID && p.default_model.trim().is_empty());
            if needs {
                if let Some(g) = profiles
                    .iter_mut()
                    .find(|p| p.id == agents::DEFAULT_AGENT_ID)
                {
                    g.default_model = m.clone();
                    if let Err(e) = agents::save(&profiles) {
                        tracing::warn!("模型迁移写入 agents.json 失败：{e}");
                    } else {
                        tracing::info!("已迁移旧「模型」设置（{m}）→ grok 档案默认模型");
                    }
                }
            }
        }
    }
    // Drop a stale GROK_HOME (e.g. leftover after relocating ~/.grok) so every
    // later probe / `grok agent stdio` child inherits the real install.
    {
        let grok_path = desktop.lock().prefs.grok_path.clone();
        let home = paths::apply_resolved_grok_home(if grok_path.trim().is_empty() {
            None
        } else {
            Some(grok_path.as_str())
        });
        tracing::info!("GROK_HOME={}", home.display());
    }
    let supervisor = Arc::new(Supervisor::new(desktop.clone()));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 二次启动：从磁盘重载状态并通知前端刷新（避免托盘旧进程仍显示空列表）
            if let Some(state) = app.try_state::<AppState>() {
                config::DesktopState::reload_into(&state.desktop);
                let snapshot = state.desktop.lock().clone();
                let _ = app.emit("app://state-reloaded", snapshot);
            }
            // Also surface pending permission/error session if any
            commands::show_and_focus_pending(app);
        }))
        .manage(AppState {
            desktop: desktop.clone(),
            supervisor: supervisor.clone(),
            focus_session: Arc::new(StdMutex::new(None)),
        })
        .setup(|app| {
            // System tray
            let show_i = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let focus_i =
                MenuItem::with_id(app, "focus_pending", "处理待办会话", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &focus_i, &quit_i])?;

            let mut tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .tooltip("GrokFree");
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            let _tray = tray
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" | "focus_pending" => {
                        commands::show_and_focus_pending(app);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        commands::show_and_focus_pending(tray.app_handle());
                    }
                })
                .build(app)?;

            // Hide to tray on close
            if let Some(window) = app.get_webview_window("main") {
                let window_ = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_.hide();
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::reload_state,
            commands::get_default_projects_dir,
            commands::probe_environment,
            commands::update_prefs,
            commands::set_onboarding_done,
            commands::add_project,
            commands::remove_project,
            commands::session::create_session,
            commands::session::resume_session,
            commands::agents_cmd::list_agents,
            commands::agents_cmd::save_agents,
            commands::session::list_live_sessions,
            commands::session::send_prompt,
            commands::session::cancel_prompt,
            commands::session::respond_permission,
            commands::session::handle_server_request,
            commands::session::hibernate_session,
            commands::open_config_file,
            commands::open_path,
            commands::open_in_editor,
            commands::reveal_logs,
            commands::read_file,
            commands::app_info,
            commands::open_installers_dir,
            commands::disk::list_disk_sessions,
            commands::disk::resolve_disk_session_path,
            commands::disk::load_disk_transcript,
            commands::disk::delete_disk_session,
            commands::disk::rename_session,
            commands::disk::remove_session_meta,
            commands::disk::purge_stale_session_meta,
            commands::update::check_cloud_update,
            commands::update::launch_cloud_update,
            commands::git_status,
            commands::apply_diff,
            commands::reject_diff,
            commands::export_diagnostics,
            commands::open_external_terminal,
            commands::list_skills_mcp,
            commands::cli_capabilities,
            commands::update_tray_status,
            commands::focus_main_window,
            ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                // Best-effort kill agents
                let state = app_handle.state::<AppState>();
                let supervisor = state.supervisor.clone();
                tauri::async_runtime::block_on(async move {
                    supervisor.kill_all().await;
                });
            }
        });
}

fn init_logging() {
    let log_dir = paths::desktop_logs_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
