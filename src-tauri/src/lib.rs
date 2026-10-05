//! Auto Apply LUT - 视频LUT批量处理应用
//!
//! 基于Tauri框架的桌面应用程序，用于批量为视频文件应用LUT色彩校正。

use std::sync::Mutex;
use tracing::info;
use tracing_subscriber;

// 模块导入
pub mod commands;
pub mod core;
pub mod database;
pub mod events;
pub mod types;
pub mod utils;

use crate::core::task::TaskEvent;
use crate::database::runtime::upsert_task_snapshot;
use tauri::Manager;
use types::ApiResponse;

/// 获取应用信息
#[tauri::command]
fn get_app_info() -> ApiResponse<serde_json::Value> {
    let info = serde_json::json!({
        "name": "Auto Apply LUT",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "视频LUT批量处理应用"
    });
    ApiResponse::success(info)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    info!("启动 Auto Apply LUT 应用");

    // 初始化核心服务
    let lut_manager = core::lut::LutManager::new();
    let gpu_manager = core::gpu::GpuManager::new();
    let (task_manager, mut task_events) = core::task::TaskManager::new();
    let (db_manager, config_manager, startup_state) = match commands::startup::initialize_storage()
    {
        Ok(storage) => storage,
        Err(error) => {
            tracing::error!("无法初始化本地或临时存储：{error}");
            return;
        }
    };
    // 初始化 VideoProcessor 使用配置或自动发现的 ffmpeg 路径
    let ffmpeg_path = if let Some(p) = config_manager
        .get_config()
        .ffmpeg_path
        .clone()
        .filter(|s| !s.trim().is_empty())
    {
        std::path::PathBuf::from(p)
    } else {
        match core::ffmpeg::discover_ffmpeg_path() {
            Ok(pb) => pb,
            Err(e) => {
                tracing::warn!("自动发现 FFmpeg 失败: {}，退回使用 PATH", e);
                std::path::PathBuf::from("ffmpeg")
            }
        }
    };
    let video_processor = core::ffmpeg::processor::VideoProcessor::new(ffmpeg_path);
    let task_manager_for_events = task_manager.clone();
    let db_for_events = db_manager.clone();

    let application = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::preview::PreviewState::default())
        .manage(lut_manager)
        .manage(gpu_manager)
        .manage(task_manager)
        .manage(db_manager)
        .manage(video_processor)
        .manage(Mutex::new(config_manager))
        .manage(startup_state)
        .manage(commands::workspace::WorkspaceStore::default())
        .manage(commands::workspace::ExitGuard::default())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if !window.state::<commands::workspace::ExitGuard>().approved() {
                    api.prevent_close();
                    commands::workspace::notify_close_requested(window.app_handle());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            commands::workspace::load_workspace,
            commands::workspace::save_workspace,
            commands::workspace::set_export_guard,
            commands::workspace::request_app_exit,
            commands::startup::startup_status,
            commands::startup::recover_startup_storage,
            commands::system_manager::get_app_settings,
            commands::system_manager::update_app_settings,
            commands::system_manager::get_system_info,
            commands::system_manager::get_ffmpeg_info,
            commands::system_manager::get_ffmpeg_path_config,
            commands::system_manager::set_ffmpeg_path_config,
            commands::system_manager::get_log_files,
            commands::system_manager::read_log_file,
            commands::system_manager::get_cache_size,
            // GPU
            commands::gpu_manager::get_gpu_info,
            commands::gpu_manager::check_hardware_acceleration,
            commands::gpu_manager::test_hardware_acceleration,
            commands::file_manager::open_folder,
            commands::file_manager::open_file_location,
            commands::processor::get_video_info,
            commands::preview::generate_video_preview,
            commands::preview::cancel_video_preview,
            // LUT
            commands::lut_manager::validate_lut_file,
            commands::lut_manager::get_lut_info,
            commands::lut_manager::get_supported_lut_formats,
            commands::lut_manager::remember_lut_files,
            commands::lut_manager::list_lut_library,
            commands::lut_manager::import_lut_directory,
            commands::lut_manager::remove_lut_from_library,
            // Batch
            commands::batch_manager::scan_directory_for_videos,
            commands::batch_manager::start_batch_processing,
            commands::batch_manager::get_batch_progress,
            commands::batch_manager::cancel_batch,
            commands::batch_manager::generate_batch_from_directory,
        ])
        .setup(move |_app| {
            info!("应用初始化完成");

            let task_manager = task_manager_for_events.clone();
            let db = db_for_events.clone();
            tauri::async_runtime::spawn(async move {
                while let Some(event) = task_events.recv().await {
                    let maybe_task = match event {
                        TaskEvent::Created(task) => Some(task),
                        TaskEvent::Started(task_id)
                        | TaskEvent::ProgressUpdated(task_id, _)
                        | TaskEvent::Completed(task_id)
                        | TaskEvent::Failed(task_id, _)
                        | TaskEvent::Cancelled(task_id) => match task_manager.get_task(&task_id) {
                            Ok(Some(task)) => Some(task),
                            Ok(None) => None,
                            Err(error) => {
                                tracing::warn!("读取任务快照失败 {}: {}", task_id, error);
                                None
                            }
                        },
                    };

                    if let Some(task) = maybe_task {
                        if let Err(error) = upsert_task_snapshot(&db, &task) {
                            tracing::warn!("持久化任务 {} 失败: {}", task.id, error);
                        }
                    }
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!());
    match application {
        Ok(application) => application.run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if !app.state::<commands::workspace::ExitGuard>().approved() {
                    api.prevent_exit();
                    commands::workspace::notify_close_requested(app);
                }
            }
        }),
        Err(error) => tracing::error!("启动窗口失败：{error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_tests_require_real_tools_in_release_ci() {
        if std::env::var("LUTLAB_REQUIRE_MEDIA_TESTS").as_deref() != Ok("1") {
            return;
        }
        for tool in ["ffmpeg", "ffprobe"] {
            let output = std::process::Command::new(tool)
                .arg("-version")
                .output()
                .unwrap_or_else(|error| panic!("Release CI requires a runnable {tool}: {error}"));
            assert!(
                output.status.success(),
                "Release CI requires a working {tool}"
            );
            assert!(
                String::from_utf8_lossy(&output.stdout).starts_with(&format!("{tool} version")),
                "Invalid {tool} executable"
            );
        }
    }

    #[test]
    fn command_interface_get_app_info() {
        let response = get_app_info();
        assert!(response.success);
        let data = response.data.expect("missing app info data");
        assert_eq!(data["name"], "Auto Apply LUT");
        assert_eq!(data["version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn unused_destructive_commands_are_not_exposed_to_the_webview() {
        let source = include_str!("lib.rs");
        let start = source.find("tauri::generate_handler![").unwrap();
        let handler = &source[start..];
        let handler = &handler[..handler.find("])").unwrap()];
        for removed in [
            "delete_path",
            "copy_file",
            "move_file",
            "create_directory",
            "play_with_ffplay",
            "stop_ffplay",
            "clear_cache",
            "start_video_processing",
            "get_task_progress",
            "cancel_task",
            "get_all_tasks",
        ] {
            assert!(
                !handler.contains(removed),
                "unexpected mutation command: {removed}"
            );
        }
    }
}
