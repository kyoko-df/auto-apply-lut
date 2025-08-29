//! Auto Apply LUT - 视频LUT批量处理应用
//! 
//! 基于Tauri框架的桌面应用程序，用于批量为视频文件应用LUT色彩校正。

use tauri::Manager;
use tracing::{info, error};
use tracing_subscriber;

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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            greet,
            get_app_info,
            commands::system_manager::get_available_codecs
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
