//! 文件工具模块
//! 提供文件操作的辅助功能

use crate::types::{AppError, AppResult};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 确保目录存在
pub fn ensure_dir_exists(path: &Path) -> AppResult<()> {
    if !path.exists() {
        fs::create_dir_all(path)
            .map_err(|e| AppError::Io(format!("Failed to create directory {:?}: {}", path, e)))?
    }
    Ok(())
}

/// 获取文件大小
pub fn get_file_size(path: &Path) -> AppResult<u64> {
    let metadata = fs::metadata(path)
        .map_err(|e| AppError::Io(format!("Failed to get file metadata for {:?}: {}", path, e)))?;
    Ok(metadata.len())
}

/// 检查文件是否存在
pub fn file_exists(path: &Path) -> bool {
    path.exists() && path.is_file()
}

/// 检查目录是否存在
pub fn dir_exists(path: &Path) -> bool {
    path.exists() && path.is_dir()
}

/// 复制文件
pub fn copy_file(src: &Path, dst: &Path) -> AppResult<()> {
    if let Some(parent) = dst.parent() {
        ensure_dir_exists(parent)?;
    }

    fs::copy(src, dst).map_err(|e| {
        AppError::Io(format!(
            "Failed to copy file from {:?} to {:?}: {}",
            src, dst, e
        ))
    })?;
    Ok(())
}

/// 移动文件
pub fn move_file(src: &Path, dst: &Path) -> AppResult<()> {
    if let Some(parent) = dst.parent() {
        ensure_dir_exists(parent)?;
    }

    fs::rename(src, dst).map_err(|e| {
        AppError::Io(format!(
            "Failed to move file from {:?} to {:?}: {}",
            src, dst, e
        ))
    })?;
    Ok(())
}

/// 删除文件
pub fn delete_file(path: &Path) -> AppResult<()> {
    if path.exists() {
        fs::remove_file(path)
            .map_err(|e| AppError::Io(format!("Failed to delete file {:?}: {}", path, e)))?;
    }
    Ok(())
}

/// 删除目录及其内容
pub fn delete_dir(path: &Path) -> AppResult<()> {
    if path.exists() {
        fs::remove_dir_all(path)
            .map_err(|e| AppError::Io(format!("Failed to delete directory {:?}: {}", path, e)))?;
    }
    Ok(())
}

/// 获取文件扩展名
pub fn get_file_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
}

/// 获取文件名（不含扩展名）
pub fn get_file_stem(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|s| s.to_string())
}

/// 列出目录中的文件
pub fn list_files(dir: &Path, recursive: bool) -> AppResult<Vec<PathBuf>> {
    let mut files = Vec::new();

    if !dir.is_dir() {
        return Err(AppError::Io(format!("{:?} is not a directory", dir)));
    }

    list_files_recursive(dir, recursive, &mut files)?;
    Ok(files)
}

fn list_files_recursive(dir: &Path, recursive: bool, files: &mut Vec<PathBuf>) -> AppResult<()> {
    let entries = fs::read_dir(dir)
        .map_err(|e| AppError::Io(format!("Failed to read directory {:?}: {}", dir, e)))?;

    for entry in entries {
        let entry =
            entry.map_err(|e| AppError::Io(format!("Failed to read directory entry: {}", e)))?;
        let path = entry.path();

        if path.is_file() {
            files.push(path);
        } else if path.is_dir() && recursive {
            list_files_recursive(&path, recursive, files)?;
        }
    }

    Ok(())
}

/// 计算目录大小
pub fn calculate_dir_size(dir: &Path) -> AppResult<u64> {
    let mut total_size = 0;

    if !dir.is_dir() {
        return Err(AppError::Io(format!("{:?} is not a directory", dir)));
    }

    calculate_dir_size_recursive(dir, &mut total_size)?;
    Ok(total_size)
}

fn calculate_dir_size_recursive(dir: &Path, total_size: &mut u64) -> AppResult<()> {
    let entries = fs::read_dir(dir)
        .map_err(|e| AppError::Io(format!("Failed to read directory {:?}: {}", dir, e)))?;

    for entry in entries {
        let entry =
            entry.map_err(|e| AppError::Io(format!("Failed to read directory entry: {}", e)))?;
        let path = entry.path();

        if path.is_file() {
            let size = get_file_size(&path)?;
            *total_size += size;
        } else if path.is_dir() {
            calculate_dir_size_recursive(&path, total_size)?;
        }
    }

    Ok(())
}
