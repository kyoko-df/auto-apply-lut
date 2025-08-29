//! 文件工具模块
//! 提供常用的文件操作辅助函数

use crate::types::{AppResult, AppError};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::{self, Read, Write};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use chrono::{DateTime, Utc};
use regex::Regex;

/// 文件操作工具集
pub struct FileUtils;

impl FileUtils {
    /// 安全地创建文件名（移除非法字符）
    pub fn sanitize_filename(filename: &str) -> String {
        let illegal_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        let mut sanitized = filename.to_string();
        
        for char in illegal_chars {
            sanitized = sanitized.replace(char, "_");
        }
        
        // 移除前后空格和点
        sanitized = sanitized.trim().trim_matches('.').to_string();
        
        // 确保文件名不为空
        if sanitized.is_empty() {
            sanitized = "untitled".to_string();
        }
        
        // 限制文件名长度
        if sanitized.len() > 255 {
            sanitized.truncate(255);
        }
        
        sanitized
    }

    /// 生成唯一的文件名（如果文件已存在）
    pub fn generate_unique_filename<P: AsRef<Path>>(path: P) -> PathBuf {
        let path = path.as_ref();
        
        if !path.exists() {
            return path.to_path_buf();
        }
        
        let parent = path.parent().unwrap_or(Path::new(""));
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        
        let mut counter = 1;
        loop {
            let new_name = if extension.is_empty() {
                format!("{}_{}", stem, counter)
            } else {
                format!("{}_{}.{}", stem, counter, extension)
            };
            
            let new_path = parent.join(new_name);
            if !new_path.exists() {
                return new_path;
            }
            
            counter += 1;
            if counter > 9999 {
                // 防止无限循环
                break;
            }
        }
        
        // 如果仍然无法生成唯一名称，使用时间戳
        let timestamp = Utc::now().timestamp();
        let new_name = if extension.is_empty() {
            format!("{}_{}", stem, timestamp)
        } else {
            format!("{}_{}.{}", stem, timestamp, extension)
        };
        
        parent.join(new_name)
    }

    /// 递归复制目录
    pub fn copy_directory<P: AsRef<Path>>(from: P, to: P) -> AppResult<()> {
        let from = from.as_ref();
        let to = to.as_ref();
        
        if !from.exists() {
            return Err(AppError::Io(format!(
                "Source directory does not exist: {}",
                from.display()
            )));
        }
        
        if !from.is_dir() {
            return Err(AppError::Io(format!(
                "Source is not a directory: {}",
                from.display()
            )));
        }
        
        // 创建目标目录
        fs::create_dir_all(to)
            .map_err(|e| AppError::Io(format!("Failed to create directory {}: {}", to.display(), e)))?;
        
        // 递归复制内容
        let entries = fs::read_dir(from)
            .map_err(|e| AppError::Io(format!("Failed to read directory {}: {}", from.display(), e)))?;
        
        for entry in entries {
            let entry = entry
                .map_err(|e| AppError::Io(format!("Failed to read directory entry: {}", e)))?;
            let entry_path = entry.path();
            let file_name = entry.file_name();
            let dest_path = to.join(file_name);
            
            if entry_path.is_dir() {
                Self::copy_directory(&entry_path, &dest_path)?;
            } else {
                fs::copy(&entry_path, &dest_path)
                    .map_err(|e| AppError::Io(format!(
                        "Failed to copy file from {} to {}: {}",
                        entry_path.display(),
                        dest_path.display(),
                        e
                    )))?;
            }
        }
        
        Ok(())
    }

    /// 递归删除目录（安全版本，有确认机制）
    pub fn remove_directory_safe<P: AsRef<Path>>(path: P, confirm: bool) -> AppResult<()> {
        let path = path.as_ref();
        
        if !path.exists() {
            return Ok(()); // 目录不存在，认为删除成功
        }
        
        if !path.is_dir() {
            return Err(AppError::Io(format!(
                "Path is not a directory: {}",
                path.display()
            )));
        }
        
        if !confirm {
            return Err(AppError::Validation(
                "Directory deletion requires confirmation".to_string()
            ));
        }
        
        fs::remove_dir_all(path)
            .map_err(|e| AppError::Io(format!("Failed to remove directory {}: {}", path.display(), e)))
    }

