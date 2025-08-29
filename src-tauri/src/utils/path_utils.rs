//! 路径工具模块

use crate::types::error::{AppError, AppResult};
use std::path::{Path, PathBuf};
use std::env;

/// 获取应用数据目录
pub fn get_app_data_dir() -> AppResult<PathBuf> {
    let app_name = "auto-apply-lut";
    
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = env::var_os("HOME") {
            let mut path = PathBuf::from(home);
            path.push("Library");
            path.push("Application Support");
            path.push(app_name);
            return Ok(path);
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = env::var_os("APPDATA") {
            let mut path = PathBuf::from(appdata);
            path.push(app_name);
            return Ok(path);
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = env::var_os("HOME") {
            let mut path = PathBuf::from(home);
            path.push(".local");
            path.push("share");
            path.push(app_name);
            return Ok(path);
        }
    }
    
    Err(AppError::Internal("无法确定应用数据目录".to_string()))
}

/// 获取临时目录
pub fn get_temp_dir() -> AppResult<PathBuf> {
    let mut temp_dir = std::env::temp_dir();
    temp_dir.push("auto-apply-lut");
    
    if !temp_dir.exists() {
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| AppError::Io(format!("创建临时目录失败: {}", e)))?;
    }
    
    Ok(temp_dir)
}

/// 获取缓存目录
pub fn get_cache_dir() -> AppResult<PathBuf> {
    let mut cache_dir = get_app_data_dir()?;
    cache_dir.push("cache");
    
    if !cache_dir.exists() {
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| AppError::Io(format!("创建缓存目录失败: {}", e)))?;
    }
    
    Ok(cache_dir)
}

/// 规范化路径
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    
    for component in path.components() {
        match component {
            std::path::Component::Normal(name) => components.push(name),
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            _ => components.push(component.as_os_str()),
        }
    }
    
    components.iter().collect()
}

/// 获取相对路径
pub fn get_relative_path(from: &Path, to: &Path) -> AppResult<PathBuf> {
    pathdiff::diff_paths(to, from)
        .ok_or_else(|| AppError::Internal("无法计算相对路径".to_string()))
}

/// 确保目录存在
pub fn ensure_dir_exists(path: &Path) -> AppResult<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)
            .map_err(|e| AppError::Io(format!("创建目录失败: {}", e)))?;
    }
    Ok(())
}

/// 获取文件扩展名
pub fn get_file_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
}

/// 更改文件扩展名
pub fn change_extension(path: &Path, new_ext: &str) -> PathBuf {
    let mut new_path = path.to_path_buf();
    new_path.set_extension(new_ext);
    new_path
}

/// 生成唯一文件名
pub fn generate_unique_filename(dir: &Path, base_name: &str, extension: &str) -> PathBuf {
    let mut counter = 0;
    let mut filename = format!("{}.{}", base_name, extension);
    let mut full_path = dir.join(&filename);
    
    while full_path.exists() {
        counter += 1;
        filename = format!("{}_{}.{}", base_name, counter, extension);
        full_path = dir.join(&filename);
    }
    
    full_path
}