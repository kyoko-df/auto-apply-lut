use crate::core::ffmpeg::utils::FFmpegUtils;
use crate::types::{ui_err, ui_err_p};
use crate::utils::{config::ConfigManager, logger};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use sysinfo::{Disks, System};
use tauri::State;

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
    pub output_format: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub quality_preset: String,
    pub resolution: String,
    pub fps: Option<f64>,
    pub bitrate: String,
    pub lut_intensity: f32,
    pub lut_error_strategy: String,
    pub color_space: String,
    pub two_pass_encoding: bool,
    pub preserve_metadata: bool,
    #[serde(default = "default_output_bit_depth")]
    pub output_bit_depth: String,
    #[serde(default = "default_input_color_space")]
    pub input_color_space: String,
    #[serde(default = "default_preview_quality")]
    pub preview_quality: String,
    #[serde(default)]
    pub photo_options: crate::core::photo::PhotoSettings,
}

fn default_output_bit_depth() -> String {
    "8".into()
}
fn default_input_color_space() -> String {
    "auto".into()
}
fn default_preview_quality() -> String {
    "fast".into()
}

fn validate_color_settings(settings: &AppSettings) -> Result<(), String> {
    settings.photo_options.output.validate(false)?;
    if !matches!(settings.output_bit_depth.as_str(), "8" | "10") {
        return Err(ui_err("settings.bad_bit_depth", "输出位深必须为 8 或 10"));
    }
    if settings.output_bit_depth == "10"
        && !matches!(settings.video_codec.as_str(), "libx265" | "prores_ks")
    {
        return Err(ui_err(
            "settings.bad_10bit_codec",
            "10-bit 输出仅支持 HEVC 和 ProRes HQ",
        ));
    }
    if !matches!(
        settings.input_color_space.as_str(),
        "auto" | "rec709" | "rec2020-pq" | "rec2020-hlg"
    ) {
        return Err(ui_err("settings.bad_input_space", "不支持的输入色彩空间"));
    }
    if !matches!(settings.preview_quality.as_str(), "fast" | "accurate") {
        return Err(ui_err("settings.bad_preview_quality", "不支持的预览质量"));
    }
    Ok(())
}

#[tauri::command]
pub async fn get_system_info() -> Result<SystemInfo, String> {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_count = sys.cpus().len();
    let cpu_usage = sys.global_cpu_usage();

    let total_memory = sys.total_memory();
    let available_memory = sys.available_memory();
    let memory_usage = if total_memory > 0 {
        ((total_memory - available_memory) as f64 / total_memory as f64) * 100.0
    } else {
        0.0
    };

    let disk_usage = Disks::new_with_refreshed_list()
        .iter()
        .map(|disk| {
            let total_space = disk.total_space();
            let available_space = disk.available_space();
            let usage_percentage = if total_space > 0 {
                ((total_space - available_space) as f64 / total_space as f64) * 100.0
            } else {
                0.0
            };

            DiskInfo {
                name: disk.name().to_string_lossy().to_string(),
                mount_point: disk.mount_point().to_string_lossy().to_string(),
                total_space,
                available_space,
                usage_percentage,
            }
        })
        .collect::<Vec<_>>();

    Ok(SystemInfo {
        cpu_usage,
        memory_usage,
        total_memory,
        available_memory,
        disk_usage,
        cpu_count,
        system_name: System::name().unwrap_or_else(|| "Unknown".to_string()),
        system_version: System::long_os_version()
            .or_else(System::os_version)
            .unwrap_or_else(|| "Unknown".to_string()),
    })
}

#[tauri::command]
pub async fn get_app_settings(
    config_manager: State<'_, Mutex<ConfigManager>>,
) -> Result<AppSettings, String> {
    let cfg = config_manager
        .lock()
        .map_err(|e| format!("Config lock poisoned: {}", e))?;
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
        output_format: config.output_format.clone(),
        video_codec: config.video_codec.clone(),
        audio_codec: config.audio_codec.clone(),
        quality_preset: config.quality_preset.clone(),
        resolution: config.resolution.clone(),
        fps: config.fps,
        bitrate: config.bitrate.clone(),
        lut_intensity: config.lut_intensity,
        lut_error_strategy: config.lut_error_strategy.clone(),
        color_space: config.color_space.clone(),
        two_pass_encoding: config.two_pass_encoding,
        preserve_metadata: config.preserve_metadata,
        output_bit_depth: config.output_bit_depth.clone(),
        input_color_space: config.input_color_space.clone(),
        preview_quality: config.preview_quality.clone(),
        photo_options: config.photo_options.clone(),
    })
}