    /// 计算目录中文件的总数
    pub fn count_files_in_directory<P: AsRef<Path>>(path: P, recursive: bool) -> AppResult<usize> {
        let path = path.as_ref();
        
        if !path.exists() || !path.is_dir() {
            return Ok(0);
        }
        
        let mut count = 0;
        let entries = fs::read_dir(path)
            .map_err(|e| AppError::Io(format!("Failed to read directory {}: {}", path.display(), e)))?;
        
        for entry in entries {
            let entry = entry
                .map_err(|e| AppError::Io(format!("Failed to read directory entry: {}", e)))?;
            let entry_path = entry.path();
            
            if entry_path.is_file() {
                count += 1;
            } else if entry_path.is_dir() && recursive {
                count += Self::count_files_in_directory(&entry_path, recursive)?;
            }
        }
        
        Ok(count)
    }

    /// 查找重复文件（基于大小和哈希）
    pub fn find_duplicate_files<P: AsRef<Path>>(
        paths: Vec<P>,
        use_hash: bool,
    ) -> AppResult<Vec<Vec<PathBuf>>> {
        use std::collections::HashMap;
        
        let mut size_groups: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        
        // 按文件大小分组
        for path in paths {
            let path = path.as_ref();
            if path.is_file() {
                if let Ok(metadata) = fs::metadata(path) {
                    let size = metadata.len();
                    size_groups.entry(size).or_default().push(path.to_path_buf());
                }
            }
        }
        
        let mut duplicates = Vec::new();
        
        for (_, files) in size_groups {
            if files.len() > 1 {
                if use_hash {
                    // 使用哈希进一步验证
                    let hash_groups = Self::group_files_by_hash(&files)?;
                    for (_, hash_files) in hash_groups {
                        if hash_files.len() > 1 {
                            duplicates.push(hash_files);
                        }
                    }
                } else {
                    // 仅基于大小
                    duplicates.push(files);
                }
            }
        }
        
        Ok(duplicates)
    }

    /// 按哈希值分组文件
    fn group_files_by_hash(files: &[PathBuf]) -> AppResult<std::collections::HashMap<String, Vec<PathBuf>>> {
        use std::collections::HashMap;
        use sha2::{Sha256, Digest};
        
        let mut hash_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
        
        for file in files {
            let hash = Self::calculate_file_hash_sync(file)?;
            hash_groups.entry(hash).or_default().push(file.clone());
        }
        
        Ok(hash_groups)
    }

    /// 同步计算文件哈希
    fn calculate_file_hash_sync<P: AsRef<Path>>(path: P) -> AppResult<String> {
        use sha2::{Sha256, Digest};
        
        let mut file = fs::File::open(path.as_ref())
            .map_err(|e| AppError::Io(format!("Failed to open file for hashing: {}", e)))?;
        
        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192];
        
        loop {
            let bytes_read = file.read(&mut buffer)
                .map_err(|e| AppError::Io(format!("Failed to read file for hashing: {}", e)))?;
            
            if bytes_read == 0 {
                break;
            }
            
            hasher.update(&buffer[..bytes_read]);
        }
        
