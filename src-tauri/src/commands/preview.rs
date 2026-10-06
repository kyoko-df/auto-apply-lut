//! Bounded, cancellable frame previews independent of WebView codec support.

use crate::core::ffmpeg::lut::{build_lut_filter, prepare_luts};
use crate::types::{ui_err, ui_err_p};
use crate::utils::config::ConfigManager;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};
use tauri::State;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

const CACHE_ENTRIES: usize = 32;
const CACHE_BYTES: usize = 32 * 1024 * 1024;
const FRAME_CACHE_BYTES: usize = 256 * 1024 * 1024;
const FRAME_FILE_BYTES: u64 = 128 * 1024 * 1024;
const LUT_CACHE_BYTES: usize = 128 * 1024 * 1024;
const LUT_FILE_BYTES: u64 = 64 * 1024 * 1024;
const IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const PREVIEW_TIMEOUT: Duration = Duration::from_secs(30);
const CANCELLED: &str = "\u{1f}preview.cancelled\u{1f}\u{1f}预览已取消";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewQuality {
    #[default]
    Fast,
    Accurate,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VideoPreviewRequest {
    pub video_path: String,
    #[serde(default)]
    pub time_seconds: f64,
    #[serde(default)]
    pub lut_path: Option<String>,
    #[serde(default = "default_intensity")]
    pub intensity: f32,
    /// Maximum image edge, bounded to 160–1920 px; defaults to 960.
    #[serde(default = "default_width")]
    pub max_width: u32,
    /// Requests with the same client ID replace and cancel their predecessor.
    #[serde(default = "default_client")]
    pub client_id: String,
    /// Accurate applies the color transform and LUT before downscaling.
    #[serde(default)]
    pub quality: PreviewQuality,
    #[serde(default = "default_input_space")]
    pub input_color_space: String,
}

fn default_intensity() -> f32 {
    1.0
}
fn default_width() -> u32 {
    960
}
fn default_client() -> String {
    "main".into()
}
fn default_input_space() -> String {
    "auto".into()
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoPreviewResponse {
    pub original_image: String,
    pub processed_image: String,
    pub time_seconds: f64,
    pub cached: bool,
    #[serde(skip)]
    pub(crate) photo_size: Option<(u32, u32)>,
}

impl VideoPreviewResponse {
    fn bytes(&self) -> usize {
        self.original_image.len() + self.processed_image.len()
    }
}

#[derive(Default)]
struct PreviewCache {
    entries: VecDeque<(String, VideoPreviewResponse)>,
    bytes: usize,
}

impl PreviewCache {
    fn get(&mut self, key: &str) -> Option<VideoPreviewResponse> {
        let position = self.entries.iter().position(|(entry, _)| entry == key)?;
        let entry = self.entries.remove(position)?;
        let mut response = entry.1.clone();
        response.cached = true;
        self.entries.push_back(entry);
        Some(response)
    }

    fn insert(&mut self, key: String, response: VideoPreviewResponse) {
        let size = response.bytes();
        if size > CACHE_BYTES {
            return;
        }
        if let Some(position) = self.entries.iter().position(|(entry, _)| entry == &key) {
            if let Some((_, old)) = self.entries.remove(position) {
                self.bytes -= old.bytes();
            }
        }
        while self.entries.len() >= CACHE_ENTRIES || self.bytes + size > CACHE_BYTES {
            if let Some((_, old)) = self.entries.pop_front() {
                self.bytes -= old.bytes();
            } else {
                break;
            }
        }
        self.bytes += size;
        self.entries.push_back((key, response));
    }
}

/// Arc leases keep cached files alive while FFmpeg reads them. TempDir removes
/// evicted artifacts and all remaining artifacts when the managed state drops.
struct Artifact {
    _directory: tempfile::TempDir,
    paths: Vec<PathBuf>,
    original_image: String,
    bytes: usize,
}

struct ArtifactCache {
    entries: VecDeque<(String, Arc<Artifact>)>,
    bytes: usize,
    byte_limit: usize,
}

impl ArtifactCache {
    fn new(byte_limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            byte_limit,
        }
    }
    fn get(&mut self, key: &str) -> Option<Arc<Artifact>> {
        let position = self.entries.iter().position(|(entry, _)| entry == key)?;
        let entry = self.entries.remove(position)?;
        let artifact = entry.1.clone();
        self.entries.push_back(entry);
        Some(artifact)
    }
    fn insert(&mut self, key: String, artifact: Arc<Artifact>) {
        if artifact.bytes > self.byte_limit {
            return;
        }
        // Never evict leased entries: retained on-disk files must remain included
        // in the budget. At most two uncached artifacts exist under render permits.
        if self.entries.iter().any(|(entry, _)| entry == &key) {
            return;
        }
        while self.entries.len() >= 8 || self.bytes + artifact.bytes > self.byte_limit {
            let Some(position) = self
                .entries
                .iter()
                .position(|(_, entry)| Arc::strong_count(entry) == 1)
            else {
                return;
            };
            if let Some((_, old)) = self.entries.remove(position) {
                self.bytes -= old.bytes;
            }
        }
        self.bytes += artifact.bytes;
        self.entries.push_back((key, artifact));
    }
}

pub struct PreviewState {
    active: Mutex<HashMap<String, (uuid::Uuid, CancellationToken)>>,
    cache: Mutex<PreviewCache>,
    frames: Mutex<ArtifactCache>,
    luts: Mutex<ArtifactCache>,
    pub(crate) permits: Arc<Semaphore>,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            active: Mutex::new(HashMap::new()),
            cache: Mutex::new(PreviewCache::default()),
            frames: Mutex::new(ArtifactCache::new(FRAME_CACHE_BYTES)),
            luts: Mutex::new(ArtifactCache::new(LUT_CACHE_BYTES)),
            permits: Arc::new(Semaphore::new(2)),
        }
    }
}