#[tauri::command]
pub async fn update_app_settings(
    settings: AppSettings,
    config_manager: State<'_, Mutex<ConfigManager>>,
) -> Result<String, String> {
    validate_color_settings(&settings)?;
    let mut cfg = config_manager
        .lock()
        .map_err(|e| format!("Config lock poisoned: {}", e))?;

    let next_ffmpeg_path =
        (!settings.ffmpeg_path.trim().is_empty()).then(|| settings.ffmpeg_path.clone());
    let engine_changed = cfg.get_config().ffmpeg_path != next_ffmpeg_path;

    cfg.update_config(|config| {
        config.default_output_dir = if settings.default_output_dir.trim().is_empty() {
            None
        } else {
            Some(settings.default_output_dir.clone())
        };
        config.ffmpeg_path = if settings.ffmpeg_path.trim().is_empty() {
            None
        } else {
            Some(settings.ffmpeg_path.clone())
        };
        config.max_concurrent_tasks = settings.max_concurrent_tasks;
        config.cache_size_limit = settings.cache_size_mb as u64;
        config.enable_hardware_acceleration = settings.hardware_acceleration;
        config.log_level = settings.log_level.clone();
        config.theme = settings.ui_theme.clone();
        config.language = settings.language.clone();
        config.output_format = settings.output_format.clone();
        config.video_codec = settings.video_codec.clone();
        config.audio_codec = settings.audio_codec.clone();
        config.quality_preset = settings.quality_preset.clone();
        config.resolution = settings.resolution.clone();
        config.fps = settings.fps;
        config.bitrate = settings.bitrate.clone();
        config.lut_intensity = settings.lut_intensity;
        config.lut_error_strategy = settings.lut_error_strategy.clone();
        config.color_space = settings.color_space.clone();
        config.two_pass_encoding = settings.two_pass_encoding;
        config.preserve_metadata = settings.preserve_metadata;
        config.output_bit_depth = settings.output_bit_depth.clone();
        config.input_color_space = settings.input_color_space.clone();
        config.preview_quality = settings.preview_quality.clone();
        config.photo_options = settings.photo_options.clone();
    })
    .map_err(|e| format!("Failed to update settings: {}", e))?;

    if engine_changed {
        crate::core::ffmpeg::invalidate_discovery_cache();
    }

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

    read_log_file_in(&log_dir, &file_name)
}