        Ok(format!("{:x}", hasher.finalize()))
    }

    /// 清理临时文件
    pub fn cleanup_temp_files<P: AsRef<Path>>(directory: P, max_age_hours: u64) -> AppResult<usize> {
        let directory = directory.as_ref();
        let mut cleaned_count = 0;
        
        if !directory.exists() || !directory.is_dir() {
            return Ok(0);
        }
        
        let cutoff_time = Utc::now() - chrono::Duration::hours(max_age_hours as i64);
        let entries = fs::read_dir(directory)
            .map_err(|e| AppError::Io(format!("Failed to read directory {}: {}", directory.display(), e)))?;
        
        for entry in entries {
            let entry = entry
                .map_err(|e| AppError::Io(format!("Failed to read directory entry: {}", e)))?;
            let entry_path = entry.path();
            
            // 检查是否为临时文件
            if Self::is_temp_file(&entry_path) {
                if let Ok(metadata) = fs::metadata(&entry_path) {
                    if let Ok(modified) = metadata.modified() {
                        let modified_utc: DateTime<Utc> = modified.into();
                        if modified_utc < cutoff_time {
                            if entry_path.is_file() {
                                if fs::remove_file(&entry_path).is_ok() {
                                    cleaned_count += 1;
                                }
                            } else if entry_path.is_dir() {
                                if fs::remove_dir_all(&entry_path).is_ok() {
                                    cleaned_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(cleaned_count)
    }

    /// 检查是否为临时文件
    fn is_temp_file<P: AsRef<Path>>(path: P) -> bool {
        let path = path.as_ref();
        
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let name_lower = name.to_lowercase();
            
            // 检查临时文件模式
            name_lower.ends_with(".tmp") ||
            name_lower.ends_with(".temp") ||
            name_lower.starts_with("tmp_") ||
            name_lower.starts_with("temp_") ||
            name_lower.contains("~") ||
            name_lower.starts_with(".#")
        } else {
            false
        }
    }

    /// 验证文件路径安全性
    pub fn validate_path_security<P: AsRef<Path>>(path: P, allowed_base: P) -> AppResult<()> {
        let path = path.as_ref();
        let allowed_base = allowed_base.as_ref();
        
        // 规范化路径
        let canonical_path = path.canonicalize()
            .map_err(|e| AppError::Validation(format!("Invalid path: {}", e)))?;
        
        let canonical_base = allowed_base.canonicalize()
            .map_err(|e| AppError::Validation(format!("Invalid base path: {}", e)))?;
        
        // 检查路径是否在允许的基础目录内
        if !canonical_path.starts_with(&canonical_base) {
            return Err(AppError::Validation(
                "Path is outside allowed directory".to_string()
            ));
        }
        
        Ok(())
    }

    /// 创建备份文件
    pub fn create_backup<P: AsRef<Path>>(original_path: P) -> AppResult<PathBuf> {
        let original_path = original_path.as_ref();
        
        if !original_path.exists() {
            return Err(AppError::Io(format!(
                "Original file does not exist: {}",
                original_path.display()
            )));
        }
        
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_path = if let Some(extension) = original_path.extension() {
            original_path.with_extension(format!("{}.backup.{}", timestamp, extension.to_string_lossy()))
        } else {
            original_path.with_extension(format!("{}.backup", timestamp))
        };
        
        fs::copy(original_path, &backup_path)
            .map_err(|e| AppError::Io(format!(
                "Failed to create backup from {} to {}: {}",
                original_path.display(),
                backup_path.display(),
                e
            )))?;
        
        Ok(backup_path)
    }

    /// 恢复备份文件
    pub fn restore_backup<P: AsRef<Path>>(backup_path: P, target_path: P) -> AppResult<()> {
        let backup_path = backup_path.as_ref();
        let target_path = target_path.as_ref();
        
        if !backup_path.exists() {
            return Err(AppError::Io(format!(
                "Backup file does not exist: {}",
                backup_path.display()
            )));
        }
        
        fs::copy(backup_path, target_path)
            .map_err(|e| AppError::Io(format!(
                "Failed to restore backup from {} to {}: {}",
                backup_path.display(),
                target_path.display(),
                e
            )))?;
        
        Ok(())
    }

    /// 按模式搜索文件
    pub fn search_files_by_pattern<P: AsRef<Path>>(
        directory: P,
        pattern: &str,
        recursive: bool,
    ) -> AppResult<Vec<PathBuf>> {
        let directory = directory.as_ref();
        let regex = Regex::new(pattern)
            .map_err(|e| AppError::Validation(format!("Invalid regex pattern: {}", e)))?;
        
        let mut results = Vec::new();
        Self::search_files_recursive(directory, &regex, recursive, &mut results)?;
        
        Ok(results)
    }

    /// 递归搜索文件
    fn search_files_recursive(
        directory: &Path,
        regex: &Regex,
        recursive: bool,
        results: &mut Vec<PathBuf>,
    ) -> AppResult<()> {
        let entries = fs::read_dir(directory)
            .map_err(|e| AppError::Io(format!("Failed to read directory {}: {}", directory.display(), e)))?;
        
        for entry in entries {
            let entry = entry
                .map_err(|e| AppError::Io(format!("Failed to read directory entry: {}", e)))?;
            let entry_path = entry.path();
            
            if entry_path.is_file() {
                if let Some(file_name) = entry_path.file_name().and_then(|n| n.to_str()) {
                    if regex.is_match(file_name) {
                        results.push(entry_path);
                    }
                }
            } else if entry_path.is_dir() && recursive {
                Self::search_files_recursive(&entry_path, regex, recursive, results)?;
            }
        }
        
        Ok(())
    }

    /// 获取文件扩展名统计
    pub fn get_extension_stats<P: AsRef<Path>>(
        directory: P,
        recursive: bool,
    ) -> AppResult<std::collections::HashMap<String, usize>> {
        use std::collections::HashMap;
        
        let directory = directory.as_ref();
        let mut stats = HashMap::new();
        
        Self::collect_extension_stats(directory, recursive, &mut stats)?;
        
        Ok(stats)
    }

    /// 收集扩展名统计
    fn collect_extension_stats(
        directory: &Path,
        recursive: bool,
        stats: &mut std::collections::HashMap<String, usize>,
    ) -> AppResult<()> {
        let entries = fs::read_dir(directory)
            .map_err(|e| AppError::Io(format!("Failed to read directory {}: {}", directory.display(), e)))?;
        
        for entry in entries {
            let entry = entry
                .map_err(|e| AppError::Io(format!("Failed to read directory entry: {}", e)))?;
            let entry_path = entry.path();
            
            if entry_path.is_file() {
                let extension = entry_path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|s| s.to_lowercase())
                    .unwrap_or_else(|| "(no extension)".to_string());
                
                *stats.entry(extension).or_insert(0) += 1;
            } else if entry_path.is_dir() && recursive {
                Self::collect_extension_stats(&entry_path, recursive, stats)?;
            }
        }
        
        Ok(())
    }
}

/// 文件操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOperationResult {
    pub success: bool,
    pub message: String,
    pub files_processed: usize,
    pub errors: Vec<String>,
}

/// 批量文件操作
pub struct BatchFileOperations;

impl BatchFileOperations {
    /// 批量重命名文件
    pub fn batch_rename(
        files: Vec<PathBuf>,
        pattern: &str,
        replacement: &str,
    ) -> FileOperationResult {
        let mut processed = 0;
        let mut errors = Vec::new();
        
        let regex = match Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => {
                return FileOperationResult {
                    success: false,
                    message: format!("Invalid regex pattern: {}", e),
                    files_processed: 0,
                    errors: vec![e.to_string()],
                };
            }
        };
        
        for file in files {
            if let Some(file_name) = file.file_name().and_then(|n| n.to_str()) {
                let new_name = regex.replace_all(file_name, replacement);
                if new_name != file_name {
                    let new_path = file.with_file_name(new_name.as_ref());
                    match fs::rename(&file, &new_path) {
                        Ok(_) => processed += 1,
                        Err(e) => errors.push(format!(
                            "Failed to rename {} to {}: {}",
                            file.display(),
                            new_path.display(),
                            e
                        )),
                    }
                }
            }
        }
        
        FileOperationResult {
            success: errors.is_empty(),
            message: format!("Processed {} files", processed),
            files_processed: processed,
            errors,
        }
    }

    /// 批量移动文件
    pub fn batch_move(
        files: Vec<PathBuf>,
        destination: &Path,
    ) -> FileOperationResult {
        let mut processed = 0;
        let mut errors = Vec::new();
        
        // 确保目标目录存在
        if let Err(e) = fs::create_dir_all(destination) {
            return FileOperationResult {
                success: false,
                message: format!("Failed to create destination directory: {}", e),
                files_processed: 0,
                errors: vec![e.to_string()],
            };
        }
        
        for file in files {
            if let Some(file_name) = file.file_name() {
                let dest_path = destination.join(file_name);
                let dest_path = FileUtils::generate_unique_filename(dest_path);
                
                match fs::rename(&file, &dest_path) {
                    Ok(_) => processed += 1,
                    Err(e) => errors.push(format!(
                        "Failed to move {} to {}: {}",
                        file.display(),
                        dest_path.display(),
                        e
                    )),
                }
            }
        }
        
        FileOperationResult {
            success: errors.is_empty(),
            message: format!("Moved {} files", processed),
            files_processed: processed,
            errors,
        }
    }

    /// 批量删除文件
    pub fn batch_delete(files: Vec<PathBuf>, confirm: bool) -> FileOperationResult {
        if !confirm {
            return FileOperationResult {
                success: false,
                message: "Deletion requires confirmation".to_string(),
                files_processed: 0,
                errors: vec!["Confirmation required".to_string()],
            };
        }
        
        let mut processed = 0;
        let mut errors = Vec::new();
        
        for file in files {
            let result = if file.is_file() {
                fs::remove_file(&file)
            } else if file.is_dir() {
                fs::remove_dir_all(&file)
            } else {
                continue;
            };
            
            match result {
                Ok(_) => processed += 1,
                Err(e) => errors.push(format!(
                    "Failed to delete {}: {}",
                    file.display(),
                    e
                )),
            }
        }
        
        FileOperationResult {
            success: errors.is_empty(),
            message: format!("Deleted {} files", processed),
            files_processed: processed,
            errors,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(FileUtils::sanitize_filename("test<>file.txt"), "test__file.txt");
        assert_eq!(FileUtils::sanitize_filename("  .test.  "), "test");
        assert_eq!(FileUtils::sanitize_filename(""), "untitled");
        assert_eq!(FileUtils::sanitize_filename("normal_file.txt"), "normal_file.txt");
    }

    #[test]
    fn test_generate_unique_filename() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        
        // 文件不存在时应返回原路径
        assert_eq!(FileUtils::generate_unique_filename(&test_file), test_file);
        
        // 创建文件后应生成新名称
        File::create(&test_file).unwrap();
        let unique_path = FileUtils::generate_unique_filename(&test_file);
        assert_ne!(unique_path, test_file);
        assert!(unique_path.to_string_lossy().contains("test_1.txt"));
    }

    #[test]
    fn test_is_temp_file() {
        assert!(FileUtils::is_temp_file(Path::new("test.tmp")));
        assert!(FileUtils::is_temp_file(Path::new("test.temp")));
        assert!(FileUtils::is_temp_file(Path::new("tmp_test.txt")));
        assert!(FileUtils::is_temp_file(Path::new("temp_test.txt")));
        assert!(FileUtils::is_temp_file(Path::new("test~")));
        assert!(FileUtils::is_temp_file(Path::new(".#test")));
        assert!(!FileUtils::is_temp_file(Path::new("normal.txt")));
    }

    #[test]
    fn test_count_files_in_directory() {
        let temp_dir = tempdir().unwrap();
        
        // 创建测试文件
        File::create(temp_dir.path().join("file1.txt")).unwrap();
        File::create(temp_dir.path().join("file2.txt")).unwrap();
        
        // 创建子目录和文件
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        File::create(sub_dir.join("file3.txt")).unwrap();
        
        // 非递归计数
        let count = FileUtils::count_files_in_directory(temp_dir.path(), false).unwrap();
        assert_eq!(count, 2);
        
        // 递归计数
        let count = FileUtils::count_files_in_directory(temp_dir.path(), true).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_extension_stats() {
        let temp_dir = tempdir().unwrap();
        
        // 创建不同扩展名的文件
        File::create(temp_dir.path().join("file1.txt")).unwrap();
        File::create(temp_dir.path().join("file2.txt")).unwrap();
        File::create(temp_dir.path().join("file3.mp4")).unwrap();
        File::create(temp_dir.path().join("file4")).unwrap(); // 无扩展名
        
        let stats = FileUtils::get_extension_stats(temp_dir.path(), false).unwrap();
        
        assert_eq!(stats.get("txt"), Some(&2));
        assert_eq!(stats.get("mp4"), Some(&1));
        assert_eq!(stats.get("(no extension)"), Some(&1));
    }

    #[test]
    fn test_batch_rename() {
        let temp_dir = tempdir().unwrap();
        
        // 创建测试文件
        let file1 = temp_dir.path().join("test_001.txt");
        let file2 = temp_dir.path().join("test_002.txt");
        File::create(&file1).unwrap();
        File::create(&file2).unwrap();
        
        let files = vec![file1, file2];
        let result = BatchFileOperations::batch_rename(files, r"test_(\d+)", "file_$1");
        
        assert!(result.success);
        assert_eq!(result.files_processed, 2);
        
        // 检查文件是否被重命名
        assert!(temp_dir.path().join("file_001.txt").exists());
        assert!(temp_dir.path().join("file_002.txt").exists());
    }

    #[test]
    fn test_create_backup() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        
        // 创建测试文件
        fs::write(&test_file, "test content").unwrap();
        
        let backup_path = FileUtils::create_backup(&test_file).unwrap();
        
        assert!(backup_path.exists());
        assert!(backup_path.to_string_lossy().contains("backup"));
        
        // 验证备份内容
        let backup_content = fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, "test content");
    }
}