// Tauri can drop an invocation when its WebView disappears. Cleanup must also
// run on that path, not only when the rendering future returns normally.
pub(crate) struct ActivePreview<'a> {
    state: &'a PreviewState,
    client_id: &'a str,
    id: uuid::Uuid,
    pub(crate) token: CancellationToken,
}

impl Drop for ActivePreview<'_> {
    fn drop(&mut self) {
        self.token.cancel();
        if let Ok(mut active) = self.state.active.lock() {
            if active
                .get(self.client_id)
                .is_some_and(|(id, _)| *id == self.id)
            {
                active.remove(self.client_id);
            }
        }
    }
}

fn validate_request(request: &VideoPreviewRequest) -> Result<(), String> {
    if request.video_path.trim().is_empty() {
        return Err(ui_err("preview.no_video", "请先选择视频"));
    }
    if !request.time_seconds.is_finite() || request.time_seconds < 0.0 {
        return Err(ui_err("preview.bad_time", "预览时间必须是非负有限数值"));
    }
    if !request.intensity.is_finite() || !(0.0..=1.0).contains(&request.intensity) {
        return Err(ui_err(
            "preview.bad_intensity",
            "LUT 强度必须在 0 到 1 之间",
        ));
    }
    if !(160..=1920).contains(&request.max_width) {
        return Err(ui_err(
            "preview.bad_size",
            "预览尺寸必须在 160 到 1920 之间",
        ));
    }
    if request.client_id.is_empty() || request.client_id.len() > 128 {
        return Err(ui_err("preview.bad_client", "预览客户端标识无效"));
    }
    crate::core::ffmpeg::color::input_filter(&request.input_color_space)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn generate_video_preview(
    request: VideoPreviewRequest,
    preview_state: State<'_, PreviewState>,
    config_manager: State<'_, Mutex<ConfigManager>>,
) -> Result<VideoPreviewResponse, String> {
    // Registration inside generate_with_resolver precedes polling this future.
    // A delayed discovery can therefore never cancel a newer render afterward.
    generate_with_resolver(request, &preview_state, async {
        let configured_path = config_manager
            .lock()
            .map_err(|error| error.to_string())?
            .get_config()
            .ffmpeg_path
            .clone()
            .filter(|path| !path.trim().is_empty());
        if let Some(path) = configured_path {
            Ok(PathBuf::from(path))
        } else {
            tokio::task::spawn_blocking(crate::core::ffmpeg::discover_ffmpeg_path)
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())
        }
    })
    .await
}

#[tauri::command]
pub async fn cancel_video_preview(
    client_id: Option<String>,
    preview_state: State<'_, PreviewState>,
) -> Result<(), String> {
    cancel_client(&preview_state, &client_id.unwrap_or_else(default_client))
}

pub(crate) fn cancel_client(state: &PreviewState, client: &str) -> Result<(), String> {
    if let Some((_, token)) = state
        .active
        .lock()
        .map_err(|error| error.to_string())?
        .remove(client)
    {
        token.cancel();
    }
    Ok(())
}

impl PreviewState {
    pub(crate) fn begin<'a>(&'a self, client_id: &'a str) -> Result<ActivePreview<'a>, String> {
        let id = uuid::Uuid::new_v4();
        let token = CancellationToken::new();
        let mut active = self.active.lock().map_err(|e| e.to_string())?;
        if active.len() >= 64 && !active.contains_key(client_id) {
            return Err(ui_err("preview.too_many", "同时请求的预览过多，请稍后重试"));
        }
        if let Some((_, previous)) = active.insert(client_id.to_string(), (id, token.clone())) {
            previous.cancel();
        }
        Ok(ActivePreview {
            state: self,
            client_id,
            id,
            token,
        })
    }
    pub(crate) fn display_get(&self, key: &str) -> Result<Option<VideoPreviewResponse>, String> {
        Ok(self.cache.lock().map_err(|e| e.to_string())?.get(key))
    }
    pub(crate) fn display_put(
        &self,
        key: String,
        value: VideoPreviewResponse,
    ) -> Result<(), String> {
        self.cache
            .lock()
            .map_err(|e| e.to_string())?
            .insert(key, value);
        Ok(())
    }
}

async fn generate_with_resolver(
    request: VideoPreviewRequest,
    state: &PreviewState,
    resolver: impl Future<Output = Result<PathBuf, String>>,
) -> Result<VideoPreviewResponse, String> {
    validate_request(&request)?;
    let _active = state.begin(&request.client_id)?;
    let token = _active.token.clone();
    let ffmpeg_path = tokio::select! {
        biased;
        _ = token.cancelled() => return Err(CANCELLED.into()),
        resolved = resolver => resolved?,
    };
    let result = generate_cached(&request, state, &ffmpeg_path, &token).await;
    if token.is_cancelled() {
        Err(CANCELLED.into())
    } else {
        result
    }
}

