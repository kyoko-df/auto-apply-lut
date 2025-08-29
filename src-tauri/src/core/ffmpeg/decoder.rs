//! FFmpeg解码器模块
//! 提供视频解码和信息提取功能

use crate::types::{AppResult, AppError};
use crate::core::ffmpeg::{VideoInfo, StreamInfo, Resolution};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use tokio::process::Command as AsyncCommand;
use serde::{Serialize, Deserialize};
use std::time::Duration;

/// 视频解码器
pub struct VideoDecoder {
    /// FFprobe可执行文件路径
    ffprobe_path: PathBuf,
    /// FFmpeg可执行文件路径
    ffmpeg_path: PathBuf,
    /// 缓存的视频信息
    info_cache: std::sync::Arc<tokio::sync::RwLock<HashMap<PathBuf, VideoInfo>>>,
}

impl VideoDecoder {
    /// 创建新的视频解码器
    pub fn new(ffprobe_path: PathBuf, ffmpeg_path: PathBuf) -> Self {
        Self {
            ffprobe_path,
            ffmpeg_path,
            info_cache: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    /// 获取视频信息
    pub async fn get_video_info(&self, video_path: &Path) -> AppResult<VideoInfo> {
        // 检查缓存
        {
            let cache = self.info_cache.read().await;
            if let Some(info) = cache.get(video_path) {
                return Ok(info.clone());
            }
        }

        // 使用FFprobe获取视频信息
        let mut cmd = AsyncCommand::new(&self.ffprobe_path);
        cmd.args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            video_path.to_str().unwrap(),
        ]);

        let output = cmd.output().await
            .map_err(|e| AppError::FFmpeg(format!("Failed to run ffprobe: {}", e)))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::FFmpeg(format!("FFprobe failed: {}", error_msg)));
        }

        let json_output = String::from_utf8_lossy(&output.stdout);
        let probe_result: ProbeResult = serde_json::from_str(&json_output)
            .map_err(|e| AppError::FFmpeg(format!("Failed to parse ffprobe output: {}", e)))?;

        let video_info = self.parse_probe_result(probe_result)?;

        // 缓存结果
        {
            let mut cache = self.info_cache.write().await;
            cache.insert(video_path.to_path_buf(), video_info.clone());
        }

        Ok(video_info)
    }

    /// 解析FFprobe结果
    fn parse_probe_result(&self, probe_result: ProbeResult) -> AppResult<VideoInfo> {
        let format = probe_result.format;
        let streams = probe_result.streams;

        // 查找视频流
        let video_stream = streams.iter()
            .find(|s| s.codec_type == "video")
            .ok_or_else(|| AppError::FFmpeg("No video stream found".to_string()))?;

        // 查找音频流
        let audio_stream = streams.iter()
            .find(|s| s.codec_type == "audio");

        let duration = format.duration.parse::<f64>()
            .map_err(|_| AppError::FFmpeg("Invalid duration format".to_string()))?;

        let bitrate = format.bit_rate.parse::<u64>()
            .unwrap_or(0);

        let fps = if let Some(r_frame_rate) = &video_stream.r_frame_rate {
            self.parse_frame_rate(r_frame_rate)?
        } else {
            30.0 // 默认帧率
        };

        let mut stream_infos = Vec::new();
        for (index, stream) in streams.iter().enumerate() {
            stream_infos.push(StreamInfo {
                index: index as u32,
                codec_type: stream.codec_type.clone(),
                codec_name: stream.codec_name.clone(),
                width: stream.width,
                height: stream.height,
                duration: stream.duration.as_ref().and_then(|d| d.parse().ok()),
            });
        }

        Ok(VideoInfo {
            duration,
            width: video_stream.width.unwrap_or(0),
            height: video_stream.height.unwrap_or(0),
            fps,
            video_codec: video_stream.codec_name.clone(),
            audio_codec: audio_stream.map(|s| s.codec_name.clone()),
            bitrate,
            format: format.format_name,
            streams: stream_infos,
        })
    }

    /// 解析帧率字符串
    fn parse_frame_rate(&self, frame_rate: &str) -> AppResult<f64> {
        if frame_rate.contains('/') {
            let parts: Vec<&str> = frame_rate.split('/').collect();
            if parts.len() == 2 {
                let numerator: f64 = parts[0].parse()
                    .map_err(|_| AppError::FFmpeg("Invalid frame rate numerator".to_string()))?;
                let denominator: f64 = parts[1].parse()
                    .map_err(|_| AppError::FFmpeg("Invalid frame rate denominator".to_string()))?;
                
                if denominator != 0.0 {
                    return Ok(numerator / denominator);
                }
            }
        }
        
        frame_rate.parse::<f64>()
            .map_err(|_| AppError::FFmpeg("Invalid frame rate format".to_string()))
    }

    /// 提取视频帧
    pub async fn extract_frames(
        &self,
        video_path: &Path,
        output_dir: &Path,
        options: FrameExtractionOptions,
    ) -> AppResult<Vec<PathBuf>> {
        tokio::fs::create_dir_all(output_dir).await
            .map_err(AppError::from)?;

        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args(["-i", video_path.to_str().unwrap()]);

        // 添加时间范围
        if let Some(start_time) = options.start_time {
            cmd.args(["-ss", &start_time.as_secs().to_string()]);
        }
        
        if let Some(duration) = options.duration {
            cmd.args(["-t", &duration.as_secs().to_string()]);
        }

        // 添加帧率设置
        match options.extraction_mode {
            FrameExtractionMode::EveryNthFrame(n) => {
                cmd.args(["-vf", &format!("select=not(mod(n\\,{}))", n)]);
            }
            FrameExtractionMode::ByFps(fps) => {
                cmd.args(["-vf", &format!("fps={}", fps)]);
            }
            FrameExtractionMode::ByInterval(interval) => {
                cmd.args(["-vf", &format!("fps=1/{}", interval.as_secs())]);
            }
            FrameExtractionMode::KeyFramesOnly => {
                cmd.args(["-vf", "select=eq(pict_type\\,I)"]);
            }
        }

        // 添加质量设置
        cmd.args(["-q:v", &options.quality.to_string()]);

        // 添加分辨率设置
        if let Some(resolution) = &options.resolution {
            cmd.args(["-s", &format!("{}x{}", resolution.width, resolution.height)]);
        }

        // 输出文件模式
        let output_pattern = output_dir.join(format!("frame_%06d.{}", options.format.extension()));
        cmd.args(["-y", output_pattern.to_str().unwrap()]);

        let output = cmd.output().await
            .map_err(|e| AppError::FFmpeg(format!("Failed to extract frames: {}", e)))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::FFmpeg(format!("Frame extraction failed: {}", error_msg)));
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

    /// 获取视频缩略图
    pub async fn get_thumbnail(
        &self,
        video_path: &Path,
        output_path: &Path,
        options: ThumbnailOptions,
    ) -> AppResult<()> {
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args(["-i", video_path.to_str().unwrap()]);

        // 设置时间点
        cmd.args(["-ss", &options.timestamp.as_secs().to_string()]);

        // 只提取一帧
        cmd.args(["-vframes", "1"]);

        // 设置质量
        cmd.args(["-q:v", &options.quality.to_string()]);

        // 设置分辨率
        if let Some(resolution) = &options.resolution {
            cmd.args(["-s", &format!("{}x{}", resolution.width, resolution.height)]);
        }

        // 输出文件
        cmd.args(["-y", output_path.to_str().unwrap()]);

        let output = cmd.output().await
            .map_err(|e| AppError::FFmpeg(format!("Failed to generate thumbnail: {}", e)))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::FFmpeg(format!("Thumbnail generation failed: {}", error_msg)));
        }

        Ok(())
    }

    /// 检测视频格式支持
    pub async fn check_format_support(&self, video_path: &Path) -> AppResult<FormatSupport> {
        let video_info = self.get_video_info(video_path).await?;
        
        let mut support = FormatSupport {
            can_decode: true,
            can_encode: true,
            supported_codecs: Vec::new(),
            recommended_settings: None,
            warnings: Vec::new(),
        };

        // 检查视频编解码器支持
        match video_info.video_codec.as_str() {
            "h264" | "libx264" => {
                support.supported_codecs.push("H.264".to_string());
            }
            "hevc" | "libx265" => {
                support.supported_codecs.push("H.265/HEVC".to_string());
            }
            "vp8" | "libvpx" => {
                support.supported_codecs.push("VP8".to_string());
            }
            "vp9" | "libvpx-vp9" => {
                support.supported_codecs.push("VP9".to_string());
            }
            "av1" | "libaom-av1" => {
                support.supported_codecs.push("AV1".to_string());
                support.warnings.push("AV1编码速度较慢".to_string());
            }
            codec => {
                support.warnings.push(format!("未知的视频编解码器: {}", codec));
                support.can_encode = false;
            }
        }

        // 检查音频编解码器支持
        if let Some(audio_codec) = &video_info.audio_codec {
            match audio_codec.as_str() {
                "aac" => {
                    support.supported_codecs.push("AAC".to_string());
                }
                "mp3" => {
                    support.supported_codecs.push("MP3".to_string());
                }
                "opus" => {
                    support.supported_codecs.push("Opus".to_string());
                }
                codec => {
                    support.warnings.push(format!("未知的音频编解码器: {}", codec));
                }
            }
        }

        // 检查分辨率
        if video_info.width > 3840 || video_info.height > 2160 {
            support.warnings.push("超高分辨率视频可能需要更多处理时间".to_string());
        }

        // 检查帧率
        if video_info.fps > 60.0 {
            support.warnings.push("高帧率视频可能需要更多处理时间".to_string());
        }

        Ok(support)
    }

    /// 分析视频质量
    pub async fn analyze_quality(&self, video_path: &Path) -> AppResult<QualityAnalysis> {
        let video_info = self.get_video_info(video_path).await?;
        
        let mut analysis = QualityAnalysis {
            overall_score: 0.0,
            resolution_score: 0.0,
            bitrate_score: 0.0,
            codec_score: 0.0,
            recommendations: Vec::new(),
        };

        // 分析分辨率
        let pixel_count = video_info.width * video_info.height;
        analysis.resolution_score = match pixel_count {
            p if p >= 3840 * 2160 => 100.0, // 4K
            p if p >= 1920 * 1080 => 90.0,  // 1080p
            p if p >= 1280 * 720 => 75.0,   // 720p
            p if p >= 854 * 480 => 60.0,    // 480p
            _ => 40.0,                       // 低分辨率
        };

        // 分析码率
        let bitrate_per_pixel = video_info.bitrate as f64 / pixel_count as f64;
        analysis.bitrate_score = if bitrate_per_pixel > 0.1 {
            100.0
        } else if bitrate_per_pixel > 0.05 {
            80.0
        } else if bitrate_per_pixel > 0.02 {
            60.0
        } else {
            40.0
        };

        // 分析编解码器
        analysis.codec_score = match video_info.video_codec.as_str() {
            "hevc" | "libx265" => 100.0, // 最新的编解码器
            "h264" | "libx264" => 90.0,  // 广泛支持
            "vp9" | "libvpx-vp9" => 85.0,
            "vp8" | "libvpx" => 70.0,
            _ => 50.0,
        };

        // 计算总分
        analysis.overall_score = (analysis.resolution_score + analysis.bitrate_score + analysis.codec_score) / 3.0;

        // 生成建议
        if analysis.resolution_score < 70.0 {
            analysis.recommendations.push("考虑提高视频分辨率以获得更好的观看体验".to_string());
        }
        
        if analysis.bitrate_score < 70.0 {
            analysis.recommendations.push("码率较低，可能影响视频质量".to_string());
        }
        
        if analysis.codec_score < 80.0 {
            analysis.recommendations.push("考虑使用更现代的编解码器（如H.265）".to_string());
        }

        Ok(analysis)
    }

    /// 清理缓存
    pub async fn clear_cache(&self) {
        let mut cache = self.info_cache.write().await;
        cache.clear();
    }

    /// 获取缓存统计
    pub async fn get_cache_stats(&self) -> CacheStats {
        let cache = self.info_cache.read().await;
        CacheStats {
            entries: cache.len(),
            memory_usage: cache.len() * std::mem::size_of::<VideoInfo>(),
        }
    }
}

