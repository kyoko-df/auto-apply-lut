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
        // 常见的FFmpeg安装路径
        let common_paths = vec![
            "/usr/local/bin/ffmpeg",
            "/usr/bin/ffmpeg",
            "/opt/homebrew/bin/ffmpeg",
            "ffmpeg", // 系统PATH中
        ];
        
        for path in common_paths {
            let path_buf = PathBuf::from(path);
            if path_buf.exists() || Self::check_command_exists("ffmpeg") {
                return Ok(path_buf);
            }
        }
        
        Err(AppError::FFmpeg("FFmpeg executable not found".to_string()))
    }

    /// 查找FFprobe可执行文件
    fn find_ffprobe_executable() -> AppResult<PathBuf> {
        let common_paths = vec![
            "/usr/local/bin/ffprobe",
            "/usr/bin/ffprobe",
            "/opt/homebrew/bin/ffprobe",
            "ffprobe",
        ];
        
        for path in common_paths {
            let path_buf = PathBuf::from(path);
            if path_buf.exists() || Self::check_command_exists("ffprobe") {
                return Ok(path_buf);
            }
        }
        
        Err(AppError::FFmpeg("FFprobe executable not found".to_string()))
    }

    /// 检查命令是否存在于系统PATH中
    fn check_command_exists(command: &str) -> bool {
        Command::new("which")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
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
            .map_err(|e| AppError::FFmpeg(format!("Failed to run ffprobe: {}", e)))?;
        
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::FFmpeg(format!("FFprobe failed: {}", error)));
        }
        
        let json_str = String::from_utf8_lossy(&output.stdout);
        let probe_result: ProbeResult = serde_json::from_str(&json_str)
            .map_err(|e| AppError::Serialization(format!("Failed to parse ffprobe output: {}", e)))?;
        
        VideoInfo::from_probe_result(probe_result)
    }

    /// 应用LUT到视频
    pub async fn apply_lut_to_video(
        &self,
        input_path: &Path,
        output_path: &Path,
        lut_path: &Path,
        settings: Option<EncodingSettings>,
    ) -> AppResult<()> {
        let settings = settings.unwrap_or_else(|| self.default_settings.clone());
        
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args([
            "-i", input_path.to_str().unwrap(),
            "-vf", &format!("lut3d={}", lut_path.to_str().unwrap()),
            "-c:v", &settings.video_codec,
            "-preset", &settings.preset,
            "-crf", &settings.crf.to_string(),
            "-c:a", &settings.audio_codec,
            "-y", // 覆盖输出文件
            output_path.to_str().unwrap(),
        ]);
        
        // 添加额外的编码参数
        for (key, value) in &settings.extra_params {
            cmd.args([key, value]);
        }
        
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::FFmpeg(format!("Failed to start ffmpeg: {}", e)))?;
        
        // 监控进度
        if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            
            while let Some(line) = lines.next_line().await.unwrap_or(None) {
                if let Some(progress) = self.parse_progress(&line) {
                    self.notify_progress(progress).await;
                }
            }
        }
        
        let status = child.wait().await
            .map_err(|e| AppError::FFmpeg(format!("FFmpeg process failed: {}", e)))?;
        
        if !status.success() {
            return Err(AppError::FFmpeg("FFmpeg encoding failed".to_string()));
        }
        
        Ok(())
    }

    /// 批量应用LUT
    pub async fn batch_apply_lut(
        &self,
        tasks: Vec<BatchTask>,
        settings: Option<EncodingSettings>,
    ) -> AppResult<Vec<BatchResult>> {
        let mut results = Vec::new();
        
        for task in tasks {
            let result = match self.apply_lut_to_video(
                &task.input_path,
                &task.output_path,
                &task.lut_path,
                settings.clone(),
            ).await {
                Ok(_) => BatchResult {
                    task_id: task.id,
                    success: true,
                    error: None,
                    output_path: Some(task.output_path),
                },
                Err(e) => BatchResult {
                    task_id: task.id,
                    success: false,
                    error: Some(e.to_string()),
                    output_path: None,
                },
            };
            
            results.push(result);
        }
        
        Ok(results)
    }

    /// 转换视频格式
    pub async fn convert_video(
        &self,
        input_path: &Path,
        output_path: &Path,
        settings: EncodingSettings,
    ) -> AppResult<()> {
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args([
            "-i", input_path.to_str().unwrap(),
            "-c:v", &settings.video_codec,
            "-preset", &settings.preset,
            "-crf", &settings.crf.to_string(),
            "-c:a", &settings.audio_codec,
        ]);
        
        // 添加分辨率设置
        if let Some(resolution) = &settings.resolution {
            cmd.args(["-s", &format!("{}x{}", resolution.width, resolution.height)]);
        }
        
        // 添加帧率设置
        if let Some(fps) = settings.fps {
            cmd.args(["-r", &fps.to_string()]);
        }
        
        // 添加额外参数
        for (key, value) in &settings.extra_params {
            cmd.args([key, value]);
        }
        
        cmd.args(["-y", output_path.to_str().unwrap()]);
        
        let status = cmd.status().await
            .map_err(|e| AppError::FFmpeg(format!("Failed to run ffmpeg: {}", e)))?;
        
        if !status.success() {
            return Err(AppError::FFmpeg("Video conversion failed".to_string()));
        }
        
        Ok(())
    }

    /// 提取视频帧
    pub async fn extract_frames(
        &self,
        input_path: &Path,
        output_dir: &Path,
        frame_rate: Option<f64>,
        format: ImageFormat,
    ) -> AppResult<Vec<PathBuf>> {
        // 确保输出目录存在
        tokio::fs::create_dir_all(output_dir).await
            .map_err(AppError::from)?;
        
        let output_pattern = output_dir.join(format!("frame_%04d.{}", format.extension()));
        
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args([
            "-i", input_path.to_str().unwrap(),
        ]);
        
        // 设置帧率
        if let Some(fps) = frame_rate {
            cmd.args(["-vf", &format!("fps={}", fps)]);
        }
        
        cmd.args([
            "-y",
            output_pattern.to_str().unwrap(),
        ]);
        
        let status = cmd.status().await
            .map_err(|e| AppError::FFmpeg(format!("Failed to extract frames: {}", e)))?;
        
        if !status.success() {
            return Err(AppError::FFmpeg("Frame extraction failed".to_string()));
        }
        
        // 收集生成的帧文件
        let mut frame_files = Vec::new();
        let mut entries = tokio::fs::read_dir(output_dir).await
            .map_err(AppError::from)?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(AppError::from)? {
            
            let path = entry.path();
            if path.is_file() && path.file_name().unwrap().to_str().unwrap().starts_with("frame_") {
                frame_files.push(path);
            }
        }
        
        frame_files.sort();
        Ok(frame_files)
    }

    /// 从帧创建视频
    pub async fn create_video_from_frames(
        &self,
        frame_dir: &Path,
        output_path: &Path,
        frame_rate: f64,
        settings: EncodingSettings,
    ) -> AppResult<()> {
        let input_pattern = frame_dir.join("frame_%04d.png");
        
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args([
            "-framerate", &frame_rate.to_string(),
            "-i", input_pattern.to_str().unwrap(),
            "-c:v", &settings.video_codec,
            "-preset", &settings.preset,
            "-crf", &settings.crf.to_string(),
            "-pix_fmt", "yuv420p",
            "-y",
            output_path.to_str().unwrap(),
        ]);
        
        let status = cmd.status().await
            .map_err(|e| AppError::FFmpeg(format!("Failed to create video: {}", e)))?;
        
        if !status.success() {
            return Err(AppError::FFmpeg("Video creation failed".to_string()));
        }
        
        Ok(())
    }

    /// 获取支持的编解码器
    pub async fn get_supported_codecs(&self) -> AppResult<Vec<CodecInfo>> {
        let output = AsyncCommand::new(&self.ffmpeg_path)
            .args(["-codecs"])
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(format!("Failed to get codecs: {}", e)))?;
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        let codecs = self.parse_codecs(&output_str)?;
        
        Ok(codecs)
    }

    /// 获取支持的格式
    pub async fn get_supported_formats(&self) -> AppResult<Vec<FormatInfo>> {
        let output = AsyncCommand::new(&self.ffmpeg_path)
            .args(["-formats"])
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(format!("Failed to get formats: {}", e)))?;
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        let formats = self.parse_formats(&output_str)?;
        
        Ok(formats)
    }

    /// 验证FFmpeg安装
    pub async fn validate_installation(&self) -> AppResult<FFmpegInfo> {
        // 检查FFmpeg版本
        let ffmpeg_output = AsyncCommand::new(&self.ffmpeg_path)
            .args(["-version"])
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(format!("Failed to check ffmpeg version: {}", e)))?;
        
        // 检查FFprobe版本
        let ffprobe_output = AsyncCommand::new(&self.ffprobe_path)
            .args(["-version"])
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(format!("Failed to check ffprobe version: {}", e)))?;
        
        let ffmpeg_version = self.parse_version(&String::from_utf8_lossy(&ffmpeg_output.stdout))?;
        let ffprobe_version = self.parse_version(&String::from_utf8_lossy(&ffprobe_output.stdout))?;
        
        Ok(FFmpegInfo {
            ffmpeg_path: self.ffmpeg_path.clone(),
            ffprobe_path: self.ffprobe_path.clone(),
            ffmpeg_version,
            ffprobe_version,
            available: true,
        })
    }

    /// 解析进度信息
    fn parse_progress(&self, line: &str) -> Option<f64> {
        // FFmpeg进度输出格式：frame=  123 fps= 25 q=28.0 size=    1024kB time=00:00:05.00 bitrate=1677.7kbits/s speed=1.02x
        if line.contains("time=") {
            // 简化的进度解析，实际实现需要更复杂的逻辑
            // 这里返回一个模拟的进度值
            Some(0.5) // 50%
        } else {
            None
        }
    }

    /// 通知进度
    async fn notify_progress(&self, progress: f64) {
        let callbacks = self.progress_callbacks.lock().await;
        for callback in callbacks.iter() {
            callback(progress);
        }
    }

    /// 添加进度回调
    pub async fn add_progress_callback<F>(&self, callback: F)
    where
        F: Fn(f64) + Send + Sync + 'static,
    {
        let mut callbacks = self.progress_callbacks.lock().await;
        callbacks.push(Box::new(callback));
    }

    /// 解析编解码器信息
    fn parse_codecs(&self, output: &str) -> AppResult<Vec<CodecInfo>> {
        let mut codecs = Vec::new();
        
        for line in output.lines() {
            if line.starts_with(" ") && line.len() > 10 {
                // 简化的解析逻辑
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    codecs.push(CodecInfo {
                        name: parts[1].to_string(),
                        description: parts[2..].join(" "),
                        codec_type: if line.contains("V") { CodecType::Video } else { CodecType::Audio },
                        can_encode: line.contains("E"),
                        can_decode: line.contains("D"),
                    });
                }
            }
        }
        
        Ok(codecs)
    }

    /// 解析格式信息
    fn parse_formats(&self, output: &str) -> AppResult<Vec<FormatInfo>> {
        let mut formats = Vec::new();
        
        for line in output.lines() {
            if line.starts_with(" ") && line.len() > 10 {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    formats.push(FormatInfo {
                        name: parts[1].to_string(),
                        description: parts[2..].join(" "),
                        can_mux: line.contains("E"),
                        can_demux: line.contains("D"),
                    });
                }
            }
        }
        
        Ok(formats)
    }

    /// 解析版本信息
    fn parse_version(&self, output: &str) -> AppResult<String> {
        for line in output.lines() {
            if line.starts_with("ffmpeg version") || line.starts_with("ffprobe version") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    return Ok(parts[2].to_string());
                }
            }
        }
        
        Err(AppError::FFmpeg("Could not parse version".to_string()))
    }

    /// 设置默认编码设置
    pub fn set_default_settings(&mut self, settings: EncodingSettings) {
        self.default_settings = settings;
    }

    /// 获取默认编码设置
    pub fn get_default_settings(&self) -> &EncodingSettings {
        &self.default_settings
    }
}

