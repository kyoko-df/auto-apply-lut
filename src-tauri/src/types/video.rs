//! 视频相关类型定义

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use chrono::{DateTime, Utc};

/// 视频文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    /// 文件路径
    pub path: PathBuf,
    /// 文件名
    pub filename: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 视频时长（秒）
    pub duration: Option<f64>,
    /// 视频宽度
    pub width: Option<u32>,
    /// 视频高度
    pub height: Option<u32>,
    /// 帧率
    pub fps: Option<f64>,
    /// 编码格式
    pub codec: Option<String>,
    /// 比特率
    pub bitrate: Option<u64>,
    /// 创建时间
    pub created_at: Option<DateTime<Utc>>,
    /// 修改时间
    pub modified_at: Option<DateTime<Utc>>,
}

/// 支持的视频格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VideoFormat {
    Mp4,
    Mov,
    Avi,
    Mkv,
    Webm,
    Flv,
    Wmv,
    M4v,
    Other(String),
    Unknown,
}

impl VideoFormat {
    /// 从文件扩展名获取视频格式
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "mp4" => VideoFormat::Mp4,
            "mov" => VideoFormat::Mov,
            "avi" => VideoFormat::Avi,
            "mkv" => VideoFormat::Mkv,
            "webm" => VideoFormat::Webm,
            "flv" => VideoFormat::Flv,
            "wmv" => VideoFormat::Wmv,
            "m4v" => VideoFormat::M4v,
            "" => VideoFormat::Unknown,
            other => VideoFormat::Other(other.to_string()),
        }
    }

    /// 获取格式的文件扩展名
    pub fn extension(&self) -> &str {
        match self {
            VideoFormat::Mp4 => "mp4",
            VideoFormat::Mov => "mov",
            VideoFormat::Avi => "avi",
            VideoFormat::Mkv => "mkv",
            VideoFormat::Webm => "webm",
            VideoFormat::Flv => "flv",
            VideoFormat::Wmv => "wmv",
            VideoFormat::M4v => "m4v",
            VideoFormat::Other(ext) => ext,
            VideoFormat::Unknown => "unknown",
        }
    }

    /// 检查是否为支持的格式
    pub fn is_supported(&self) -> bool {
        !matches!(self, VideoFormat::Other(_) | VideoFormat::Unknown)
    }
}

/// 视频质量设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoQuality {
    /// 视频编码器
    pub codec: String,
    /// 比特率（kbps）
    pub bitrate: Option<u32>,
    /// 质量因子（0-51，越小质量越高）
    pub crf: Option<u8>,
    /// 预设（ultrafast, superfast, veryfast, faster, fast, medium, slow, slower, veryslow）
    pub preset: Option<String>,
    /// 是否保持原始质量
    pub keep_original: bool,
}

impl Default for VideoQuality {
    fn default() -> Self {
        Self {
            codec: "libx264".to_string(),
            bitrate: None,
            crf: Some(23),
            preset: Some("medium".to_string()),
            keep_original: true,
        }
    }
}

/// 视频处理选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoProcessOptions {
    /// 输出格式
    pub output_format: VideoFormat,
    /// 质量设置
    pub quality: VideoQuality,
    /// 输出目录
    pub output_dir: Option<PathBuf>,
    /// 文件名后缀
    pub filename_suffix: Option<String>,
    /// 是否覆盖已存在的文件
    pub overwrite: bool,
    /// 是否使用GPU加速
    pub use_gpu: bool,
}

impl Default for VideoProcessOptions {
    fn default() -> Self {
        Self {
            output_format: VideoFormat::Mp4,
            quality: VideoQuality::default(),
            output_dir: None,
            filename_suffix: Some("_lut".to_string()),
            overwrite: false,
            use_gpu: false,
        }
    }
}