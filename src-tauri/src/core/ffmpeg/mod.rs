//! FFmpeg集成模块
//! 提供视频处理、LUT应用和格式转换功能

use crate::types::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::process::Command as AsyncCommand;
use tokio::sync::Mutex;

pub mod color;
mod engine_cache;
pub mod lut;
pub mod photo_bridge;
pub mod processor;
pub mod utils;

/// 对外导出：发现打包或系统中的 ffmpeg/ffprobe 路径
pub fn discover_ffmpeg_path() -> AppResult<PathBuf> {
    discover_ffmpeg_pair().map(|(ffmpeg, _)| ffmpeg)
}

pub fn discover_ffprobe_path() -> AppResult<PathBuf> {
    discover_ffmpeg_pair().map(|(_, ffprobe)| ffprobe)
}

pub fn discover_ffplay_path() -> AppResult<PathBuf> {
    discover_optional_tool("ffplay", "FFPLAY_PATH")
}

/// Discover a matching tool pair. A self-contained bundle takes precedence over
/// developer environment variables and system installations. The explicit app
/// setting is applied by callers before discovery.
pub fn discover_ffmpeg_pair() -> AppResult<(PathBuf, PathBuf)> {
    engine_cache::discover_cached(discover_ffmpeg_pair_uncached)
}

/// Explicit diagnostics and executable-setting changes must revalidate the pair.
/// Ordinary preview/metadata requests reuse successful discovery until either
/// executable, its permissions, or the discovery environment changes.
pub fn invalidate_discovery_cache() {
    engine_cache::invalidate();
}

fn discover_ffmpeg_pair_uncached() -> AppResult<(PathBuf, PathBuf)> {
    let platform = std::env::consts::OS;
    let directories = std::env::current_exe()
        .ok()
        .map(|executable| packaged_tool_directories(&executable, platform, std::env::consts::ARCH))
        .unwrap_or_default();
    if let Some(pair) = find_tool_pair(&directories, platform) {
        return Ok(pair);
    }

    // Preserve explicit development overrides when no bundled pair is available.
    // Resolve bare command names before deriving the sibling so export, preview
    // and metadata do not accidentally use different FFmpeg installations.
    let ffmpeg_override = environment_tool("FFMPEG_PATH");
    let ffprobe_override = environment_tool("FFPROBE_PATH");
    let override_pair = match (ffmpeg_override, ffprobe_override) {
        (Some(ffmpeg), Some(ffprobe)) => Some((ffmpeg, ffprobe)),
        (Some(ffmpeg), None) => {
            let ffprobe = ffmpeg.with_file_name(tool_filename("ffprobe", platform));
            Some((ffmpeg, ffprobe))
        }
        (None, Some(ffprobe)) => {
            let ffmpeg = ffprobe.with_file_name(tool_filename("ffmpeg", platform));
            Some((ffmpeg, ffprobe))
        }
        (None, None) => None,
    };
    if let Some((ffmpeg, ffprobe)) = override_pair {
        if executable_works(&ffmpeg) && executable_works(&ffprobe) {
            return Ok((ffmpeg, ffprobe));
        }
    }

    find_tool_pair(&system_tool_directories(), platform).ok_or_else(|| {
        AppError::FFmpeg("未找到可用的 FFmpeg / FFprobe。请使用包含媒体引擎的完整应用包，或在设置中指定 FFmpeg 路径。".into())
    })
}

fn tool_filename(tool: &str, platform: &str) -> String {
    if platform == "windows" {
        format!("{tool}.exe")
    } else {
        tool.to_owned()
    }
}

/// Pure path construction also covers the Tauri resource layout in installed
/// Linux/AppImage builds and architecture-specific Windows/macOS bundles.
fn packaged_tool_directories(executable: &Path, platform: &str, arch: &str) -> Vec<PathBuf> {
    let Some(exe_dir) = executable.parent() else {
        return Vec::new();
    };
    let mut roots = Vec::new();
    if platform == "macos" {
        // <app>.app/Contents/MacOS/<exe> -> Contents/Resources (one level up).
        if let Some(contents) = exe_dir.parent() {
            roots.push(contents.join("Resources"));
        }
    } else if platform == "linux" {
        if let (Some(prefix), Some(name)) = (exe_dir.parent(), executable.file_name()) {
            roots.push(prefix.join("lib").join(name));
            roots.push(prefix.join("lib").join(env!("CARGO_PKG_NAME")));
        }
    }
    roots.push(exe_dir.to_path_buf());

    let mut directories = Vec::new();
    for root in roots {
        // Tauri resources maps and array-style resources differ by the preserved
        // top-level resources/ prefix. Accept both, plus the old flat layout.
        for resource_root in [
            root.clone(),
            root.join("resources"),
            root.join("resources/resources"),
        ] {
            directories.push(resource_root.join("bin").join(platform).join(arch));
            directories.push(resource_root.join("bin").join(platform));
        }
        directories.push(root);
    }
    directories.dedup();
    directories
}

