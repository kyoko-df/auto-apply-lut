//! 文件扫描器模块
//! 提供目录扫描功能，支持递归扫描和文件过滤

use crate::types::{AppResult, AppError};
use crate::core::file::{FileInfo, FileManager};
use std::path::{Path, PathBuf};
use std::fs;
use serde::{Deserialize, Serialize};
use tokio::task;

/// 扫描选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanOptions {
    /// 是否递归扫描子目录
    pub recursive: bool,
    /// 最大扫描深度（仅在递归模式下有效）
    pub max_depth: Option<usize>,
    /// 是否包含隐藏文件
    pub include_hidden: bool,
    /// 是否只扫描视频文件
    pub video_only: bool,
    /// 是否只扫描LUT文件
    pub lut_only: bool,
    /// 文件大小过滤器（最小和最大字节数）
    pub size_filter: Option<(u64, u64)>,
    /// 自定义文件扩展名过滤器
    pub extension_filter: Option<Vec<String>>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            max_depth: None,
            include_hidden: false,
            video_only: false,
            lut_only: false,
            size_filter: None,
            extension_filter: None,
        }
    }
}

/// 扫描结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    /// 扫描的根目录
    pub root_path: PathBuf,
    /// 找到的文件列表
    pub files: Vec<FileInfo>,
    /// 扫描的目录数量
    pub directories_scanned: usize,
    /// 总文件数量
    pub total_files: usize,
    /// 视频文件数量
    pub video_files: usize,
    /// LUT文件数量
    pub lut_files: usize,
    /// 扫描耗时（毫秒）
    pub scan_duration_ms: u64,
    /// 错误列表
    pub errors: Vec<String>,
}

/// 文件扫描器
#[derive(Debug)]
pub struct FileScanner {
    file_manager: FileManager,
}

impl Default for FileScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl FileScanner {
    /// 创建新的文件扫描器
    pub fn new() -> Self {
        Self {
            file_manager: FileManager::new(),
        }
    }

    /// 使用自定义文件管理器创建扫描器
    pub fn with_file_manager(file_manager: FileManager) -> Self {
        Self { file_manager }
    }

    /// 扫描目录
    pub async fn scan_directory<P: AsRef<Path>>(
        &self,
        path: P,
        options: ScanOptions,
    ) -> AppResult<ScanResult> {
        let start_time = std::time::Instant::now();
        let root_path = path.as_ref().to_path_buf();

        if !self.file_manager.path_exists(&root_path) {
            return Err(AppError::Io(format!(
                "Directory does not exist: {}",
                root_path.display()
            )));
        }

        if !self.file_manager.is_directory(&root_path) {
            return Err(AppError::Io(format!(
                "Path is not a directory: {}",
                root_path.display()
            )));
        }

        let mut result = ScanResult {
            root_path: root_path.clone(),
            files: Vec::new(),
            directories_scanned: 0,
            total_files: 0,
            video_files: 0,
            lut_files: 0,
            scan_duration_ms: 0,
            errors: Vec::new(),
        };

        // 执行扫描
        self.scan_directory_recursive(&root_path, &options, 0, &mut result)
            .await?;

        // 计算扫描耗时
        result.scan_duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(result)
    }

    /// 递归扫描目录
    #[async_recursion::async_recursion]
    async fn scan_directory_recursive(
        &self,
        path: &Path,
        options: &ScanOptions,
        current_depth: usize,
        result: &mut ScanResult,
    ) -> AppResult<()> {
        // 检查深度限制
        if let Some(max_depth) = options.max_depth {
            if current_depth > max_depth {
                return Ok(());
            }
        }

        result.directories_scanned += 1;

        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(e) => {
                result.errors.push(format!(
                    "Failed to read directory {}: {}",
                    path.display(),
                    e
                ));
                return Ok(());
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    result.errors.push(format!("Failed to read directory entry: {}", e));
                    continue;
                }
            };

            let entry_path = entry.path();

            // 跳过隐藏文件（如果不包含隐藏文件）
            if !options.include_hidden && self.is_hidden_file(&entry_path) {
                continue;
            }

