//! 输入验证工具模块

use crate::types::error::{AppError, AppResult};
use std::path::Path;

/// 验证文件路径是否有效
pub fn validate_file_path(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Err(AppError::Validation(format!("文件不存在: {:?}", path)));
    }

    if !path.is_file() {
        return Err(AppError::Validation(format!("路径不是文件: {:?}", path)));
    }

    Ok(())
}

/// 验证目录路径是否有效
pub fn validate_dir_path(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Err(AppError::Validation(format!("目录不存在: {:?}", path)));
    }

    if !path.is_dir() {
        return Err(AppError::Validation(format!("路径不是目录: {:?}", path)));
    }

    Ok(())
}

/// 验证文件扩展名
pub fn validate_file_extension(path: &Path, expected_exts: &[&str]) -> AppResult<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| AppError::Validation("无法获取文件扩展名".to_string()))?;

    if !expected_exts.iter().any(|&e| e.eq_ignore_ascii_case(ext)) {
        return Err(AppError::Validation(format!(
            "不支持的文件扩展名: {}, 支持的扩展名: {:?}",
            ext, expected_exts
        )));
    }

    Ok(())
}