#[cfg(test)]
async fn generate_with_state(
    request: VideoPreviewRequest,
    state: &PreviewState,
    ffmpeg_path: &Path,
) -> Result<VideoPreviewResponse, String> {
    generate_with_resolver(
        request,
        state,
        std::future::ready(Ok(ffmpeg_path.to_path_buf())),
    )
    .await
}

async fn fingerprint(path: &Path, hasher: &mut Sha256) -> Result<PathBuf, String> {
    let path = fs::canonicalize(path).await.map_err(|error| {
        ui_err_p(
            "fs.read_file",
            serde_json::json!({ "path": path.display().to_string(), "error": error.to_string() }),
            format!("无法读取 {}：{error}", path.display()),
        )
    })?;
    let metadata = fs::metadata(&path)
        .await
        .map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err(ui_err_p(
            "fs.not_a_file",
            serde_json::json!({ "path": path.display().to_string() }),
            format!("请选择文件：{}", path.display()),
        ));
    }
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(metadata.len().to_le_bytes());
    if let Ok(modified) = metadata.modified().and_then(|time| {
        time.duration_since(UNIX_EPOCH)
            .map_err(std::io::Error::other)
    }) {
        hasher.update(modified.as_nanos().to_le_bytes());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        hasher.update(metadata.dev().to_le_bytes());
        hasher.update(metadata.ino().to_le_bytes());
    }
    Ok(path)
}

async fn generate_cached(
    request: &VideoPreviewRequest,
    state: &PreviewState,
    ffmpeg_path: &Path,
    token: &CancellationToken,
) -> Result<VideoPreviewResponse, String> {
    let mut frame_hash = Sha256::new();
    frame_hash.update(b"video-preview-layered-v2");
    let executable = crate::core::ffmpeg::resolve_executable_path(ffmpeg_path);
    let ffmpeg_path = fingerprint(&executable, &mut frame_hash).await?;
    frame_hash.update(format!("{:.6}", request.time_seconds).as_bytes());
    frame_hash.update(request.max_width.to_le_bytes());
    frame_hash.update([match request.quality {
        PreviewQuality::Fast => 0,
        PreviewQuality::Accurate => 1,
    }]);
    let video_path = fingerprint(Path::new(&request.video_path), &mut frame_hash).await?;
    let frame_key = format!("{:x}", frame_hash.finalize());
    let mut hasher = Sha256::new();
    hasher.update(frame_key.as_bytes());
    hasher.update(request.input_color_space.as_bytes());
    let lut = if let Some(path) = request
        .lut_path
        .as_deref()
        .filter(|path| request.intensity > 0.0 && !path.trim().is_empty())
    {
        let mut lut_hash = Sha256::new();
        let path = fingerprint(Path::new(path), &mut lut_hash).await?;
        let key = format!("{:x}", lut_hash.finalize());
        hasher.update(key.as_bytes());
        hasher.update(request.intensity.to_le_bytes());
        Some((key, path))
    } else {
        None
    };
    let key = format!("{:x}", hasher.finalize());
    if token.is_cancelled() {
        return Err(CANCELLED.into());
    }
    // Complete hits do not queue behind other clients' expensive renders.
    if let Some(cached) = state
        .cache
        .lock()
        .map_err(|error| error.to_string())?
        .get(&key)
    {
        return Ok(cached);
    }
    let _permit = tokio::select! {
        biased;
        _ = token.cancelled() => return Err(CANCELLED.into()),
        permit = state.permits.acquire() => permit.map_err(|error| error.to_string())?,
    };
    // Another client may have completed the same preview while we waited.
    if let Some(cached) = state
        .cache
        .lock()
        .map_err(|error| error.to_string())?
        .get(&key)
    {
        return Ok(cached);
    }
    let cached_frame = state
        .frames
        .lock()
        .map_err(|error| error.to_string())?
        .get(&frame_key);
    let frame = if let Some(frame) = cached_frame {
        frame
    } else {
        let frame = Arc::new(decode_frame(request, &ffmpeg_path, &video_path, token).await?);
        if token.is_cancelled() {
            return Err(CANCELLED.into());
        }
        state
            .frames
            .lock()
            .map_err(|error| error.to_string())?
            .insert(frame_key, frame.clone());
        frame
    };
    let prepared = if let Some((lut_key, lut_path)) = lut {
        let cached = state
            .luts
            .lock()
            .map_err(|error| error.to_string())?
            .get(&lut_key);
        if let Some(prepared) = cached {
            Some(prepared)
        } else {
            let directory = temporary_directory("lut-preview-lut-")?;
            let lut_paths = [lut_path];
            let paths = tokio::select! {
                biased;
                _ = token.cancelled() => return Err(CANCELLED.into()),
                result = prepare_luts(&lut_paths, directory.path()) => result.map_err(|error| error.to_string())?,
            };
            let mut bytes = 0;
            for path in &paths {
                bytes += bounded_file_size(
                    path,
                    LUT_FILE_BYTES,
                    ui_err("preview.lut_too_large", "LUT 缓存过大，请使用较小的 LUT").as_str(),
                )
                .await? as usize;
            }
            let prepared = Arc::new(Artifact {
                _directory: directory,
                paths,
                original_image: String::new(),
                bytes,
            });
            if token.is_cancelled() {
                return Err(CANCELLED.into());
            }
            state
                .luts
                .lock()
                .map_err(|error| error.to_string())?
                .insert(lut_key, prepared.clone());
            Some(prepared)
        }
    } else {
        None
    };
    let response = render_frame(request, &ffmpeg_path, &frame, prepared.as_deref(), token).await?;
    if token.is_cancelled() {
        return Err(CANCELLED.into());
    }
    state
        .cache
        .lock()
        .map_err(|error| error.to_string())?
        .insert(key, response.clone());
    Ok(response)
}