            if entry_path.is_dir() {
                // 递归扫描子目录
                if options.recursive {
                    if let Err(e) = self
                        .scan_directory_recursive(&entry_path, options, current_depth + 1, result)
                        .await
                    {
                        result.errors.push(format!(
                            "Failed to scan subdirectory {}: {}",
                            entry_path.display(),
                            e
                        ));
                    }
                }
            } else if entry_path.is_file() {
                // 处理文件
                if let Err(e) = self.process_file(&entry_path, options, result).await {
                    result.errors.push(format!(
                        "Failed to process file {}: {}",
                        entry_path.display(),
                        e
                    ));
                }
            }
        }

        Ok(())
    }

    /// 处理单个文件
    async fn process_file(
        &self,
        path: &Path,
        options: &ScanOptions,
        result: &mut ScanResult,
    ) -> AppResult<()> {
        result.total_files += 1;

        // 应用文件过滤器
        if !self.should_include_file(path, options) {
            return Ok(());
        }

        // 获取文件信息
        let file_info = match self.file_manager.get_file_info(path) {
            Ok(info) => info,
            Err(e) => {
                result.errors.push(format!(
                    "Failed to get file info for {}: {}",
                    path.display(),
                    e
                ));
                return Ok(());
            }
        };

        // 应用大小过滤器
        if let Some((min_size, max_size)) = options.size_filter {
            if file_info.size < min_size || file_info.size > max_size {
                return Ok(());
            }
        }

        // 统计文件类型
        if self.file_manager.is_video_file(path) {
            result.video_files += 1;
        } else if self.file_manager.is_lut_file(path) {
            result.lut_files += 1;
        }

        result.files.push(file_info);
        Ok(())
    }

    /// 检查文件是否应该被包含
    fn should_include_file(&self, path: &Path, options: &ScanOptions) -> bool {
        // 检查文件类型过滤器
        if options.video_only && !self.file_manager.is_video_file(path) {
            return false;
        }

        if options.lut_only && !self.file_manager.is_lut_file(path) {
            return false;
        }

        // 检查扩展名过滤器
        if let Some(ref extensions) = options.extension_filter {
            if let Some(ext) = path.extension() {
                if let Some(ext_str) = ext.to_str() {
                    let ext_lower = ext_str.to_lowercase();
                    if !extensions.iter().any(|e| e.to_lowercase() == ext_lower) {
                        return false;
                    }
                } else {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    /// 检查文件是否为隐藏文件
    fn is_hidden_file(&self, path: &Path) -> bool {
        if let Some(name) = path.file_name() {
            if let Some(name_str) = name.to_str() {
                return name_str.starts_with('.');
            }
        }
        false
    }

    /// 快速扫描（只获取文件路径，不获取详细信息）
    pub async fn quick_scan<P: AsRef<Path>>(
        &self,
        path: P,
        options: ScanOptions,
    ) -> AppResult<Vec<PathBuf>> {
        let root_path = path.as_ref();
        let mut files = Vec::new();

        self.quick_scan_recursive(root_path, &options, 0, &mut files)
            .await?;

        Ok(files)
    }

    /// 递归快速扫描
    #[async_recursion::async_recursion]
    async fn quick_scan_recursive(
        &self,
        path: &Path,
        options: &ScanOptions,
        current_depth: usize,
        files: &mut Vec<PathBuf>,
    ) -> AppResult<()> {
        // 检查深度限制
        if let Some(max_depth) = options.max_depth {
            if current_depth > max_depth {
                return Ok(());
            }
        }

        let entries = fs::read_dir(path)
            .map_err(|e| AppError::Io(format!("Failed to read directory {}: {}", path.display(), e)))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| AppError::Io(format!("Failed to read directory entry: {}", e)))?;
            let entry_path = entry.path();

            // 跳过隐藏文件
            if !options.include_hidden && self.is_hidden_file(&entry_path) {
                continue;
            }

            if entry_path.is_dir() {
                if options.recursive {
                    self.quick_scan_recursive(&entry_path, options, current_depth + 1, files)
                        .await?;
                }
            } else if entry_path.is_file() && self.should_include_file(&entry_path, options) {
                files.push(entry_path);
            }
        }

        Ok(())
    }

    /// 获取目录统计信息
    pub async fn get_directory_stats<P: AsRef<Path>>(&self, path: P) -> AppResult<DirectoryStats> {
        let root_path = path.as_ref();
        let mut stats = DirectoryStats::default();

        self.collect_directory_stats(root_path, &mut stats).await?;

        Ok(stats)
    }

    /// 收集目录统计信息
    #[async_recursion::async_recursion]
    async fn collect_directory_stats(
        &self,
        path: &Path,
        stats: &mut DirectoryStats,
    ) -> AppResult<()> {
        let entries = fs::read_dir(path)
            .map_err(|e| AppError::Io(format!("Failed to read directory {}: {}", path.display(), e)))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| AppError::Io(format!("Failed to read directory entry: {}", e)))?;
            let entry_path = entry.path();

            if entry_path.is_dir() {
                stats.directories += 1;
                self.collect_directory_stats(&entry_path, stats).await?;
            } else if entry_path.is_file() {
                stats.files += 1;

                if let Ok(metadata) = fs::metadata(&entry_path) {
                    stats.total_size += metadata.len();
                }

                if self.file_manager.is_video_file(&entry_path) {
                    stats.video_files += 1;
                } else if self.file_manager.is_lut_file(&entry_path) {
                    stats.lut_files += 1;
                }
            }
        }

        Ok(())
    }
}