fn find_tool_pair(directories: &[PathBuf], platform: &str) -> Option<(PathBuf, PathBuf)> {
    for directory in directories {
        let ffmpeg = directory.join(tool_filename("ffmpeg", platform));
        let ffprobe = directory.join(tool_filename("ffprobe", platform));
        if ffmpeg.is_file()
            && ffprobe.is_file()
            && executable_works(&ffmpeg)
            && executable_works(&ffprobe)
        {
            return Some((ffmpeg, ffprobe));
        }
    }
    None
}

fn system_tool_directories() -> Vec<PathBuf> {
    let mut directories: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if cfg!(target_os = "windows") {
        directories.extend([
            PathBuf::from(r"C:\ffmpeg\bin"),
            PathBuf::from(r"C:\Program Files\ffmpeg\bin"),
        ]);
    } else {
        directories.extend([
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/opt/homebrew/bin"),
        ]);
    }
    directories
}

fn environment_tool(name: &str) -> Option<PathBuf> {
    let value = std::env::var_os(name).filter(|value| !value.is_empty())?;
    resolve_executable_in_directories(
        Path::new(&value),
        &system_tool_directories(),
        std::env::consts::OS,
    )
}

/// Resolve a configured bare command before stat/fingerprinting, without
/// starting a validation subprocess. Explicit paths retain their precedence.
pub(crate) fn resolve_executable_path(path: &Path) -> PathBuf {
    resolve_executable_in_directories(path, &system_tool_directories(), std::env::consts::OS)
        .unwrap_or_else(|| path.to_path_buf())
}

fn resolve_executable_in_directories(
    path: &Path,
    directories: &[PathBuf],
    platform: &str,
) -> Option<PathBuf> {
    if path.components().count() > 1 || path.is_absolute() {
        return Some(path.to_path_buf());
    }
    let command = if platform == "windows" && path.extension().is_none() {
        path.with_extension("exe")
    } else {
        path.to_path_buf()
    };
    directories
        .iter()
        .map(|directory| directory.join(&command))
        .find(|candidate| candidate.is_file())
}

fn discover_optional_tool(tool: &str, environment_name: &str) -> AppResult<PathBuf> {
    let platform = std::env::consts::OS;
    let directories = std::env::current_exe()
        .ok()
        .map(|executable| packaged_tool_directories(&executable, platform, std::env::consts::ARCH))
        .unwrap_or_default();
    let candidates = directories
        .into_iter()
        .map(|directory| directory.join(tool_filename(tool, platform)))
        .chain(environment_tool(environment_name))
        .chain(
            system_tool_directories()
                .into_iter()
                .map(|directory| directory.join(tool_filename(tool, platform))),
        );
    candidates
        .into_iter()
        .find(|path| executable_works(path))
        .ok_or_else(|| AppError::FFmpeg(format!("{tool} executable not found")))
}

/// Discovery must not hang app startup on a broken or incorrectly configured tool.
/// A timed-out child is always killed and reaped; output is not buffered.
pub(crate) fn executable_works(path: &Path) -> bool {
    executable_works_with_timeout(path, std::time::Duration::from_secs(3))
}

