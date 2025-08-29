//! 配置管理模块

use crate::types::error::{AppError, AppResult};
use crate::utils::path_utils::get_app_data_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_output_dir: None,
            ffmpeg_path: None,
            max_concurrent_tasks: 4,
            cache_size_limit: 1024, // 1GB
            enable_hardware_acceleration: true,
            log_level: "info".to_string(),
            recent_lut_files: Vec::new(),
            recent_video_files: Vec::new(),
            theme: "light".to_string(),
            language: "zh-CN".to_string(),
        }
    }
}

pub struct ConfigManager {
    config_path: PathBuf,
    config: AppConfig,
}

impl ConfigManager {
    /// 创建新的配置管理器
    pub fn new() -> AppResult<Self> {
        let mut config_path = get_app_data_dir()?;
        config_path.push("config.json");
        
        let config = if config_path.exists() {
            Self::load_config(&config_path)?
        } else {
            AppConfig::default()
        };
        
        Ok(Self {
            config_path,
            config,
        })
    }
    
    /// 加载配置文件
    fn load_config(path: &PathBuf) -> AppResult<AppConfig> {
        let content = fs::read_to_string(path)
            .map_err(|e| AppError::Io(format!("读取配置文件失败: {}", e)))?;
        
        serde_json::from_str(&content)
            .map_err(|e| AppError::Parse(format!("解析配置文件失败: {}", e)))
    }
    
    /// 保存配置文件
    pub fn save(&self) -> AppResult<()> {
        // 确保配置目录存在
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::Io(format!("创建配置目录失败: {}", e)))?;
        }
        
        let content = serde_json::to_string_pretty(&self.config)
            .map_err(|e| AppError::Parse(format!("序列化配置失败: {}", e)))?;
        
        fs::write(&self.config_path, content)
            .map_err(|e| AppError::Io(format!("写入配置文件失败: {}", e)))?;
        
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
        updater(&mut self.config);
        self.save()
    }
    
    /// 添加最近使用的LUT文件
    pub fn add_recent_lut_file(&mut self, file_path: String) -> AppResult<()> {
        // 移除已存在的相同路径
        self.config.recent_lut_files.retain(|path| path != &file_path);
        
        // 添加到开头
        self.config.recent_lut_files.insert(0, file_path);
        
        // 限制最大数量
        if self.config.recent_lut_files.len() > 10 {
            self.config.recent_lut_files.truncate(10);
        }
        
        self.save()
    }
    
    /// 添加最近使用的视频文件
    pub fn add_recent_video_file(&mut self, file_path: String) -> AppResult<()> {
        // 移除已存在的相同路径
        self.config.recent_video_files.retain(|path| path != &file_path);
        
        // 添加到开头
        self.config.recent_video_files.insert(0, file_path);
        
        // 限制最大数量
        if self.config.recent_video_files.len() > 10 {
            self.config.recent_video_files.truncate(10);
        }
        
        self.save()
    }
    
    /// 设置默认输出目录
    pub fn set_default_output_dir(&mut self, dir: Option<String>) -> AppResult<()> {
        self.config.default_output_dir = dir;
        self.save()
    }
    
    /// 设置FFmpeg路径
    pub fn set_ffmpeg_path(&mut self, path: Option<String>) -> AppResult<()> {
        self.config.ffmpeg_path = path;
        self.save()
    }
    
    /// 设置最大并发任务数
    pub fn set_max_concurrent_tasks(&mut self, count: usize) -> AppResult<()> {
        self.config.max_concurrent_tasks = count;
        self.save()
    }
    
    /// 设置主题
    pub fn set_theme(&mut self, theme: String) -> AppResult<()> {
        self.config.theme = theme;
        self.save()
    }
    
    /// 设置语言
    pub fn set_language(&mut self, language: String) -> AppResult<()> {
        self.config.language = language;
        self.save()
    }
}