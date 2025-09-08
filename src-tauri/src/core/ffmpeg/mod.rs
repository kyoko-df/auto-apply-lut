//! FFmpeg集成模块
//! 提供视频处理、LUT应用和格式转换功能

use crate::types::{AppResult, AppError};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use tokio::process::Command as AsyncCommand;
use tokio::io::{AsyncBufReadExt, BufReader};
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod processor;
pub mod encoder;
pub mod decoder;
pub mod filters;
pub mod utils;

/// 对外导出：发现打包或系统中的 ffmpeg/ffprobe 路径
pub fn discover_ffmpeg_path() -> AppResult<PathBuf> {
    FFmpegManager::find_ffmpeg_executable()
}

pub fn discover_ffprobe_path() -> AppResult<PathBuf> {
    FFmpegManager::find_ffprobe_executable()
}

/// FFmpeg管理器
pub struct FFmpegManager {
    /// FFmpeg可执行文件路径
    ffmpeg_path: PathBuf,
    /// FFprobe可执行文件路径
    ffprobe_path: PathBuf,
    /// 默认编码设置
    default_settings: EncodingSettings,
    /// 进度回调
    progress_callbacks: Arc<Mutex<Vec<Box<dyn Fn(f64) + Send + Sync>>>>,
}

impl FFmpegManager {
    /// 创建新的FFmpeg管理器
    pub fn new() -> AppResult<Self> {
        let ffmpeg_path = Self::find_ffmpeg_executable()?;
        let ffprobe_path = Self::find_ffprobe_executable()?;
        
        Ok(Self {
            ffmpeg_path,
            ffprobe_path,
            default_settings: EncodingSettings::default(),
            progress_callbacks: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// 使用指定路径创建FFmpeg管理器
    pub fn with_paths(ffmpeg_path: PathBuf, ffprobe_path: PathBuf) -> AppResult<Self> {
        // 验证可执行文件存在
        if !ffmpeg_path.exists() {
            return Err(AppError::FFmpeg(format!("FFmpeg not found at: {:?}", ffmpeg_path)));
        }
        if !ffprobe_path.exists() {
            return Err(AppError::FFmpeg(format!("FFprobe not found at: {:?}", ffprobe_path)));
        }
        
        Ok(Self {
            ffmpeg_path,
            ffprobe_path,
            default_settings: EncodingSettings::default(),
            progress_callbacks: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// 查找FFmpeg可执行文件
    fn find_ffmpeg_executable() -> AppResult<PathBuf> {
        // 0) 环境变量优先
        if let Ok(p) = std::env::var("FFMPEG_PATH") {
            let pb = PathBuf::from(&p);
            if pb.exists() || Self::check_command_works(&p) { return Ok(pb); }
        }

        // 1) 打包的资源目录
        if let Some(pb) = Self::find_packaged_tool("ffmpeg") { return Ok(pb); }

        // 2) 常见安装路径
        #[cfg(target_os = "windows")]
        let common_paths: &[&str] = &[
            "C\\\\ffmpeg\\\\bin\\\\ffmpeg.exe",
            "C\\\\Program Files\\\\ffmpeg\\\\bin\\\\ffmpeg.exe",
            "ffmpeg",
        ];
        #[cfg(not(target_os = "windows"))]
        let common_paths: &[&str] = &[
            "/usr/local/bin/ffmpeg",
            "/usr/bin/ffmpeg",
            "/opt/homebrew/bin/ffmpeg",
            "ffmpeg",
        ];

        for p in common_paths {
            let pb = PathBuf::from(p);
            if pb.is_absolute() {
                if pb.exists() { return Ok(pb); }
            } else if Self::check_command_works(p) {
                return Ok(pb);
            }
        }
        
        Err(AppError::FFmpeg("FFmpeg executable not found".to_string()))
    }

    /// 查找FFprobe可执行文件
    fn find_ffprobe_executable() -> AppResult<PathBuf> {
        // 0) 环境变量优先
        if let Ok(p) = std::env::var("FFPROBE_PATH") {
            let pb = PathBuf::from(&p);
            if pb.exists() || Self::check_command_works(&p) { return Ok(pb); }
        }

        // 1) 打包的资源目录
        if let Some(pb) = Self::find_packaged_tool("ffprobe") { return Ok(pb); }

        // 2) 常见安装路径
        #[cfg(target_os = "windows")]
        let common_paths: &[&str] = &[
            "C\\\\ffmpeg\\\\bin\\\\ffprobe.exe",
            "C\\\\Program Files\\\\ffmpeg\\\\bin\\\\ffprobe.exe",
            "ffprobe",
        ];
        #[cfg(not(target_os = "windows"))]
        let common_paths: &[&str] = &[
            "/usr/local/bin/ffprobe",
            "/usr/bin/ffprobe",
            "/opt/homebrew/bin/ffprobe",
            "ffprobe",
        ];

        for p in common_paths {
            let pb = PathBuf::from(p);
            if pb.is_absolute() {
                if pb.exists() { return Ok(pb); }
            } else if Self::check_command_works(p) {
                return Ok(pb);
            }
        }

        Err(AppError::FFmpeg("FFprobe executable not found".to_string()))
    }

    /// 检查命令在系统上是否可调用（-version 成功退出）
    fn check_command_works(command: &str) -> bool {
        Command::new(command)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// 在打包资源目录查找工具（Windows/Mac/Linux）
    fn find_packaged_tool(tool: &str) -> Option<PathBuf> {
        let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();

        #[cfg(target_os = "windows")]
        let candidates = vec![
            exe_dir.join("resources").join("bin").join("windows").join(format!("{}.exe", tool)),
            exe_dir.join("bin").join("windows").join(format!("{}.exe", tool)),
            exe_dir.join(format!("{}.exe", tool)),
        ];

        #[cfg(target_os = "macos")]
        let candidates = {
            // app.app/Contents/MacOS/<exe>
            let resources = exe_dir.join("../../Resources");
            let resources = resources.canonicalize().unwrap_or(resources);
            vec![
                resources.join("bin").join("macos").join(tool),
                resources.join(tool),
                exe_dir.join(tool),
            ]
        };

        #[cfg(target_os = "linux")]
        let candidates = vec![
            exe_dir.join("resources").join("bin").join("linux").join(tool),
            exe_dir.join("bin").join("linux").join(tool),
            exe_dir.join(tool),
        ];

        for c in candidates {
            if c.exists() {
                // 再次确认可执行
                let s = if cfg!(target_os = "windows") { c.to_string_lossy().to_string() } else { c.to_string_lossy().to_string() };
                if Self::check_command_works(&s) { return Some(c); }
            }
        }
        None
    }

    /// 获取视频信息
    pub async fn get_video_info(&self, input_path: &Path) -> AppResult<VideoInfo> {
        let output = AsyncCommand::new(&self.ffprobe_path)
            .args([
                "-v", "quiet",
                "-print_format", "json",
                "-show_format",
                "-show_streams",
                input_path.to_str().unwrap(),
            ])
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(e.to_string()))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::FFmpeg(format!("FFprobe failed: {}", error)));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let mut probe: ProbeResult = serde_json::from_str(&json_str)
            .map_err(|e| AppError::FFmpeg(format!("Failed to parse FFprobe output: {}", e)))?;

        // 将 streams 转换为内部结构
        let streams: Vec<StreamInfo> = probe.streams.drain(..).map(Into::into).collect();
        let (duration, bitrate, format, video_codec, audio_codec, fps, width, height) = if let Some(fmt) = probe.format {
            let duration = fmt.duration.parse::<f64>().unwrap_or(0.0);
            let bitrate = fmt.bit_rate.parse::<u64>().unwrap_or(0);
            let format = fmt.format_name;
            let mut video_codec = String::from("unknown");
            let mut audio_codec = None;
            let mut fps = 0.0;
            let mut width = 0;
            let mut height = 0;
            for s in &streams {
                if s.codec_type == "video" {
                    video_codec = s.codec_name.clone();
                    fps = s.fps.unwrap_or(0.0);
                    width = s.width.unwrap_or(0);
                    height = s.height.unwrap_or(0);
                } else if s.codec_type == "audio" {
                    audio_codec = Some(s.codec_name.clone());
                }
            }
            (duration, bitrate, format, video_codec, audio_codec, fps, width, height)
        } else {
            (0.0, 0, String::from("unknown"), String::from("unknown"), None, 0.0, 0, 0)
        };

        Ok(VideoInfo {
            duration,
            width,
            height,
            fps,
            video_codec,
            audio_codec,
            bitrate,
            format,
            streams,
        })
    }

    // ... existing code ...
}

/// FFprobe结果
#[derive(Debug, Deserialize)]
struct ProbeResult {
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

/// FFprobe流信息
#[derive(Debug, Deserialize)]
struct ProbeStream {
    index: u32,
    codec_type: String,
    codec_name: String,
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: String,
    duration: Option<String>,
}

/// FFprobe格式信息
#[derive(Debug, Deserialize)]
struct ProbeFormat {
    format_name: String,
    duration: String,
    bit_rate: String,
}

/// 批处理任务
#[derive(Debug, Clone)]
pub struct BatchTask {
    pub id: String,
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub lut_path: PathBuf,
}

/// 批处理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub task_id: String,
    pub success: bool,
    pub error: Option<String>,
    pub output_path: Option<PathBuf>,
}

/// 图像格式
#[derive(Debug, Clone, Copy)]
pub enum ImageFormat {
    Png,
    Jpg,
    Bmp,
    Tiff,
}

impl ImageFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpg => "jpg",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Tiff => "tiff",
        }
    }
}

/// 编解码器信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecInfo {
    pub name: String,
    pub description: String,
    pub codec_type: CodecType,
    pub can_encode: bool,
    pub can_decode: bool,
}

/// 编解码器类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodecType {
    Video,
    Audio,
    Subtitle,
    Data,
}

