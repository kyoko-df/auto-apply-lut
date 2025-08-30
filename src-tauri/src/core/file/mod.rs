//! 文件管理模块
//! 提供文件和目录的操作功能，包括扫描、监控、元数据提取等

use crate::types::{AppResult, AppError};
use std::path::{Path, PathBuf};
use std::fs;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod scanner;
pub mod watcher;
pub mod metadata;
pub mod utils;

/// 文件信息结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub is_directory: bool,
    pub extension: Option<String>,
    pub mime_type: Option<String>,
}

/// 文件管理器
#[derive(Debug, Clone)]
pub struct FileManager {
    /// 支持的视频文件扩展名
    video_extensions: Vec<String>,
    /// 支持的LUT文件扩展名
    lut_extensions: Vec<String>,
}

impl Default for FileManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FileManager {
    /// 创建新的文件管理器实例
    pub fn new() -> Self {
        Self {
            video_extensions: vec![
                "mp4".to_string(),
                "avi".to_string(),
                "mov".to_string(),
                "mkv".to_string(),
                "wmv".to_string(),
                "flv".to_string(),
                "webm".to_string(),
                "m4v".to_string(),
                "3gp".to_string(),
                "ts".to_string(),
                "mts".to_string(),
                "m2ts".to_string(),
            ],
            lut_extensions: vec![
                "cube".to_string(),
                "3dl".to_string(),
                "lut".to_string(),
                "csp".to_string(),
                "mga".to_string(),
                "m3d".to_string(),
            ],
        }
    }

    /// 获取文件信息
    pub fn get_file_info<P: AsRef<Path>>(&self, path: P) -> AppResult<FileInfo> {
        let path = path.as_ref();
        let metadata = fs::metadata(path)
            .map_err(|e| AppError::Io(format!("Failed to get metadata for {}: {}", path.display(), e)))?;

        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let extension = path.extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_lowercase());

        let mime_type = self.get_mime_type(&extension);

        let created_at = metadata.created()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(|_| Utc::now());

        let modified_at = metadata.modified()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(|_| Utc::now());

