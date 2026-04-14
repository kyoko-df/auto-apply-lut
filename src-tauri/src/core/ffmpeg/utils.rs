//! FFmpeg工具模块
//! 提供FFmpeg相关的辅助功能和实用工具

use crate::core::ffmpeg::{EncodingSettings, Resolution, VideoInfo};
use crate::types::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::process::Command as AsyncCommand;

/// FFmpeg工具集
pub struct FFmpegUtils {
    /// FFmpeg可执行文件路径
    ffmpeg_path: PathBuf,
    /// FFprobe可执行文件路径
    ffprobe_path: PathBuf,
}

impl FFmpegUtils {
    /// 创建新的FFmpeg工具集
    pub fn new(ffmpeg_path: PathBuf, ffprobe_path: PathBuf) -> Self {
        Self {
            ffmpeg_path,
            ffprobe_path,
        }
    }

    /// 检查FFmpeg版本
    pub async fn get_ffmpeg_version(&self) -> AppResult<VersionInfo> {
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args(["-version"]);

        let output = cmd
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(format!("Failed to get FFmpeg version: {}", e)))?;

        if !output.status.success() {
            return Err(AppError::FFmpeg("Failed to get FFmpeg version".to_string()));
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        self.parse_version_info(&output_str)
    }

    /// 解析版本信息
    fn parse_version_info(&self, output: &str) -> AppResult<VersionInfo> {
        let lines: Vec<&str> = output.lines().collect();
        if lines.is_empty() {
            return Err(AppError::FFmpeg("Empty version output".to_string()));
        }

        let first_line = lines[0];
        let version = if let Some(start) = first_line.find("version ") {
            let version_start = start + 8;
            if let Some(end) = first_line[version_start..].find(' ') {
                first_line[version_start..version_start + end].to_string()
            } else {
                "unknown".to_string()
            }
        } else {
            "unknown".to_string()
        };

        let build_date = self.extract_build_date(&output);
        let configuration = self.extract_configuration(&output);

        Ok(VersionInfo {
            version,
            build_date,
            configuration,
            full_output: output.to_string(),
        })
    }

    /// 提取构建日期
    fn extract_build_date(&self, output: &str) -> Option<String> {
        for line in output.lines() {
            if line.contains("built on ") {
                if let Some(start) = line.find("built on ") {
                    let date_start = start + 9;
                    if let Some(end) = line[date_start..].find(" with ") {
                        return Some(line[date_start..date_start + end].to_string());
                    }
                }
            }
        }
        None
    }

    /// 提取配置信息
    fn extract_configuration(&self, output: &str) -> Option<String> {
        for line in output.lines() {
            if line.trim().starts_with("configuration: ") {
                return Some(line.trim()[14..].to_string());
            }
        }
        None
    }

    /// 获取支持的编解码器列表
    pub async fn get_supported_codecs(&self) -> AppResult<CodecSupport> {
        let mut video_codecs = Vec::new();
        let mut audio_codecs = Vec::new();

        // 获取编码器
        let encoders = self.get_encoders().await?;
        for encoder in encoders {
            match encoder.media_type {
                MediaType::Video => video_codecs.push(encoder),
                MediaType::Audio => audio_codecs.push(encoder),
                _ => {}
            }
        }

        Ok(CodecSupport {
            video_codecs,
            audio_codecs,
        })
    }

    /// 获取编码器列表
    async fn get_encoders(&self) -> AppResult<Vec<CodecInfo>> {
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args(["-encoders"]);

        let output = cmd
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(format!("Failed to get encoders: {}", e)))?;

        if !output.status.success() {
            return Err(AppError::FFmpeg("Failed to get encoder list".to_string()));
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        self.parse_codec_list(&output_str, true)
    }

    /// 获取解码器列表
    pub async fn get_decoders(&self) -> AppResult<Vec<CodecInfo>> {
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args(["-decoders"]);

        let output = cmd
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(format!("Failed to get decoders: {}", e)))?;

