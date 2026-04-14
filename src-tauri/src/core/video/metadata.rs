//! 视频元数据处理模块

use crate::types::AppResult;
use serde_json::Value;

/// 视频元数据
#[derive(Debug, Clone)]
pub struct VideoMetadata {
    /// 视频时长（秒）
    pub duration: Option<f64>,
    /// 视频宽度
    pub width: Option<u32>,
    /// 视频高度
    pub height: Option<u32>,
    /// 帧率
    pub frame_rate: Option<f64>,
    /// 编码格式
    pub codec: Option<String>,
    /// 比特率
    pub bitrate: Option<u64>,
    /// 像素格式
    pub pixel_format: Option<String>,
    /// 色彩空间
    pub color_space: Option<String>,
    /// 音频流信息
    pub audio_streams: Vec<AudioStreamInfo>,
    /// 视频流信息
    pub video_streams: Vec<VideoStreamInfo>,
}

/// 音频流信息
#[derive(Debug, Clone)]
pub struct AudioStreamInfo {
    /// 编码格式
    pub codec: String,
    /// 采样率
    pub sample_rate: Option<u32>,
    /// 声道数
    pub channels: Option<u32>,
    /// 比特率
    pub bitrate: Option<u64>,
}

/// 视频流信息
#[derive(Debug, Clone)]
pub struct VideoStreamInfo {
    /// 编码格式
    pub codec: String,
    /// 宽度
    pub width: u32,
    /// 高度
    pub height: u32,
    /// 帧率
    pub frame_rate: Option<f64>,
    /// 比特率
    pub bitrate: Option<u64>,
    /// 像素格式
    pub pixel_format: Option<String>,
}

impl VideoMetadata {
    /// 从FFprobe的JSON输出创建VideoMetadata
    pub fn from_ffprobe_json(json: &Value) -> AppResult<Self> {
        let mut metadata = VideoMetadata {
            duration: None,
            width: None,
            height: None,
            frame_rate: None,
            codec: None,
            bitrate: None,
            pixel_format: None,
            color_space: None,
            audio_streams: Vec::new(),
            video_streams: Vec::new(),
        };

        // 解析格式信息
        if let Some(format) = json.get("format") {
            if let Some(duration_str) = format.get("duration").and_then(|v| v.as_str()) {
                metadata.duration = duration_str.parse().ok();
            }
            if let Some(bitrate_str) = format.get("bit_rate").and_then(|v| v.as_str()) {
                metadata.bitrate = bitrate_str.parse().ok();
            }
        }

        // 解析流信息
        if let Some(streams) = json.get("streams").and_then(|v| v.as_array()) {
            for stream in streams {
                let codec_type = stream.get("codec_type").and_then(|v| v.as_str());

                match codec_type {
                    Some("video") => {
                        let video_stream = Self::parse_video_stream(stream)?;

                        // 如果这是第一个视频流，更新主要元数据
                        if metadata.video_streams.is_empty() {
                            metadata.width = Some(video_stream.width);
                            metadata.height = Some(video_stream.height);
                            metadata.frame_rate = video_stream.frame_rate;
                            metadata.codec = Some(video_stream.codec.clone());
                            metadata.pixel_format = video_stream.pixel_format.clone();
                        }

                        metadata.video_streams.push(video_stream);
                    }
                    Some("audio") => {
                        let audio_stream = Self::parse_audio_stream(stream)?;
                        metadata.audio_streams.push(audio_stream);
                    }
                    _ => {}
                }
            }
        }

        Ok(metadata)
    }

    /// 解析视频流
    fn parse_video_stream(stream: &Value) -> AppResult<VideoStreamInfo> {
        let codec = stream
            .get("codec_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let width = stream.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

        let height = stream.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

        let frame_rate = stream
            .get("r_frame_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| Self::parse_fraction(s));

        let bitrate = stream
            .get("bit_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok());

        let pixel_format = stream
            .get("pix_fmt")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok(VideoStreamInfo {
            codec,
            width,
            height,
            frame_rate,
            bitrate,
            pixel_format,
        })
    }

    /// 解析音频流
    fn parse_audio_stream(stream: &Value) -> AppResult<AudioStreamInfo> {
        let codec = stream
            .get("codec_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let sample_rate = stream
            .get("sample_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok());

        let channels = stream
            .get("channels")
            .and_then(|v| v.as_u64())
            .map(|c| c as u32);

        let bitrate = stream
            .get("bit_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok());

        Ok(AudioStreamInfo {
            codec,
            sample_rate,
            channels,
            bitrate,
        })
    }

    /// 解析分数格式的帧率（如 "30/1"）
    fn parse_fraction(fraction_str: &str) -> Option<f64> {
        let parts: Vec<&str> = fraction_str.split('/').collect();
        if parts.len() == 2 {
            if let (Ok(numerator), Ok(denominator)) =
                (parts[0].parse::<f64>(), parts[1].parse::<f64>())
            {
                if denominator != 0.0 {
                    return Some(numerator / denominator);
                }
            }
        }
        None
    }

    /// 获取视频分辨率字符串
    pub fn get_resolution_string(&self) -> Option<String> {
        if let (Some(width), Some(height)) = (self.width, self.height) {
            Some(format!("{}x{}", width, height))
        } else {
            None
        }
    }

    /// 获取时长字符串（HH:MM:SS格式）
    pub fn get_duration_string(&self) -> Option<String> {
        self.duration.map(|duration| {
            let hours = (duration / 3600.0) as u32;
            let minutes = ((duration % 3600.0) / 60.0) as u32;
            let seconds = (duration % 60.0) as u32;
            format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
        })
    }

    /// 检查是否有视频流
    pub fn has_video(&self) -> bool {
        !self.video_streams.is_empty()
    }

    /// 检查是否有音频流
    pub fn has_audio(&self) -> bool {
        !self.audio_streams.is_empty()
    }

    /// 获取主要视频流
    pub fn get_primary_video_stream(&self) -> Option<&VideoStreamInfo> {
        self.video_streams.first()
    }

    /// 获取主要音频流
    pub fn get_primary_audio_stream(&self) -> Option<&AudioStreamInfo> {
        self.audio_streams.first()
    }
}
