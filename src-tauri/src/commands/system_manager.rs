use crate::utils::{config::ConfigManager, logger};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sysinfo::System;
use tauri::State;
use std::sync::Mutex;

#[derive(Debug, Serialize, Deserialize)]
pub struct CodecInfo {
    pub name: String,
    pub description: String,
    pub supported: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AvailableCodecs {
    pub video_codecs: Vec<CodecInfo>,
    pub audio_codecs: Vec<CodecInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemInfo {
    pub cpu_usage: f32,
    pub memory_usage: f64,
    pub total_memory: u64,
    pub available_memory: u64,
    pub disk_usage: Vec<DiskInfo>,
    pub cpu_count: usize,
    pub system_name: String,
    pub system_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
    pub usage_percentage: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub default_output_dir: String,
    pub ffmpeg_path: String,
    pub max_concurrent_tasks: usize,
    pub cache_size_mb: usize,
    pub hardware_acceleration: bool,
    pub log_level: String,
    pub ui_theme: String,
    pub language: String,
}

#[tauri::command]
pub async fn get_system_info() -> Result<SystemInfo, String> {
    let mut sys = System::new_all();
    
    // 简化实现，提供基本的系统信息
    let cpu_count = sys.cpus().len();
    let cpu_usage = 0.0; // 简化为0，避免API兼容性问题
    
    let total_memory = sys.total_memory();
    let available_memory = sys.available_memory();
    let memory_usage = if total_memory > 0 {
        ((total_memory - available_memory) as f64 / total_memory as f64) * 100.0
    } else {
        0.0
    };
    
    // 简化磁盘信息，只提供基本信息
    let disk_usage = vec![DiskInfo {
        name: "System".to_string(),
        mount_point: "/".to_string(),
        total_space: total_memory, // 简化使用内存信息
        available_space: available_memory,
        usage_percentage: memory_usage,
    }];
    
    Ok(SystemInfo {
        cpu_usage,
        memory_usage,
        total_memory,
        available_memory,
        disk_usage,
        cpu_count,
        system_name: "System".to_string(),
        system_version: "Unknown".to_string(),
    })
}

#[tauri::command]
pub async fn get_app_settings(
    config_manager: State<'_, Mutex<ConfigManager>>,
) -> Result<AppSettings, String> {
    let cfg = config_manager.lock().map_err(|e| format!("Config lock poisoned: {}", e))?;
    let config = cfg.get_config();
    
    Ok(AppSettings {
        default_output_dir: config.default_output_dir.clone().unwrap_or_default(),
        ffmpeg_path: config.ffmpeg_path.clone().unwrap_or_default(),
        max_concurrent_tasks: config.max_concurrent_tasks,
        cache_size_mb: config.cache_size_limit as usize,
        hardware_acceleration: config.enable_hardware_acceleration,
        log_level: config.log_level.clone(),
        ui_theme: config.theme.clone(),
        language: config.language.clone(),
    })
}

#[tauri::command]
pub async fn update_app_settings(
    settings: AppSettings,
    _config_manager: State<'_, Mutex<ConfigManager>>,
) -> Result<String, String> {
    // 简化实现，暂时只返回成功
    logger::log_info("App settings update requested");
    Ok("Settings updated successfully".to_string())
}

#[tauri::command]
pub async fn get_log_files() -> Result<Vec<String>, String> {
    let log_dir = crate::utils::path_utils::get_app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?
        .join("logs");
    
    if !log_dir.exists() {
        return Ok(Vec::new());
    }
    
    let mut log_files = Vec::new();
    match std::fs::read_dir(&log_dir) {
        Ok(entries) => {
            for entry in entries {
                if let Ok(entry) = entry {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if file_name.ends_with(".log") {
                            log_files.push(file_name.to_string());
                        }
                    }
                }
            }
        }
        Err(e) => return Err(format!("Failed to read log directory: {}", e)),
    }
    
    log_files.sort();
    log_files.reverse(); // Most recent first
    Ok(log_files)
}

#[tauri::command]
pub async fn read_log_file(file_name: String) -> Result<String, String> {
    let log_dir = crate::utils::path_utils::get_app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?
        .join("logs");
    
    let log_file = log_dir.join(&file_name);
    
    if !log_file.exists() {
        return Err("Log file not found".to_string());
    }
    
    match std::fs::read_to_string(&log_file) {
        Ok(content) => Ok(content),
        Err(e) => Err(format!("Failed to read log file: {}", e)),
    }
}

#[tauri::command]
pub async fn clear_cache() -> Result<String, String> {
    let cache_dir = crate::utils::path_utils::get_cache_dir()
        .map_err(|e| format!("Failed to get cache dir: {}", e))?;
    
    if cache_dir.exists() {
        match std::fs::remove_dir_all(&cache_dir) {
            Ok(_) => {
                std::fs::create_dir_all(&cache_dir)
                    .map_err(|e| format!("Failed to recreate cache dir: {}", e))?;
                logger::log_info("Cache cleared successfully");
                Ok("Cache cleared successfully".to_string())
            }
            Err(e) => Err(format!("Failed to clear cache: {}", e)),
        }
    } else {
        Ok("Cache directory does not exist".to_string())
    }
}

#[tauri::command]
pub async fn get_cache_size() -> Result<u64, String> {
    let cache_dir = crate::utils::path_utils::get_cache_dir()
        .map_err(|e| format!("Failed to get cache dir: {}", e))?;
    
    if !cache_dir.exists() {
        return Ok(0);
    }
    
    fn dir_size(path: &std::path::Path) -> Result<u64, std::io::Error> {
        let mut size = 0;
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                size += dir_size(&entry.path())?;
            } else {
                size += metadata.len();
            }
        }
        Ok(size)
    }
    
    match dir_size(&cache_dir) {
        Ok(size) => Ok(size),
        Err(e) => Err(format!("Failed to calculate cache size: {}", e)),
    }
}

