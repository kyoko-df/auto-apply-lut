use super::{
    batch_manager::{BatchRequest, BatchResponse},
    preview::{PreviewState, VideoPreviewResponse},
};
use crate::{
    core::{
        ffmpeg::{
            lut::prepare_luts, photo_bridge::grade_rgb16_with_timeout, processor::VideoProcessor,
        },
        lut::LutManager,
        photo::{
            self, metadata::PhotoInfo, processor::PhotoProcessor, AlphaPolicy, PhotoFrame,
            PhotoSpace, SourceInterpretation,
        },
        task::TaskManager,
    },
    types::ui_err,
    utils::config::ConfigManager,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::State;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhotoViewport {
    Fit {
        max_edge: u32,
    },
    Region {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoPreviewRequest {
    pub photo_path: String,
    pub lut_path: Option<String>,
    pub intensity: f32,
    pub source_interpretation: SourceInterpretation,
    pub lut_space: Option<PhotoSpace>,
    pub lut_fingerprint: Option<String>,
    pub viewport: PhotoViewport,
    pub quality: String,
    pub alpha_policy: AlphaPolicy,
    pub client_id: String,
    pub request_id: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct PhotoPreviewResponse {
    pub original_image: String,
    pub processed_image: String,
    pub request_id: String,
    pub source_version: String,
    pub width: u32,
    pub height: u32,
    pub cached: bool,
}

struct Normalized {
    directory: tempfile::TempDir,
    width: u32,
    height: u32,
    alpha: bool,
    bytes: usize,
}
impl Normalized {
    fn store(frame: &PhotoFrame) -> Result<Self, String> {
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let mut file =
            std::fs::File::create(directory.path().join("rgb16.bin")).map_err(|e| e.to_string())?;
        for chunk in frame.rgb.chunks(32768) {
            let bytes: Vec<u8> = chunk.iter().flat_map(|v| v.to_le_bytes()).collect();
            file.write_all(&bytes).map_err(|e| e.to_string())?;
        }
        if let Some(alpha) = &frame.alpha {
            for chunk in alpha.chunks(32768) {
                let bytes: Vec<u8> = chunk.iter().flat_map(|v| v.to_le_bytes()).collect();
                file.write_all(&bytes).map_err(|e| e.to_string())?;
            }
        }
        Ok(Self {
            directory,
            width: frame.width,
            height: frame.height,
            alpha: frame.alpha.is_some(),
            bytes: frame.rgb.len() * 2 + frame.alpha.as_ref().map_or(0, |a| a.len() * 2),
        })
    }
    fn load(&self, token: &CancellationToken) -> Result<PhotoFrame, String> {
        let mut file = std::fs::File::open(self.directory.path().join("rgb16.bin"))
            .map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() != self.bytes as u64 {
            return Err(ui_err("photo.cache_stale", "照片缓存已变化，请重试"));
        }
        let pixels = self.width as usize * self.height as usize;
        let mut values = Vec::with_capacity(self.bytes / 2);
        let mut buffer = [0u8; 65536];
        let mut remaining = self.bytes;
        while remaining > 0 {
            photo::check_cancel(token)?;
            let count = remaining.min(buffer.len());
            file.read_exact(&mut buffer[..count])
                .map_err(|e| e.to_string())?;
            values.extend(
                buffer[..count]
                    .chunks_exact(2)
                    .map(|v| u16::from_le_bytes([v[0], v[1]])),
            );
            remaining -= count;
        }
        let alpha = self.alpha.then(|| values.split_off(pixels * 3));
        Ok(PhotoFrame {
            width: self.width,
            height: self.height,
            rgb: values,
            alpha,
        })
    }
}
#[derive(Default)]
pub struct PhotoPreviewState {
    normalized: Mutex<VecDeque<(String, Arc<Normalized>)>>,
}
impl PhotoPreviewState {
    fn get(&self, key: &str) -> Result<Option<Arc<Normalized>>, String> {
        let mut cache = self.normalized.lock().map_err(|e| e.to_string())?;
        if let Some(index) = cache.iter().position(|(k, _)| k == key) {
            let item = cache.remove(index).unwrap();
            let result = item.1.clone();
            cache.push_back(item);
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }
    fn put(&self, key: String, frame: Arc<Normalized>) -> Result<(), String> {
        let mut cache = self.normalized.lock().map_err(|e| e.to_string())?;
        if frame.bytes > 256 * 1024 * 1024 || cache.iter().any(|(k, _)| k == &key) {
            return Ok(());
        }
        while cache.len() >= 8
            || cache.iter().map(|(_, v)| v.bytes).sum::<usize>() + frame.bytes > 256 * 1024 * 1024
        {
            let Some(index) = cache.iter().position(|(_, v)| Arc::strong_count(v) == 1) else {
                return Ok(());
            };
            cache.remove(index);
        }
        cache.push_back((key, frame));
        Ok(())
    }
}

pub async fn current_engine(config: &Mutex<ConfigManager>) -> Result<PathBuf, String> {
    let path = config
        .lock()
        .map_err(|e| e.to_string())?
        .get_config()
        .ffmpeg_path
        .clone()
        .filter(|s| !s.trim().is_empty());
    if let Some(path) = path {
        Ok(crate::core::ffmpeg::resolve_executable_path(Path::new(
            &path,
        )))
    } else {
        tokio::task::spawn_blocking(crate::core::ffmpeg::discover_ffmpeg_path)
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }
}

pub(super) async fn lut_digest(
    path: &Path,
    processor: &PhotoProcessor,
    token: &CancellationToken,
) -> Result<String, String> {
    let bytes = tokio::fs::metadata(path)
        .await
        .map_err(|e| e.to_string())?
        .len();
    if bytes > 64 * 1024 * 1024 {
        return Err(ui_err("lut.too_large", "LUT 不能超过 64 MiB"));
    }
    let _memory = processor.reserve(bytes + 1024 * 1024, token).await?;
    lut_digest_with_memory(path, token).await
}
async fn lut_digest_with_memory(path: &Path, token: &CancellationToken) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    let mut bounded = file.take(64 * 1024 * 1024 + 1);
    tokio::select! { biased; _ = token.cancelled() => return Err(ui_err("preview.cancelled", "预览已取消")), result = bounded.read_to_end(&mut data) => { result.map_err(|e| e.to_string())?; } }
    if data.len() > 64 * 1024 * 1024 {
        return Err(ui_err("lut.too_large", "LUT 不能超过 64 MiB"));
    }
    Ok(format!("{:x}", Sha256::digest(&data)))
}

#[tauri::command]
pub async fn get_photo_lut_fingerprint(
    path: String,
    photo_processor: State<'_, PhotoProcessor>,
) -> Result<String, String> {
    lut_digest(
        Path::new(&path),
        &photo_processor,
        &CancellationToken::new(),
    )
    .await
}

#[tauri::command]
pub async fn get_photo_info(
    path: String,
    photo_processor: State<'_, PhotoProcessor>,
) -> Result<PhotoInfo, String> {
    Ok(photo_processor
        .inspect(Path::new(&path), &CancellationToken::new())
        .await?
        .info)
}

#[tauri::command]
pub async fn scan_directory_for_photos(
    directory: String,
) -> Result<super::batch_manager::ScanResult, String> {
    let path = PathBuf::from(directory);
    tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        let mut luts = Vec::new();
        let mut size = 0;
        super::batch_manager::scan_recursive(
            &path,
            &["jpg", "jpeg", "png", "tif", "tiff"],
            &super::batch_manager::lut_extensions(),
            &mut files,
            &mut luts,
            &mut size,
        )
        .map_err(|e| e.to_string())?;
        Ok(super::batch_manager::ScanResult {
            video_files: files,
            lut_files: luts,
            total_size: size,
            estimated_time: None,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn start_photo_batch_processing(
    request: BatchRequest,
    task_manager: State<'_, TaskManager>,
    video_processor: State<'_, VideoProcessor>,
    lut_manager: State<'_, LutManager>,
    config_manager: State<'_, Mutex<ConfigManager>>,
    photo_processor: State<'_, PhotoProcessor>,
) -> Result<BatchResponse, String> {
    if request.photo_options.is_none() {
        return Err(ui_err("photo.missing_options", "缺少照片导出选项"));
    }
    super::batch_manager::start_batch_processing(
        request,
        task_manager,
        video_processor,
        lut_manager,
        config_manager,
        photo_processor,
    )
    .await
}

#[tauri::command]
pub async fn cancel_photo_preview(
    client_id: String,
    preview_state: State<'_, PreviewState>,
) -> Result<(), String> {
    super::preview::cancel_client(&preview_state, &client_id)
}

fn resize(
    mut frame: PhotoFrame,
    edge: u32,
    token: &CancellationToken,
) -> Result<PhotoFrame, String> {
    let scale = (edge as f64 / frame.width.max(frame.height) as f64).min(1.0);
    if scale >= 1.0 {
        return Ok(frame);
    }
    let width = ((frame.width as f64 * scale).round() as u32).max(1);
    let height = ((frame.height as f64 * scale).round() as u32).max(1);
    let mut values = Vec::with_capacity(frame.width as usize * frame.height as usize * 4);
    for (i, pixel) in frame.rgb.chunks_exact(3).enumerate() {
        if i % 16384 == 0 {
            photo::check_cancel(token)?;
        }
        let a = frame.alpha.as_ref().map_or(1.0, |a| a[i] as f32 / 65535.0);
        for v in pixel {
            let v = *v as f32 / 65535.0;
            values.push(
                (if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }) * a,
            );
        }
        values.push(a);
    }
    let image =
        image::ImageBuffer::<image::Rgba<f32>, _>::from_raw(frame.width, frame.height, values)
            .ok_or_else(|| ui_err("photo.thumb_failed", "照片缩图失败"))?;
    let resized =
        image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle);
    frame.rgb.clear();
    if let Some(a) = &mut frame.alpha {
        a.clear();
    }
    for p in resized.pixels() {
        let a = p.0[3];
        for v in &p.0[..3] {
            let v = if a > 0.0 {
                (*v / a).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let v = if v <= 0.0031308 {
                v * 12.92
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            };
            frame.rgb.push((v * 65535.0).round() as u16);
        }
        if let Some(alpha) = &mut frame.alpha {
            alpha.push((a.clamp(0.0, 1.0) * 65535.0).round() as u16);
        }
    }
    frame.width = width;
    frame.height = height;
    Ok(frame)
}
fn crop(frame: PhotoFrame, view: &PhotoViewport) -> Result<PhotoFrame, String> {
    let PhotoViewport::Region {
        x,
        y,
        width,
        height,
    } = view
    else {
        return Ok(frame);
    };
    if *width == 0
        || *height == 0
        || *width > 1024
        || *height > 1024
        || x.checked_add(*width).is_none_or(|v| v > frame.width)
        || y.checked_add(*height).is_none_or(|v| v > frame.height)
    {
        return Err(ui_err(
            "photo.region_bounds",
            "照片局部查看区域越界或超过 1024 像素",
        ));
    }
    let mut rgb = Vec::with_capacity(*width as usize * *height as usize * 3);
    let mut alpha = frame.alpha.as_ref().map(|_| Vec::new());
    for row in *y..(*y + *height) {
        let start = (row as usize * frame.width as usize + *x as usize) * 3;
        rgb.extend_from_slice(&frame.rgb[start..start + *width as usize * 3]);
        if let (Some(dst), Some(src)) = (&mut alpha, &frame.alpha) {
            dst.extend_from_slice(&src[start / 3..start / 3 + *width as usize]);
        }
    }
    Ok(PhotoFrame {
        width: *width,
        height: *height,
        rgb,
        alpha,
    })
}
fn display(frame: &PhotoFrame, token: &CancellationToken) -> Result<String, String> {
    let mut file = tempfile::tempfile().map_err(|e| e.to_string())?;
    photo::codec::encode(
        frame,
        &photo::PhotoOutput::Png {
            bit_depth: 8,
            alpha_policy: AlphaPolicy::Preserve,
        },
        &mut file,
        token,
    )?;
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    file.take(12 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 12 * 1024 * 1024 {
        return Err(ui_err(
            "photo.display_too_large",
            "照片显示图超过大小限制，请缩小查看区域",
        ));
    }
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(data)))
}

#[tauri::command]
pub async fn generate_photo_preview(
    request: PhotoPreviewRequest,
    preview_state: State<'_, PreviewState>,
    photo_preview_state: State<'_, PhotoPreviewState>,
    photo_processor: State<'_, PhotoProcessor>,
    config_manager: State<'_, Mutex<ConfigManager>>,
) -> Result<PhotoPreviewResponse, String> {
    generate_with_resolver(
        request,
        &preview_state,
        &photo_preview_state,
        &photo_processor,
        current_engine(&config_manager),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn engine() -> Option<PathBuf> {
        let bundled =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/bin/macos/aarch64/ffmpeg");
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) && bundled.is_file() {
            return Some(bundled);
        }
        match crate::core::ffmpeg::discover_ffmpeg_path() {
            Ok(path) => Some(path),
            Err(e) if std::env::var_os("LUTLAB_REQUIRE_MEDIA_TESTS").is_some() => {
                panic!("media engine required: {e}")
            }
            Err(_) => None,
        }
    }
    fn fixture(path: &Path, value: u16) {
        let frame = PhotoFrame {
            width: 8,
            height: 4,
            rgb: vec![value; 96],
            alpha: None,
        };
        photo::codec::encode(
            &frame,
            &photo::PhotoOutput::Png {
                bit_depth: 16,
                alpha_policy: AlphaPolicy::Preserve,
            },
            &mut std::fs::File::create(path).unwrap(),
            &CancellationToken::new(),
        )
        .unwrap();
    }
    fn request(path: &Path) -> PhotoPreviewRequest {
        PhotoPreviewRequest {
            photo_path: path.to_string_lossy().into(),
            lut_path: None,
            intensity: 0.5,
            source_interpretation: SourceInterpretation::Embedded,
            lut_space: None,
            lut_fingerprint: None,
            viewport: PhotoViewport::Fit { max_edge: 160 },
            quality: "accurate".into(),
            alpha_policy: AlphaPolicy::Preserve,
            client_id: "photo-tests".into(),
            request_id: "first".into(),
        }
    }
    #[tokio::test]
    async fn actual_preview_caches_bypass_shared_slots_and_invalidate_for_changed_source_and_region(
    ) {
        let Some(engine) = engine() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.png");
        fixture(&path, 12000);
        let state = PreviewState::default();
        let photos = PhotoPreviewState::default();
        let processor = PhotoProcessor::with_budget(256 * 1024 * 1024);
        let first = generate_with_resolver(
            request(&path),
            &state,
            &photos,
            &processor,
            std::future::ready(Ok(engine.clone())),
        )
        .await
        .unwrap();
        let busy = state.permits.acquire_many(2).await.unwrap();
        let mut next = request(&path);
        next.request_id = "new-client-request".into();
        let hit = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            generate_with_resolver(
                next,
                &state,
                &photos,
                &processor,
                std::future::ready(Ok(engine.clone())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(hit.cached);
        assert_eq!(hit.request_id, "new-client-request");
        assert_eq!(hit.processed_image, first.processed_image);
        drop(busy);
        fixture(&path, 42000);
        let changed = generate_with_resolver(
            request(&path),
            &state,
            &photos,
            &processor,
            std::future::ready(Ok(engine.clone())),
        )
        .await
        .unwrap();
        assert!(!changed.cached);
        assert_ne!(changed.processed_image, first.processed_image);
        let mut crop = request(&path);
        crop.viewport = PhotoViewport::Region {
            x: 6,
            y: 0,
            width: 8,
            height: 4,
        };
        assert!(generate_with_resolver(
            crop,
            &state,
            &photos,
            &processor,
            std::future::ready(Ok(engine))
        )
        .await
        .unwrap_err()
        .contains("越界"));
    }
    #[tokio::test]
    async fn photo_cancel_is_registered_before_engine_discovery_and_does_not_cancel_other_client() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.png");
        fixture(&path, 12000);
        let state = Arc::new(PreviewState::default());
        let photos = Arc::new(PhotoPreviewState::default());
        let processor = PhotoProcessor::with_budget(256 * 1024 * 1024);
        let (started, ready) = tokio::sync::oneshot::channel();
        let old = state.clone();
        let p = photos.clone();
        let worker = processor.clone();
        let input = request(&path);
        let handle = tokio::spawn(async move {
            generate_with_resolver(input, &old, &p, &worker, async {
                let _ = started.send(());
                std::future::pending::<Result<PathBuf, String>>().await
            })
            .await
        });
        ready.await.unwrap();
        let survivor = state.begin("separate-photo-instance").unwrap();
        super::super::preview::cancel_client(&state, "photo-tests").unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), handle)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert!(!survivor.token.is_cancelled());
    }
    #[tokio::test]
    async fn changing_a_confirmed_lut_requires_a_new_fingerprint() {
        let Some(engine) = engine() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.png");
        fixture(&path, 12000);
        let lut = dir.path().join("look.cube");
        std::fs::write(&lut, "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
        let state = PreviewState::default();
        let photos = PhotoPreviewState::default();
        let processor = PhotoProcessor::with_budget(256 * 1024 * 1024);
        let mut input = request(&path);
        input.lut_path = Some(lut.to_string_lossy().into());
        input.lut_space = Some(PhotoSpace::Srgb);
        input.lut_fingerprint = Some(
            lut_digest(&lut, &processor, &CancellationToken::new())
                .await
                .unwrap(),
        );
        generate_with_resolver(
            input.clone(),
            &state,
            &photos,
            &processor,
            std::future::ready(Ok(engine.clone())),
        )
        .await
        .unwrap();
        std::fs::write(&lut, "LUT_1D_SIZE 2\n1 1 1\n0 0 0\n").unwrap();
        assert!(generate_with_resolver(
            input,
            &state,
            &photos,
            &processor,
            std::future::ready(Ok(engine))
        )
        .await
        .unwrap_err()
        .contains("重新确认"));
    }
}

async fn generate_with_resolver(
    request: PhotoPreviewRequest,
    preview_state: &PreviewState,
    photo_preview_state: &PhotoPreviewState,
    photo_processor: &PhotoProcessor,
    resolver: impl std::future::Future<Output = Result<PathBuf, String>>,
) -> Result<PhotoPreviewResponse, String> {
    if request.client_id.is_empty()
        || request.client_id.len() > 128
        || request.request_id.len() > 128
        || !request.intensity.is_finite()
        || !(0.0..=1.0).contains(&request.intensity)
        || !matches!(request.quality.as_str(), "fast" | "accurate")
    {
        return Err(ui_err("photo.bad_request", "无效照片预览请求"));
    }
    if let PhotoViewport::Fit { max_edge } = request.viewport {
        if !(160..=1920).contains(&max_edge) {
            return Err(ui_err("photo.bad_size", "照片预览尺寸必须在 160–1920 之间"));
        }
    }
    let active = preview_state.begin(&request.client_id)?;
    let token = active.token.clone();
    let engine = tokio::select! { biased; _ = token.cancelled() => return Err(ui_err("preview.cancelled", "预览已取消")), engine = resolver => engine? };
    let header = photo_processor
        .inspect(Path::new(&request.photo_path), &token)
        .await?;
    let lut_hash = if let Some(path) = request
        .lut_path
        .as_ref()
        .filter(|_| request.intensity > 0.0)
    {
        if request.lut_space != Some(PhotoSpace::Srgb) {
            return Err(ui_err(
                "photo.lut_needs_srgb",
                "请明确按 sRGB 输入和输出使用此 LUT",
            ));
        }
        let hash = lut_digest(Path::new(path), &photo_processor, &token).await?;
        if request.lut_fingerprint.as_deref() != Some(&hash) {
            return Err(ui_err(
                "photo.lut_unconfirmed",
                "LUT 已变化或尚未确认，请重新确认 sRGB 用法",
            ));
        }
        hash
    } else {
        String::new()
    };
    let mut base = Sha256::new();
    base.update(b"photo-normalized-v1");
    base.update(header.info.source_version.as_bytes());
    base.update(request.photo_path.as_bytes());
    base.update(serde_json::to_vec(&request.source_interpretation).map_err(|e| e.to_string())?);
    base.update(header.icc.as_deref().unwrap_or_default());
    base.update([header.info.orientation]);
    let normalized_key = format!("{:x}", base.finalize());
    let mut hash = Sha256::new();
    hash.update(normalized_key.as_bytes());
    hash.update(lut_hash.as_bytes());
    hash.update(request.intensity.to_le_bytes());
    hash.update(serde_json::to_vec(&request.viewport).map_err(|e| e.to_string())?);
    hash.update(request.quality.as_bytes());
    hash.update(serde_json::to_vec(&request.alpha_policy).map_err(|e| e.to_string())?);
    hash.update(photo::metadata::version(&engine)?.as_bytes());
    let key = format!("photo:{:x}", hash.finalize());
    let response = |images: VideoPreviewResponse, width, height| PhotoPreviewResponse {
        original_image: images.original_image,
        processed_image: images.processed_image,
        request_id: request.request_id.clone(),
        source_version: header.info.source_version.clone(),
        width: images.photo_size.map_or(width, |s| s.0),
        height: images.photo_size.map_or(height, |s| s.1),
        cached: images.cached,
    };
    let dims = match request.viewport {
        PhotoViewport::Region { width, height, .. } => (width, height),
        PhotoViewport::Fit { max_edge } => {
            let scale =
                (max_edge as f64 / header.info.width.max(header.info.height) as f64).min(1.0);
            (
                (header.info.width as f64 * scale).round().max(1.0) as u32,
                (header.info.height as f64 * scale).round().max(1.0) as u32,
            )
        }
    };
    photo::check_cancel(&token)?;
    if let Some(images) = preview_state.display_get(&key)? {
        return Ok(response(images, dims.0, dims.1));
    }
    let render = tokio::select! { biased; _ = token.cancelled() => return Err(ui_err("preview.cancelled", "预览已取消")), permit = preview_state.permits.clone().acquire_owned() => Arc::new(permit.map_err(|e| e.to_string())?) };
    if let Some(images) = preview_state.display_get(&key)? {
        photo::check_cancel(&token)?;
        return Ok(response(images, dims.0, dims.1));
    }
    let memory = photo_processor
        .reserve(
            128 * 1024 * 1024
                + u64::from(header.info.width) * u64::from(header.info.height) * 64
                + header.info.size,
            &token,
        )
        .await?;
    let cached = photo_preview_state.get(&normalized_key)?;
    let path = PathBuf::from(&request.photo_path);
    let source = request.source_interpretation.clone();
    let cancellation = token.clone();
    let lease = memory.clone();
    let slot = render.clone();
    let (mut frame, artifact) = tokio::task::spawn_blocking(move || {
        let (_lease, _slot) = (lease, slot);
        if let Some(cached) = cached {
            return Ok::<_, String>((cached.load(&cancellation)?, None));
        }
        let frame = photo::codec::decode(&path, &source, &cancellation)?;
        let bytes = frame.rgb.len() * 2 + frame.alpha.as_ref().map_or(0, |a| a.len() * 2);
        let artifact = if bytes <= 256 * 1024 * 1024 {
            Some(Arc::new(Normalized::store(&frame)?))
        } else {
            None
        };
        photo::check_cancel(&cancellation)?;
        Ok((frame, artifact))
    })
    .await
    .map_err(|e| e.to_string())??;
    if let Some(artifact) = artifact {
        photo_preview_state.put(normalized_key, artifact)?;
    }
    photo::check_cancel(&token)?;
    frame = crop(frame, &request.viewport)?;
    let edge = match request.viewport {
        PhotoViewport::Fit { max_edge } => Some(max_edge),
        _ => None,
    };
    if request.quality == "fast" {
        if let Some(edge) = edge {
            let cancellation = token.clone();
            let lease = memory.clone();
            let slot = render.clone();
            frame = tokio::task::spawn_blocking(move || {
                let (_lease, _slot) = (lease, slot);
                resize(frame, edge, &cancellation)
            })
            .await
            .map_err(|e| e.to_string())??;
        }
    }
    let mut original = frame.clone();
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let luts = if !lut_hash.is_empty() {
        prepare_luts(
            &[PathBuf::from(request.lut_path.as_ref().unwrap())],
            temporary.path(),
        )
        .await
        .map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };
    frame.rgb = grade_rgb16_with_timeout(
        &engine,
        frame.width,
        frame.height,
        frame.rgb,
        &luts,
        request.intensity,
        &token,
        std::time::Duration::from_secs(30),
    )
    .await?;
    let cancellation = token.clone();
    let policy = request.alpha_policy.clone();
    let lease = memory.clone();
    let slot = render.clone();
    let (images, width, height) = tokio::task::spawn_blocking(move || {
        let (_lease, _slot) = (lease, slot);
        photo::color::apply_alpha(&mut original, &policy, &cancellation)?;
        photo::color::apply_alpha(&mut frame, &policy, &cancellation)?;
        if let Some(edge) = edge {
            original = resize(original, edge, &cancellation)?;
            frame = resize(frame, edge, &cancellation)?;
        }
        loop {
            let images = VideoPreviewResponse {
                original_image: display(&original, &cancellation)?,
                processed_image: display(&frame, &cancellation)?,
                time_seconds: 0.0,
                cached: false,
                photo_size: Some((frame.width, frame.height)),
            };
            if images.original_image.len() + images.processed_image.len() <= 16 * 1024 * 1024 {
                photo::check_cancel(&cancellation)?;
                return Ok::<_, String>((images, frame.width, frame.height));
            }
            if edge.is_none() || frame.width.max(frame.height) <= 160 {
                return Err(ui_err(
                    "photo.compare_too_large",
                    "照片对比图超过 16 MiB，请缩小查看区域",
                ));
            }
            let smaller = (frame.width.max(frame.height) * 3 / 4).max(160);
            original = resize(original, smaller, &cancellation)?;
            frame = resize(frame, smaller, &cancellation)?;
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    photo::check_cancel(&token)?;
    if photo::metadata::version(Path::new(&request.photo_path))? != header.info.source_version {
        return Err(ui_err("photo.source_changed", "源照片已变化，请重新预览"));
    }
    if !lut_hash.is_empty()
        && lut_digest_with_memory(Path::new(request.lut_path.as_ref().unwrap()), &token).await?
            != lut_hash
    {
        return Err(ui_err(
            "photo.lut_changed",
            "LUT 在预览期间发生变化，请重新确认",
        ));
    }
    preview_state.display_put(key, images.clone())?;
    Ok(response(images, width, height))
}
