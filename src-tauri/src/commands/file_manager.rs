//! 文件管理命令模块
//! 提供文件操作相关的Tauri命令

use crate::utils::logger;
use serde::{Deserialize, Serialize};
use crate::core::file::{FileManager as CoreFileManager, FileInfo as CoreFileInfo};
use std::process::Command;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;
use crate::utils::config::ConfigManager;
use crate::FfplayState;

#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryListing {
    pub path: String,
    pub files: Vec<CoreFileInfo>,
    pub total_size: u64,
}

/// 列出目录内容
#[tauri::command]
pub async fn list_directory(path: String) -> Result<DirectoryListing, String> {
    logger::log_info(&format!("Listing directory: {}", path));

    let fm = CoreFileManager::new();

    // 调用核心模块获取文件列表（非递归）
    let files: Vec<CoreFileInfo> = fm.list_directory(&path).map_err(|e| e.to_string())?;

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
pub async fn get_file_info(path: String) -> Result<CoreFileInfo, String> {
    logger::log_info(&format!("Getting file info: {}", path));

    let fm = CoreFileManager::new();
    if !fm.path_exists(&path) {
        return Err("File does not exist".to_string());
    }

    let info = fm.get_file_info(&path).map_err(|e| e.to_string())?;
    Ok(info)
}

/// 打开文件（使用系统默认程序）
#[tauri::command]
pub async fn open_file(path: String) -> Result<String, String> {
    logger::log_info(&format!("Opening file: {}", path));

    if !Path::new(&path).exists() {
        return Err("File does not exist".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/c", "start", "", &path])
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }

    Ok("File opened successfully".to_string())
}

/// 打开文件夹（在文件管理器中显示）
#[tauri::command]
pub async fn open_folder(path: String) -> Result<String, String> {
    logger::log_info(&format!("Opening folder: {}", path));

    if !Path::new(&path).exists() {
        return Err("Folder does not exist".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok("Folder opened successfully".to_string())
}

fn resolve_ffplay_executable(cfg_ffmpeg_path: Option<String>) -> Result<String, String> {
    let ffplay_name = if cfg!(target_os = "windows") { "ffplay.exe" } else { "ffplay" };

    let mut candidates: Vec<String> = Vec::new();

    if let Some(ffmpeg_path) = cfg_ffmpeg_path.filter(|s| !s.trim().is_empty()) {
        let pb = PathBuf::from(&ffmpeg_path);
        if pb.is_absolute() {
            if pb.is_file() {
                if let Some(parent) = pb.parent() {
                    candidates.push(parent.join(ffplay_name).to_string_lossy().to_string());
                }
            } else if let Some(parent) = pb.parent() {
                candidates.push(parent.join(ffplay_name).to_string_lossy().to_string());
            }
        } else {
            candidates.push(ffplay_name.to_string());
        }
    }

    #[cfg(target_os = "windows")]
    {
        candidates.extend([
            "C:\\ffmpeg\\bin\\ffplay.exe".to_string(),
            "C:\\Program Files\\ffmpeg\\bin\\ffplay.exe".to_string(),
            "ffplay.exe".to_string(),
        ]);
    }

    #[cfg(not(target_os = "windows"))]
    {
        candidates.extend([
            "/opt/homebrew/bin/ffplay".to_string(),
            "/usr/local/bin/ffplay".to_string(),
            "/usr/bin/ffplay".to_string(),
            "ffplay".to_string(),
        ]);
    }

    for cand in candidates {
        let pb = PathBuf::from(&cand);
        if pb.is_absolute() {
            if pb.exists() {
                return Ok(cand);
            }
        } else if Command::new(&cand).arg("-version").output().is_ok() {
            return Ok(cand);
        }
    }

    Err("ffplay not found. Please install FFmpeg with ffplay or add it to PATH.".to_string())
}

#[tauri::command]
pub async fn play_with_ffplay(
    path: String,
    config_manager: State<'_, Mutex<ConfigManager>>,
    ffplay_state: State<'_, FfplayState>,
) -> Result<String, String> {
    logger::log_info(&format!("Playing with ffplay: {}", path));

    if !Path::new(&path).exists() {
        return Err("File does not exist".to_string());
    }

    let cfg_ffmpeg_path = config_manager
        .lock()
        .map_err(|e| format!("Config lock poisoned: {}", e))?
        .get_config()
        .ffmpeg_path
        .clone();

    let ffplay = resolve_ffplay_executable(cfg_ffmpeg_path)?;

    let mut guard = ffplay_state
        .0
        .lock()
        .map_err(|e| format!("ffplay state lock poisoned: {}", e))?;

    if let Some(mut prev) = guard.take() {
        let _ = prev.kill();
        let _ = prev.wait();
    }

    let child = Command::new(ffplay)
        .args(["-autoexit", "-loglevel", "warning"])
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to launch ffplay: {}", e))?;

    *guard = Some(child);

    Ok("ffplay launched".to_string())
}

#[tauri::command]
pub async fn stop_ffplay(ffplay_state: State<'_, FfplayState>) -> Result<String, String> {
    let mut guard = ffplay_state
        .0
        .lock()
        .map_err(|e| format!("ffplay state lock poisoned: {}", e))?;

    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        let _ = child.wait();
        return Ok("ffplay stopped".to_string());
    }

    Ok("ffplay not running".to_string())
}