        if !output.status.success() {
            return Err(AppError::FFmpeg("Failed to get decoder list".to_string()));
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        self.parse_codec_list(&output_str, false)
    }

    /// 解析编解码器列表
    fn parse_codec_list(&self, output: &str, is_encoder: bool) -> AppResult<Vec<CodecInfo>> {
        let mut codecs = Vec::new();
        let mut in_list = false;

        for line in output.lines() {
            if line.contains("------") {
                in_list = true;
                continue;
            }

            if !in_list || line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let flags = parts[0];
                let name = parts[1];
                let description = parts[2..].join(" ");

                let media_type = if flags.contains('V') {
                    MediaType::Video
                } else if flags.contains('A') {
                    MediaType::Audio
                } else if flags.contains('S') {
                    MediaType::Subtitle
                } else {
                    MediaType::Data
                };

                codecs.push(CodecInfo {
                    name: name.to_string(),
                    description,
                    media_type,
                    is_encoder,
                    supports_hardware: flags.contains('H'),
                    supports_lossless: flags.contains('L'),
                    supports_lossy: flags.contains('S'),
                });
            }
        }

        Ok(codecs)
    }

    /// 获取支持的格式列表
    pub async fn get_supported_formats(&self) -> AppResult<FormatSupport> {
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args(["-formats"]);

        let output = cmd
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(format!("Failed to get formats: {}", e)))?;

        if !output.status.success() {
            return Err(AppError::FFmpeg("Failed to get format list".to_string()));
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        self.parse_format_list(&output_str)
    }

    /// 解析格式列表
    fn parse_format_list(&self, output: &str) -> AppResult<FormatSupport> {
        let mut formats = Vec::new();
        let mut in_list = false;

        for line in output.lines() {
            if line.contains("--") {
                in_list = true;
                continue;
            }

            if !in_list || line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let flags = parts[0];
                let name = parts[1];
                let description = parts[2..].join(" ");

                formats.push(FormatInfo {
                    name: name.to_string(),
                    description,
                    can_demux: flags.contains('D'),
                    can_mux: flags.contains('E'),
                });
            }
        }

        Ok(FormatSupport { formats })
    }

    /// 验证视频文件
    pub async fn validate_video_file(&self, file_path: &Path) -> AppResult<ValidationResult> {
        let mut result = ValidationResult {
            is_valid: false,
            file_exists: false,
            is_readable: false,
            has_video_stream: false,
            has_audio_stream: false,
            duration_valid: false,
            format_supported: false,
            codec_supported: false,
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        // 检查文件是否存在
        if !file_path.exists() {
            result.errors.push("File does not exist".to_string());
            return Ok(result);
        }
        result.file_exists = true;

        // 检查文件是否可读
        match tokio::fs::metadata(file_path).await {
            Ok(metadata) => {
                if metadata.is_file() {
                    result.is_readable = true;
                } else {
                    result.errors.push("Path is not a file".to_string());
                    return Ok(result);
                }
            }
            Err(e) => {
                result
                    .errors
                    .push(format!("Cannot read file metadata: {}", e));
                return Ok(result);
            }
        }

        // 使用FFprobe验证视频
        let mut cmd = AsyncCommand::new(&self.ffprobe_path);
        cmd.args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            file_path.to_str().unwrap(),
        ]);

        let output = cmd
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(format!("Failed to probe file: {}", e)))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            result.errors.push(format!("FFprobe failed: {}", error_msg));
            return Ok(result);
        }

        // 解析FFprobe输出
        let json_output = String::from_utf8_lossy(&output.stdout);
        match serde_json::from_str::<serde_json::Value>(&json_output) {
            Ok(data) => {
                // 检查流
                if let Some(streams) = data["streams"].as_array() {
                    for stream in streams {
                        if let Some(codec_type) = stream["codec_type"].as_str() {
                            match codec_type {
                                "video" => result.has_video_stream = true,
                                "audio" => result.has_audio_stream = true,
                                _ => {}
                            }
                        }
                    }
                }

                // 检查格式
                if let Some(format) = data["format"].as_object() {
                    if let Some(duration) = format["duration"].as_str() {
                        if let Ok(dur) = duration.parse::<f64>() {
                            result.duration_valid = dur > 0.0;
                        }
                    }

                    if let Some(format_name) = format["format_name"].as_str() {
                        result.format_supported =
                            self.is_format_supported(format_name).await.unwrap_or(false);
                    }
                }

                result.codec_supported = true; // 如果FFprobe能解析，说明编解码器支持
            }
            Err(e) => {
                result
                    .errors
                    .push(format!("Failed to parse probe output: {}", e));
                return Ok(result);
            }
        }

        // 添加警告
        if !result.has_video_stream {
            result.warnings.push("No video stream found".to_string());
        }

        if !result.has_audio_stream {
            result.warnings.push("No audio stream found".to_string());
        }

        // 判断整体有效性
        result.is_valid = result.file_exists
            && result.is_readable
            && result.has_video_stream
            && result.duration_valid
            && result.format_supported
            && result.codec_supported;

        Ok(result)
    }

    /// 检查格式是否支持
    async fn is_format_supported(&self, format_name: &str) -> AppResult<bool> {
        let supported_formats = self.get_supported_formats().await?;
        Ok(supported_formats
            .formats
            .iter()
            .any(|f| f.name == format_name))
    }

    /// 估算处理时间
    pub fn estimate_processing_time(
        &self,
        video_info: &VideoInfo,
        settings: &EncodingSettings,
        operation: ProcessingOperation,
    ) -> Duration {
        let base_time = video_info.duration;
        let resolution_factor = self.calculate_resolution_factor(video_info, settings);
        let codec_factor = self.calculate_codec_factor(settings);
        let operation_factor = self.calculate_operation_factor(&operation);

        let estimated_seconds = base_time * resolution_factor * codec_factor * operation_factor;
        Duration::from_secs_f64(estimated_seconds.max(1.0))
    }

    /// 计算分辨率因子
    fn calculate_resolution_factor(
        &self,
        video_info: &VideoInfo,
        settings: &EncodingSettings,
    ) -> f64 {
        let input_pixels = video_info.width as f64 * video_info.height as f64;

        if let Some(resolution) = &settings.resolution {
            let output_pixels = resolution.width as f64 * resolution.height as f64;
            (output_pixels / input_pixels).sqrt() // 平方根关系
        } else {
            1.0
        }
    }

    /// 计算编解码器因子
    fn calculate_codec_factor(&self, settings: &EncodingSettings) -> f64 {
        match settings.video_codec.as_str() {
            "copy" => 0.1, // 复制流很快
            "libx264" => 1.0,
            "libx265" => 3.0,
            "libvpx-vp9" => 5.0,
            "libaom-av1" => 15.0,
            _ => 1.5,
        }
    }

    /// 计算操作因子
    fn calculate_operation_factor(&self, operation: &ProcessingOperation) -> f64 {
        match operation {
            ProcessingOperation::Copy => 0.1,
            ProcessingOperation::Transcode => 1.0,
            ProcessingOperation::ApplyLut => 1.2,
            ProcessingOperation::ApplyFilters => 1.5,
            ProcessingOperation::ExtractFrames => 0.8,
            ProcessingOperation::CreateVideo => 1.3,
        }
    }

    /// 生成处理建议
    pub fn generate_processing_suggestions(
        &self,
        video_info: &VideoInfo,
        target_use_case: ProcessingUseCase,
    ) -> ProcessingSuggestions {
        let mut suggestions = ProcessingSuggestions {
            recommended_settings: EncodingSettings::default(),
            optimization_tips: Vec::new(),
            warnings: Vec::new(),
            estimated_time: Duration::from_secs(0),
            estimated_size: 0,
        };

        // 根据用途生成建议
        match target_use_case {
            ProcessingUseCase::Archive => {
                suggestions.recommended_settings = EncodingSettings {
                    video_codec: "libx265".to_string(),
                    audio_codec: "aac".to_string(),
                    preset: "slow".to_string(),
                    crf: 18,
                    resolution: None, // 保持原分辨率
                    fps: None,
                    bitrate: None,
                    extra_params: HashMap::new(),
                };
                suggestions
                    .optimization_tips
                    .push("使用HEVC编码器获得更好的压缩率".to_string());
                suggestions
                    .optimization_tips
                    .push("使用较慢的预设以获得最佳质量".to_string());
            }
            ProcessingUseCase::Streaming => {
                let target_resolution = if video_info.width > 1920 {
                    Some(Resolution {
                        width: 1920,
                        height: 1080,
                    })
                } else {
                    None
                };

                suggestions.recommended_settings = EncodingSettings {
                    video_codec: "libx264".to_string(),
                    audio_codec: "aac".to_string(),
                    preset: "medium".to_string(),
                    crf: 23,
                    resolution: target_resolution,
                    fps: Some(30.0),
                    bitrate: Some("2M".to_string()),
                    extra_params: {
                        let mut params = HashMap::new();
                        params.insert("-movflags".to_string(), "+faststart".to_string());
                        params
                    },
                };
                suggestions
                    .optimization_tips
                    .push("使用H.264编码器确保兼容性".to_string());
                suggestions
                    .optimization_tips
                    .push("添加faststart标志优化流媒体播放".to_string());
            }
            ProcessingUseCase::Mobile => {
                suggestions.recommended_settings = EncodingSettings {
                    video_codec: "libx264".to_string(),
                    audio_codec: "aac".to_string(),
                    preset: "medium".to_string(),
                    crf: 26,
                    resolution: Some(Resolution {
                        width: 1280,
                        height: 720,
                    }),
                    fps: Some(30.0),
                    bitrate: Some("1M".to_string()),
                    extra_params: {
                        let mut params = HashMap::new();
                        params.insert("-profile:v".to_string(), "baseline".to_string());
                        params.insert("-level".to_string(), "3.1".to_string());
                        params
                    },
                };
                suggestions
                    .optimization_tips
                    .push("使用baseline profile确保移动设备兼容性".to_string());
                suggestions
                    .optimization_tips
                    .push("降低分辨率和码率以适应移动网络".to_string());
            }
            ProcessingUseCase::Preview => {
                suggestions.recommended_settings = EncodingSettings {
                    video_codec: "libx264".to_string(),
                    audio_codec: "aac".to_string(),
                    preset: "ultrafast".to_string(),
                    crf: 28,
                    resolution: Some(Resolution {
                        width: 854,
                        height: 480,
                    }),
                    fps: Some(15.0),
                    bitrate: None,
                    extra_params: HashMap::new(),
                };
                suggestions
                    .optimization_tips
                    .push("使用ultrafast预设快速生成预览".to_string());
                suggestions
                    .optimization_tips
                    .push("降低分辨率和帧率以加快处理速度".to_string());
            }
        }

        // 生成警告
        if video_info.width > 3840 || video_info.height > 2160 {
            suggestions
                .warnings
                .push("超高分辨率视频处理时间较长".to_string());
        }

        if video_info.fps > 60.0 {
            suggestions
                .warnings
                .push("高帧率视频处理时间较长".to_string());
        }

        if video_info.duration > 3600.0 {
            suggestions
                .warnings
                .push("长视频处理时间较长，建议分段处理".to_string());
        }

        // 估算处理时间
        suggestions.estimated_time = self.estimate_processing_time(
            video_info,
            &suggestions.recommended_settings,
            ProcessingOperation::Transcode,
        );

        // 估算文件大小（简化计算）
        if let Some(bitrate_str) = &suggestions.recommended_settings.bitrate {
            if let Ok(bitrate) = bitrate_str.trim_end_matches('M').parse::<f64>() {
                suggestions.estimated_size =
                    (bitrate * 1_000_000.0 * video_info.duration / 8.0) as u64;
            }
        }

        suggestions
    }

    /// 清理临时文件
    pub async fn cleanup_temp_files(&self, temp_dir: &Path) -> AppResult<CleanupResult> {
        let mut result = CleanupResult {
            files_removed: 0,
            bytes_freed: 0,
            errors: Vec::new(),
        };

        if !temp_dir.exists() {
            return Ok(result);
        }

        let mut entries = tokio::fs::read_dir(temp_dir)
            .await
            .map_err(AppError::from)?;

        while let Some(entry) = entries.next_entry().await.map_err(AppError::from)? {
            let path = entry.path();

            if path.is_file() {
                match tokio::fs::metadata(&path).await {
                    Ok(metadata) => {
                        let size = metadata.len();
                        match tokio::fs::remove_file(&path).await {
                            Ok(_) => {
                                result.files_removed += 1;
                                result.bytes_freed += size;
                            }
                            Err(e) => {
                                result.errors.push(format!(
                                    "Failed to remove {}: {}",
                                    path.display(),
                                    e
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        result.errors.push(format!(
                            "Failed to get metadata for {}: {}",
                            path.display(),
                            e
                        ));
                    }
                }
            }
        }

        Ok(result)
    }
}

/// 版本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub build_date: Option<String>,
    pub configuration: Option<String>,
    pub full_output: String,
}

/// 编解码器支持
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecSupport {
    pub video_codecs: Vec<CodecInfo>,
    pub audio_codecs: Vec<CodecInfo>,
}

/// 编解码器信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecInfo {
    pub name: String,
    pub description: String,
    pub media_type: MediaType,
    pub is_encoder: bool,
    pub supports_hardware: bool,
    pub supports_lossless: bool,
    pub supports_lossy: bool,
}

/// 媒体类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MediaType {
    Video,
    Audio,
    Subtitle,
    Data,
}

