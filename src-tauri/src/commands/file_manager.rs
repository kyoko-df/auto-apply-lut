//! 文件管理命令模块
//! 提供文件操作相关的Tauri命令

use crate::types::{AppResult, AppError};
use crate::utils::{file_utils, logger};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_directory: bool,
    pub extension: Option<String>,
    pub modified: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryListing {
    pub path: String,
    pub files: Vec<FileInfo>,
    pub total_size: u64,
}

/// 列出目录内容
#[tauri::command]
pub async fn list_directory(path: String) -> Result<DirectoryListing, String> {
    logger::log_info(&format!("Listing directory: {}", path));
    
    let dir_path = Path::new(&path);
    if !dir_path.exists() {
        return Err("Directory does not exist".to_string());
    }
    
    if !dir_path.is_dir() {
        return Err("Path is not a directory".to_string());
    }
    
    let files = file_utils::list_files(dir_path, false)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|file_path| {
            let metadata = std::fs::metadata(&file_path).ok();
            FileInfo {
                path: file_path.to_string_lossy().to_string(),
                name: file_path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                is_directory: file_path.is_dir(),
                extension: file_utils::get_file_extension(&file_path),
                modified: metadata
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
            }
        })
        .collect();
    
    let total_size = file_utils::calculate_dir_size(dir_path)
        .map_err(|e| e.to_string())?;
    
    Ok(DirectoryListing {
        path: path.clone(),
        files,
        total_size,
    })
}

/// 创建目录
#[tauri::command]
pub async fn create_directory(path: String) -> Result<String, String> {
    logger::log_info(&format!("Creating directory: {}", path));
    
    let dir_path = Path::new(&path);
    file_utils::ensure_dir_exists(dir_path)
        .map_err(|e| e.to_string())?;
    
    Ok("Directory created successfully".to_string())
}

/// 删除文件或目录
#[tauri::command]
pub async fn delete_path(path: String) -> Result<String, String> {
    logger::log_info(&format!("Deleting path: {}", path));
    
    let target_path = Path::new(&path);
    if !target_path.exists() {
        return Err("Path does not exist".to_string());
    }
    
    if target_path.is_dir() {
        file_utils::delete_dir(target_path)
            .map_err(|e| e.to_string())?;
    } else {
        file_utils::delete_file(target_path)
            .map_err(|e| e.to_string())?;
    }
    
    Ok("Path deleted successfully".to_string())
}

/// 复制文件
#[tauri::command]
pub async fn copy_file(src: String, dst: String) -> Result<String, String> {
    logger::log_info(&format!("Copying file from {} to {}", src, dst));
    
    let src_path = Path::new(&src);
    let dst_path = Path::new(&dst);
    
    if !src_path.exists() {
        return Err("Source file does not exist".to_string());
    }
    
    file_utils::copy_file(src_path, dst_path)
        .map_err(|e| e.to_string())?;
    
    Ok("File copied successfully".to_string())
}

/// 移动文件
#[tauri::command]
pub async fn move_file(src: String, dst: String) -> Result<String, String> {
    logger::log_info(&format!("Moving file from {} to {}", src, dst));
    
    let src_path = Path::new(&src);
    let dst_path = Path::new(&dst);
    
    if !src_path.exists() {
        return Err("Source file does not exist".to_string());
    }
    
    file_utils::move_file(src_path, dst_path)
        .map_err(|e| e.to_string())?;
    
    Ok("File moved successfully".to_string())
}

/// 获取文件信息
#[tauri::command]
pub async fn get_file_info(path: String) -> Result<FileInfo, String> {
    logger::log_info(&format!("Getting file info: {}", path));
    
    let file_path = Path::new(&path);
    if !file_path.exists() {
        return Err("File does not exist".to_string());
    }
    
    let metadata = std::fs::metadata(file_path)
        .map_err(|e| format!("Failed to get file metadata: {}", e))?;
    
    Ok(FileInfo {
        path: path.clone(),
        name: file_path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        size: metadata.len(),
        is_directory: metadata.is_dir(),
        extension: file_utils::get_file_extension(file_path),
        modified: metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    })
}