//! 文件元数据提取模块
//! 提供文件元数据提取和分析功能

use crate::types::{AppError, AppResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 扩展的文件元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedFileMetadata {
    /// 基本文件信息
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub accessed_at: Option<DateTime<Utc>>,
    pub is_directory: bool,
    pub is_readonly: bool,
    pub is_hidden: bool,

    /// 文件类型信息
    pub extension: Option<String>,
    pub mime_type: Option<String>,
    pub file_type: FileType,

    /// 权限信息
    pub permissions: FilePermissions,

    /// 哈希值（可选）
    pub hash: Option<FileHash>,

    /// 自定义属性
    pub custom_attributes: HashMap<String, String>,
}

/// 文件类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileType {
    Video,
    Lut,
    Image,
    Audio,
    Document,
    Archive,
    Executable,
    Directory,
    Unknown,
}

/// 文件权限信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePermissions {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub mode: Option<u32>, // Unix权限模式
}

/// 文件哈希信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHash {
    pub algorithm: HashAlgorithm,
    pub value: String,
}

/// 哈希算法枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HashAlgorithm {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

/// 元数据提取选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataOptions {
    /// 是否计算文件哈希
    pub calculate_hash: bool,
    /// 哈希算法
    pub hash_algorithm: HashAlgorithm,
    /// 是否提取自定义属性
    pub extract_custom_attributes: bool,
    /// 是否包含访问时间
    pub include_access_time: bool,
}

impl Default for MetadataOptions {
    fn default() -> Self {
        Self {
            calculate_hash: false,
            hash_algorithm: HashAlgorithm::Sha256,
            extract_custom_attributes: false,
            include_access_time: false,
        }
    }
}

/// 元数据提取器
#[derive(Debug)]
pub struct MetadataExtractor {
    video_extensions: Vec<String>,
    lut_extensions: Vec<String>,
    image_extensions: Vec<String>,
    audio_extensions: Vec<String>,
    document_extensions: Vec<String>,
    archive_extensions: Vec<String>,
    executable_extensions: Vec<String>,
}