/// 格式支持
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatSupport {
    pub formats: Vec<FormatInfo>,
}

/// 格式信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatInfo {
    pub name: String,
    pub description: String,
    pub can_demux: bool,
    pub can_mux: bool,
}

/// 验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub file_exists: bool,
    pub is_readable: bool,
    pub has_video_stream: bool,
    pub has_audio_stream: bool,
    pub duration_valid: bool,
    pub format_supported: bool,
    pub codec_supported: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// 处理操作类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessingOperation {
    Copy,
    Transcode,
    ApplyLut,
    ApplyFilters,
    ExtractFrames,
    CreateVideo,
}

/// 处理用途
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessingUseCase {
    Archive,   // 存档
    Streaming, // 流媒体
    Mobile,    // 移动设备
    Preview,   // 预览
}

/// 处理建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingSuggestions {
    pub recommended_settings: EncodingSettings,
    pub optimization_tips: Vec<String>,
    pub warnings: Vec<String>,
    pub estimated_time: Duration,
    pub estimated_size: u64,
}

/// 清理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupResult {
    pub files_removed: u32,
    pub bytes_freed: u64,
    pub errors: Vec<String>,
}

impl CleanupResult {
    /// 格式化释放的空间
    pub fn format_bytes_freed(&self) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = self.bytes_freed as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_version_info() {
        let version_info = VersionInfo {
            version: "4.4.0".to_string(),
            build_date: Some("2021-07-14".to_string()),
            configuration: Some("--enable-gpl --enable-libx264".to_string()),
            full_output: "ffmpeg version 4.4.0".to_string(),
        };

        assert_eq!(version_info.version, "4.4.0");
        assert!(version_info.build_date.is_some());
        assert!(version_info.configuration.is_some());
    }