/// FFprobe输出结构
#[derive(Debug, Deserialize)]
struct ProbeResult {
    format: ProbeFormat,
    streams: Vec<ProbeStream>,
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    format_name: String,
    duration: String,
    bit_rate: String,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: String,
    codec_name: String,
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: Option<String>,
    duration: Option<String>,
    bit_rate: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    tags: Option<StreamTags>,
}

#[derive(Debug, Deserialize)]
struct StreamTags {
    language: Option<String>,
    title: Option<String>,
}

/// 帧提取选项
#[derive(Debug, Clone)]
pub struct FrameExtractionOptions {
    pub extraction_mode: FrameExtractionMode,
    pub start_time: Option<Duration>,
    pub duration: Option<Duration>,
    pub quality: u8, // 1-31, 1为最高质量
    pub format: ImageFormat,
    pub resolution: Option<Resolution>,
}

impl Default for FrameExtractionOptions {
    fn default() -> Self {
        Self {
            extraction_mode: FrameExtractionMode::ByFps(1.0),
            start_time: None,
            duration: None,
            quality: 2,
            format: ImageFormat::Jpeg,
            resolution: None,
        }
    }
}

/// 帧提取模式
#[derive(Debug, Clone)]
pub enum FrameExtractionMode {
    EveryNthFrame(u32),      // 每N帧提取一次
    ByFps(f64),              // 按指定帧率提取
    ByInterval(Duration),    // 按时间间隔提取
    KeyFramesOnly,           // 只提取关键帧
}