fn temporary_directory(prefix: &str) -> Result<tempfile::TempDir, String> {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .map_err(|error| error.to_string())
}

fn scale_filter(edge: u32) -> String {
    // Scale square display pixels, including anamorphic inputs and auto rotation.
    let scale = format!("min(1,min({edge}/(iw*sar),{edge}/ih))");
    format!("scale=w='max(2,trunc(iw*sar*{scale}/2)*2)':h='max(2,trunc(ih*{scale}/2)*2)',setsar=1")
}

fn preview_command(ffmpeg: &Path) -> Command {
    let mut command = Command::new(ffmpeg);
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-threads",
        "2",
    ]);
    command
}

fn jpeg_output(command: &mut Command, label: &str, path: &Path) {
    command
        .args([
            "-map",
            label,
            "-frames:v",
            "1",
            "-an",
            "-sn",
            "-c:v",
            "mjpeg",
            "-q:v",
            "3",
            "-pix_fmt",
            "yuvj444p",
            "-threads:v",
            "1",
            "-fs",
        ])
        .arg(IMAGE_BYTES.to_string())
        .arg(path);
}

async fn bounded_file_size(path: &Path, limit: u64, error: &str) -> Result<u64, String> {
    let bytes = fs::metadata(path)
        .await
        .map_err(|_| {
            ui_err(
                "preview.no_frame",
                "此时间点没有可解码帧，请向前移动预览位置",
            )
        })?
        .len();
    if bytes == 0 || bytes >= limit {
        return Err(error.to_owned());
    }
    Ok(bytes)
}

async fn read_jpeg(path: &Path) -> Result<String, String> {
    bounded_file_size(
        path,
        IMAGE_BYTES,
        ui_err("preview.frame_too_large", "预览帧过大，请降低预览尺寸").as_str(),
    )
    .await?;
    let image = fs::read(path).await.map_err(|error| error.to_string())?;
    Ok(format!("data:image/jpeg;base64,{}", STANDARD.encode(image)))
}

async fn decode_frame(
    request: &VideoPreviewRequest,
    ffmpeg_path: &Path,
    video_path: &Path,
    token: &CancellationToken,
) -> Result<Artifact, String> {
    let directory = temporary_directory("lut-preview-frame-")?;
    let working_path = directory.path().join("working.nut");
    let original_path = directory.path().join("original.jpg");
    let scale = scale_filter(request.max_width);
    let graph = match request.quality {
        PreviewQuality::Fast => format!("[0:v:0]{scale},split[work][original];[work]format=gbrp16le[frame]"),
        PreviewQuality::Accurate => format!("[0:v:0]split[work][original_input];[work]format=gbrp16le[frame];[original_input]{scale}[original]"),
    };
    let mut command = preview_command(ffmpeg_path);
    command
        .arg("-ss")
        .arg(format!("{:.6}", request.time_seconds))
        .arg("-i")
        .arg(video_path)
        .args(["-filter_complex_threads", "1", "-filter_complex"])
        .arg(graph)
        .args([
            "-map",
            "[frame]",
            "-frames:v",
            "1",
            "-an",
            "-sn",
            "-dn",
            "-c:v",
            "ffv1",
            "-level",
            "3",
            "-g",
            "1",
            "-threads:v",
            "1",
            "-fs",
        ])
        .arg(FRAME_FILE_BYTES.to_string())
        .arg(&working_path);
    jpeg_output(&mut command, "[original]", &original_path);
    run_preview(command, token).await?;
    let bytes = bounded_file_size(
        &working_path,
        FRAME_FILE_BYTES,
        ui_err("preview.raw_frame_too_large", "原始帧过大，请切换快速预览").as_str(),
    )
    .await? as usize;
    let original_image = read_jpeg(&original_path).await?;
    // Retain one compressed working frame plus its display JPEG, not the whole video.
    fs::remove_file(&original_path)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Artifact {
        _directory: directory,
        paths: vec![working_path],
        bytes: bytes + original_image.len(),
        original_image,
    })
}

async fn render_frame(
    request: &VideoPreviewRequest,
    ffmpeg_path: &Path,
    frame: &Artifact,
    prepared: Option<&Artifact>,
    token: &CancellationToken,
) -> Result<VideoPreviewResponse, String> {
    let color_filter = crate::core::ffmpeg::color::input_filter(&request.input_color_space)
        .map_err(|error| error.to_string())?;
    let processed_image = if prepared.is_none() && color_filter == "null" {
        frame.original_image.clone()
    } else {
        let directory = temporary_directory("lut-preview-render-")?;
        let output = directory.path().join("processed.jpg");
        let lut_filter = build_lut_filter(
            prepared
                .map(|artifact| artifact.paths.as_slice())
                .unwrap_or(&[]),
            request.intensity,
        )
        .map_err(|error| error.to_string())?;
        let scale = if request.quality == PreviewQuality::Accurate {
            scale_filter(request.max_width)
        } else {
            "null".into()
        };
        let graph = format!("[0:v:0]{color_filter},{lut_filter},{scale}[processed]");
        let mut command = preview_command(ffmpeg_path);
        command
            .arg("-i")
            .arg(&frame.paths[0])
            .args(["-filter_complex_threads", "1", "-filter_complex"])
            .arg(graph);
        jpeg_output(&mut command, "[processed]", &output);
        run_preview(command, token).await?;
        read_jpeg(&output).await?
    };
    Ok(VideoPreviewResponse {
        original_image: frame.original_image.clone(),
        processed_image,
        time_seconds: request.time_seconds,
        cached: false,
        photo_size: None,
    })
}