fn executable_works_with_timeout(path: &Path, timeout: std::time::Duration) -> bool {
    if path.components().count() > 1 && !path.is_file() {
        return false;
    }
    let Ok(mut child) = Command::new(path)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

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
        let (ffmpeg_path, ffprobe_path) = discover_ffmpeg_pair()?;

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
            return Err(AppError::FFmpeg(format!(
                "FFmpeg not found at: {:?}",
                ffmpeg_path
            )));
        }
        if !ffprobe_path.exists() {
            return Err(AppError::FFmpeg(format!(
                "FFprobe not found at: {:?}",
                ffprobe_path
            )));
        }

        Ok(Self {
            ffmpeg_path,
            ffprobe_path,
            default_settings: EncodingSettings::default(),
            progress_callbacks: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// 获取视频信息
    pub async fn get_video_info(&self, input_path: &Path) -> AppResult<VideoInfo> {
        let output = AsyncCommand::new(&self.ffprobe_path)
            .args([
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                input_path.to_str().unwrap(),
            ])
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(e.to_string()))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::FFmpeg(format!("FFprobe failed: {}", error)));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let mut probe: ProbeResult = serde_json::from_str(&json_str)
            .map_err(|e| AppError::FFmpeg(format!("Failed to parse FFprobe output: {}", e)))?;

        // 将 streams 转换为内部结构
        let streams: Vec<StreamInfo> = probe.streams.drain(..).map(Into::into).collect();
        let (duration, bitrate, format, video_codec, audio_codec, fps, width, height) =
            if let Some(fmt) = probe.format {
                let duration = fmt.duration.parse::<f64>().unwrap_or(0.0);
                let bitrate = fmt.bit_rate.parse::<u64>().unwrap_or(0);
                let format = fmt.format_name;
                let mut video_codec = String::from("unknown");
                let mut audio_codec = None;
                let mut fps = 0.0;
                let mut width = 0;
                let mut height = 0;
                for s in &streams {
                    if s.codec_type == "video" {
                        video_codec = s.codec_name.clone();
                        fps = s.fps.unwrap_or(0.0);
                        width = s.width.unwrap_or(0);
                        height = s.height.unwrap_or(0);
                    } else if s.codec_type == "audio" {
                        audio_codec = Some(s.codec_name.clone());
                    }
                }
                (
                    duration,
                    bitrate,
                    format,
                    video_codec,
                    audio_codec,
                    fps,
                    width,
                    height,
                )
            } else {
                (
                    0.0,
                    0,
                    String::from("unknown"),
                    String::from("unknown"),
                    None,
                    0.0,
                    0,
                    0,
                )
            };

        Ok(VideoInfo {
            duration,
            width,
            height,
            fps,
            video_codec,
            audio_codec,
            bitrate,
            format,
            streams,
        })
    }

    // ... existing code ...
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

    #[test]
    fn configured_command_names_resolve_without_probing_or_global_environment_changes() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        // These are deliberately not runnable binaries: resolution must only
        // inspect paths, never add -version processes to the preview hot path.
        std::fs::write(second.join("ffmpeg"), "second").unwrap();
        let directories = [first.clone(), second.clone()];
        assert_eq!(
            resolve_executable_in_directories(Path::new("ffmpeg"), &directories, "macos"),
            Some(second.join("ffmpeg"))
        );
        std::fs::write(first.join("ffmpeg"), "first").unwrap();
        assert_eq!(
            resolve_executable_in_directories(Path::new("ffmpeg"), &directories, "macos"),
            Some(first.join("ffmpeg"))
        );
        assert_eq!(
            resolve_executable_in_directories(&second.join("ffmpeg"), &directories, "macos"),
            Some(second.join("ffmpeg"))
        );
        std::fs::write(first.join("ffmpeg.exe"), "windows").unwrap();
        assert_eq!(
            resolve_executable_in_directories(Path::new("ffmpeg"), &directories, "windows"),
            Some(first.join("ffmpeg.exe"))
        );
        assert_eq!(
            resolve_executable_in_directories(Path::new("not-installed"), &directories, "macos"),
            None
        );
    }

    #[test]
    fn bundle_directories_follow_macos_contents_and_current_architecture() {
        let directories = packaged_tool_directories(
            Path::new("/Applications/LUTlab.app/Contents/MacOS/LUTlab"),
            "macos",
            "aarch64",
        );
        assert_eq!(
            directories[0],
            PathBuf::from("/Applications/LUTlab.app/Contents/Resources/bin/macos/aarch64")
        );
        assert!(directories.contains(&PathBuf::from(
            "/Applications/LUTlab.app/Contents/Resources/resources/bin/macos/aarch64"
        )));
        assert!(!directories
            .iter()
            .any(|path| path.to_string_lossy().contains("x86_64")));
    }

    #[test]
    fn bundle_directories_support_windows_arm_and_linux_appimage() {
        let windows =
            packaged_tool_directories(Path::new("/package/LUTlab.exe"), "windows", "aarch64");
        assert!(windows.contains(&PathBuf::from("/package/resources/bin/windows/aarch64")));
        assert_eq!(tool_filename("ffprobe", "windows"), "ffprobe.exe");
        let linux = packaged_tool_directories(
            Path::new("/AppDir/usr/bin/auto-apply-lut"),
            "linux",
            "x86_64",
        );
        assert_eq!(
            linux[0],
            PathBuf::from("/AppDir/usr/lib/auto-apply-lut/bin/linux/x86_64")
        );
        assert!(linux.contains(&PathBuf::from(
            "/AppDir/usr/lib/auto-apply-lut/resources/bin/linux/x86_64"
        )));
    }

    #[cfg(unix)]
    fn fake_tool(directory: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(directory).unwrap();
        let path = directory.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn discovers_relocated_bundle_pair_without_path_or_system_tools() {
        let temporary = tempfile::tempdir().unwrap();
        let application = temporary.path().join("LUT lab 中文.app");
        let executable = application.join("Contents/MacOS/LUTlab");
        let tools = application.join("Contents/Resources/resources/bin/macos/aarch64");
        let ffmpeg = fake_tool(&tools, "ffmpeg", "exit 0");
        let ffprobe = fake_tool(&tools, "ffprobe", "exit 0");
        let directories = packaged_tool_directories(&executable, "macos", "aarch64");
        assert_eq!(
            find_tool_pair(&directories, "macos"),
            Some((ffmpeg, ffprobe))
        );
        assert_eq!(
            find_tool_pair(
                &packaged_tool_directories(&executable, "macos", "x86_64"),
                "macos"
            ),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn discovery_keeps_tools_paired_and_skips_broken_candidates() {
        let temporary = tempfile::tempdir().unwrap();
        let incomplete = temporary.path().join("incomplete");
        fake_tool(&incomplete, "ffmpeg", "exit 0");
        let broken = temporary.path().join("broken");
        fake_tool(&broken, "ffmpeg", "exit 0");
        fake_tool(&broken, "ffprobe", "exit 1");
        let packaged = temporary.path().join("packaged");
        let ffmpeg = fake_tool(&packaged, "ffmpeg", "exit 0");
        let ffprobe = fake_tool(&packaged, "ffprobe", "exit 0");
        let system = temporary.path().join("system");
        fake_tool(&system, "ffmpeg", "exit 0");
        fake_tool(&system, "ffprobe", "exit 0");
        assert_eq!(
            find_tool_pair(&[incomplete, broken, packaged, system], "macos"),
            Some((ffmpeg, ffprobe))
        );
    }

    #[cfg(unix)]
    #[test]
    fn discovery_probe_times_out_and_reaps_child() {
        let temporary = tempfile::tempdir().unwrap();
        let pid_file = temporary.path().join("pid");
        let tool = fake_tool(
            temporary.path(),
            "hanging-tool",
            &format!("if [ \"$1\" = \"--warm-up\" ]; then exit 0; fi\necho $$ > '{}'\nexec /bin/sleep 30", pid_file.display()),
        );
        assert!(Command::new(&tool)
            .arg("--warm-up")
            .status()
            .unwrap()
            .success());
        let started = std::time::Instant::now();
        assert!(!executable_works_with_timeout(
            &tool,
            std::time::Duration::from_millis(500)
        ));
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        let pid = std::fs::read_to_string(pid_file).unwrap();
        assert!(!Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success());
    }

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

/// 分辨率
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

/// 编码设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingSettings {
    /// 视频编码器，例如 libx264、libx265、copy
    pub video_codec: String,
    /// 音频编码器，例如 aac、copy
    pub audio_codec: String,
    /// 预设，例如 ultrafast、medium、slow
    pub preset: String,
    /// 质量因子（CRF），当未指定码率时使用
    pub crf: i32,
    /// 目标分辨率，None 表示保持原分辨率
    pub resolution: Option<Resolution>,
    /// 目标帧率
    pub fps: Option<f64>,
    /// 目标码率（如 "2M"），与 crf 互斥，优先使用码率
    pub bitrate: Option<String>,
    /// 额外参数，形如（"-movflags" => "+faststart"）
    pub extra_params: std::collections::HashMap<String, String>,
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
            extra_params: std::collections::HashMap::new(),
        }
    }
}

/// 媒体流信息（统一对外结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    pub index: u32,
    pub codec_type: String, // video / audio / subtitle / data
    pub codec_name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub duration: Option<f64>,
}

/// 视频信息（FFprobe侧重的技术信息）
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

/// 解析帧率字符串，如 "30000/1001" 或 "30"
fn parse_frame_rate_str(s: &str) -> Option<f64> {
    if let Some((num, den)) = s.split_once('/') {
        let n: f64 = num.trim().parse().ok()?;
        let d: f64 = den.trim().parse().ok()?;
        if d != 0.0 {
            Some(n / d)
        } else {
            None
        }
    } else {
        s.trim().parse::<f64>().ok()
    }
}

impl From<ProbeStream> for StreamInfo {
    fn from(ps: ProbeStream) -> Self {
        let fps = parse_frame_rate_str(&ps.r_frame_rate);
        let duration = ps.duration.as_ref().and_then(|d| d.parse::<f64>().ok());
        StreamInfo {
            index: ps.index,
            codec_type: ps.codec_type,
            codec_name: ps.codec_name,
            width: ps.width,
            height: ps.height,
            fps,
            duration,
        }
    }
}
