//! 视频处理核心模块
//! 提供视频文件的分析、处理和转换功能

use crate::types::{AppResult, VideoInfo, VideoFormat};
use std::path::Path;
use tokio::process::Command;
use serde_json::Value;
use chrono::{DateTime, Utc};

pub mod metadata;

pub use metadata::VideoMetadata;

/// 视频处理管理器
#[derive(Debug)]
pub struct VideoManager {
    /// FFmpeg路径
    ffmpeg_path: String,
    /// FFprobe路径
    ffprobe_path: String,
}

impl VideoManager {
    /// 创建新的视频管理器
    pub fn new() -> AppResult<Self> {
        let ffmpeg_path = Self::find_ffmpeg_path()?;
        let ffprobe_path = Self::find_ffprobe_path()?;
        
        Ok(Self {
            ffmpeg_path,
            ffprobe_path,
        })
    }

    /// 使用自定义路径创建视频管理器
    pub fn with_paths(ffmpeg_path: String, ffprobe_path: String) -> Self {
        Self {
            ffmpeg_path,
            ffprobe_path,
        }
    }

    /// 获取视频信息
    pub async fn get_video_info<P: AsRef<Path>>(&self, path: P) -> AppResult<VideoInfo> {
        let path = path.as_ref();
        
        // 检查文件是否存在
        if !path.exists() {
            return Err(crate::types::AppError::FileSystem(
                format!("Video file not found: {}", path.display())
            ));
        }

        // 获取文件基本信息
        let metadata = tokio::fs::metadata(path).await
            .map_err(|e| crate::types::AppError::FileSystem(e.to_string()))?;
        
        let file_size = metadata.len();
        let created_at = metadata.created()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());
        let modified_at = metadata.modified()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());

        // 使用FFprobe获取视频元数据
        let video_metadata = self.probe_video_metadata(path).await?;
        
        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let format = path.extension()
            .and_then(|ext| ext.to_str())
            .map(VideoFormat::from_extension)
            .unwrap_or(VideoFormat::Other("unknown".to_string()));

        Ok(VideoInfo {
            path: path.to_path_buf(),
            filename: file_name,
            size: file_size,
            duration: video_metadata.duration,
            width: video_metadata.width,
            height: video_metadata.height,
            fps: video_metadata.frame_rate,
            codec: video_metadata.codec,
            bitrate: video_metadata.bitrate,
            created_at: Some(created_at),
            modified_at: Some(modified_at),
        })
    }

    /// 检查视频文件是否有效
    pub async fn is_valid_video<P: AsRef<Path>>(&self, path: P) -> bool {
        match self.get_video_info(path).await {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// 获取支持的视频格式列表
    pub fn get_supported_formats() -> Vec<VideoFormat> {
        vec![
            VideoFormat::Mp4,
            VideoFormat::Mov,
            VideoFormat::Avi,
            VideoFormat::Mkv,
            VideoFormat::Wmv,
            VideoFormat::Flv,
            VideoFormat::Webm,
        ]
    }

    /// 检查格式是否支持
    pub fn is_format_supported(format: &VideoFormat) -> bool {
        !matches!(format, VideoFormat::Unknown)
    }

    /// 获取FFmpeg路径
    pub fn get_ffmpeg_path(&self) -> &str {
        &self.ffmpeg_path
    }

    /// 获取FFprobe路径
    pub fn get_ffprobe_path(&self) -> &str {
        &self.ffprobe_path
    }

    /// 查找FFmpeg可执行文件路径
    fn find_ffmpeg_path() -> AppResult<String> {
        // 首先尝试从环境变量获取
        if let Ok(path) = std::env::var("FFMPEG_PATH") {
            return Ok(path);
        }

        // 尝试常见的安装路径
        let common_paths = [
            "ffmpeg",
            "/usr/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
            "/opt/homebrew/bin/ffmpeg",
            "C:\\ffmpeg\\bin\\ffmpeg.exe",
            "C:\\Program Files\\ffmpeg\\bin\\ffmpeg.exe",
        ];

        for path in &common_paths {
            if Self::is_executable_available(path) {
                return Ok(path.to_string());
            }
        }

        Err(crate::types::AppError::Configuration(
            "FFmpeg not found. Please install FFmpeg or set FFMPEG_PATH environment variable".to_string()
        ))
    }

    /// 查找FFprobe可执行文件路径
    fn find_ffprobe_path() -> AppResult<String> {
        // 首先尝试从环境变量获取
        if let Ok(path) = std::env::var("FFPROBE_PATH") {
            return Ok(path);
        }

        // 尝试常见的安装路径
        let common_paths = [
            "ffprobe",
            "/usr/bin/ffprobe",
            "/usr/local/bin/ffprobe",
            "/opt/homebrew/bin/ffprobe",
            "C:\\ffmpeg\\bin\\ffprobe.exe",
            "C:\\Program Files\\ffmpeg\\bin\\ffprobe.exe",
        ];

        for path in &common_paths {
            if Self::is_executable_available(path) {
                return Ok(path.to_string());
            }
        }

        Err(crate::types::AppError::Configuration(
            "FFprobe not found. Please install FFmpeg or set FFPROBE_PATH environment variable".to_string()
        ))
    }

    /// 检查可执行文件是否可用
    fn is_executable_available(path: &str) -> bool {
        std::process::Command::new(path)
            .arg("-version")
            .output()
            .is_ok()
    }

    /// 使用FFprobe获取视频元数据
    async fn probe_video_metadata<P: AsRef<Path>>(&self, path: P) -> AppResult<VideoMetadata> {
        let output = Command::new(&self.ffprobe_path)
            .args([
                "-v", "quiet",
                "-print_format", "json",
                "-show_format",
                "-show_streams",
                path.as_ref().to_str().unwrap(),
            ])
            .output()
            .await
            .map_err(|e| crate::types::AppError::FFmpeg(e.to_string()))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(crate::types::AppError::FFmpeg(
                format!("FFprobe failed: {}", error)
            ));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let json: Value = serde_json::from_str(&json_str)
            .map_err(|e| crate::types::AppError::FFmpeg(
                format!("Failed to parse FFprobe output: {}", e)
            ))?;

        VideoMetadata::from_ffprobe_json(&json)
    }
}

impl Default for VideoManager {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            ffmpeg_path: "ffmpeg".to_string(),
            ffprobe_path: "ffprobe".to_string(),
        })
    }
}