async fn run_preview(mut command: Command, token: &CancellationToken) -> Result<(), String> {
    if token.is_cancelled() {
        return Err(CANCELLED.into());
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| {
        ui_err_p(
            "ffmpeg.spawn_failed",
            serde_json::json!({ "error": error.to_string() }),
            format!("无法启动 FFmpeg，请检查设置中的路径：{error}"),
        )
    })?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| ui_err("ffmpeg.no_stderr", "无法读取 FFmpeg 输出"))?;
    let errors = tokio::spawn(async move {
        let mut retained = Vec::new();
        let mut buffer = [0u8; 4096];
        while let Ok(count) = stderr.read(&mut buffer).await {
            if count == 0 {
                break;
            }
            retained.extend_from_slice(&buffer[..count]);
            if retained.len() > 16384 {
                retained.drain(..retained.len() - 16384);
            }
        }
        String::from_utf8_lossy(&retained).into_owned()
    });
    let status = tokio::select! {
        result = child.wait() => result.map_err(|error| error.to_string()),
        _ = token.cancelled() => { let _ = child.kill().await; Err(CANCELLED.into()) },
        _ = tokio::time::sleep(PREVIEW_TIMEOUT) => {
            let _ = child.kill().await;
            Err(ui_err("preview.timeout", "预览超时，请重试或选择视频中更早的位置"))
        },
    };
    let stderr = errors.await.unwrap_or_default();
    if !status?.success() {
        return Err(ui_err_p(
            "preview.render_failed",
            serde_json::json!({ "error": stderr.trim() }),
            format!("无法生成预览：{}", stderr.trim()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> VideoPreviewRequest {
        VideoPreviewRequest {
            video_path: "video.mp4".into(),
            time_seconds: 0.0,
            lut_path: None,
            intensity: 1.0,
            max_width: 960,
            client_id: "main".into(),
            quality: PreviewQuality::Fast,
            input_color_space: "auto".into(),
        }
    }

    #[test]
    fn rejects_invalid_time_strength_and_size() {
        let mut input = request();
        input.time_seconds = f64::NAN;
        assert!(validate_request(&input).is_err());
        input.time_seconds = 0.0;
        input.intensity = 1.1;
        assert!(validate_request(&input).is_err());
        input.intensity = 0.5;
        input.max_width = u32::MAX;
        assert!(validate_request(&input).is_err());
    }

    #[test]
    fn cache_eviction_and_replacement_stay_bounded() {
        let mut cache = PreviewCache::default();
        let response = VideoPreviewResponse {
            original_image: "a".repeat(500_000),
            processed_image: "b".repeat(500_000),
            time_seconds: 0.0,
            cached: false,
            photo_size: None,
        };
        for index in 0..100 {
            cache.insert(index.to_string(), response.clone());
        }
        assert!(cache.bytes <= CACHE_BYTES);
        assert!(cache.entries.len() <= CACHE_ENTRIES);
        assert!(cache.get("0").is_none());
        assert!(cache.get("99").unwrap().cached);
        let before = cache.bytes;
        cache.insert("99".into(), response);
        assert_eq!(before, cache.bytes);
    }

    async fn mean_rgb(ffmpeg: &Path, data_url: &str, directory: &Path) -> [f64; 3] {
        let jpeg = STANDARD
            .decode(data_url.split_once(',').unwrap().1)
            .unwrap();
        let path = directory.join("decoded.jpg");
        fs::write(&path, jpeg).await.unwrap();
        let output = Command::new(ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(path)
            .args([
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "pipe:1",
            ])
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut sums = [0.0; 3];
        for pixel in output.stdout.chunks_exact(3) {
            for channel in 0..3 {
                sums[channel] += pixel[channel] as f64;
            }
        }
        sums.map(|sum| sum / (output.stdout.len() / 3) as f64)
    }

    #[tokio::test]
    async fn ffmpeg_previews_blend_real_pixels_and_invalidate_changed_luts() {
        let Ok(ffmpeg) = crate::core::ffmpeg::discover_ffmpeg_path() else {
            eprintln!("FFmpeg unavailable; skipping pixel integration test");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("video ' [name],;.ppm");
        let lut = directory.path().join("invert ' [name],;.cube");
        let mut ppm = b"P6\n32 32\n255\n".to_vec();
        for _ in 0..32 * 32 {
            ppm.extend_from_slice(&[30, 90, 180]);
        }
        fs::write(&source, ppm).await.unwrap();
        let mut cube = "LUT_3D_SIZE 2\n".to_string();
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    cube.push_str(&format!("{} {} {}\n", 1 - r, 1 - g, 1 - b));
                }
            }
        }
        fs::write(&lut, &cube).await.unwrap();
        let state = PreviewState::default();
        let mut input = request();
        input.video_path = source.to_string_lossy().into();
        input.lut_path = Some(lut.to_string_lossy().into());
        input.intensity = 0.0;
        let zero = generate_with_state(input.clone(), &state, &ffmpeg)
            .await
            .unwrap();
        assert_eq!(zero.original_image, zero.processed_image);
        let mut unchanged = input.clone();
        unchanged.lut_path = None;
        unchanged.intensity = 0.8;
        assert!(
            generate_with_state(unchanged, &state, &ffmpeg)
                .await
                .unwrap()
                .cached
        );
        input.intensity = 1.0;
        let full = generate_with_state(input.clone(), &state, &ffmpeg)
            .await
            .unwrap();
        let original_rgb = mean_rgb(&ffmpeg, &full.original_image, directory.path()).await;
        let full_rgb = mean_rgb(&ffmpeg, &full.processed_image, directory.path()).await;
        for channel in 0..3 {
            assert!(
                (full_rgb[channel] - (255.0 - original_rgb[channel])).abs() < 6.0,
                "Inversion failed: original={original_rgb:?}, graded={full_rgb:?}"
            );
        }
        assert!(
            generate_with_state(input.clone(), &state, &ffmpeg)
                .await
                .unwrap()
                .cached
        );
        input.intensity = 0.5;
        let half = generate_with_state(input.clone(), &state, &ffmpeg)
            .await
            .unwrap();
        let half_rgb = mean_rgb(&ffmpeg, &half.processed_image, directory.path()).await;
        for channel in 0..3 {
            assert!(
                (half_rgb[channel] - (original_rgb[channel] + full_rgb[channel]) / 2.0).abs() < 4.0,
                "Blend failed: original={original_rgb:?}, half={half_rgb:?}, graded={full_rgb:?}"
            );
        }
        fs::write(&lut, format!("{cube}\n# changed LUT\n"))
            .await
            .unwrap();
        assert!(
            !generate_with_state(input, &state, &ffmpeg)
                .await
                .unwrap()
                .cached
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_preview_reaps_the_running_process() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("slow-ffmpeg");
        fs::write(&executable, "#!/bin/sh\nexec sleep 20\n")
            .await
            .unwrap();
        fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
            .await
            .unwrap();
        let source = directory.path().join("input.mp4");
        fs::write(&source, "placeholder").await.unwrap();
        let mut input = request();
        input.video_path = source.to_string_lossy().into();
        let state = PreviewState::default();
        let render = generate_with_state(input, &state, &executable);
        let cancel = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            state.active.lock().unwrap().get("main").unwrap().1.cancel();
        };
        let started = std::time::Instant::now();
        let (result, _) = tokio::join!(render, cancel);
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(state.active.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dropped_invocations_do_not_leak_active_clients() {
        let state = std::sync::Arc::new(PreviewState::default());
        let _busy = state.permits.acquire_many(2).await.unwrap();
        for index in 0..100 {
            let mut input = request();
            input.client_id = format!("client-{index}");
            let worker_state = state.clone();
            let worker = tokio::spawn(async move {
                generate_with_resolver(input, &worker_state, std::future::pending()).await
            });
            tokio::task::yield_now().await;
            assert_eq!(state.active.lock().unwrap().len(), 1);
            worker.abort();
            assert!(worker.await.unwrap_err().is_cancelled());
            assert!(state.active.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn stale_request_cleanup_preserves_its_replacement() {
        let state = std::sync::Arc::new(PreviewState::default());
        let _busy = state.permits.acquire_many(2).await.unwrap();
        let old_state = state.clone();
        let old = tokio::spawn(async move {
            generate_with_resolver(request(), &old_state, std::future::pending()).await
        });
        tokio::task::yield_now().await;
        let new_state = state.clone();
        let new = tokio::spawn(async move {
            generate_with_resolver(request(), &new_state, std::future::pending()).await
        });
        assert_eq!(old.await.unwrap().unwrap_err(), CANCELLED);
        assert_eq!(state.active.lock().unwrap().len(), 1);
        new.abort();
        assert!(new.await.unwrap_err().is_cancelled());
        assert!(state.active.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancellation_and_replacement_cover_slow_engine_discovery() {
        let state = Arc::new(PreviewState::default());
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (discovered_tx, discovered_rx) = tokio::sync::oneshot::channel();
        let old_state = state.clone();
        let old = tokio::spawn(async move {
            generate_with_resolver(request(), &old_state, async {
                started_tx.send(()).unwrap();
                discovered_rx.await.map_err(|error| error.to_string())
            })
            .await
        });
        started_rx.await.unwrap();
        assert_eq!(state.active.lock().unwrap().len(), 1);
        let (new_started_tx, new_started_rx) = tokio::sync::oneshot::channel();
        let new_state = state.clone();
        let new = tokio::spawn(async move {
            generate_with_resolver(request(), &new_state, async {
                new_started_tx.send(()).unwrap();
                std::future::pending().await
            })
            .await
        });
        new_started_rx.await.unwrap();
        assert_eq!(old.await.unwrap().unwrap_err(), CANCELLED);
        assert!(discovered_tx
            .send(PathBuf::from("old-discovery-result"))
            .is_err());
        assert_eq!(state.active.lock().unwrap().len(), 1);
        cancel_client(&state, "main").unwrap();
        assert_eq!(new.await.unwrap().unwrap_err(), CANCELLED);
        assert!(state.active.lock().unwrap().is_empty());
    }

    fn artifact(bytes: usize) -> Arc<Artifact> {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cached.bin");
        std::fs::write(&path, vec![0; bytes]).unwrap();
        Arc::new(Artifact {
            _directory: directory,
            paths: vec![path],
            original_image: String::new(),
            bytes,
        })
    }

    #[test]
    fn artifact_eviction_deletes_files_and_accounts_for_active_leases() {
        let mut cache = ArtifactCache::new(20);
        let first = artifact(10);
        let first_path = first.paths[0].clone();
        cache.insert("first".into(), first.clone());
        let second = artifact(10);
        let second_path = second.paths[0].clone();
        cache.insert("second".into(), second.clone());
        // Both files are currently leased; a third must remain uncached.
        cache.insert("third".into(), artifact(10));
        assert_eq!(cache.bytes, 20);
        assert!(cache.get("third").is_none());
        drop(second);
        cache.insert("third".into(), artifact(10));
        assert!(first_path.exists());
        assert!(!second_path.exists());
        assert_eq!(cache.bytes, 20);
        drop(first);
        drop(cache);
        assert!(!first_path.exists());
    }

    #[tokio::test]
    async fn accurate_preview_grades_before_scaling_and_cache_hits_bypass_busy_workers() {
        let Ok(ffmpeg) = crate::core::ffmpeg::discover_ffmpeg_path() else {
            eprintln!("FFmpeg unavailable; skipping quality integration test");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("checker.ppm");
        let lut = directory.path().join("quadratic.cube");
        let mut ppm = b"P6\n384 384\n255\n".to_vec();
        for y in 0..384 {
            for x in 0..384 {
                ppm.extend_from_slice(&[if (x + y) % 2 == 0 { 0 } else { 255 }; 3]);
            }
        }
        fs::write(&source, ppm).await.unwrap();
        let mut cube = "LUT_3D_SIZE 3\n".to_string();
        for b in 0..3 {
            for g in 0..3 {
                for r in 0..3 {
                    cube.push_str(&format!(
                        "{} {} {}\n",
                        (r as f32 / 2.0).powi(2),
                        (g as f32 / 2.0).powi(2),
                        (b as f32 / 2.0).powi(2)
                    ));
                }
            }
        }
        fs::write(&lut, cube).await.unwrap();
        let mut input = request();
        input.video_path = source.to_string_lossy().into();
        input.lut_path = Some(lut.to_string_lossy().into());
        input.max_width = 160;
        let state = PreviewState::default();
        let fast = generate_with_state(input.clone(), &state, &ffmpeg)
            .await
            .unwrap();
        input.quality = PreviewQuality::Accurate;
        let accurate = generate_with_state(input.clone(), &state, &ffmpeg)
            .await
            .unwrap();
        let fast_rgb = mean_rgb(&ffmpeg, &fast.processed_image, directory.path()).await;
        let accurate_rgb = mean_rgb(&ffmpeg, &accurate.processed_image, directory.path()).await;
        assert!(
            accurate_rgb[0] - fast_rgb[0] > 35.0,
            "fast={fast_rgb:?}, accurate={accurate_rgb:?}"
        );
        let _busy = state.permits.acquire_many(2).await.unwrap();
        let cached = tokio::time::timeout(
            Duration::from_secs(1),
            generate_with_state(input, &state, &ffmpeg),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(cached.cached);
        assert_eq!(cached.processed_image, accurate.processed_image);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn strength_edits_reuse_video_decode_and_prepared_lut() {
        use std::os::unix::fs::PermissionsExt;
        let Ok(ffmpeg) = crate::core::ffmpeg::discover_ffmpeg_path() else {
            eprintln!("FFmpeg unavailable; skipping layer integration test");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.mp4");
        let generated = Command::new(&ffmpeg)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=10",
                "-t",
                "1",
                "-c:v",
                "libx264",
                "-threads",
                "1",
            ])
            .arg(&source)
            .output()
            .await
            .unwrap();
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );
        let lut = directory.path().join("inverse.cube");
        let mut cube = "LUT_3D_SIZE 2\n".to_owned();
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    cube.push_str(&format!("{} {} {}\n", 1 - r, 1 - g, 1 - b));
                }
            }
        }
        fs::write(&lut, cube).await.unwrap();
        let log = directory.path().join("invocations.txt");
        let wrapper = directory.path().join("ffmpeg-wrapper");
        let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"));
        fs::write(
            &wrapper,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" >> {}\nexec {} \"$@\"\n",
                quote(&log),
                quote(&ffmpeg)
            ),
        )
        .await
        .unwrap();
        fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700))
            .await
            .unwrap();
        let state = PreviewState::default();
        let mut input = request();
        input.video_path = source.to_string_lossy().into();
        input.lut_path = Some(lut.to_string_lossy().into());
        input.intensity = 0.5;
        let half = generate_with_state(input.clone(), &state, &wrapper)
            .await
            .unwrap();
        let lut_prepared = state.luts.lock().unwrap().entries[0].1.paths[0].clone();
        let lut_modified = fs::metadata(&lut_prepared)
            .await
            .unwrap()
            .modified()
            .unwrap();
        input.intensity = 0.75;
        let stronger = generate_with_state(input.clone(), &state, &wrapper)
            .await
            .unwrap();
        assert_ne!(half.processed_image, stronger.processed_image);
        assert_eq!(half.original_image, stronger.original_image);
        assert_eq!(
            fs::metadata(&lut_prepared)
                .await
                .unwrap()
                .modified()
                .unwrap(),
            lut_modified
        );
        let calls = fs::read_to_string(&log).await.unwrap();
        assert_eq!(
            calls
                .lines()
                .filter(|line| *line == source.canonicalize().unwrap().to_string_lossy())
                .count(),
            1,
            "one source decode per frame"
        );
        input.time_seconds = 0.5;
        generate_with_state(input, &state, &wrapper).await.unwrap();
        let calls = fs::read_to_string(&log).await.unwrap();
        assert_eq!(
            calls
                .lines()
                .filter(|line| *line == source.canonicalize().unwrap().to_string_lossy())
                .count(),
            2,
            "seeking decodes the new source frame"
        );
        drop(state);
        assert!(
            !lut_prepared.exists(),
            "cache files must be removed on shutdown"
        );
    }

    #[tokio::test]
    async fn accurate_hdr_preview_matches_direct_export_filter_order() {
        let Ok(ffmpeg) = crate::core::ffmpeg::discover_ffmpeg_path() else {
            eprintln!("FFmpeg unavailable; skipping explicit HDR integration test");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        for (space, transfer) in [("rec2020-pq", "smpte2084"), ("rec2020-hlg", "arib-std-b67")] {
            let source = directory.path().join(format!("{space}.mkv"));
            let generated = Command::new(&ffmpeg)
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=gray:s=64x64:r=1",
                    "-frames:v",
                    "1",
                    "-pix_fmt",
                    "yuv420p10le",
                    "-color_primaries",
                    "bt2020",
                    "-color_trc",
                    transfer,
                    "-colorspace",
                    "bt2020nc",
                    "-c:v",
                    "ffv1",
                    "-threads",
                    "1",
                ])
                .arg(&source)
                .output()
                .await
                .unwrap();
            assert!(
                generated.status.success(),
                "{}",
                String::from_utf8_lossy(&generated.stderr)
            );
            let state = PreviewState::default();
            let mut input = request();
            input.video_path = source.to_string_lossy().into();
            input.quality = PreviewQuality::Accurate;
            input.input_color_space = space.into();
            let preview = generate_with_state(input, &state, &ffmpeg).await.unwrap();
            let output = directory.path().join("direct.jpg");
            let color = crate::core::ffmpeg::color::input_filter(space).unwrap();
            let generated = Command::new(&ffmpeg)
                .args(["-y", "-v", "error", "-i"])
                .arg(&source)
                .arg("-vf")
                .arg(format!("{color},{}", scale_filter(960)))
                .args([
                    "-frames:v",
                    "1",
                    "-c:v",
                    "mjpeg",
                    "-q:v",
                    "3",
                    "-pix_fmt",
                    "yuvj444p",
                    "-threads",
                    "1",
                ])
                .arg(&output)
                .output()
                .await
                .unwrap();
            assert!(
                generated.status.success(),
                "{}",
                String::from_utf8_lossy(&generated.stderr)
            );
            let direct = read_jpeg(&output).await.unwrap();
            let preview_rgb = mean_rgb(&ffmpeg, &preview.processed_image, directory.path()).await;
            let direct_rgb = mean_rgb(&ffmpeg, &direct, directory.path()).await;
            for channel in 0..3 {
                assert!((preview_rgb[channel]-direct_rgb[channel]).abs() < 3.0,
                    "{space} must match direct source transform: preview={preview_rgb:?}, direct={direct_rgb:?}");
            }
            assert_ne!(
                preview.original_image, preview.processed_image,
                "explicit HDR interpretation must actually change pixels"
            );
        }
    }

    /// Manual, real-media baseline. Timings are reported rather than asserted:
    /// CPU scheduling and source compression make absolute CI budgets unreliable.
    #[tokio::test]
    #[ignore = "manual 4K preview benchmark; run with --ignored --nocapture"]
    async fn four_k_preview_benchmark() {
        let ffmpeg =
            crate::core::ffmpeg::discover_ffmpeg_path().expect("benchmark requires FFmpeg");
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("4k.mp4");
        let generated = Command::new(&ffmpeg)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=3840x2160:rate=24:duration=0.5",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-threads",
                "2",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&source)
            .output()
            .await
            .unwrap();
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );
        let lut = directory.path().join("inverse.cube");
        fs::write(
            &lut,
            "LUT_3D_SIZE 2\n1 1 1\n0 1 1\n1 0 1\n0 0 1\n1 1 0\n0 1 0\n1 0 0\n0 0 0\n",
        )
        .await
        .unwrap();
        let mut input = request();
        input.video_path = source.to_string_lossy().into();
        input.lut_path = Some(lut.to_string_lossy().into());
        input.max_width = 1280;
        input.intensity = 0.5;
        let state = PreviewState::default();
        for (label, strength, quality) in [
            ("cold fast", 0.5, PreviewQuality::Fast),
            ("warm strength", 0.75, PreviewQuality::Fast),
            ("complete cache", 0.75, PreviewQuality::Fast),
            ("cold accurate", 0.5, PreviewQuality::Accurate),
            ("warm accurate strength", 0.75, PreviewQuality::Accurate),
        ] {
            input.intensity = strength;
            input.quality = quality;
            let start = std::time::Instant::now();
            let result = generate_with_state(input.clone(), &state, &ffmpeg)
                .await
                .unwrap();
            println!(
                "4K → 1280 px preview | {label}: {:.1} ms | result cache={}",
                start.elapsed().as_secs_f64() * 1000.0,
                result.cached
            );
        }
    }
}
