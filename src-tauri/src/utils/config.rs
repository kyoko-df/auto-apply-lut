//! 配置管理模块

use crate::types::error::{AppError, AppResult};
use crate::utils::path_utils::get_app_data_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// 默认输出目录
    pub default_output_dir: Option<String>,
    /// FFmpeg路径
    pub ffmpeg_path: Option<String>,
    /// 最大并发任务数
    pub max_concurrent_tasks: usize,
    /// 缓存大小限制（MB）
    pub cache_size_limit: u64,
    /// 是否启用硬件加速
    pub enable_hardware_acceleration: bool,
    /// 日志级别
    pub log_level: String,
    /// 最近使用的LUT文件
    pub recent_lut_files: Vec<String>,
    /// 最近使用的视频文件
    pub recent_video_files: Vec<String>,
    /// UI主题
    pub theme: String,
    /// 语言设置
    pub language: String,
    /// 输出格式
    pub output_format: String,
    /// 视频编码器
    pub video_codec: String,
    /// 音频编码器
    pub audio_codec: String,
    /// 质量预设
    pub quality_preset: String,
    /// 分辨率
    pub resolution: String,
    /// 帧率
    pub fps: Option<f64>,
    /// 码率
    pub bitrate: String,
    /// LUT 强度
    pub lut_intensity: f32,
    /// LUT 错误处理策略
    pub lut_error_strategy: String,
    /// 色彩空间
    pub color_space: String,
    pub output_bit_depth: String,
    pub input_color_space: String,
    pub preview_quality: String,
    /// 是否启用双通编码
    pub two_pass_encoding: bool,
    /// 是否保留元数据
    pub preserve_metadata: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_output_dir: None,
            ffmpeg_path: None,
            max_concurrent_tasks: 2,
            cache_size_limit: 1024, // 1GB
            enable_hardware_acceleration: true,
            log_level: "info".to_string(),
            recent_lut_files: Vec::new(),
            recent_video_files: Vec::new(),
            theme: "dark".to_string(),
            language: "zh-CN".to_string(),
            output_format: "mp4".to_string(),
            video_codec: "libx264".to_string(),
            audio_codec: "aac".to_string(),
            quality_preset: "balanced".to_string(),
            resolution: "original".to_string(),
            fps: None,
            bitrate: "auto".to_string(),
            lut_intensity: 100.0,
            lut_error_strategy: "StopOnError".to_string(),
            color_space: "original".to_string(),
            output_bit_depth: "8".into(),
            input_color_space: "auto".into(),
            preview_quality: "fast".into(),
            two_pass_encoding: false,
            preserve_metadata: true,
        }
    }
}

pub struct ConfigManager {
    config_path: PathBuf,
    config: AppConfig,
    // If an unreadable/corrupt original could not be backed up, keep it intact.
    // Defaults still allow startup; a settings write returns an actionable error.
    save_blocked_reason: Option<String>,
}

impl ConfigManager {
    /// Keep the application usable if its data directory cannot be located.
    /// A temporary config is deliberately unsavable, so it cannot overwrite a
    /// user file at a guessed fallback path.
    pub fn temporary(reason: String) -> Self {
        Self {
            config_path: PathBuf::new(),
            config: AppConfig::default(),
            save_blocked_reason: Some(reason),
        }
    }

    pub fn persistence_error(&self) -> Option<&str> {
        self.save_blocked_reason.as_deref()
    }

    /// 创建新的配置管理器
    pub fn new() -> AppResult<Self> {
        let mut config_path = get_app_data_dir()?;
        config_path.push("config.json");

        Ok(Self::from_path(config_path))
    }

