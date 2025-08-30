//! 文件管理命令模块
//! 提供文件操作相关的Tauri命令

use crate::utils::logger;
use serde::{Deserialize, Serialize};
use std::path::Path;
use crate::core::file::{FileManager as CoreFileManager, FileInfo as CoreFileInfo};

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

fn convert_file_info(info: CoreFileInfo) -> FileInfo {
    FileInfo {
        path: info.path.to_string_lossy().to_string(),
        name: info.name,
        size: info.size,
        is_directory: info.is_directory,
        extension: info.extension,
        modified: info.modified_at.timestamp(),
    }
}

/// 列出目录内容
#[tauri::command]
pub async fn list_directory(path: String) -> Result<DirectoryListing, String> {
    logger::log_info(&format!("Listing directory: {}", path));

    let fm = CoreFileManager::new();

    // 调用核心模块获取文件列表（非递归）
    let items = fm.list_directory(&path).map_err(|e| e.to_string())?;
    let files: Vec<FileInfo> = items.into_iter().map(convert_file_info).collect();

    // 计算目录总大小
    let total_size = fm.get_directory_size(&path).map_err(|e| e.to_string())?;

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

    let fm = CoreFileManager::new();
    fm.create_directory(&path).map_err(|e| e.to_string())?;

    Ok("Directory created successfully".to_string())
}

/// 删除文件或目录
#[tauri::command]
pub async fn delete_path(path: String) -> Result<String, String> {
    logger::log_info(&format!("Deleting path: {}", path));

    let fm = CoreFileManager::new();
    if fm.is_directory(&path) {
        fm.delete_directory(&path).map_err(|e| e.to_string())?;
    } else if fm.is_file(&path) {
        fm.delete_file(&path).map_err(|e| e.to_string())?;
    } else {
        return Err("Path does not exist".to_string());
    }

    Ok("Path deleted successfully".to_string())
}

/// 复制文件
#[tauri::command]
pub async fn copy_file(src: String, dst: String) -> Result<String, String> {
    logger::log_info(&format!("Copying file from {} to {}", src, dst));

    let fm = CoreFileManager::new();
    if !fm.is_file(&src) {
        return Err("Source file does not exist".to_string());
    }

    let _ = fm.copy_file(&src, &dst).map_err(|e| e.to_string())?;
    Ok("File copied successfully".to_string())
}

/// 移动文件
#[tauri::command]
pub async fn move_file(src: String, dst: String) -> Result<String, String> {
    logger::log_info(&format!("Moving file from {} to {}", src, dst));

    let fm = CoreFileManager::new();
    if !fm.is_file(&src) {
        return Err("Source file does not exist".to_string());
    }

    fm.move_file(&src, &dst).map_err(|e| e.to_string())?;
    Ok("File moved successfully".to_string())
}

/// 获取文件信息
#[tauri::command]
pub async fn get_file_info(path: String) -> Result<FileInfo, String> {
    logger::log_info(&format!("Getting file info: {}", path));

    let fm = CoreFileManager::new();
    if !fm.path_exists(&path) {
        return Err("File does not exist".to_string());
    }

    let info = fm.get_file_info(&path).map_err(|e| e.to_string())?;
    Ok(convert_file_info(info))
}