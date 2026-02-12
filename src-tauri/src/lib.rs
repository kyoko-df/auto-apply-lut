//! Auto Apply LUT - 视频LUT批量处理应用
//! 
//! 基于Tauri框架的桌面应用程序，用于批量为视频文件应用LUT色彩校正。

use tauri::Manager;
use tracing::{info, error};
use tracing_subscriber;
use std::sync::Mutex;

// 模块导入
pub mod commands;
pub mod core;
pub mod types;
pub mod utils;
pub mod events;
pub mod database;

use commands::*;
use types::ApiResponse;

/// 应用程序状态
#[derive(Default)]
pub struct AppState {
    pub db: Option<database::DatabaseManager>,
}

pub struct FfplayState(pub Mutex<Option<std::process::Child>>);

// 示例命令 - 后续会被实际功能替换
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

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
    let task_manager = core::task::TaskManager::default();
    // 全局配置管理器需提前初始化，以便读取 ffmpeg 路径
    let config_manager = utils::config::ConfigManager::new().expect("初始化配置管理器失败");
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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .manage(lut_manager)
        .manage(task_manager)
        .manage(video_processor)
        .manage(Mutex::new(config_manager))
        .manage(FfplayState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            greet,
            get_app_info,
            commands::system_manager::get_available_codecs,
            commands::system_manager::get_ffmpeg_info,
            commands::system_manager::get_ffmpeg_path_config,
            commands::system_manager::set_ffmpeg_path_config,
            commands::file_manager::get_file_info,
            commands::file_manager::open_file,
            commands::file_manager::open_folder,
            commands::file_manager::play_with_ffplay,
            commands::file_manager::stop_ffplay,
            commands::processor::get_video_info,
            // LUT
            commands::lut_manager::validate_lut_file,
            commands::lut_manager::get_lut_info,
            commands::lut_manager::get_supported_lut_formats,
            // Task
            commands::processor::start_video_processing,
            commands::processor::get_task_progress,
            commands::processor::cancel_task,
            commands::processor::get_all_tasks,
        ])
        .setup(|app| {
            info!("应用初始化完成");
            
            // 这里可以添加应用启动时的初始化逻辑
            // 比如数据库初始化、配置加载等
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("启动Tauri应用时发生错误");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_interface_greet() {
        let text = greet("Codex");
        assert!(text.contains("Codex"));
        assert!(text.contains("greeted from Rust"));
    }

    #[test]
    fn command_interface_get_app_info() {
        let response = get_app_info();
        assert!(response.success);
        let data = response.data.expect("missing app info data");
        assert_eq!(data["name"], "Auto Apply LUT");
        assert_eq!(data["version"], env!("CARGO_PKG_VERSION"));
    }
}