impl Default for MetadataExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataExtractor {
    /// 创建新的元数据提取器
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
            image_extensions: vec![
                "jpg".to_string(),
                "jpeg".to_string(),
                "png".to_string(),
                "gif".to_string(),
                "bmp".to_string(),
                "tiff".to_string(),
                "webp".to_string(),
                "svg".to_string(),
                "ico".to_string(),
                "raw".to_string(),
                "cr2".to_string(),
                "nef".to_string(),
            ],
            audio_extensions: vec![
                "mp3".to_string(),
                "wav".to_string(),
                "flac".to_string(),
                "aac".to_string(),
                "ogg".to_string(),
                "wma".to_string(),
                "m4a".to_string(),
                "opus".to_string(),
            ],
            document_extensions: vec![
                "pdf".to_string(),
                "doc".to_string(),
                "docx".to_string(),
                "xls".to_string(),
                "xlsx".to_string(),
                "ppt".to_string(),
                "pptx".to_string(),
                "txt".to_string(),
                "rtf".to_string(),
                "odt".to_string(),
                "ods".to_string(),
                "odp".to_string(),
            ],
            archive_extensions: vec![
                "zip".to_string(),
                "rar".to_string(),
                "7z".to_string(),
                "tar".to_string(),
                "gz".to_string(),
                "bz2".to_string(),
                "xz".to_string(),
                "dmg".to_string(),
                "iso".to_string(),
            ],
            executable_extensions: vec![
                "exe".to_string(),
                "msi".to_string(),
                "app".to_string(),
                "deb".to_string(),
                "rpm".to_string(),
                "pkg".to_string(),
                "run".to_string(),
                "bin".to_string(),
            ],
        }
    }

    /// 提取文件元数据
    pub async fn extract_metadata<P: AsRef<Path>>(
        &self,
        path: P,
        options: MetadataOptions,
    ) -> AppResult<ExtendedFileMetadata> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(AppError::Io(format!(
                "File does not exist: {}",
                path.display()
            )));
        }

        let metadata = fs::metadata(path).map_err(|e| {
            AppError::Io(format!(
                "Failed to get metadata for {}: {}",
                path.display(),
                e
            ))
        })?;

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_lowercase());

        let file_type = self.determine_file_type(&extension, metadata.is_dir());
        let mime_type = self.get_mime_type(&extension, &file_type);
        let permissions = self.extract_permissions(&metadata);
        let is_hidden = self.is_hidden_file(path);

        let created_at = metadata
            .created()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(|_| Utc::now());

        let modified_at = metadata
            .modified()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(|_| Utc::now());

        let accessed_at = if options.include_access_time {
            metadata.accessed().map(DateTime::<Utc>::from).ok()
        } else {
            None
        };

        let hash = if options.calculate_hash && metadata.is_file() {
            Some(
                self.calculate_file_hash(path, &options.hash_algorithm)
                    .await?,
            )
        } else {
            None
        };

        let custom_attributes = if options.extract_custom_attributes {
            self.extract_custom_attributes(path, &file_type).await?
        } else {
            HashMap::new()
        };

        Ok(ExtendedFileMetadata {
            path: path.to_path_buf(),
            name,
            size: metadata.len(),
            created_at,
            modified_at,
            accessed_at,
            is_directory: metadata.is_dir(),
            is_readonly: metadata.permissions().readonly(),
            is_hidden,
            extension,
            mime_type,
            file_type,
            permissions,
            hash,
            custom_attributes,
        })
    }

    /// 批量提取元数据
    pub async fn extract_metadata_batch<P: AsRef<Path>>(
        &self,
        paths: Vec<P>,
        options: MetadataOptions,
    ) -> Vec<AppResult<ExtendedFileMetadata>> {
        let mut results = Vec::new();

        for path in paths {
            let result = self.extract_metadata(path, options.clone()).await;
            results.push(result);
        }

        results
    }

    /// 确定文件类型
    fn determine_file_type(&self, extension: &Option<String>, is_dir: bool) -> FileType {
        if is_dir {
            return FileType::Directory;
        }

        if let Some(ext) = extension {
            let ext_lower = ext.to_lowercase();

            if self.video_extensions.contains(&ext_lower) {
                return FileType::Video;
            }
            if self.lut_extensions.contains(&ext_lower) {
                return FileType::Lut;
            }
            if self.image_extensions.contains(&ext_lower) {
                return FileType::Image;
            }
            if self.audio_extensions.contains(&ext_lower) {
                return FileType::Audio;
            }
            if self.document_extensions.contains(&ext_lower) {
                return FileType::Document;
            }
            if self.archive_extensions.contains(&ext_lower) {
                return FileType::Archive;
            }
            if self.executable_extensions.contains(&ext_lower) {
                return FileType::Executable;
            }
        }

        FileType::Unknown
    }

    /// 获取MIME类型
    fn get_mime_type(&self, extension: &Option<String>, file_type: &FileType) -> Option<String> {
        match file_type {
            FileType::Video => match extension.as_ref()?.as_str() {
                "mp4" => Some("video/mp4".to_string()),
                "avi" => Some("video/x-msvideo".to_string()),
                "mov" => Some("video/quicktime".to_string()),
                "mkv" => Some("video/x-matroska".to_string()),
                "webm" => Some("video/webm".to_string()),
                _ => Some("video/*".to_string()),
            },
            FileType::Image => match extension.as_ref()?.as_str() {
                "jpg" | "jpeg" => Some("image/jpeg".to_string()),
                "png" => Some("image/png".to_string()),
                "gif" => Some("image/gif".to_string()),
                "webp" => Some("image/webp".to_string()),
                "svg" => Some("image/svg+xml".to_string()),
                _ => Some("image/*".to_string()),
            },
            FileType::Audio => match extension.as_ref()?.as_str() {
                "mp3" => Some("audio/mpeg".to_string()),
                "wav" => Some("audio/wav".to_string()),
                "flac" => Some("audio/flac".to_string()),
                "ogg" => Some("audio/ogg".to_string()),
                _ => Some("audio/*".to_string()),
            },
            FileType::Document => match extension.as_ref()?.as_str() {
                "pdf" => Some("application/pdf".to_string()),
                "doc" => Some("application/msword".to_string()),
                "docx" => Some(
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                        .to_string(),
                ),
                "txt" => Some("text/plain".to_string()),
                _ => Some("application/*".to_string()),
            },
            FileType::Lut => Some("application/x-lut".to_string()),
            FileType::Directory => Some("inode/directory".to_string()),
            _ => None,
        }
    }

    /// 提取文件权限信息
    fn extract_permissions(&self, metadata: &fs::Metadata) -> FilePermissions {
        let permissions = metadata.permissions();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = permissions.mode();

            FilePermissions {
                readable: mode & 0o400 != 0,
                writable: mode & 0o200 != 0,
                executable: mode & 0o100 != 0,
                mode: Some(mode),
            }
        }

        #[cfg(not(unix))]
        {
            FilePermissions {
                readable: true, // 假设可读
                writable: !permissions.readonly(),
                executable: false, // Windows上难以确定
                mode: None,
            }
        }
    }

    /// 检查是否为隐藏文件
    fn is_hidden_file(&self, path: &Path) -> bool {
        #[cfg(unix)]
        {
            if let Some(name) = path.file_name() {
                if let Some(name_str) = name.to_str() {
                    return name_str.starts_with('.');
                }
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if let Ok(metadata) = fs::metadata(path) {
                const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
                return metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0;
            }
        }

        false
    }

    /// 计算文件哈希
    async fn calculate_file_hash(
        &self,
        path: &Path,
        algorithm: &HashAlgorithm,
    ) -> AppResult<FileHash> {
        use tokio::fs::File;
        use tokio::io::AsyncReadExt;

        let mut file = File::open(path)
            .await
            .map_err(|e| AppError::Io(format!("Failed to open file for hashing: {}", e)))?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .await
            .map_err(|e| AppError::Io(format!("Failed to read file for hashing: {}", e)))?;

        let hash_value = match algorithm {
            HashAlgorithm::Md5 => {
                use md5::{Digest, Md5};
                let mut hasher = Md5::new();
                hasher.update(&buffer);
                format!("{:x}", hasher.finalize())
            }
            HashAlgorithm::Sha1 => {
                use sha1::{Digest, Sha1};
                let mut hasher = Sha1::new();
                hasher.update(&buffer);
                format!("{:x}", hasher.finalize())
            }
            HashAlgorithm::Sha256 => {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(&buffer);
                format!("{:x}", hasher.finalize())
            }
            HashAlgorithm::Sha512 => {
                use sha2::{Digest, Sha512};
                let mut hasher = Sha512::new();
                hasher.update(&buffer);
                format!("{:x}", hasher.finalize())
            }
        };

        Ok(FileHash {
            algorithm: algorithm.clone(),
            value: hash_value,
        })
    }

    /// 提取自定义属性
    async fn extract_custom_attributes(
        &self,
        path: &Path,
        file_type: &FileType,
    ) -> AppResult<HashMap<String, String>> {
        let mut attributes = HashMap::new();

        // 根据文件类型提取特定属性
        match file_type {
            FileType::Video => {
                // 可以在这里添加视频特定的元数据提取
                attributes.insert("category".to_string(), "video".to_string());
            }
            FileType::Lut => {
                // 可以在这里添加LUT特定的元数据提取
                attributes.insert("category".to_string(), "lut".to_string());
            }
            FileType::Image => {
                attributes.insert("category".to_string(), "image".to_string());
            }
            _ => {}
        }

        // 添加路径相关信息
        if let Some(parent) = path.parent() {
            attributes.insert("parent_directory".to_string(), parent.display().to_string());
        }

        Ok(attributes)
    }

    /// 比较两个文件的元数据
    pub fn compare_metadata(
        &self,
        metadata1: &ExtendedFileMetadata,
        metadata2: &ExtendedFileMetadata,
    ) -> MetadataComparison {
        MetadataComparison {
            same_size: metadata1.size == metadata2.size,
            same_modified_time: metadata1.modified_at == metadata2.modified_at,
            same_hash: match (&metadata1.hash, &metadata2.hash) {
                (Some(h1), Some(h2)) => h1.value == h2.value,
                _ => false,
            },
            same_type: metadata1.file_type == metadata2.file_type,
            size_difference: metadata1.size.abs_diff(metadata2.size),
        }
    }
}

