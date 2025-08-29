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
#[derive(Debug)]
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

        if unit_index == 0 {
            format!("{} {}", size as u64, UNITS[unit_index])
        } else {
            format!("{:.2} {}", size, UNITS[unit_index])
        }
    }

    /// 根据扩展名获取MIME类型
    fn get_mime_type(&self, extension: &Option<String>) -> Option<String> {
        match extension.as_ref()?.as_str() {
            // 视频文件
            "mp4" => Some("video/mp4".to_string()),
            "avi" => Some("video/x-msvideo".to_string()),
            "mov" => Some("video/quicktime".to_string()),
            "mkv" => Some("video/x-matroska".to_string()),
            "wmv" => Some("video/x-ms-wmv".to_string()),
            "flv" => Some("video/x-flv".to_string()),
            "webm" => Some("video/webm".to_string()),
            "m4v" => Some("video/x-m4v".to_string()),
            "3gp" => Some("video/3gpp".to_string()),
            "ts" => Some("video/mp2t".to_string()),
            "mts" | "m2ts" => Some("video/mp2t".to_string()),
            
            // LUT文件
            "cube" => Some("application/x-cube-lut".to_string()),
            "3dl" => Some("application/x-3dl-lut".to_string()),
            "lut" => Some("application/x-lut".to_string()),
            "csp" => Some("application/x-csp-lut".to_string()),
            "mga" => Some("application/x-mga-lut".to_string()),
            "m3d" => Some("application/x-m3d-lut".to_string()),
            
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
        assert!(!manager.get_video_extensions().is_empty());
        assert!(!manager.get_lut_extensions().is_empty());
    }

    #[test]
    fn test_video_file_detection() {
        let manager = FileManager::new();
        assert!(manager.is_video_file("test.mp4"));
        assert!(manager.is_video_file("test.MP4"));
        assert!(!manager.is_video_file("test.txt"));
    }

    #[test]
    fn test_lut_file_detection() {
        let manager = FileManager::new();
        assert!(manager.is_lut_file("test.cube"));
        assert!(manager.is_lut_file("test.CUBE"));
        assert!(!manager.is_lut_file("test.txt"));
    }

    #[test]
    fn test_file_size_formatting() {
        assert_eq!(FileManager::format_file_size(512), "512 B");
        assert_eq!(FileManager::format_file_size(1024), "1.00 KB");
        assert_eq!(FileManager::format_file_size(1536), "1.50 KB");
        assert_eq!(FileManager::format_file_size(1048576), "1.00 MB");
    }

    #[test]
    fn test_extension_management() {
        let mut manager = FileManager::new();
        let initial_count = manager.get_video_extensions().len();
        
        manager.add_video_extension("test".to_string());
        assert_eq!(manager.get_video_extensions().len(), initial_count + 1);
        
        manager.remove_video_extension("test");
        assert_eq!(manager.get_video_extensions().len(), initial_count);
    }

    #[tokio::test]
    async fn test_file_operations() {
        let manager = FileManager::new();
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        
        // 创建测试文件
        File::create(&test_file).unwrap();
        
        // 测试文件信息获取
        let file_info = manager.get_file_info(&test_file).unwrap();
        assert_eq!(file_info.name, "test.txt");
        assert!(!file_info.is_directory);
        assert_eq!(file_info.extension, Some("txt".to_string()));
        
        // 测试路径检查
        assert!(manager.path_exists(&test_file));
        assert!(manager.is_file(&test_file));
        assert!(!manager.is_directory(&test_file));
    }
}