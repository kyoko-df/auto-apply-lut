//! FFmpeg编码器模块
//! 提供视频编码和压缩功能

use crate::core::ffmpeg::{EncodingSettings, Resolution};
use crate::types::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::process::Command as AsyncCommand;

/// 视频编码器
pub struct VideoEncoder {
    /// FFmpeg可执行文件路径
    ffmpeg_path: PathBuf,
    /// 编码预设
    presets: HashMap<String, EncodingPreset>,
}

impl VideoEncoder {
    /// 创建新的视频编码器
    pub fn new(ffmpeg_path: PathBuf) -> Self {
        let mut encoder = Self {
            ffmpeg_path,
            presets: HashMap::new(),
        };

        // 初始化默认预设
        encoder.init_default_presets();
        encoder
    }

    /// 初始化默认编码预设
    fn init_default_presets(&mut self) {
        // 高质量预设
        self.presets.insert(
            "high_quality".to_string(),
            EncodingPreset {
                name: "高质量".to_string(),
                description: "适用于存档和高质量输出".to_string(),
                settings: EncodingSettings {
                    video_codec: "libx264".to_string(),
                    audio_codec: "aac".to_string(),
                    preset: "slow".to_string(),
                    crf: 18,
                    resolution: None,
                    fps: None,
                    bitrate: None,
                    extra_params: {
                        let mut params = HashMap::new();
                        params.insert("-profile:v".to_string(), "high".to_string());
                        params.insert("-level".to_string(), "4.1".to_string());
                        params
                    },
                },
                target_use_case: UseCase::Archive,
            },
        );

        // 平衡预设
        self.presets.insert(
            "balanced".to_string(),
            EncodingPreset {
                name: "平衡".to_string(),
                description: "质量和文件大小的平衡".to_string(),
                settings: EncodingSettings {
                    video_codec: "libx264".to_string(),
                    audio_codec: "aac".to_string(),
                    preset: "medium".to_string(),
                    crf: 23,
                    resolution: None,
                    fps: None,
                    bitrate: None,
                    extra_params: HashMap::new(),
                },
                target_use_case: UseCase::General,
            },
        );

        // 快速预设
        self.presets.insert(
            "fast".to_string(),
            EncodingPreset {
                name: "快速".to_string(),
                description: "快速编码，适用于预览".to_string(),
                settings: EncodingSettings {
                    video_codec: "libx264".to_string(),
                    audio_codec: "aac".to_string(),
                    preset: "ultrafast".to_string(),
                    crf: 28,
                    resolution: None,
                    fps: None,
                    bitrate: None,
                    extra_params: HashMap::new(),
                },
                target_use_case: UseCase::Preview,
            },
        );

        // 网络分享预设
        self.presets.insert(
            "web".to_string(),
            EncodingPreset {
                name: "网络分享".to_string(),
                description: "适用于网络上传和分享".to_string(),
                settings: EncodingSettings {
                    video_codec: "libx264".to_string(),
                    audio_codec: "aac".to_string(),
                    preset: "medium".to_string(),
                    crf: 25,
                    resolution: Some(Resolution {
                        width: 1920,
                        height: 1080,
                    }),
                    fps: Some(30.0),
                    bitrate: Some("2M".to_string()),
                    extra_params: {
                        let mut params = HashMap::new();
                        params.insert("-movflags".to_string(), "+faststart".to_string());
                        params
                    },
                },
                target_use_case: UseCase::Web,
            },
        );

        // 移动设备预设
        self.presets.insert(
            "mobile".to_string(),
            EncodingPreset {
                name: "移动设备".to_string(),
                description: "适用于移动设备播放".to_string(),
                settings: EncodingSettings {
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
                        params.insert("-movflags".to_string(), "+faststart".to_string());
                        params
                    },
                },
                target_use_case: UseCase::Mobile,
            },
        );
    }

    /// 使用预设编码视频
    pub async fn encode_with_preset(
        &self,
        input_path: &Path,
        output_path: &Path,
        preset_name: &str,
    ) -> AppResult<EncodingResult> {
        let preset = self
            .presets
            .get(preset_name)
            .ok_or_else(|| AppError::FFmpeg(format!("Preset not found: {}", preset_name)))?;

        self.encode_video(input_path, output_path, &preset.settings)
            .await
    }

    /// 编码视频
    pub async fn encode_video(
        &self,
        input_path: &Path,
        output_path: &Path,
        settings: &EncodingSettings,
    ) -> AppResult<EncodingResult> {
        let start_time = Instant::now();

        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args([
            "-i",
            input_path.to_str().unwrap(),
            "-c:v",
            &settings.video_codec,
            "-preset",
            &settings.preset,
        ]);

        // 添加质量设置
        if let Some(bitrate) = &settings.bitrate {
            cmd.args(["-b:v", bitrate]);
        } else {
            cmd.args(["-crf", &settings.crf.to_string()]);
        }

        // 添加音频编码
        cmd.args(["-c:a", &settings.audio_codec]);

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

        // 输出文件
        cmd.args(["-y", output_path.to_str().unwrap()]);

        let output = cmd
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(format!("Failed to run ffmpeg: {}", e)))?;

        let elapsed = start_time.elapsed();

        if output.status.success() {
            let output_size = self.get_file_size(output_path).await.unwrap_or(0);
            let input_size = self.get_file_size(input_path).await.unwrap_or(0);

            Ok(EncodingResult {
                success: true,
                input_path: input_path.to_path_buf(),
                output_path: output_path.to_path_buf(),
                input_size,
                output_size,
                compression_ratio: if input_size > 0 {
                    output_size as f64 / input_size as f64
                } else {
                    0.0
                },
                encoding_time: elapsed,
                settings: settings.clone(),
                error: None,
            })
        } else {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            Ok(EncodingResult {
                success: false,
                input_path: input_path.to_path_buf(),
                output_path: output_path.to_path_buf(),
                input_size: 0,
                output_size: 0,
                compression_ratio: 0.0,
                encoding_time: elapsed,
                settings: settings.clone(),
                error: Some(error_msg.to_string()),
            })
        }
    }

    /// 批量编码
    pub async fn batch_encode(
        &self,
        tasks: Vec<EncodingTask>,
        max_concurrent: usize,
    ) -> AppResult<Vec<EncodingResult>> {
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent));
        let mut handles = Vec::new();

        for task in tasks {
            let semaphore = semaphore.clone();
            let encoder = self.clone_for_task();

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();

                if let Some(preset_name) = &task.preset_name {
                    encoder
                        .encode_with_preset(&task.input_path, &task.output_path, preset_name)
                        .await
                } else {
                    encoder
                        .encode_video(&task.input_path, &task.output_path, &task.settings)
                        .await
                }
            });

            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result?),
                Err(e) => {
                    results.push(EncodingResult {
                        success: false,
                        input_path: PathBuf::new(),
                        output_path: PathBuf::new(),
                        input_size: 0,
                        output_size: 0,
                        compression_ratio: 0.0,
                        encoding_time: Duration::from_secs(0),
                        settings: EncodingSettings::default(),
                        error: Some(format!("Task failed: {}", e)),
                    });
                }
            }
        }

        Ok(results)
    }

    /// 创建自定义预设
    pub fn create_preset(&mut self, name: String, preset: EncodingPreset) -> AppResult<()> {
        if self.presets.contains_key(&name) {
            return Err(AppError::FFmpeg(format!("Preset already exists: {}", name)));
        }

        self.presets.insert(name, preset);
        Ok(())
    }

    /// 更新预设
    pub fn update_preset(&mut self, name: &str, preset: EncodingPreset) -> AppResult<()> {
        if !self.presets.contains_key(name) {
            return Err(AppError::FFmpeg(format!("Preset not found: {}", name)));
        }

        self.presets.insert(name.to_string(), preset);
        Ok(())
    }

    /// 删除预设
    pub fn delete_preset(&mut self, name: &str) -> AppResult<()> {
        if !self.presets.contains_key(name) {
            return Err(AppError::FFmpeg(format!("Preset not found: {}", name)));
        }

        self.presets.remove(name);
        Ok(())
    }

    /// 获取所有预设
    pub fn get_presets(&self) -> &HashMap<String, EncodingPreset> {
        &self.presets
    }

    /// 获取预设
    pub fn get_preset(&self, name: &str) -> Option<&EncodingPreset> {
        self.presets.get(name)
    }

    /// 估算编码时间
    pub async fn estimate_encoding_time(
        &self,
        input_path: &Path,
        settings: &EncodingSettings,
    ) -> AppResult<Duration> {
        // 获取视频信息
        let video_info = self.get_video_info(input_path).await?;

        // 基于视频时长、分辨率和编码设置估算时间
        let base_time = video_info.duration;
        let resolution_factor = self.calculate_resolution_factor(&video_info, settings);
        let codec_factor = self.calculate_codec_factor(settings);
        let preset_factor = self.calculate_preset_factor(settings);

        let estimated_seconds = base_time * resolution_factor * codec_factor * preset_factor;

        Ok(Duration::from_secs_f64(estimated_seconds))
    }

    /// 计算分辨率因子
    fn calculate_resolution_factor(
        &self,
        video_info: &crate::core::ffmpeg::VideoInfo,
        settings: &EncodingSettings,
    ) -> f64 {
        let input_pixels = video_info.width as f64 * video_info.height as f64;

        if let Some(resolution) = &settings.resolution {
            let output_pixels = resolution.width as f64 * resolution.height as f64;
            output_pixels / input_pixels
        } else {
            1.0
        }
    }

    /// 计算编解码器因子
    fn calculate_codec_factor(&self, settings: &EncodingSettings) -> f64 {
        match settings.video_codec.as_str() {
            "libx264" => 1.0,
            "libx265" => 2.5,     // HEVC编码更慢
            "libvpx-vp9" => 3.0,  // VP9编码很慢
            "libaom-av1" => 10.0, // AV1编码非常慢
            _ => 1.0,
        }
    }

    /// 计算预设因子
    fn calculate_preset_factor(&self, settings: &EncodingSettings) -> f64 {
        match settings.preset.as_str() {
            "ultrafast" => 0.1,
            "superfast" => 0.2,
            "veryfast" => 0.3,
            "faster" => 0.5,
            "fast" => 0.7,
            "medium" => 1.0,
            "slow" => 2.0,
            "slower" => 4.0,
            "veryslow" => 8.0,
            _ => 1.0,
        }
    }

    /// 获取视频信息（简化版）
    async fn get_video_info(&self, input_path: &Path) -> AppResult<crate::core::ffmpeg::VideoInfo> {
        // 这里应该调用FFprobe获取视频信息
        // 为了简化，返回默认值
        Ok(crate::core::ffmpeg::VideoInfo {
            duration: 60.0, // 60秒
            width: 1920,
            height: 1080,
            fps: 30.0,
            video_codec: "h264".to_string(),
            audio_codec: Some("aac".to_string()),
            bitrate: 5000000, // 5Mbps
            format: "mp4".to_string(),
            streams: Vec::new(),
        })
    }

    /// 获取文件大小
    async fn get_file_size(&self, path: &Path) -> AppResult<u64> {
        let metadata = tokio::fs::metadata(path).await.map_err(AppError::from)?;
        Ok(metadata.len())
    }

    /// 克隆编码器用于任务
    fn clone_for_task(&self) -> Self {
        Self {
            ffmpeg_path: self.ffmpeg_path.clone(),
            presets: self.presets.clone(),
        }
    }

    /// 优化设置建议
    pub fn suggest_settings(
        &self,
        video_info: &crate::core::ffmpeg::VideoInfo,
        target_use_case: UseCase,
    ) -> EncodingSettings {
        match target_use_case {
            UseCase::Archive => {
                // 高质量存档
                EncodingSettings {
                    video_codec: "libx265".to_string(), // 使用HEVC获得更好压缩
                    audio_codec: "aac".to_string(),
                    preset: "slow".to_string(),
                    crf: 18,
                    resolution: None, // 保持原分辨率
                    fps: None,        // 保持原帧率
                    bitrate: None,
                    extra_params: {
                        let mut params = HashMap::new();
                        params.insert("-profile:v".to_string(), "main".to_string());
                        params
                    },
                }
            }
            UseCase::Web => {
                // 网络分享优化
                let target_resolution = if video_info.width > 1920 {
                    Some(Resolution {
                        width: 1920,
                        height: 1080,
                    })
                } else {
                    None
                };

                EncodingSettings {
                    video_codec: "libx264".to_string(),
                    audio_codec: "aac".to_string(),
                    preset: "medium".to_string(),
                    crf: 25,
                    resolution: target_resolution,
                    fps: Some(30.0),
                    bitrate: Some("2M".to_string()),
                    extra_params: {
                        let mut params = HashMap::new();
                        params.insert("-movflags".to_string(), "+faststart".to_string());
                        params
                    },
                }
            }
            UseCase::Mobile => {
                // 移动设备优化
                EncodingSettings {
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
                }
            }
            UseCase::Preview => {
                // 快速预览
                EncodingSettings {
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
                }
            }
            UseCase::General => {
                // 通用设置
                self.presets
                    .get("balanced")
                    .map(|p| p.settings.clone())
                    .unwrap_or_default()
            }
        }
    }
}