    fn from_path(config_path: PathBuf) -> Self {
        let mut save_blocked_reason = None;
        let config = match fs::read(&config_path) {
            Ok(bytes) => match serde_json::from_slice::<AppConfig>(&bytes) {
                Ok(config) => config,
                Err(error) => {
                    let backup_path = config_path
                        .with_file_name(format!("config.invalid-{}.json", uuid::Uuid::new_v4()));
                    // Use create_new so even the recovery path cannot overwrite a
                    // previous backup. Preserve bytes, including invalid UTF-8.
                    let backup = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&backup_path)
                        .and_then(|mut file| {
                            file.write_all(&bytes)?;
                            file.sync_all()
                        });
                    match backup {
                        Ok(()) => crate::utils::logger::log_warn(&format!(
                            "配置文件损坏，已备份到 {}，本次使用默认配置：{}",
                            backup_path.display(),
                            error
                        )),
                        Err(backup_error) => {
                            let message = format!(
                                "配置文件损坏且无法备份，原文件已保留：{}（{}）",
                                config_path.display(),
                                backup_error
                            );
                            crate::utils::logger::log_warn(&message);
                            save_blocked_reason = Some(message);
                        }
                    }
                    AppConfig::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => AppConfig::default(),
            Err(error) => {
                let message = format!(
                    "无法读取配置文件，使用默认配置且保留原文件：{}（{}）",
                    config_path.display(),
                    error
                );
                crate::utils::logger::log_warn(&message);
                save_blocked_reason = Some(message);
                AppConfig::default()
            }
        };
        Self {
            config_path,
            config,
            save_blocked_reason,
        }
    }

    /// Save on the destination filesystem and atomically replace only after all
    /// serialized bytes are durable. A crash cannot leave truncated JSON behind.
    pub fn save(&self) -> AppResult<()> {
        if let Some(reason) = &self.save_blocked_reason {
            return Err(AppError::Io(format!(
                "{}；请修复配置目录的访问权限后重启应用",
                reason
            )));
        }
        let parent = self
            .config_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| AppError::Io(format!("创建配置目录失败: {}", error)))?;
        let content = serde_json::to_vec_pretty(&self.config)
            .map_err(|error| AppError::Parse(format!("序列化配置失败: {}", error)))?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".config-")
            .suffix(".tmp")
            .tempfile_in(parent)
            .map_err(|error| AppError::Io(format!("创建临时配置失败: {}", error)))?;
        temporary
            .write_all(&content)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|error| AppError::Io(format!("写入配置文件失败: {}", error)))?;
        temporary
            .persist(&self.config_path)
            .map_err(|error| AppError::Io(format!("替换配置文件失败: {}", error.error)))?;
        #[cfg(unix)]
        if let Err(error) = fs::File::open(parent).and_then(|directory| directory.sync_all()) {
            crate::utils::logger::log_warn(&format!("配置已保存，但目录同步失败: {}", error));
        }
        Ok(())
    }

    /// 获取配置
    pub fn get_config(&self) -> &AppConfig {
        &self.config
    }

    /// 更新配置
    pub fn update_config<F>(&mut self, updater: F) -> AppResult<()>
    where
        F: FnOnce(&mut AppConfig),
    {
        let previous = self.config.clone();
        updater(&mut self.config);
        if let Err(error) = self.save() {
            self.config = previous;
            return Err(error);
        }
        Ok(())
    }

    /// 添加最近使用的LUT文件
    pub fn add_recent_lut_file(&mut self, file_path: String) -> AppResult<()> {
        self.update_config(|config| {
            config.recent_lut_files.retain(|path| path != &file_path);
            config.recent_lut_files.insert(0, file_path);
            config.recent_lut_files.truncate(10);
        })
    }

    /// 添加最近使用的视频文件
    pub fn add_recent_video_file(&mut self, file_path: String) -> AppResult<()> {
        self.update_config(|config| {
            config.recent_video_files.retain(|path| path != &file_path);
            config.recent_video_files.insert(0, file_path);
            config.recent_video_files.truncate(10);
        })
    }

    pub fn set_default_output_dir(&mut self, dir: Option<String>) -> AppResult<()> {
        self.update_config(|config| config.default_output_dir = dir)
    }

    pub fn set_ffmpeg_path(&mut self, path: Option<String>) -> AppResult<()> {
        self.update_config(|config| config.ffmpeg_path = path)
    }

    pub fn set_max_concurrent_tasks(&mut self, count: usize) -> AppResult<()> {
        self.update_config(|config| config.max_concurrent_tasks = count.clamp(1, 4))
    }

    pub fn set_theme(&mut self, theme: String) -> AppResult<()> {
        self.update_config(|config| config.theme = theme)
    }

    pub fn set_language(&mut self, language: String) -> AppResult<()> {
        self.update_config(|config| config.language = language)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_partial_configs_match_workspace_defaults() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        let manager = ConfigManager::from_path(path.clone());
        assert_eq!(manager.get_config().max_concurrent_tasks, 2);
        assert!(manager.get_config().enable_hardware_acceleration);
        assert_eq!(manager.get_config().theme, "dark");
        assert_eq!(manager.get_config().color_space, "original");
        fs::write(&path, br#"{"ffmpeg_path":"/custom/ffmpeg"}"#).unwrap();
        let restored = ConfigManager::from_path(path);
        assert_eq!(
            restored.get_config().ffmpeg_path.as_deref(),
            Some("/custom/ffmpeg")
        );
        assert_eq!(restored.get_config().max_concurrent_tasks, 2);
        assert!(restored.get_config().enable_hardware_acceleration);
    }

    #[test]
    fn corrupt_config_is_backed_up_byte_for_byte_and_recoverable() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        let damaged = b"{\"ffmpeg_path\":\xff";
        fs::write(&path, damaged).unwrap();
        let mut manager = ConfigManager::from_path(path.clone());
        assert_eq!(manager.get_config().theme, "dark");
        let backup = fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("config.invalid-")
            })
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), damaged);
        assert_eq!(
            fs::read(&path).unwrap(),
            damaged,
            "startup alone must preserve the original"
        );
        manager.set_ffmpeg_path(Some("/new/ffmpeg".into())).unwrap();
        let restored = ConfigManager::from_path(path);
        assert_eq!(
            restored.get_config().ffmpeg_path.as_deref(),
            Some("/new/ffmpeg")
        );
    }

    #[test]
    fn failed_atomic_save_rolls_back_in_memory_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        let mut manager = ConfigManager::from_path(path.clone());
        manager.set_language("zh-CN".into()).unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(manager.set_language("en".into()).is_err());
        assert_eq!(manager.get_config().language, "zh-CN");
        assert!(path.is_dir());
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".config-")));
    }

    #[test]
    fn unreadable_original_starts_with_defaults_without_overwriting() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        fs::create_dir(&path).unwrap();
        let mut manager = ConfigManager::from_path(path.clone());
        assert_eq!(manager.get_config().theme, "dark");
        assert!(manager.set_theme("light".into()).is_err());
        assert_eq!(manager.get_config().theme, "dark");
        assert!(path.is_dir());
    }

    #[test]
    fn settings_replace_atomically_and_recent_lists_remain_unique() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nested/config.json");
        let mut manager = ConfigManager::from_path(path.clone());
        manager.set_max_concurrent_tasks(99).unwrap();
        for index in 0..12 {
            manager
                .add_recent_video_file(format!("clip-{index}.mp4"))
                .unwrap();
        }
        manager.add_recent_video_file("clip-5.mp4".into()).unwrap();
        let restored = ConfigManager::from_path(path);
        assert_eq!(restored.get_config().max_concurrent_tasks, 4);
        assert_eq!(restored.get_config().recent_video_files.len(), 10);
        assert_eq!(restored.get_config().recent_video_files[0], "clip-5.mp4");
        assert_eq!(
            restored
                .get_config()
                .recent_video_files
                .iter()
                .filter(|path| *path == "clip-5.mp4")
                .count(),
            1
        );
    }
}