/// 编码设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingSettings {
    pub video_codec: String,
    pub audio_codec: String,
    pub preset: String,
    pub crf: u8,
    pub resolution: Option<Resolution>,
    pub fps: Option<f64>,
    pub bitrate: Option<String>,
    pub extra_params: HashMap<String, String>,
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
            extra_params: HashMap::new(),
        }
    }
}

/// 分辨率
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

/// 视频信息
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

impl VideoInfo {
    fn from_probe_result(probe: ProbeResult) -> AppResult<Self> {
        let format = probe.format.ok_or_else(|| {
            AppError::FFmpeg("No format information found".to_string())
        })?;
        
        let video_stream = probe.streams.iter()
            .find(|s| s.codec_type == "video")
            .ok_or_else(|| AppError::FFmpeg("No video stream found".to_string()))?;
        
        let audio_stream = probe.streams.iter()
            .find(|s| s.codec_type == "audio");
        
        Ok(Self {
            duration: format.duration.parse().unwrap_or(0.0),
            width: video_stream.width.unwrap_or(0),
            height: video_stream.height.unwrap_or(0),
            fps: video_stream.r_frame_rate.parse().unwrap_or(0.0),
            video_codec: video_stream.codec_name.clone(),
            audio_codec: audio_stream.map(|s| s.codec_name.clone()),
            bitrate: format.bit_rate.parse().unwrap_or(0),
            format: format.format_name,
            streams: probe.streams.into_iter().map(StreamInfo::from).collect(),
        })
    }
}

/// 流信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    pub index: u32,
    pub codec_type: String,
    pub codec_name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration: Option<f64>,
}

impl From<ProbeStream> for StreamInfo {
    fn from(stream: ProbeStream) -> Self {
        Self {
            index: stream.index,
            codec_type: stream.codec_type,
            codec_name: stream.codec_name,
            width: stream.width,
            height: stream.height,
            duration: stream.duration.and_then(|d| d.parse().ok()),
        }
    }
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