/// 目录统计信息
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DirectoryStats {
    pub files: usize,
    pub directories: usize,
    pub video_files: usize,
    pub lut_files: usize,
    pub total_size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_scan_empty_directory() {
        let scanner = FileScanner::new();
        let temp_dir = tempdir().unwrap();
        
        let options = ScanOptions::default();
        let result = scanner.scan_directory(temp_dir.path(), options).await.unwrap();
        
        assert_eq!(result.files.len(), 0);
        assert_eq!(result.directories_scanned, 1);
        assert_eq!(result.total_files, 0);
    }

    #[tokio::test]
    async fn test_scan_with_files() {
        let scanner = FileScanner::new();
        let temp_dir = tempdir().unwrap();
        
        // 创建测试文件
        File::create(temp_dir.path().join("video.mp4")).unwrap();
        File::create(temp_dir.path().join("lut.cube")).unwrap();
        File::create(temp_dir.path().join("other.txt")).unwrap();
        
        let options = ScanOptions::default();
        let result = scanner.scan_directory(temp_dir.path(), options).await.unwrap();
        
        assert_eq!(result.files.len(), 3);
        assert_eq!(result.video_files, 1);
        assert_eq!(result.lut_files, 1);
    }

    #[tokio::test]
    async fn test_video_only_filter() {
        let scanner = FileScanner::new();
        let temp_dir = tempdir().unwrap();
        
        File::create(temp_dir.path().join("video.mp4")).unwrap();
        File::create(temp_dir.path().join("lut.cube")).unwrap();
        
        let options = ScanOptions {
            video_only: true,
            ..Default::default()
        };
        let result = scanner.scan_directory(temp_dir.path(), options).await.unwrap();
        
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.video_files, 1);
        assert_eq!(result.lut_files, 0);
    }

    #[tokio::test]
    async fn test_extension_filter() {
        let scanner = FileScanner::new();
        let temp_dir = tempdir().unwrap();
        
        File::create(temp_dir.path().join("file1.mp4")).unwrap();
        File::create(temp_dir.path().join("file2.avi")).unwrap();
        File::create(temp_dir.path().join("file3.txt")).unwrap();
        
        let options = ScanOptions {
            extension_filter: Some(vec!["mp4".to_string(), "avi".to_string()]),
            ..Default::default()
        };
        let result = scanner.scan_directory(temp_dir.path(), options).await.unwrap();
        
        assert_eq!(result.files.len(), 2);
    }

    #[tokio::test]
    async fn test_directory_stats() {
        let scanner = FileScanner::new();
        let temp_dir = tempdir().unwrap();
        
        File::create(temp_dir.path().join("video.mp4")).unwrap();
        File::create(temp_dir.path().join("lut.cube")).unwrap();
        
        let stats = scanner.get_directory_stats(temp_dir.path()).await.unwrap();
        
        assert_eq!(stats.files, 2);
        assert_eq!(stats.video_files, 1);
        assert_eq!(stats.lut_files, 1);
    }
}