/// 格式信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatInfo {
    pub name: String,
    pub description: String,
    pub can_mux: bool,
    pub can_demux: bool,
}

/// FFmpeg信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FFmpegInfo {
    pub ffmpeg_path: PathBuf,
    pub ffprobe_path: PathBuf,
    pub ffmpeg_version: String,
    pub ffprobe_version: String,
    pub available: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_ffmpeg_manager_creation() {
        // 这个测试可能会失败，如果系统没有安装FFmpeg
        match FFmpegManager::new() {
            Ok(manager) => {
                assert!(!manager.ffmpeg_path.as_os_str().is_empty());
                assert!(!manager.ffprobe_path.as_os_str().is_empty());
            }
            Err(_) => {
                // FFmpeg未安装，跳过测试
                println!("FFmpeg not installed, skipping test");
            }
        }
    }

    #[test]
    fn test_encoding_settings_default() {
        let settings = EncodingSettings::default();
        assert_eq!(settings.video_codec, "libx264");
        assert_eq!(settings.audio_codec, "aac");
        assert_eq!(settings.preset, "medium");
        assert_eq!(settings.crf, 23);
    }

    #[test]
    fn test_resolution() {
        let resolution = Resolution {
            width: 1920,
            height: 1080,
        };
        assert_eq!(resolution.width, 1920);
        assert_eq!(resolution.height, 1080);
    }

    #[test]
    fn test_image_format_extension() {
        assert_eq!(ImageFormat::Png.extension(), "png");
        assert_eq!(ImageFormat::Jpg.extension(), "jpg");
        assert_eq!(ImageFormat::Bmp.extension(), "bmp");
        assert_eq!(ImageFormat::Tiff.extension(), "tiff");
    }

    #[test]
    fn test_batch_task() {
        let task = BatchTask {
            id: "test_task".to_string(),
            input_path: PathBuf::from("/input/video.mp4"),
            output_path: PathBuf::from("/output/video.mp4"),
            lut_path: PathBuf::from("/luts/test.cube"),
        };
        
        assert_eq!(task.id, "test_task");
        assert_eq!(task.input_path, PathBuf::from("/input/video.mp4"));
    }

    #[test]
    fn test_batch_result() {
        let result = BatchResult {
            task_id: "test_task".to_string(),
            success: true,
            error: None,
            output_path: Some(PathBuf::from("/output/video.mp4")),
        };
        
        assert!(result.success);
        assert!(result.error.is_none());
        assert!(result.output_path.is_some());
    }

    #[test]
    fn test_codec_info() {
        let codec = CodecInfo {
            name: "libx264".to_string(),
            description: "H.264 encoder".to_string(),
            codec_type: CodecType::Video,
            can_encode: true,
            can_decode: false,
        };
        
        assert_eq!(codec.name, "libx264");
        assert!(codec.can_encode);
        assert!(!codec.can_decode);
    }

    #[test]
    fn test_format_info() {
        let format = FormatInfo {
            name: "mp4".to_string(),
            description: "MP4 format".to_string(),
            can_mux: true,
            can_demux: true,
        };
        
        assert_eq!(format.name, "mp4");
        assert!(format.can_mux);
        assert!(format.can_demux);
    }
}

