//! LUT管理命令模块
//! 提供LUT文件操作相关的Tauri命令

use crate::core::lut::{LutManager, LutData};
use crate::types::LutInfo;
use crate::utils::logger;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct LutValidationResult {
    pub is_valid: bool,
    pub format: Option<String>,
    pub size: Option<String>,
    pub error: Option<String>,
}

/// 验证LUT文件
#[tauri::command]
pub async fn validate_lut_file(
    lut_path: String,
    lut_manager: State<'_, LutManager>,
) -> Result<LutValidationResult, String> {
    logger::log_info(&format!("Validating LUT file: {}", lut_path));
    
    let path = Path::new(&lut_path);
    if !path.exists() {
        return Ok(LutValidationResult {
            is_valid: false,
            format: None,
            size: None,
            error: Some("File does not exist".to_string()),
        });
    }
    
    match lut_manager.is_valid_lut(&lut_path).await {
        true => {
            match lut_manager.get_lut_info(&lut_path).await {
                Ok(info) => Ok(LutValidationResult {
                    is_valid: true,
                    format: Some(format!("{:?}", info.format)),
                    size: Some(format!("{}x{}x{}", info.size, info.size, info.size)),
                    error: None,
                }),
                Err(e) => Ok(LutValidationResult {
                    is_valid: false,
                    format: None,
                    size: None,
                    error: Some(e.to_string()),
                }),
            }
        }
        false => Ok(LutValidationResult {
            is_valid: false,
            format: None,
            size: None,
            error: Some("Invalid LUT format".to_string()),
        }),
    }
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

/// 获取支持的LUT格式（来自核心模块）
#[tauri::command]
pub async fn get_supported_lut_formats(
    lut_manager: State<'_, LutManager>,
) -> Result<Vec<String>, String> {
    // 将核心模块的枚举映射为前端友好的小写扩展名字符串
    let formats = lut_manager.get_supported_formats();
    let list = formats.iter().map(|f| match f {
        crate::types::LutFormat::Cube => "cube".to_string(),
        crate::types::LutFormat::ThreeDL => "3dl".to_string(),
        crate::types::LutFormat::Lut => "lut".to_string(),
        crate::types::LutFormat::Csp => "csp".to_string(),
        crate::types::LutFormat::M3d => "m3d".to_string(),
        crate::types::LutFormat::Look => "look".to_string(),
        crate::types::LutFormat::Vlt => "vlt".to_string(),
        crate::types::LutFormat::Mga => "mga".to_string(),
        crate::types::LutFormat::Unknown => "unknown".to_string(),
    }).collect();
    Ok(list)
}