        Ok(FileInfo {
            path: path.to_path_buf(),
            name,
            size: metadata.len(),
            created_at,
            modified_at,
            is_directory: metadata.is_dir(),
            extension,
            mime_type,
        })
    }

    /// 非递归列出目录内容（文件与子目录）
    pub fn list_directory<P: AsRef<Path>>(&self, path: P) -> AppResult<Vec<FileInfo>> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(AppError::Io(format!("Directory does not exist: {}", path.display())));
        }
        if !path.is_dir() {
            return Err(AppError::Io(format!("Path is not a directory: {}", path.display())));
        }

        let mut items = Vec::new();
        let entries = fs::read_dir(path)
            .map_err(|e| AppError::Io(format!("Failed to read directory {}: {}", path.display(), e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| AppError::Io(format!("Failed to read directory entry: {}", e)))?;
            let entry_path = entry.path();
            match self.get_file_info(&entry_path) {
                Ok(info) => items.push(info),
                Err(e) => {
                    // 忽略单个条目的错误，继续处理其它项
                    tracing::warn!("list_directory: failed to get info for {}: {}", entry_path.display(), e);
                }
            }
        }

        Ok(items)
    }

    /// 检查文件是否为视频文件
    pub fn is_video_file<P: AsRef<Path>>(&self, path: P) -> bool {
        if let Some(ext) = path.as_ref().extension() {
            if let Some(ext_str) = ext.to_str() {
                return self.video_extensions.contains(&ext_str.to_lowercase());
            }
        }
        false
    }

    /// 检查文件是否为LUT文件
    pub fn is_lut_file<P: AsRef<Path>>(&self, path: P) -> bool {
        if let Some(ext) = path.as_ref().extension() {
            if let Some(ext_str) = ext.to_str() {
                return self.lut_extensions.contains(&ext_str.to_lowercase());
            }
        }
        false
    }

    /// 获取支持的视频文件扩展名
    pub fn get_video_extensions(&self) -> &[String] {
        &self.video_extensions
    }

    /// 获取支持的LUT文件扩展名
    pub fn get_lut_extensions(&self) -> &[String] {
        &self.lut_extensions
    }

    /// 添加视频文件扩展名
    pub fn add_video_extension(&mut self, extension: String) {
        let ext = extension.to_lowercase();
        if !self.video_extensions.contains(&ext) {
            self.video_extensions.push(ext);
        }
    }

    /// 添加LUT文件扩展名
    pub fn add_lut_extension(&mut self, extension: String) {
        let ext = extension.to_lowercase();
        if !self.lut_extensions.contains(&ext) {
            self.lut_extensions.push(ext);
        }
    }

    /// 移除视频文件扩展名
    pub fn remove_video_extension(&mut self, extension: &str) {
        let ext = extension.to_lowercase();
        self.video_extensions.retain(|e| e != &ext);
    }

    /// 移除LUT文件扩展名
    pub fn remove_lut_extension(&mut self, extension: &str) {
        let ext = extension.to_lowercase();
        self.lut_extensions.retain(|e| e != &ext);
    }

    /// 检查路径是否存在
    pub fn path_exists<P: AsRef<Path>>(&self, path: P) -> bool {
        path.as_ref().exists()
    }

    /// 检查路径是否为目录
    pub fn is_directory<P: AsRef<Path>>(&self, path: P) -> bool {
        path.as_ref().is_dir()
    }

    /// 检查路径是否为文件
    pub fn is_file<P: AsRef<Path>>(&self, path: P) -> bool {
        path.as_ref().is_file()
    }

    /// 创建目录
    pub fn create_directory<P: AsRef<Path>>(&self, path: P) -> AppResult<()> {
        fs::create_dir_all(path.as_ref())
            .map_err(|e| AppError::Io(format!("Failed to create directory {}: {}", path.as_ref().display(), e)))
    }

    /// 删除文件
    pub fn delete_file<P: AsRef<Path>>(&self, path: P) -> AppResult<()> {
        fs::remove_file(path.as_ref())
            .map_err(|e| AppError::Io(format!("Failed to delete file {}: {}", path.as_ref().display(), e)))
    }

    /// 删除目录
    pub fn delete_directory<P: AsRef<Path>>(&self, path: P) -> AppResult<()> {
        fs::remove_dir_all(path.as_ref())
            .map_err(|e| AppError::Io(format!("Failed to delete directory {}: {}", path.as_ref().display(), e)))
    }

    /// 复制文件
    pub fn copy_file<P: AsRef<Path>>(&self, from: P, to: P) -> AppResult<u64> {
        fs::copy(from.as_ref(), to.as_ref())
            .map_err(|e| AppError::Io(format!(
                "Failed to copy file from {} to {}: {}",
                from.as_ref().display(),
                to.as_ref().display(),
                e
            )))
    }

    /// 移动文件
    pub fn move_file<P: AsRef<Path>>(&self, from: P, to: P) -> AppResult<()> {
        fs::rename(from.as_ref(), to.as_ref())
            .map_err(|e| AppError::Io(format!(
                "Failed to move file from {} to {}: {}",
                from.as_ref().display(),
                to.as_ref().display(),
                e
            )))
    }

    /// 获取目录大小
    pub fn get_directory_size<P: AsRef<Path>>(&self, path: P) -> AppResult<u64> {
        let mut total_size = 0;
        let entries = fs::read_dir(path.as_ref())
            .map_err(|e| AppError::Io(format!("Failed to read directory {}: {}", path.as_ref().display(), e)))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| AppError::Io(format!("Failed to read directory entry: {}", e)))?;
            let metadata = entry.metadata()
                .map_err(|e| AppError::Io(format!("Failed to get metadata: {}", e)))?;

            if metadata.is_file() {
                total_size += metadata.len();
            } else if metadata.is_dir() {
                total_size += self.get_directory_size(entry.path())?;
            }
        }

        Ok(total_size)
    }

    /// 格式化文件大小
    pub fn format_file_size(size: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = size as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        format!("{:.2} {}", size, UNITS[unit_index])
    }

    fn get_mime_type(&self, extension: &Option<String>) -> Option<String> {
        match extension.as_deref() {
            Some("mp4") => Some("video/mp4".to_string()),
            Some("mov") => Some("video/quicktime".to_string()),
            Some("avi") => Some("video/x-msvideo".to_string()),
            Some("mkv") => Some("video/x-matroska".to_string()),
            Some("wmv") => Some("video/x-ms-wmv".to_string()),
            Some("flv") => Some("video/x-flv".to_string()),
            Some("webm") => Some("video/webm".to_string()),
            Some("m4v") => Some("video/x-m4v".to_string()),
            Some("cube") => Some("application/x-cube-lut".to_string()),
            Some("3dl") => Some("application/x-3dl-lut".to_string()),
            Some("lut") => Some("application/x-lut".to_string()),
            Some("csp") => Some("application/x-csp-lut".to_string()),
            Some("mga") => Some("application/x-mga-lut".to_string()),
            Some("m3d") => Some("application/x-m3d-lut".to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_file_manager_creation() {
        let manager = FileManager::new();
        assert!(!manager.video_extensions.is_empty());
        assert!(!manager.lut_extensions.is_empty());
    }

    #[test]
    fn test_video_file_detection() {
        let manager = FileManager::new();
        assert!(manager.is_video_file("video.mp4"));
        assert!(!manager.is_video_file("document.txt"));
    }

    #[test]
    fn test_lut_file_detection() {
        let manager = FileManager::new();
        assert!(manager.is_lut_file("style.cube"));
        assert!(!manager.is_lut_file("image.jpg"));
    }

    #[test]
    fn test_file_size_formatting() {
        let formatted = FileManager::format_file_size(2048);
        assert!(formatted.contains("KB"));
    }

    #[test]
    fn test_extension_management() {
        let mut manager = FileManager::new();
        manager.add_video_extension("test".to_string());
        assert!(manager.is_video_file("file.test"));
        manager.remove_video_extension("test");
        assert!(!manager.is_video_file("file.test"));
    }

    #[tokio::test]
    async fn test_file_operations() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let manager = FileManager::new();
        assert!(manager.path_exists(&file_path));
        assert!(manager.is_file(&file_path));

        let info = manager.get_file_info(&file_path).unwrap();
        assert_eq!(info.name, "test.txt");

        let copy_path = dir.path().join("copy.txt");
        manager.copy_file(&file_path, &copy_path).unwrap();
        assert!(copy_path.exists());

        manager.delete_file(&copy_path).unwrap();
        assert!(!copy_path.exists());
    }
}