/// 元数据比较结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataComparison {
    pub same_size: bool,
    pub same_modified_time: bool,
    pub same_hash: bool,
    pub same_type: bool,
    pub size_difference: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_metadata_extraction() {
        let extractor = MetadataExtractor::new();
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.mp4");

        // 创建测试文件
        File::create(&test_file).unwrap();

        let options = MetadataOptions::default();
        let metadata = extractor
            .extract_metadata(&test_file, options)
            .await
            .unwrap();

        assert_eq!(metadata.name, "test.mp4");
        assert_eq!(metadata.file_type, FileType::Video);
        assert_eq!(metadata.extension, Some("mp4".to_string()));
        assert!(!metadata.is_directory);
    }

    #[tokio::test]
    async fn test_file_type_detection() {
        let extractor = MetadataExtractor::new();

        assert_eq!(
            extractor.determine_file_type(&Some("mp4".to_string()), false),
            FileType::Video
        );
        assert_eq!(
            extractor.determine_file_type(&Some("cube".to_string()), false),
            FileType::Lut
        );
        assert_eq!(
            extractor.determine_file_type(&Some("jpg".to_string()), false),
            FileType::Image
        );
        assert_eq!(
            extractor.determine_file_type(&None, true),
            FileType::Directory
        );
        assert_eq!(
            extractor.determine_file_type(&Some("unknown".to_string()), false),
            FileType::Unknown
        );
    }

    #[tokio::test]
    async fn test_hash_calculation() {
        let extractor = MetadataExtractor::new();
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.txt");

        // 创建测试文件并写入内容
        let mut file = tokio::fs::File::create(&test_file).await.unwrap();
        file.write_all(b"Hello, World!").await.unwrap();
        file.flush().await.unwrap();

        let options = MetadataOptions {
            calculate_hash: true,
            hash_algorithm: HashAlgorithm::Sha256,
            ..Default::default()
        };

        let metadata = extractor
            .extract_metadata(&test_file, options)
            .await
            .unwrap();

        assert!(metadata.hash.is_some());
        let hash = metadata.hash.unwrap();
        assert!(matches!(hash.algorithm, HashAlgorithm::Sha256));
        assert!(!hash.value.is_empty());
    }

    #[tokio::test]
    async fn test_batch_extraction() {
        let extractor = MetadataExtractor::new();
        let temp_dir = tempdir().unwrap();

        let file1 = temp_dir.path().join("video.mp4");
        let file2 = temp_dir.path().join("lut.cube");

        File::create(&file1).unwrap();
        File::create(&file2).unwrap();

        let options = MetadataOptions::default();
        let results = extractor
            .extract_metadata_batch(vec![&file1, &file2], options)
            .await;

        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());

        let metadata1 = results[0].as_ref().unwrap();
        let metadata2 = results[1].as_ref().unwrap();

        assert_eq!(metadata1.file_type, FileType::Video);
        assert_eq!(metadata2.file_type, FileType::Lut);
    }

    #[test]
    fn test_metadata_comparison() {
        let extractor = MetadataExtractor::new();

        let metadata1 = ExtendedFileMetadata {
            path: PathBuf::from("test1.mp4"),
            name: "test1.mp4".to_string(),
            size: 1024,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            accessed_at: None,
            is_directory: false,
            is_readonly: false,
            is_hidden: false,
            extension: Some("mp4".to_string()),
            mime_type: Some("video/mp4".to_string()),
            file_type: FileType::Video,
            permissions: FilePermissions {
                readable: true,
                writable: true,
                executable: false,
                mode: None,
            },
            hash: Some(FileHash {
                algorithm: HashAlgorithm::Sha256,
                value: "abc123".to_string(),
            }),
            custom_attributes: HashMap::new(),
        };

        let metadata2 = ExtendedFileMetadata {
            size: 2048,
            hash: Some(FileHash {
                algorithm: HashAlgorithm::Sha256,
                value: "def456".to_string(),
            }),
            ..metadata1.clone()
        };

        let comparison = extractor.compare_metadata(&metadata1, &metadata2);

        assert!(!comparison.same_size);
        assert!(!comparison.same_hash);
        assert!(comparison.same_type);
        assert_eq!(comparison.size_difference, 1024);
    }
}