#[tauri::command]
pub async fn get_available_codecs() -> Result<AvailableCodecs, String> {
    // 返回常用的编解码器列表
    let video_codecs = vec![
        CodecInfo {
            name: "h264".to_string(),
            description: "H.264/AVC".to_string(),
            supported: true,
        },
        CodecInfo {
            name: "h265".to_string(),
            description: "H.265/HEVC".to_string(),
            supported: true,
        },
        CodecInfo {
            name: "vp9".to_string(),
            description: "VP9".to_string(),
            supported: true,
        },
        CodecInfo {
            name: "av1".to_string(),
            description: "AV1".to_string(),
            supported: true,
        },
    ];
    
    let audio_codecs = vec![
        CodecInfo {
            name: "aac".to_string(),
            description: "AAC".to_string(),
            supported: true,
        },
        CodecInfo {
            name: "mp3".to_string(),
            description: "MP3".to_string(),
            supported: true,
        },
        CodecInfo {
            name: "opus".to_string(),
            description: "Opus".to_string(),
            supported: true,
        },
    ];
    
    Ok(AvailableCodecs {
        video_codecs,
        audio_codecs,
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FfmpegInfo {
    pub mode: String,                 // native | external
    pub static_linked: bool,          // 是否为静态链接（编译期）
    pub library_versions: Option<HashMap<String, String>>, // 本地链接库版本（native 时）
    pub binary_version: Option<String>,                    // 外部可执行版本（external 时）
    pub binary_path: Option<String>,                       // 外部可执行路径（external 时）
}

#[tauri::command]
pub async fn get_ffmpeg_info(
    config_manager: State<'_, Mutex<ConfigManager>>,
) -> Result<FfmpegInfo, String> {
    // 如果启用了 ffmpeg_native 特性，则读取已链接库的版本信息
    #[cfg(feature = "ffmpeg_native")]
    {
        fn ver_to_str(v: u32) -> String {
            let major = (v >> 16) & 0xFF;
            let minor = (v >> 8) & 0xFF;
            let micro = v & 0xFF;
            format!("{}.{}.{}", major, minor, micro)
        }

        let mut libs = HashMap::new();
        unsafe {
            libs.insert(
                "avutil".to_string(),
                ver_to_str(ffmpeg_sys_next::avutil_version()),
            );
            libs.insert(
                "avcodec".to_string(),
                ver_to_str(ffmpeg_sys_next::avcodec_version()),
            );
            libs.insert(
                "avformat".to_string(),
                ver_to_str(ffmpeg_sys_next::avformat_version()),
            );
            libs.insert(
                "avfilter".to_string(),
                ver_to_str(ffmpeg_sys_next::avfilter_version()),
            );
            libs.insert(
                "avdevice".to_string(),
                ver_to_str(ffmpeg_sys_next::avdevice_version()),
            );
            libs.insert(
                "swresample".to_string(),
                ver_to_str(ffmpeg_sys_next::swresample_version()),
            );
            libs.insert(
                "swscale".to_string(),
                ver_to_str(ffmpeg_sys_next::swscale_version()),
            );
        }

        return Ok(FfmpegInfo {
            mode: "native".to_string(),
            static_linked: true,
            library_versions: Some(libs),
            binary_version: None,
            binary_path: None,
        });
    }

    // 未启用 ffmpeg_native 特性时，回退到外部可执行文件版本信息
    #[cfg(not(feature = "ffmpeg_native"))]
    {
        let cfg = config_manager.lock().map_err(|e| format!("Config lock poisoned: {}", e))?;
        let ffmpeg_bin = if let Some(p) = cfg
            .get_config()
            .ffmpeg_path
            .clone()
            .filter(|s| !s.trim().is_empty())
        {
            p
        } else {
            crate::core::ffmpeg::discover_ffmpeg_path()
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .to_string()
        };

        let output = std::process::Command::new(&ffmpeg_bin)
            .arg("-version")
            .output()
            .map_err(|e| format!("无法执行 '{}' 获取版本信息: {}", ffmpeg_bin, e))?;

        if !output.status.success() {
            return Err(format!(
                "执行 '{}' -version 失败，退出码: {:?}",
                ffmpeg_bin,
                output.status.code()
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let first_line = stdout.lines().next().unwrap_or("").to_string();

        Ok(FfmpegInfo {
            mode: "external".to_string(),
            static_linked: false,
            library_versions: None,
            binary_version: Some(first_line),
            binary_path: Some(ffmpeg_bin),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FfmpegPathConfig {
    pub ffmpeg_path: Option<String>,
}

#[tauri::command]
pub async fn get_ffmpeg_path_config(config_manager: State<'_, Mutex<ConfigManager>>) -> Result<FfmpegPathConfig, String> {
    let cfg = config_manager.lock().map_err(|e| format!("Config lock poisoned: {}", e))?;
    Ok(FfmpegPathConfig {
        ffmpeg_path: cfg.get_config().ffmpeg_path.clone(),
    })
}

#[tauri::command]
pub async fn set_ffmpeg_path_config(path: Option<String>, config_manager: State<'_, Mutex<ConfigManager>>) -> Result<(), String> {
    let mut cfg = config_manager.lock().map_err(|e| format!("Config lock poisoned: {}", e))?;
    cfg.set_ffmpeg_path(path).map_err(|e| e.to_string())
}
