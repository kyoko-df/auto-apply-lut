//! 视频处理核心模块
//! 提供视频文件的分析、处理和转换功能

use crate::types::{AppResult, VideoFormat, VideoInfo};
use crate::utils::config::ConfigManager;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::process::Command;

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
        // 0) 优先读取配置中的 ffmpeg 路径
        if let Ok(cfg) = ConfigManager::new() {
            if let Some(cfg_ffmpeg) = cfg
                .get_config()
                .ffmpeg_path
                .as_ref()
                .filter(|s| !s.trim().is_empty())
            {
                // 推断 ffprobe 路径：同目录下可执行名替换
                let probe_name = if cfg!(target_os = "windows") {
                    "ffprobe.exe"
                } else {
                    "ffprobe"
                };
                let mut ffprobe_path = PathBuf::from(cfg_ffmpeg);
                if ffprobe_path.is_file() {
                    ffprobe_path.pop();
                    ffprobe_path.push(probe_name);
                } else if ffprobe_path.ends_with("ffmpeg") || cfg_ffmpeg.ends_with("ffmpeg.exe") {
                    ffprobe_path.pop();
                    ffprobe_path.push(probe_name);
                } else {
                    ffprobe_path = PathBuf::from(probe_name);
                }

                let ffmpeg_path = cfg_ffmpeg.clone();
                let ffprobe_path = ffprobe_path.to_string_lossy().to_string();

                // 如果可执行不可用则回退到自动发现
                if Self::is_executable_available(&ffmpeg_path)
                    && Self::is_executable_available(&ffprobe_path)
                {
                    return Ok(Self {
                        ffmpeg_path,
                        ffprobe_path,
                    });
                }
            }
        }

        // 1) 自动发现
        let (ffmpeg_path, ffprobe_path) = crate::core::ffmpeg::discover_ffmpeg_pair()?;
        let ffmpeg_path = ffmpeg_path.to_string_lossy().into_owned();
        let ffprobe_path = ffprobe_path.to_string_lossy().into_owned();
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
            return Err(crate::types::AppError::FileSystem(format!(
                "Video file not found: {}",
                path.display()
            )));
        }

        // 获取文件基本信息
        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|e| crate::types::AppError::FileSystem(e.to_string()))?;

        let file_size = metadata.len();
        let created_at = metadata
            .created()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());
        let modified_at = metadata
            .modified()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());

        // 使用FFprobe获取视频元数据
        let video_metadata = self.probe_video_metadata(path).await?;

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let format = path
            .extension()
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
            pixel_format: video_metadata.pixel_format,
            bit_depth: video_metadata.bit_depth,
            color_primaries: video_metadata.color_primaries,
            color_transfer: video_metadata.color_transfer,
            color_matrix: video_metadata.color_space,
            color_range: video_metadata.color_range,
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

    /// Check configured tools with the same bounded probe used by discovery.
    fn is_executable_available(path: &str) -> bool {
        crate::core::ffmpeg::executable_works(Path::new(path))
    }

    /// 使用FFprobe获取视频元数据
    async fn probe_video_metadata<P: AsRef<Path>>(&self, path: P) -> AppResult<VideoMetadata> {
        self.probe_video_metadata_with_timeout(path, std::time::Duration::from_secs(20))
            .await
    }

    async fn probe_video_metadata_with_timeout<P: AsRef<Path>>(
        &self,
        path: P,
        timeout: std::time::Duration,
    ) -> AppResult<VideoMetadata> {
        let mut command = Command::new(&self.ffprobe_path);
        command
            .args([
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
            ])
            .arg(path.as_ref())
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        let output = tokio::time::timeout(timeout, command.output())
            .await
            .map_err(|_| {
                crate::types::AppError::FFmpeg(
                    "读取视频信息超时，请检查视频文件或 FFprobe 路径".into(),
                )
            })?
            .map_err(|e| {
                crate::types::AppError::FFmpeg(format!(
                    "无法执行 FFprobe {}：{e}",
                    self.ffprobe_path
                ))
            })?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(crate::types::AppError::FFmpeg(format!(
                "FFprobe failed: {}",
                error
            )));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let json: Value = serde_json::from_str(&json_str).map_err(|e| {
            crate::types::AppError::FFmpeg(format!("Failed to parse FFprobe output: {}", e))
        })?;

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

#[cfg(all(test, unix))]
mod probe_lifecycle_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    async fn executable(path: &Path, body: &str) {
        tokio::fs::write(path, format!("#!/bin/sh\n{body}\n"))
            .await
            .unwrap();
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn configured_probe_returns_the_frontend_metadata_contract() {
        let directory = tempfile::tempdir().unwrap();
        let probe = directory.path().join("configured ffprobe");
        executable(&probe, r#"printf '%s\n' '{"format":{"duration":"2.5","bit_rate":"100000"},"streams":[{"codec_type":"video","codec_name":"h264","width":640,"height":360,"r_frame_rate":"25/1"}]}'"#).await;
        let source = directory.path().join("source ' clip.mp4");
        tokio::fs::write(&source, b"source").await.unwrap();
        let manager = VideoManager::with_paths("unused".into(), probe.to_string_lossy().into());
        let info = manager.get_video_info(&source).await.unwrap();
        assert_eq!(info.path, source);
        assert_eq!(info.duration, Some(2.5));
        assert_eq!(
            (info.width, info.height, info.fps),
            (Some(640), Some(360), Some(25.0))
        );
        let json = serde_json::to_value(info).unwrap();
        assert_eq!(json["filename"], "source ' clip.mp4");
        assert!(json["path"].is_string());
        assert_eq!(json["size"], 6);
    }

    #[tokio::test]
    async fn unresponsive_probe_is_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let probe = directory.path().join("slow-probe");
        executable(&probe, "exec sleep 20").await;
        let manager = VideoManager::with_paths("unused".into(), probe.to_string_lossy().into());
        let started = std::time::Instant::now();
        let error = manager
            .probe_video_metadata_with_timeout("unused.mp4", std::time::Duration::from_millis(50))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("超时"));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}