fn read_log_file_in(log_dir: &std::path::Path, file_name: &str) -> Result<String, String> {
    use std::io::Read;
    const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
    let name = std::path::Path::new(file_name);
    if file_name.is_empty()
        || !file_name.ends_with(".log")
        || file_name.contains(['/', '\\'])
        || name.file_name().and_then(|value| value.to_str()) != Some(file_name)
    {
        return Err(ui_err(
            "log.bad_name",
            "日志名称必须是日志目录内的 .log 文件名",
        ));
    }
    let root = log_dir.canonicalize().map_err(|error| {
        ui_err_p(
            "log.dir_failed",
            serde_json::json!({ "error": error.to_string() }),
            format!("无法读取日志目录：{error}"),
        )
    })?;
    let candidate = root.join(file_name);
    if !std::fs::symlink_metadata(&candidate)
        .map_err(|error| error.to_string())?
        .file_type()
        .is_file()
    {
        return Err(ui_err("log.not_file", "日志路径必须是普通文件"));
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if canonical.parent() != Some(root.as_path()) {
        return Err(ui_err("log.outside_dir", "日志文件不在应用日志目录内"));
    }
    let mut content = Vec::new();
    std::fs::File::open(canonical)
        .map_err(|error| error.to_string())?
        .take(MAX_LOG_BYTES + 1)
        .read_to_end(&mut content)
        .map_err(|error| error.to_string())?;
    if content.len() as u64 > MAX_LOG_BYTES {
        return Err(ui_err(
            "log.too_large",
            "日志文件超过 2 MB，请在日志目录中直接查看",
        ));
    }
    String::from_utf8(content).map_err(|error| {
        ui_err_p(
            "log.not_utf8",
            serde_json::json!({ "error": error.to_string() }),
            format!("日志内容不是 UTF-8：{error}"),
        )
    })
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
    let fallback = AvailableCodecs {
        video_codecs: vec![
            CodecInfo {
                name: "libx264".to_string(),
                description: "H.264 (libx264)".to_string(),
                supported: true,
            },
            CodecInfo {
                name: "libx265".to_string(),
                description: "H.265/HEVC (libx265)".to_string(),
                supported: true,
            },
            CodecInfo {
                name: "libvpx-vp9".to_string(),
                description: "VP9 (libvpx-vp9)".to_string(),
                supported: true,
            },
            CodecInfo {
                name: "libaom-av1".to_string(),
                description: "AV1 (libaom-av1)".to_string(),
                supported: true,
            },
        ],
        audio_codecs: vec![
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
        ],
    };

    let ffmpeg_path =
        crate::core::ffmpeg::discover_ffmpeg_path().unwrap_or_else(|_| PathBuf::from("ffmpeg"));
    let ffprobe_path = ffmpeg_path
        .parent()
        .map(|parent| {
            parent.join(if cfg!(target_os = "windows") {
                "ffprobe.exe"
            } else {
                "ffprobe"
            })
        })
        .unwrap_or_else(|| {
            PathBuf::from(if cfg!(target_os = "windows") {
                "ffprobe.exe"
            } else {
                "ffprobe"
            })
        });

    let utils = FFmpegUtils::new(ffmpeg_path, ffprobe_path);
    let supported = match utils.get_supported_codecs().await {
        Ok(supported) => supported,
        Err(_) => return Ok(fallback),
    };

    let video_codecs = supported
        .video_codecs
        .into_iter()
        .map(|codec| CodecInfo {
            name: codec.name,
            description: codec.description,
            supported: true,
        })
        .collect::<Vec<_>>();

    let audio_codecs = supported
        .audio_codecs
        .into_iter()
        .map(|codec| CodecInfo {
            name: codec.name,
            description: codec.description,
            supported: true,
        })
        .collect::<Vec<_>>();

    Ok(AvailableCodecs {
        video_codecs: if video_codecs.is_empty() {
            fallback.video_codecs
        } else {
            video_codecs
        },
        audio_codecs: if audio_codecs.is_empty() {
            fallback.audio_codecs
        } else {
            audio_codecs
        },
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FfmpegInfo {
    pub mode: String,                                      // native | external
    pub static_linked: bool,                               // 是否为静态链接（编译期）
    pub library_versions: Option<HashMap<String, String>>, // 本地链接库版本（native 时）
    pub binary_version: Option<String>,                    // 外部可执行版本（external 时）
    pub binary_path: Option<String>,                       // 外部可执行路径（external 时）
    pub probe_path: Option<String>,
    pub probe_version: Option<String>,
}

async fn read_tool_version(
    path: &std::path::Path,
    name: &str,
    timeout: std::time::Duration,
) -> Result<String, String> {
    let mut command = tokio::process::Command::new(path);
    command
        .arg("-version")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| {
            ui_err_p(
                "ffmpeg.probe_timeout",
                serde_json::json!({ "name": name, "path": path.display().to_string() }),
                format!("{name} 检查超时，请检查可执行文件：{}", path.display()),
            )
        })?
        .map_err(|error| {
            ui_err_p(
                "ffmpeg.probe_exec",
                serde_json::json!({ "name": name, "path": path.display().to_string(), "error": error.to_string() }),
                format!("无法执行 {name} {}：{error}", path.display()),
            )
        })?;
    if !output.status.success() {
        return Err(ui_err_p(
            "ffmpeg.probe_failed",
            serde_json::json!({ "name": name, "stderr": String::from_utf8_lossy(&output.stderr).trim().to_string() }),
            format!(
                "{name} 版本检查失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    let version = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    if !version
        .to_ascii_lowercase()
        .starts_with(&format!("{} version", name.to_ascii_lowercase()))
    {
        return Err(ui_err_p(
            "ffmpeg.invalid_tool",
            serde_json::json!({ "name": name, "path": path.display().to_string() }),
            format!("所选程序不是有效的 {name}：{}", path.display()),
        ));
    }
    Ok(version)
}

#[cfg(all(test, unix))]
mod engine_probe_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn version_checks_validate_the_tool_and_bound_unresponsive_processes() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("tool");
        tokio::fs::write(&executable, "#!/bin/sh\nprintf 'ffmpeg version test\\n'\n")
            .await
            .unwrap();
        tokio::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
            .await
            .unwrap();
        let limit = std::time::Duration::from_secs(1);
        assert_eq!(
            read_tool_version(&executable, "FFmpeg", limit)
                .await
                .unwrap(),
            "ffmpeg version test"
        );
        assert!(read_tool_version(&executable, "FFprobe", limit)
            .await
            .is_err());
        tokio::fs::write(&executable, "#!/bin/sh\nexec sleep 20\n")
            .await
            .unwrap();
        let started = std::time::Instant::now();
        assert!(
            read_tool_version(&executable, "FFmpeg", std::time::Duration::from_millis(50))
                .await
                .unwrap_err()
                .contains("超时")
        );
        assert!(started.elapsed() < limit);
    }
}

#[tauri::command]
pub async fn get_ffmpeg_info(
    config_manager: State<'_, Mutex<ConfigManager>>,
) -> Result<FfmpegInfo, String> {
    // Diagnostics are an explicit recheck, including after replacing a binary.
    crate::core::ffmpeg::invalidate_discovery_cache();
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
            probe_path: None,
            probe_version: None,
        });
    }

    // 未启用 ffmpeg_native 特性时，回退到外部可执行文件版本信息
    #[cfg(not(feature = "ffmpeg_native"))]
    {
        // Never retain the settings lock while discovering or running binaries.
        let configured = config_manager
            .lock()
            .map_err(|e| format!("Config lock poisoned: {}", e))?
            .get_config()
            .ffmpeg_path
            .clone()
            .filter(|s| !s.trim().is_empty());
        let (ffmpeg_bin, ffprobe_bin) = if let Some(path) = configured {
            let ffmpeg = PathBuf::from(path);
            let probe_name = if cfg!(target_os = "windows") {
                "ffprobe.exe"
            } else {
                "ffprobe"
            };
            let probe = ffmpeg
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(|parent| parent.join(probe_name))
                .unwrap_or_else(|| PathBuf::from(probe_name));
            (ffmpeg, probe)
        } else {
            tokio::task::spawn_blocking(|| {
                Ok::<_, String>((
                    crate::core::ffmpeg::discover_ffmpeg_path()
                        .map_err(|error| error.to_string())?,
                    crate::core::ffmpeg::discover_ffprobe_path()
                        .map_err(|error| error.to_string())?,
                ))
            })
            .await
            .map_err(|error| error.to_string())??
        };
        let limit = std::time::Duration::from_secs(10);
        let (version, probe_version) = tokio::try_join!(
            read_tool_version(&ffmpeg_bin, "FFmpeg", limit),
            read_tool_version(&ffprobe_bin, "FFprobe", limit),
        )?;

        Ok(FfmpegInfo {
            mode: "external".to_string(),
            static_linked: false,
            library_versions: None,
            binary_version: Some(version),
            binary_path: Some(ffmpeg_bin.to_string_lossy().into()),
            probe_path: Some(ffprobe_bin.to_string_lossy().into()),
            probe_version: Some(probe_version),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FfmpegPathConfig {
    pub ffmpeg_path: Option<String>,
}

#[tauri::command]
pub async fn get_ffmpeg_path_config(
    config_manager: State<'_, Mutex<ConfigManager>>,
) -> Result<FfmpegPathConfig, String> {
    let cfg = config_manager
        .lock()
        .map_err(|e| format!("Config lock poisoned: {}", e))?;
    Ok(FfmpegPathConfig {
        ffmpeg_path: cfg.get_config().ffmpeg_path.clone(),
    })
}

#[tauri::command]
pub async fn set_ffmpeg_path_config(
    path: Option<String>,
    config_manager: State<'_, Mutex<ConfigManager>>,
) -> Result<(), String> {
    let mut cfg = config_manager
        .lock()
        .map_err(|e| format!("Config lock poisoned: {}", e))?;
    cfg.set_ffmpeg_path(path).map_err(|e| e.to_string())?;
    crate::core::ffmpeg::invalidate_discovery_cache();
    Ok(())
}

#[cfg(test)]
mod bounded_log_tests {
    use super::*;

    #[test]
    fn log_reads_require_a_basename_and_reject_large_files() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("app.log"), "hello").unwrap();
        assert_eq!(
            read_log_file_in(directory.path(), "app.log").unwrap(),
            "hello"
        );
        for invalid in [
            "../secret.log",
            "/tmp/secret.log",
            "sub/app.log",
            "sub\\app.log",
            "app.txt",
            "",
        ] {
            assert!(read_log_file_in(directory.path(), invalid).is_err());
        }
        std::fs::write(
            directory.path().join("large.log"),
            vec![0; 2 * 1024 * 1024 + 1],
        )
        .unwrap();
        assert!(read_log_file_in(directory.path(), "large.log")
            .unwrap_err()
            .contains("2 MB"));
    }

    #[cfg(unix)]
    #[test]
    fn log_reads_reject_symlinks_even_if_the_suffix_matches() {
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), directory.path().join("outside.log")).unwrap();
        assert!(read_log_file_in(directory.path(), "outside.log").is_err());
    }
}