/// 图像格式
#[derive(Debug, Clone)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Bmp,
    Tiff,
}

impl ImageFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Png => "png",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Tiff => "tiff",
        }
    }
}

/// 缩略图选项
#[derive(Debug, Clone)]
pub struct ThumbnailOptions {
    pub timestamp: Duration,
    pub quality: u8,
    pub resolution: Option<Resolution>,
}

impl Default for ThumbnailOptions {
    fn default() -> Self {
        Self {
            timestamp: Duration::from_secs(10), // 10秒处
            quality: 2,
            resolution: Some(Resolution { width: 320, height: 240 }),
        }
    }
}

/// 格式支持信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatSupport {
    pub can_decode: bool,
    pub can_encode: bool,
    pub supported_codecs: Vec<String>,
    pub recommended_settings: Option<String>,
    pub warnings: Vec<String>,
}

/// 质量分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityAnalysis {
    pub overall_score: f64,
    pub resolution_score: f64,
    pub bitrate_score: f64,
    pub codec_score: f64,
    pub recommendations: Vec<String>,
}

/// 缓存统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub entries: usize,
    pub memory_usage: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_frame_extraction_options() {
        let options = FrameExtractionOptions::default();
        assert!(matches!(options.extraction_mode, FrameExtractionMode::ByFps(1.0)));
        assert_eq!(options.quality, 2);
        assert!(matches!(options.format, ImageFormat::Jpeg));
    }

    #[test]
    fn test_image_format_extension() {
        assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
        assert_eq!(ImageFormat::Png.extension(), "png");
        assert_eq!(ImageFormat::Bmp.extension(), "bmp");
        assert_eq!(ImageFormat::Tiff.extension(), "tiff");
    }

    #[test]
    fn test_thumbnail_options() {
        let options = ThumbnailOptions::default();
        assert_eq!(options.timestamp, Duration::from_secs(10));
        assert_eq!(options.quality, 2);
        assert!(options.resolution.is_some());
    }

    #[test]
    fn test_frame_rate_parsing() {
        let decoder = VideoDecoder::new(
            PathBuf::from("/usr/bin/ffprobe"),
            PathBuf::from("/usr/bin/ffmpeg"),
        );
        
        // 测试分数格式
        assert_eq!(decoder.parse_frame_rate("30000/1001").unwrap(), 30000.0 / 1001.0);
        assert_eq!(decoder.parse_frame_rate("25/1").unwrap(), 25.0);
        
        // 测试小数格式
        assert_eq!(decoder.parse_frame_rate("29.97").unwrap(), 29.97);
        assert_eq!(decoder.parse_frame_rate("60").unwrap(), 60.0);
    }

    #[test]
    fn test_quality_analysis() {
        let mut analysis = QualityAnalysis {
            overall_score: 0.0,
            resolution_score: 90.0,
            bitrate_score: 80.0,
            codec_score: 85.0,
            recommendations: Vec::new(),
        };
        
        analysis.overall_score = (analysis.resolution_score + analysis.bitrate_score + analysis.codec_score) / 3.0;
        assert!((analysis.overall_score - 85.0).abs() < 0.1);
    }

    #[test]
    fn test_format_support() {
        let support = FormatSupport {
            can_decode: true,
            can_encode: true,
            supported_codecs: vec!["H.264".to_string(), "AAC".to_string()],
            recommended_settings: None,
            warnings: Vec::new(),
        };
        
        assert!(support.can_decode);
        assert!(support.can_encode);
        assert_eq!(support.supported_codecs.len(), 2);
    }

    #[tokio::test]
    async fn test_cache_operations() {
        let decoder = VideoDecoder::new(
            PathBuf::from("/usr/bin/ffprobe"),
            PathBuf::from("/usr/bin/ffmpeg"),
        );
        
        // 测试缓存统计
        let stats = decoder.get_cache_stats().await;
        assert_eq!(stats.entries, 0);
        
        // 测试清理缓存
        decoder.clear_cache().await;
        let stats = decoder.get_cache_stats().await;
        assert_eq!(stats.entries, 0);
    }

    #[test]
    fn test_extraction_mode() {
        let mode1 = FrameExtractionMode::EveryNthFrame(10);
        let mode2 = FrameExtractionMode::ByFps(2.0);
        let mode3 = FrameExtractionMode::ByInterval(Duration::from_secs(5));
        let mode4 = FrameExtractionMode::KeyFramesOnly;
        
        // 确保所有模式都能正确创建
        assert!(matches!(mode1, FrameExtractionMode::EveryNthFrame(10)));
        assert!(matches!(mode2, FrameExtractionMode::ByFps(f) if (f - 2.0).abs() < 0.1));
        assert!(matches!(mode3, FrameExtractionMode::ByInterval(_)));
        assert!(matches!(mode4, FrameExtractionMode::KeyFramesOnly));
    }

    #[test]
    fn test_cache_stats_serialization() {
        let stats = CacheStats {
            entries: 10,
            memory_usage: 1024,
        };
        
        let serialized = serde_json::to_string(&stats).unwrap();
        let deserialized: CacheStats = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(deserialized.entries, 10);
        assert_eq!(deserialized.memory_usage, 1024);
    }
}