/// 分辨率
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

/// 编码设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingSettings {
    /// 视频编码器，例如 libx264、libx265、copy
    pub video_codec: String,
    /// 音频编码器，例如 aac、copy
    pub audio_codec: String,
    /// 预设，例如 ultrafast、medium、slow
    pub preset: String,
    /// 质量因子（CRF），当未指定码率时使用
    pub crf: i32,
    /// 目标分辨率，None 表示保持原分辨率
    pub resolution: Option<Resolution>,
    /// 目标帧率
    pub fps: Option<f64>,
    /// 目标码率（如 "2M"），与 crf 互斥，优先使用码率
    pub bitrate: Option<String>,
    /// 额外参数，形如（"-movflags" => "+faststart"）
    pub extra_params: std::collections::HashMap<String, String>,
}

impl Default for EncodingSettings {
    fn default() -> Self {
        Self {
            video_codec: "libx264".to_string(),
            audio_codec: "aac".to_string(),
            preset: "medium".to_string(),
            crf: 23,
            resolution: None,
            fps: None,
            bitrate: None,
            extra_params: std::collections::HashMap::new(),
        }
    }
}

/// 媒体流信息（统一对外结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    pub index: u32,
    pub codec_type: String,  // video / audio / subtitle / data
    pub codec_name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub duration: Option<f64>,
}

/// 视频信息（FFprobe侧重的技术信息）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub video_codec: String,
    pub audio_codec: Option<String>,
    pub bitrate: u64,
    pub format: String,
    pub streams: Vec<StreamInfo>,
}

/// 解析帧率字符串，如 "30000/1001" 或 "30"
fn parse_frame_rate_str(s: &str) -> Option<f64> {
    if let Some((num, den)) = s.split_once('/') {
        let n: f64 = num.trim().parse().ok()?;
        let d: f64 = den.trim().parse().ok()?;
        if d != 0.0 { Some(n / d) } else { None }
    } else {
        s.trim().parse::<f64>().ok()
    }
}

impl From<ProbeStream> for StreamInfo {
    fn from(ps: ProbeStream) -> Self {
        let fps = parse_frame_rate_str(&ps.r_frame_rate);
        let duration = ps.duration.as_ref().and_then(|d| d.parse::<f64>().ok());
        StreamInfo {
            index: ps.index,
            codec_type: ps.codec_type,
            codec_name: ps.codec_name,
            width: ps.width,
            height: ps.height,
            fps,
            duration,
        }
    }
}