mod acp;
mod agents;
mod cli_caps;
mod cloud_update;
mod commands;
mod config;
mod control;
mod diagnostics;
mod diff_ops;
mod git_ops;
mod job_object;
mod journal;
mod paths;
mod polish;
mod process_util;
mod session_fsm;
mod sessions_disk;
mod supervisor;
mod terminal;
mod turn_lease;
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
    // 启动修复（turn lease）：上次运行中未收尾的轮次 → 会话标 interrupted +
    // 自有日志追加说明；meta 里残留的 running/waiting_permission 归一化。
    // 事件循环还没起，这里的少量小文件 IO 不影响窗口。
    {
        let mut st = desktop.lock();
        let repaired = turn_lease::repair_on_startup(&mut st);
        if repaired > 0 {
            tracing::warn!("启动修复：{repaired} 个会话上轮被中断（已标记 interrupted）");
        }
        let _ = st.save();
    }
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
        .setup(move |app| {
            // 后台看门狗：流静默提示（不自动取消）+ 闲置进程回收
            supervisor::spawn_watchdog(
                app.handle().clone(),
                app.state::<AppState>().supervisor.clone(),
            );

            // 本地控制口：外部程序（助手/脚本）可直接建会话、发 prompt、取消，
            // 不必去驱动 WebView 界面。只绑 127.0.0.1，需 token。
            {
                let st = app.state::<AppState>();
                control::spawn(
                    app.handle().clone(),
                    st.desktop.clone(),
                    st.supervisor.clone(),
                );
            }

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
            commands::polish_prompt,
            commands::session::respond_permission,
            commands::session::handle_server_request,
            commands::session::hibernate_session,
            commands::session::set_active_project,
            commands::session::stall_keep_waiting,
            commands::session::cancel_start,
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
            commands::disk::load_journal,
            commands::disk::save_journal,
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

/// 控制台窗口已在整个应用里隐藏（见 main.rs），tracing 输出改写到文件。
/// 零依赖实现：一个把 Write 转发到 File 的 MakeWriter。
struct FileWriter(Arc<StdMutex<std::fs::File>>);

impl std::io::Write for FileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut f = self.0.lock();
        // 进程一直开着时，启动滚动覆盖不到。超限就截断当前文件，避免无限长大。
        if f.metadata().map(|m| m.len()).unwrap_or(0) >= LOG_MAX_BYTES {
            f.set_len(0)?;
            let _ = f.write_all(b"--- log truncated ---\n");
        }
        f.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().flush()
    }
}

#[derive(Clone)]
struct FileMakeWriter(Arc<StdMutex<std::fs::File>>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for FileMakeWriter {
    type Writer = FileWriter;
    fn make_writer(&'a self) -> Self::Writer {
        FileWriter(self.0.clone())
    }
}

const LOG_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// 启动时若日志已超过上限，把当前文件留成 grokfree.log.1，再开一份新的。
fn rotate_log_if_needed(path: &std::path::Path, max_bytes: u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() <= max_bytes {
        return;
    }
    let rotated = path.with_file_name("grokfree.log.1");
    let _ = std::fs::remove_file(&rotated);
    let _ = std::fs::rename(path, &rotated);
}

fn init_logging() {
    let log_dir = paths::desktop_logs_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let log_path = log_dir.join("grokfree.log");
    rotate_log_if_needed(&log_path, LOG_MAX_BYTES);
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(f) => {
            let writer = FileMakeWriter(Arc::new(StdMutex::new(f)));
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(false)
                .compact()
                .with_writer(writer)
                .init();
        }
        Err(_) => {
            // 打不开日志文件就退回默认（无控制台时等于静默），不影响主程序。
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(false)
                .compact()
                .init();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::rotate_log_if_needed;

    #[test]
    fn rotates_oversized_log_and_keeps_the_previous_file() {
        let dir = std::env::temp_dir().join(format!(
            "grokfree-log-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("grokfree.log");
        std::fs::write(&path, b"0123456789").unwrap();
        rotate_log_if_needed(&path, 4);
        let rotated = dir.join("grokfree.log.1");
        assert_eq!(std::fs::read(&rotated).unwrap(), b"0123456789");
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