    #[test]
    fn test_codec_info() {
        let codec = CodecInfo {
            name: "libx264".to_string(),
            description: "H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10".to_string(),
            media_type: MediaType::Video,
            is_encoder: true,
            supports_hardware: false,
            supports_lossless: true,
            supports_lossy: true,
        };

        assert_eq!(codec.name, "libx264");
        assert_eq!(codec.media_type, MediaType::Video);
        assert!(codec.is_encoder);
        assert!(codec.supports_lossless);
        assert!(codec.supports_lossy);
    }

    #[test]
    fn test_format_info() {
        let format = FormatInfo {
            name: "mp4".to_string(),
            description: "MP4 (MPEG-4 Part 14)".to_string(),
            can_demux: true,
            can_mux: true,
        };

        assert_eq!(format.name, "mp4");
        assert!(format.can_demux);
        assert!(format.can_mux);
    }

    #[test]
    fn test_validation_result() {
        let result = ValidationResult {
            is_valid: true,
            file_exists: true,
            is_readable: true,
            has_video_stream: true,
            has_audio_stream: true,
            duration_valid: true,
            format_supported: true,
            codec_supported: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        assert!(result.is_valid);
        assert!(result.has_video_stream);
        assert!(result.has_audio_stream);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_processing_operation() {
        let operation = ProcessingOperation::ApplyLut;
        assert!(matches!(operation, ProcessingOperation::ApplyLut));
    }

    #[test]
    fn test_processing_use_case() {
        let use_case = ProcessingUseCase::Archive;
        assert!(matches!(use_case, ProcessingUseCase::Archive));
    }

    #[test]
    fn test_cleanup_result() {
        let result = CleanupResult {
            files_removed: 5,
            bytes_freed: 1024 * 1024 * 10, // 10MB
            errors: Vec::new(),
        };

        assert_eq!(result.files_removed, 5);
        assert_eq!(result.format_bytes_freed(), "10.00 MB");
    }

    #[test]
    fn test_cleanup_result_format_bytes() {
        let result1 = CleanupResult {
            files_removed: 0,
            bytes_freed: 1024,
            errors: Vec::new(),
        };
        assert_eq!(result1.format_bytes_freed(), "1.00 KB");

        let result2 = CleanupResult {
            files_removed: 0,
            bytes_freed: 1024 * 1024 * 1024,
            errors: Vec::new(),
        };
        assert_eq!(result2.format_bytes_freed(), "1.00 GB");
    }

    #[test]
    fn test_media_type_equality() {
        assert_eq!(MediaType::Video, MediaType::Video);
        assert_ne!(MediaType::Video, MediaType::Audio);
    }

    #[tokio::test]
    async fn test_ffmpeg_utils_creation() {
        let utils = FFmpegUtils::new(
            PathBuf::from("/usr/bin/ffmpeg"),
            PathBuf::from("/usr/bin/ffprobe"),
        );

        // 基本创建测试
        assert_eq!(utils.ffmpeg_path, PathBuf::from("/usr/bin/ffmpeg"));
        assert_eq!(utils.ffprobe_path, PathBuf::from("/usr/bin/ffprobe"));
    }

    #[test]
    fn test_codec_support() {
        let support = CodecSupport {
            video_codecs: vec![CodecInfo {
                name: "libx264".to_string(),
                description: "H.264".to_string(),
                media_type: MediaType::Video,
                is_encoder: true,
                supports_hardware: false,
                supports_lossless: true,
                supports_lossy: true,
            }],
            audio_codecs: vec![CodecInfo {
                name: "aac".to_string(),
                description: "AAC".to_string(),
                media_type: MediaType::Audio,
                is_encoder: true,
                supports_hardware: false,
                supports_lossless: false,
                supports_lossy: true,
            }],
        };

        assert_eq!(support.video_codecs.len(), 1);
        assert_eq!(support.audio_codecs.len(), 1);
        assert_eq!(support.video_codecs[0].name, "libx264");
        assert_eq!(support.audio_codecs[0].name, "aac");
    }

    #[test]
    fn test_format_support() {
        let support = FormatSupport {
            formats: vec![FormatInfo {
                name: "mp4".to_string(),
                description: "MP4".to_string(),
                can_demux: true,
                can_mux: true,
            }],
        };

        assert_eq!(support.formats.len(), 1);
        assert_eq!(support.formats[0].name, "mp4");
        assert!(support.formats[0].can_demux);
        assert!(support.formats[0].can_mux);
    }
}