/// 编码预设
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingPreset {
    pub name: String,
    pub description: String,
    pub settings: EncodingSettings,
    pub target_use_case: UseCase,
}

/// 使用场景
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UseCase {
    Archive, // 存档
    Web,     // 网络分享
    Mobile,  // 移动设备
    Preview, // 预览
    General, // 通用
}

/// 编码任务
#[derive(Debug, Clone)]
pub struct EncodingTask {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub settings: EncodingSettings,
    pub preset_name: Option<String>,
}

/// 编码结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingResult {
    pub success: bool,
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub input_size: u64,
    pub output_size: u64,
    pub compression_ratio: f64,
    pub encoding_time: Duration,
    pub settings: EncodingSettings,
    pub error: Option<String>,
}

impl EncodingResult {
    /// 计算压缩百分比
    pub fn compression_percentage(&self) -> f64 {
        (1.0 - self.compression_ratio) * 100.0
    }

    /// 计算编码速度（倍速）
    pub fn encoding_speed(&self, video_duration: f64) -> f64 {
        if self.encoding_time.as_secs_f64() > 0.0 {
            video_duration / self.encoding_time.as_secs_f64()
        } else {
            0.0
        }
    }

    /// 格式化文件大小
    pub fn format_size(size: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = size as f64;
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
    fn test_encoding_preset() {
        let preset = EncodingPreset {
            name: "Test Preset".to_string(),
            description: "Test description".to_string(),
            settings: EncodingSettings::default(),
            target_use_case: UseCase::General,
        };

        assert_eq!(preset.name, "Test Preset");
        assert!(matches!(preset.target_use_case, UseCase::General));
    }

    #[test]
    fn test_encoding_task() {
        let task = EncodingTask {
            input_path: PathBuf::from("/input/video.mp4"),
            output_path: PathBuf::from("/output/video.mp4"),
            settings: EncodingSettings::default(),
            preset_name: Some("balanced".to_string()),
        };

        assert_eq!(task.preset_name, Some("balanced".to_string()));
    }

    #[test]
    fn test_encoding_result() {
        let result = EncodingResult {
            success: true,
            input_path: PathBuf::from("/input/video.mp4"),
            output_path: PathBuf::from("/output/video.mp4"),
            input_size: 1024 * 1024 * 100, // 100MB
            output_size: 1024 * 1024 * 50, // 50MB
            compression_ratio: 0.5,
            encoding_time: Duration::from_secs(60),
            settings: EncodingSettings::default(),
            error: None,
        };

        assert!(result.success);
        assert_eq!(result.compression_percentage(), 50.0);
        assert_eq!(result.encoding_speed(120.0), 2.0); // 2x speed
    }

    #[test]
    fn test_format_size() {
        assert_eq!(EncodingResult::format_size(1024), "1.00 KB");
        assert_eq!(EncodingResult::format_size(1024 * 1024), "1.00 MB");
        assert_eq!(EncodingResult::format_size(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_use_case_serialization() {
        let use_case = UseCase::Web;
        let serialized = serde_json::to_string(&use_case).unwrap();
        let deserialized: UseCase = serde_json::from_str(&serialized).unwrap();

        assert!(matches!(deserialized, UseCase::Web));
    }

    #[tokio::test]
    async fn test_video_encoder_creation() {
        let encoder = VideoEncoder::new(PathBuf::from("/usr/bin/ffmpeg"));

        // 检查默认预设是否已加载
        assert!(encoder.get_preset("balanced").is_some());
        assert!(encoder.get_preset("high_quality").is_some());
        assert!(encoder.get_preset("fast").is_some());
        assert!(encoder.get_preset("web").is_some());
        assert!(encoder.get_preset("mobile").is_some());
    }

    #[test]
    fn test_preset_management() {
        let mut encoder = VideoEncoder::new(PathBuf::from("/usr/bin/ffmpeg"));

        // 创建自定义预设
        let custom_preset = EncodingPreset {
            name: "Custom".to_string(),
            description: "Custom preset".to_string(),
            settings: EncodingSettings::default(),
            target_use_case: UseCase::General,
        };

        let result = encoder.create_preset("custom".to_string(), custom_preset.clone());
        assert!(result.is_ok());

        // 检查预设是否存在
        assert!(encoder.get_preset("custom").is_some());

        // 更新预设
        let updated_preset = EncodingPreset {
            name: "Updated Custom".to_string(),
            ..custom_preset
        };

        let result = encoder.update_preset("custom", updated_preset);
        assert!(result.is_ok());

        // 删除预设
        let result = encoder.delete_preset("custom");
        assert!(result.is_ok());
        assert!(encoder.get_preset("custom").is_none());
    }

    #[test]
    fn test_codec_factor_calculation() {
        let encoder = VideoEncoder::new(PathBuf::from("/usr/bin/ffmpeg"));

        let settings_x264 = EncodingSettings {
            video_codec: "libx264".to_string(),
            ..Default::default()
        };

        let settings_x265 = EncodingSettings {
            video_codec: "libx265".to_string(),
            ..Default::default()
        };

        assert_eq!(encoder.calculate_codec_factor(&settings_x264), 1.0);
        assert_eq!(encoder.calculate_codec_factor(&settings_x265), 2.5);
    }

    #[test]
    fn test_preset_factor_calculation() {
        let encoder = VideoEncoder::new(PathBuf::from("/usr/bin/ffmpeg"));

        let settings_ultrafast = EncodingSettings {
            preset: "ultrafast".to_string(),
            ..Default::default()
        };

        let settings_veryslow = EncodingSettings {
            preset: "veryslow".to_string(),
            ..Default::default()
        };

        assert_eq!(encoder.calculate_preset_factor(&settings_ultrafast), 0.1);
        assert_eq!(encoder.calculate_preset_factor(&settings_veryslow), 8.0);
    }
}
