//! LUT管理命令模块
//! 提供LUT文件操作相关的Tauri命令

use crate::core::lut::LutManager;
use crate::types::{LutInfo, LutValidationResult, LutFormat};
use crate::utils::logger;
use tauri::State;

/// 验证LUT文件（委托核心模块）
#[tauri::command]
pub async fn validate_lut_file(
    lut_path: String,
    lut_manager: State<'_, LutManager>,
) -> Result<LutValidationResult, String> {
    logger::log_info(&format!("Validating LUT file: {}", lut_path));
    lut_manager.validate_lut(&lut_path).await.map_err(|e| e.to_string())
}

/// 获取LUT信息
#[tauri::command]
pub async fn get_lut_info(
    lut_path: String,
    lut_manager: State<'_, LutManager>,
) -> Result<LutInfo, String> {
    logger::log_info(&format!("Getting LUT info: {}", lut_path));
    lut_manager.get_lut_info(&lut_path).await
        .map_err(|e| e.to_string())
}

/// 获取支持的LUT格式（来自核心模块，返回扩展名字符串）
#[tauri::command]
pub async fn get_supported_lut_formats(
    lut_manager: State<'_, LutManager>,
) -> Result<Vec<String>, String> {
    let formats = lut_manager.get_supported_formats();
    let list = formats
        .iter()
        .map(|f| f.extension().to_string())
        .collect();
    Ok(list)
}