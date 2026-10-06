use crate::commands::encoding_options::{build_encoding_settings, ProcessingOptions};
use crate::core::ffmpeg::processor::{ProcessingProgress, ProcessingResult, VideoProcessor};
use crate::core::lut::LutManager;
use crate::core::photo::{
    processor::{PhotoControl, PhotoJob, PhotoProcessor},
    PhotoItemOptions, PhotoSettings, SourceInterpretation,
};
use crate::core::task::{TaskManager, TaskType};
use crate::types::LutFormat;
use crate::utils::config::ConfigManager;
use crate::utils::logger;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use tauri::State;
use tokio::fs;
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use uuid::Uuid;

static UNFINISHED_BATCHES: AtomicUsize = AtomicUsize::new(0);

pub fn has_unfinished_batches() -> bool {
    UNFINISHED_BATCHES.load(Ordering::SeqCst) != 0
}

/// Shared by the coordinator and every spawned item. If the coordinator
/// unwinds, detached items continue to protect shutdown until their own cleanup.
struct BatchLifetime;

impl BatchLifetime {
    fn try_new() -> Result<Self, String> {
        UNFINISHED_BATCHES
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "已有批次正在启动或导出，请等待其实际结束".to_string())?;
        Ok(Self)
    }
}

impl Drop for BatchLifetime {
    fn drop(&mut self) {
        UNFINISHED_BATCHES.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BatchItem {
    pub input_path: String,
    pub output_path: String,
    #[serde(default)]
    pub lut_paths: Vec<String>,
    #[serde(default)]
    pub lut_path: Option<String>,
    pub intensity: f32,
    #[serde(default)]
    pub photo: Option<PhotoItemOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BatchRequest {
    pub items: Vec<BatchItem>,
    pub output_directory: String,
    #[serde(default)]
    pub preserve_structure: bool,
    #[serde(default)]
    pub max_concurrent: Option<usize>,
    #[serde(flatten)]
    pub options: ProcessingOptions,
    #[serde(default)]
    pub photo_options: Option<PhotoSettings>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchResponse {
    pub batch_id: String,
    pub total_items: usize,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchProgress {
    pub batch_id: String,
    pub total_items: usize,
    pub completed_items: usize,
    pub failed_items: usize,
    pub cancelled_items: usize,
    pub current_item: Option<String>,
    pub overall_progress: f32,
    pub status: String,
    pub errors: Vec<String>,
    pub items: Vec<BatchItemProgress>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BatchItemProgress {
    pub input_path: String,
    pub output_path: String,
    pub status: String,
    pub progress: f32,
    pub error: Option<String>,
    pub encoder: Option<String>,
    pub speed: Option<f64>,
    pub eta_seconds: Option<f64>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanResult {
    pub video_files: Vec<String>,
    pub lut_files: Vec<String>,
    pub total_size: u64,
    pub estimated_time: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BatchRuntimeStatus {
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}

impl BatchRuntimeStatus {
    fn as_str(self) -> &'static str {
        match self {
            BatchRuntimeStatus::Running => "Running",
            BatchRuntimeStatus::Cancelling => "Cancelling",
            BatchRuntimeStatus::Completed => "Completed",
            BatchRuntimeStatus::Failed => "Failed",
            BatchRuntimeStatus::Cancelled => "Cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BatchItemRuntimeStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl BatchItemRuntimeStatus {
    fn as_str(self) -> &'static str {
        match self {
            BatchItemRuntimeStatus::Pending => "Pending",
            BatchItemRuntimeStatus::Running => "Running",
            BatchItemRuntimeStatus::Completed => "Completed",
            BatchItemRuntimeStatus::Failed => "Failed",
            BatchItemRuntimeStatus::Cancelled => "Cancelled",
        }
    }
}

#[derive(Debug, Clone)]
struct BatchItemRuntime {
    input_path: String,
    output_path: String,
    status: BatchItemRuntimeStatus,
    progress: f32,
    task_id: Option<String>,
    error: Option<String>,
    encoder: Option<String>,
    speed: Option<f64>,
    eta_seconds: Option<f64>,
    message: Option<String>,
}

struct BatchRuntime {
    status: BatchRuntimeStatus,
    cancel_requested: bool,
    errors: Vec<String>,
    item_states: Vec<BatchItemRuntime>,
    processor: Option<Arc<VideoProcessor>>,
    photo_controls: Vec<PhotoControl>,
}

type BatchStateMap = HashMap<String, Arc<AsyncMutex<BatchRuntime>>>;
static BATCH_STATES: OnceLock<AsyncMutex<BatchStateMap>> = OnceLock::new();

fn batch_states() -> &'static AsyncMutex<BatchStateMap> {
    BATCH_STATES.get_or_init(|| AsyncMutex::new(HashMap::new()))
}

fn resolve_output_path(
    item: &BatchItem,
    output_directory: &str,
    extension: &str,
    structure_root: Option<&Path>,
) -> String {
    if !item.output_path.trim().is_empty() {
        return item.output_path.clone();
    }

    let input_path = Path::new(&item.input_path);
    let file_stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let mut target_dir = if !output_directory.trim().is_empty() {
        PathBuf::from(output_directory)
    } else {
        input_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };

    if let (Some(root), Some(parent)) = (structure_root, input_path.parent()) {
        if let Ok(relative) = parent.strip_prefix(root) {
            target_dir = target_dir.join(relative);
        }
    }
    target_dir
        .join(format!("{}_lut_applied.{}", file_stem, extension))
        .to_string_lossy()
        .to_string()
}

/// Resolve collisions before launch, including identical filenames imported from
/// different folders. The processor still publishes atomically without overwrite.
pub(super) fn available_output_path(
    path: &Path,
    reserved: &mut HashSet<PathBuf>,
) -> Result<PathBuf, String> {
    crate::core::output::available_path(path, reserved)
}

fn common_input_parent(items: &[BatchItem]) -> Option<PathBuf> {
    let mut common = Path::new(&items.first()?.input_path)
        .parent()?
        .to_path_buf();
    for item in items.iter().skip(1) {
        let parent = Path::new(&item.input_path).parent()?;
        while !parent.starts_with(&common) {
            if !common.pop() {
                return None;
            }
        }
    }
    Some(common)
}

async fn ensure_output_parent_exists(output_path: &str) -> Result<(), String> {
    if let Some(parent) = Path::new(output_path).parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }
    Ok(())
}

pub(super) fn lut_extensions() -> Vec<String> {
    LutFormat::supported_extensions()
        .into_iter()
        .map(|ext| ext.to_string())
        .collect()
}

pub(super) fn scan_recursive(
    path: &Path,
    video_exts: &[&str],
    lut_exts: &[String],
    videos: &mut Vec<String>,
    luts: &mut Vec<String>,
    size: &mut u64,
) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            scan_recursive(&entry_path, video_exts, lut_exts, videos, luts, size)?;
            continue;
        }

        if !entry_path.is_file() {
            continue;
        }

        let ext = entry_path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_lowercase());

        let Some(ext) = ext else {
            continue;
        };

        if video_exts.contains(&ext.as_str()) {
            videos.push(entry_path.to_string_lossy().to_string());
            if let Ok(metadata) = entry.metadata() {
                *size += metadata.len();
            }
        } else if lut_exts.iter().any(|candidate| candidate == &ext) {
            luts.push(entry_path.to_string_lossy().to_string());
        }
    }

    Ok(())
}

fn batch_counts(item_states: &[BatchItemRuntime]) -> (usize, usize, usize) {
    let mut completed = 0usize;
    let mut failed = 0usize;
    let mut cancelled = 0usize;

    for state in item_states {
        match state.status {
            BatchItemRuntimeStatus::Completed => completed += 1,
            BatchItemRuntimeStatus::Failed => failed += 1,
            BatchItemRuntimeStatus::Cancelled => cancelled += 1,
            BatchItemRuntimeStatus::Pending | BatchItemRuntimeStatus::Running => {}
        }
    }

    (completed, failed, cancelled)
}

fn current_item(item_states: &[BatchItemRuntime]) -> Option<String> {
    item_states
        .iter()
        .find(|state| state.status == BatchItemRuntimeStatus::Running)
        .map(|state| state.input_path.clone())
}

fn overall_progress(item_states: &[BatchItemRuntime]) -> f32 {
    if item_states.is_empty() {
        return 0.0;
    }

    let total: f32 = item_states
        .iter()
        .map(|state| match state.status {
            BatchItemRuntimeStatus::Pending => 0.0,
            BatchItemRuntimeStatus::Running => state.progress.clamp(0.0, 99.0),
            BatchItemRuntimeStatus::Completed
            | BatchItemRuntimeStatus::Failed
            | BatchItemRuntimeStatus::Cancelled => 100.0,
        })
        .sum();

    (total / item_states.len() as f32).clamp(0.0, 100.0)
}

fn finalize_batch_status(runtime: &mut BatchRuntime) {
    let (completed, failed, cancelled) = batch_counts(&runtime.item_states);
    let total = runtime.item_states.len();

    let active = runtime.item_states.iter().any(|state| {
        matches!(
            state.status,
            BatchItemRuntimeStatus::Running | BatchItemRuntimeStatus::Pending
        )
    });
    runtime.status = if active {
        if runtime.cancel_requested {
            BatchRuntimeStatus::Cancelling
        } else {
            BatchRuntimeStatus::Running
        }
    } else if runtime.cancel_requested && cancelled > 0 {
        BatchRuntimeStatus::Cancelled
    } else if failed > 0 {
        BatchRuntimeStatus::Failed
    } else if completed == total && total > 0 {
        BatchRuntimeStatus::Completed
    } else if cancelled > 0 {
        BatchRuntimeStatus::Cancelled
    } else {
        BatchRuntimeStatus::Failed
    };
}

#[tauri::command]
pub async fn scan_directory_for_videos(directory: String) -> Result<ScanResult, String> {
    let dir_path = PathBuf::from(&directory);

    if !dir_path.exists() || !dir_path.is_dir() {
        return Err("Directory does not exist or is not a directory".to_string());
    }

    let video_extensions: Vec<&str> = vec!["mp4", "mov", "avi", "mkv", "wmv", "flv", "webm", "m4v"];
    let lut_extensions = lut_extensions();

    let scan_result = tokio::task::spawn_blocking(move || {
        let mut video_files = Vec::new();
        let mut lut_files = Vec::new();
        let mut total_size = 0u64;

        scan_recursive(
            &dir_path,
            &video_extensions,
            &lut_extensions,
            &mut video_files,
            &mut lut_files,
            &mut total_size,
        )
        .map(|_| (video_files, lut_files, total_size))
    })
    .await
    .map_err(|e| format!("Directory scan task failed: {}", e))?;

    match scan_result {
        Ok((video_files, lut_files, total_size)) => Ok(ScanResult {
            estimated_time: if total_size > 0 {
                Some((total_size / (1024 * 1024 * 1024)) * 300)
            } else {
                None
            },
            video_files,
            lut_files,
            total_size,
        }),
        Err(error) => Err(format!("Failed to scan directory: {}", error)),
    }
}

#[tauri::command]
pub async fn start_batch_processing(
    request: BatchRequest,
    task_manager: State<'_, TaskManager>,
    video_processor: State<'_, VideoProcessor>,
    lut_manager: State<'_, LutManager>,
    config_manager: State<'_, std::sync::Mutex<ConfigManager>>,
    photo_processor: State<'_, PhotoProcessor>,
) -> Result<BatchResponse, String> {
    let batch_lifetime = Arc::new(BatchLifetime::try_new()?);
    let photo_options = request.photo_options.clone();
    let photo_processor = photo_processor.inner().clone();
    let prepared_directory = Arc::new(tempfile::tempdir().map_err(|e| e.to_string())?);
    let mut frozen_luts = HashMap::<String, String>::new();
    logger::log_info(&format!(
        "Starting batch processing with {} items",
        request.items.len()
    ));

    if request.items.is_empty() || request.items.len() > 10000 {
        return Err("批次需要包含 1–10000 个素材".to_string());
    }

    if !request.output_directory.trim().is_empty() {
        fs::create_dir_all(&request.output_directory)
            .await
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    let mut settings = if photo_options.is_some() {
        crate::core::ffmpeg::EncodingSettings::default()
    } else {
        build_encoding_settings(&request.options)?
    };
    let format = photo_options
        .as_ref()
        .map(|p| p.output.extension())
        .unwrap_or_else(|| {
            request
                .options
                .output_format
                .as_deref()
                .unwrap_or("mp4")
                .trim_start_matches('.')
        });
    let structure_root = if request.preserve_structure {
        common_input_parent(&request.items)
    } else {
        None
    };
    let mut reserved = HashSet::new();
    let mut validated_luts = HashSet::new();
    let mut normalized_items = Vec::with_capacity(request.items.len());
    for mut item in request.items {
        if !item.intensity.is_finite() || !(0.0..=1.0).contains(&item.intensity) {
            return Err("LUT intensity must be between 0 and 1".to_string());
        }
        if !fs::metadata(&item.input_path)
            .await
            .map(|m| m.is_file())
            .unwrap_or(false)
        {
            return Err(format!("Input file does not exist: {}", item.input_path));
        }

        if let Some(options) = &photo_options {
            let path = PathBuf::from(&item.input_path);
            let info = photo_processor
                .inspect(&path, &tokio_util::sync::CancellationToken::new())
                .await?
                .info;
            options.output.validate(info.has_alpha)?;
            let photo = item.photo.get_or_insert_with(PhotoItemOptions::default);
            if matches!(photo.source_interpretation, SourceInterpretation::Embedded)
                && !matches!(info.color_status.as_str(), "embedded" | "srgb")
            {
                return Err(format!("{}：请明确指定输入照片的色彩空间", item.input_path));
            }
            if photo
                .source_version
                .as_ref()
                .is_some_and(|v| *v != info.source_version)
            {
                return Err(format!("{}：源照片已变化，请重新读取", item.input_path));
            }
            photo.source_version = Some(info.source_version);
            if (item.lut_path.is_some() || !item.lut_paths.is_empty())
                && item.intensity > 0.0
                && photo.lut_space != Some(crate::core::photo::PhotoSpace::Srgb)
            {
                return Err("请明确按 sRGB 输入和输出使用照片 LUT".into());
            }
        }

        let mut lut_paths = item.lut_paths.clone();
        if lut_paths.is_empty() {
            if let Some(single) = item.lut_path.clone() {
                if !single.trim().is_empty() {
                    lut_paths.push(single);
                }
            }
        }
        lut_paths.retain(|path| !path.trim().is_empty());
        for lut_path in &lut_paths {
            if validated_luts.contains(lut_path) {
                continue;
            }
            if fs::metadata(lut_path).await.is_err() {
                return Err(format!("LUT file does not exist: {}", lut_path));
            }

            let validation = lut_manager
                .validate_lut(lut_path)
                .await
                .map_err(|e| format!("Failed to validate LUT {}: {}", lut_path, e))?;
            if !validation.is_valid {
                return Err(format!(
                    "Invalid LUT file {}: {}",
                    lut_path,
                    validation.errors.join("; ")
                ));
            }
            validated_luts.insert(lut_path.clone());
        }

        if photo_options.is_some() && item.intensity > 0.0 {
            for path in &lut_paths {
                let hash = super::photo::lut_digest(
                    Path::new(path),
                    &photo_processor,
                    &tokio_util::sync::CancellationToken::new(),
                )
                .await?;
                if item
                    .photo
                    .as_ref()
                    .and_then(|p| p.lut_fingerprint.as_deref())
                    != Some(hash.as_str())
                {
                    return Err("照片 LUT 已变化或尚未确认，请重新确认 sRGB 用法".into());
                }
            }
        }
        if photo_options.is_some() && item.intensity > 0.0 {
            for path in &mut lut_paths {
                if let Some(frozen) = frozen_luts.get(path) {
                    *path = frozen.clone();
                    continue;
                }
                let directory = prepared_directory.path().join(Uuid::new_v4().to_string());
                fs::create_dir(&directory)
                    .await
                    .map_err(|e| e.to_string())?;
                let frozen =
                    crate::core::ffmpeg::lut::prepare_luts(&[PathBuf::from(&*path)], &directory)
                        .await
                        .map_err(|e| e.to_string())?[0]
                        .to_string_lossy()
                        .to_string();
                let hash = super::photo::lut_digest(
                    Path::new(path),
                    &photo_processor,
                    &tokio_util::sync::CancellationToken::new(),
                )
                .await?;
                if item
                    .photo
                    .as_ref()
                    .and_then(|p| p.lut_fingerprint.as_deref())
                    != Some(hash.as_str())
                {
                    return Err("照片 LUT 在准备期间变化，请重新确认 sRGB 用法".into());
                }
                frozen_luts.insert(path.clone(), frozen.clone());
                *path = frozen;
            }
        }

        let output_path = resolve_output_path(
            &item,
            &request.output_directory,
            format,
            structure_root.as_deref(),
        );
        ensure_output_parent_exists(&output_path).await?;
        if Path::new(&output_path).exists()
            && std::fs::canonicalize(&output_path).ok()
                == std::fs::canonicalize(&item.input_path).ok()
        {
            return Err("Output path must differ from input video".to_string());
        }
        let output_path = available_output_path(Path::new(&output_path), &mut reserved)?
            .to_string_lossy()
            .to_string();

        normalized_items.push(BatchItem {
            lut_paths,
            output_path,
            ..item
        });
    }

    let batch_id = Uuid::new_v4().to_string();
    let total_items = normalized_items.len();
    let (configured_concurrent, configured_ffmpeg) = {
        let manager = config_manager
            .lock()
            .map_err(|e| format!("Config lock poisoned: {}", e))?;
        (
            manager.get_config().max_concurrent_tasks,
            manager.get_config().ffmpeg_path.clone(),
        )
    };
    let ffmpeg_path = match configured_ffmpeg.filter(|path| !path.trim().is_empty()) {
        Some(path) => PathBuf::from(path),
        None => tokio::task::spawn_blocking(crate::core::ffmpeg::discover_ffmpeg_path)
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?,
    };
    let max_concurrent = request
        .max_concurrent
        .unwrap_or(configured_concurrent)
        .clamp(1, if photo_options.is_some() { 2 } else { 4 });
    settings.extra_params.insert(
        "__threads__".into(),
        (num_cpus::get() / max_concurrent).clamp(1, 8).to_string(),
    );

    let mut processor = video_processor.clone_for_task();
    processor.set_ffmpeg_path(ffmpeg_path.clone());
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<ProcessingProgress>();
    processor.set_progress_sender(progress_tx);
    let processor = Arc::new(processor);

    let runtime = Arc::new(AsyncMutex::new(BatchRuntime {
        status: BatchRuntimeStatus::Running,
        cancel_requested: false,
        errors: Vec::new(),
        item_states: normalized_items
            .iter()
            .map(|item| BatchItemRuntime {
                input_path: item.input_path.clone(),
                output_path: item.output_path.clone(),
                status: BatchItemRuntimeStatus::Pending,
                progress: 0.0,
                task_id: None,
                error: None,
                encoder: None,
                speed: None,
                eta_seconds: None,
                message: None,
            })
            .collect(),
        photo_controls: if photo_options.is_some() {
            (0..total_items).map(|_| PhotoControl::default()).collect()
        } else {
            Vec::new()
        },
        processor: Some(processor.clone()),
    }));

    {
        let mut states = batch_states().lock().await;
        states.insert(batch_id.clone(), runtime.clone());
    }

    let task_manager_ref = task_manager.inner().clone();
    let runtime_for_progress = runtime.clone();
    let task_manager_for_progress = task_manager_ref.clone();
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let percent = (progress.progress * 100.0) as f64;
            let _ = task_manager_for_progress
                .update_description(&progress.task_id, progress.message.clone());
            let _ = task_manager_for_progress.update_progress(&progress.task_id, percent);

            let mut rt = runtime_for_progress.lock().await;
            if let Some(state) = rt
                .item_states
                .iter_mut()
                .find(|state| state.task_id.as_deref() == Some(progress.task_id.as_str()))
            {
                if state.status == BatchItemRuntimeStatus::Pending
                    || state.status == BatchItemRuntimeStatus::Running
                {
                    state.progress = ((progress.progress * 100.0) as f32).clamp(0.0, 99.0);
                    if progress.encoder.is_some() {
                        state.encoder = progress.encoder;
                    }
                    state.speed = progress.speed;
                    state.eta_seconds = progress.eta_seconds;
                    state.message = Some(progress.message);
                }
                if state.status == BatchItemRuntimeStatus::Pending {
                    state.status = BatchItemRuntimeStatus::Running;
                }
            }
            finalize_batch_status(&mut rt);
        }
    });

    let runtime_for_worker = runtime.clone();
    let batch_id_for_worker = batch_id.clone();
    let processor_for_worker = processor.clone();
    let settings_for_worker = settings.clone();

    tokio::spawn(async move {
        let batch_lifetime = batch_lifetime;
        let photo_controls = runtime_for_worker.lock().await.photo_controls.clone();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
        let mut handles = Vec::with_capacity(normalized_items.len());

        for (index, item) in normalized_items.into_iter().enumerate() {
            let permit_pool = semaphore.clone();
            let runtime_for_item = runtime_for_worker.clone();
            let task_manager_for_item = task_manager_ref.clone();
            let processor_for_item = processor_for_worker.clone();
            let settings_for_item = settings_for_worker.clone();
            let batch_label = batch_id_for_worker.clone();
            let item_lifetime = batch_lifetime.clone();
            let frozen_lease = prepared_directory.clone();
            let photo_control = photo_controls.get(index).cloned();
            let photo_options = photo_options.clone();
            let photo_processor = photo_processor.clone();
            let photo_engine = ffmpeg_path.clone();

            handles.push(tokio::spawn(async move {
                let _item_lifetime = item_lifetime;
                let _frozen_lease = frozen_lease;
                let permit = match permit_pool.acquire_owned().await {
                    Ok(permit) => permit,
                    Err(_) => return,
                };

                {
                    let mut rt = runtime_for_item.lock().await;
                    if rt.cancel_requested {
                        if let Some(state) = rt.item_states.get_mut(index) {
                            state.status = BatchItemRuntimeStatus::Cancelled;
                            state.progress = 100.0;
                            state.error = None;
                        }
                        finalize_batch_status(&mut rt);
                        drop(permit);
                        return;
                    }
                }

                let task_id = match task_manager_for_item.create_task(
                    if photo_options.is_some() {
                        TaskType::PhotoProcessing
                    } else {
                        TaskType::VideoProcessing
                    },
                    format!("Batch {}: {}", batch_label, item.input_path),
                ) {
                    Ok(id) => id,
                    Err(error) => {
                        let mut rt = runtime_for_item.lock().await;
                        if let Some(state) = rt.item_states.get_mut(index) {
                            state.status = BatchItemRuntimeStatus::Failed;
                            state.progress = 100.0;
                            state.error = Some("Failed to create task".to_string());
                        }
                        rt.errors.push(format!(
                            "Failed to create task for {}: {}",
                            item.input_path, error
                        ));
                        finalize_batch_status(&mut rt);
                        drop(permit);
                        return;
                    }
                };

                let _ = task_manager_for_item.start_task(&task_id);
                let _ = task_manager_for_item.set_output_path(&task_id, item.output_path.clone());

                {
                    let mut rt = runtime_for_item.lock().await;
                    if let Some(state) = rt.item_states.get_mut(index) {
                        state.status = BatchItemRuntimeStatus::Running;
                        state.progress = 0.0;
                        state.task_id = Some(task_id.clone());
                        state.error = None;
                    }
                    finalize_batch_status(&mut rt);
                }

                {
                    let rt = runtime_for_item.lock().await;
                    if rt.cancel_requested {
                        drop(rt);
                        let _ = task_manager_for_item.cancel_task(&task_id);
                        let _ = processor_for_item.cancel_task(&task_id).await;
                    }
                }

                let lut_paths = item.lut_paths.iter().map(PathBuf::from).collect::<Vec<_>>();

                let result = if let (Some(options), Some(control)) = (photo_options, photo_control)
                {
                    let photo = item.photo.clone().unwrap_or_default();
                    let runtime = runtime_for_item.clone();
                    let stage_id = task_id.clone();
                    let stage_sender = task_manager_for_item.clone();
                    let stage = Arc::new(move |message: &str| {
                        let _ = stage_sender.update_description(&stage_id, message.to_string());
                        if let Ok(mut rt) = runtime.try_lock() {
                            if let Some(state) = rt.item_states.get_mut(index) {
                                state.message = Some(message.to_string());
                            }
                        }
                    });
                    let started = std::time::Instant::now();
                    photo_processor
                        .process(
                            &photo_engine,
                            PhotoJob {
                                input: item.input_path.clone().into(),
                                output: item.output_path.clone().into(),
                                luts: lut_paths,
                                intensity: item.intensity,
                                lut_space: photo.lut_space,
                                source: photo.source_interpretation,
                                format: options.output,
                                preserve_metadata: options.preserve_metadata,
                                preserve_gps: options.preserve_gps,
                                expected_version: photo.source_version,
                            },
                            control,
                            stage,
                        )
                        .await
                        .map(|path| ProcessingResult {
                            task_id: task_id.clone(),
                            success: true,
                            file_size: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                            output_path: Some(path),
                            error: None,
                            elapsed: started.elapsed(),
                        })
                        .map_err(crate::types::AppError::InvalidInput)
                } else {
                    processor_for_item
                        .apply_luts_with_task_id(
                            Path::new(&item.input_path),
                            Path::new(&item.output_path),
                            &lut_paths,
                            &settings_for_item,
                            task_id.clone(),
                            item.intensity,
                        )
                        .await
                };

                let mut rt = runtime_for_item.lock().await;
                let cancel_requested = rt.cancel_requested;

                if let Some(state) = rt.item_states.get_mut(index) {
                    state.task_id = Some(task_id.clone());
                    match result {
                        Ok(processing_result) if processing_result.success => {
                            let _ = task_manager_for_item.update_progress(&task_id, 100.0);
                            let _ = task_manager_for_item.complete_task(&task_id);
                            if let Some(path) = processing_result.output_path {
                                state.output_path = path.to_string_lossy().into();
                            }
                            state.status = BatchItemRuntimeStatus::Completed;
                            state.progress = 100.0;
                            state.error = None;
                        }
                        Ok(processing_result) if cancel_requested => {
                            let _ = task_manager_for_item.cancel_task(&task_id);
                            state.status = BatchItemRuntimeStatus::Cancelled;
                            state.progress = 100.0;
                            state.error = None;
                            let _ = processing_result;
                        }
                        Ok(processing_result) => {
                            let error = processing_result
                                .error
                                .unwrap_or_else(|| "Unknown batch item failure".to_string());
                            let _ = task_manager_for_item.fail_task(&task_id, error.clone());
                            state.status = BatchItemRuntimeStatus::Failed;
                            state.progress = 100.0;
                            state.error = Some(error.clone());
                            rt.errors.push(format!("{}: {}", item.input_path, error));
                        }
                        Err(error) if cancel_requested => {
                            let _ = task_manager_for_item.cancel_task(&task_id);
                            state.status = BatchItemRuntimeStatus::Cancelled;
                            state.progress = 100.0;
                            state.error = None;
                            let _ = error;
                        }
                        Err(error) => {
                            let message = error.to_string();
                            let _ = task_manager_for_item.fail_task(&task_id, message.clone());
                            state.status = BatchItemRuntimeStatus::Failed;
                            state.progress = 100.0;
                            state.error = Some(message.clone());
                            rt.errors.push(format!("{}: {}", item.input_path, message));
                        }
                    }
                }

                finalize_batch_status(&mut rt);
                drop(permit);
            }));
        }

        for handle in handles {
            if let Err(error) = handle.await {
                let mut rt = runtime_for_worker.lock().await;
                rt.errors.push(format!("Export worker failed: {}", error));
            }
        }

        let mut rt = runtime_for_worker.lock().await;
        let cancelled = rt.cancel_requested;
        for state in &mut rt.item_states {
            if matches!(
                state.status,
                BatchItemRuntimeStatus::Pending | BatchItemRuntimeStatus::Running
            ) {
                state.status = if cancelled {
                    BatchItemRuntimeStatus::Cancelled
                } else {
                    BatchItemRuntimeStatus::Failed
                };
                state.progress = 100.0;
                state.error = if cancelled {
                    None
                } else {
                    Some("Export worker ended unexpectedly".to_string())
                };
            }
        }
        finalize_batch_status(&mut rt);
        rt.processor = None;

        let cleanup_batch_id = batch_id_for_worker.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            let mut states = batch_states().lock().await;
            states.remove(&cleanup_batch_id);
        });
    });

    Ok(BatchResponse {
        batch_id,
        total_items,
        status: "Started".to_string(),
        message: format!(
            "Batch processing started with {} items (max concurrency: {})",
            total_items, max_concurrent
        ),
    })
}

#[tauri::command]
pub async fn get_batch_progress(batch_id: String) -> Result<BatchProgress, String> {
    let runtime = {
        let states = batch_states().lock().await;
        states.get(&batch_id).cloned()
    }
    .ok_or_else(|| "Batch not found".to_string())?;

    let rt = runtime.lock().await;
    let (completed_items, failed_items, cancelled_items) = batch_counts(&rt.item_states);

    let items = rt
        .item_states
        .iter()
        .map(|state| BatchItemProgress {
            input_path: state.input_path.clone(),
            output_path: state.output_path.clone(),
            status: state.status.as_str().to_string(),
            progress: match state.status {
                BatchItemRuntimeStatus::Pending => 0.0,
                BatchItemRuntimeStatus::Running => state.progress.clamp(0.0, 99.0),
                BatchItemRuntimeStatus::Completed
                | BatchItemRuntimeStatus::Failed
                | BatchItemRuntimeStatus::Cancelled => 100.0,
            },
            error: state.error.clone(),
            encoder: state.encoder.clone(),
            speed: (state.status == BatchItemRuntimeStatus::Running)
                .then_some(state.speed)
                .flatten(),
            eta_seconds: (state.status == BatchItemRuntimeStatus::Running)
                .then_some(state.eta_seconds)
                .flatten(),
            message: state.message.clone(),
        })
        .collect::<Vec<_>>();

    Ok(BatchProgress {
        batch_id,
        total_items: rt.item_states.len(),
        completed_items,
        failed_items,
        cancelled_items,
        current_item: current_item(&rt.item_states),
        overall_progress: overall_progress(&rt.item_states),
        status: rt.status.as_str().to_string(),
        errors: rt.errors.clone(),
        items,
    })
}

#[tauri::command]
pub async fn cancel_batch(
    batch_id: String,
    task_manager: State<'_, TaskManager>,
) -> Result<String, String> {
    let runtime = {
        let states = batch_states().lock().await;
        states.get(&batch_id).cloned()
    }
    .ok_or_else(|| "Batch not found".to_string())?;

    let (running_task_ids, processor) = {
        let mut rt = runtime.lock().await;
        if matches!(
            rt.status,
            BatchRuntimeStatus::Completed
                | BatchRuntimeStatus::Failed
                | BatchRuntimeStatus::Cancelled
        ) {
            return Ok("Batch already finished".into());
        }
        rt.cancel_requested = true;
        for control in &rt.photo_controls {
            control.cancel();
        }
        if matches!(rt.status, BatchRuntimeStatus::Running) {
            rt.status = BatchRuntimeStatus::Cancelling;
        }

        for state in &mut rt.item_states {
            if state.status == BatchItemRuntimeStatus::Pending {
                state.status = BatchItemRuntimeStatus::Cancelled;
                state.progress = 100.0;
                state.error = None;
            }
        }

        let task_ids = rt
            .item_states
            .iter()
            .filter(|state| state.status == BatchItemRuntimeStatus::Running)
            .filter_map(|state| state.task_id.clone())
            .collect::<Vec<_>>();

        finalize_batch_status(&mut rt);
        (task_ids, rt.processor.clone())
    };

    for task_id in running_task_ids {
        let _ = task_manager.cancel_task(&task_id);
        if let Some(processor) = &processor {
            let _ = processor.cancel_task(&task_id).await;
        }
    }

    logger::log_info(&format!("Batch cancel requested: {}", batch_id));
    Ok("Batch cancellation requested".to_string())
}

#[tauri::command]
pub async fn generate_batch_from_directory(
    input_directory: String,
    lut_path: String,
    output_directory: String,
    intensity: f32,
) -> Result<Vec<BatchItem>, String> {
    let scan_result = scan_directory_for_videos(input_directory).await?;
    let mut batch_items = Vec::new();

    for video_file in scan_result.video_files {
        let input_path = PathBuf::from(&video_file);
        let file_stem = input_path.file_stem().ok_or("Invalid file name")?;
        let extension = input_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("mp4");
        let output_file_name = format!("{}_processed.{}", file_stem.to_string_lossy(), extension);
        let output_path = PathBuf::from(&output_directory).join(output_file_name);

        batch_items.push(BatchItem {
            input_path: video_file,
            output_path: output_path.to_string_lossy().to_string(),
            lut_paths: vec![lut_path.clone()],
            lut_path: Some(lut_path.clone()),
            intensity,
            photo: None,
        });
    }

    Ok(batch_items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(input: &Path) -> BatchItem {
        BatchItem {
            input_path: input.to_string_lossy().into(),
            output_path: String::new(),
            lut_paths: vec![],
            lut_path: None,
            intensity: 1.0,
            photo: None,
        }
    }

    #[test]
    fn outputs_preserve_requested_format_structure_and_resolve_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("input");
        let output = dir.path().join("output");
        std::fs::create_dir_all(output.join("day-1")).unwrap();
        let clip = item(&source.join("day-1/clip.mov"));
        let resolved = resolve_output_path(&clip, output.to_str().unwrap(), "mp4", Some(&source));
        assert_eq!(
            Path::new(&resolved),
            output.join("day-1/clip_lut_applied.mp4")
        );
        std::fs::write(&resolved, b"existing export").unwrap();
        let mut reserved = HashSet::new();
        let first = available_output_path(Path::new(&resolved), &mut reserved).unwrap();
        let second = available_output_path(Path::new(&resolved), &mut reserved).unwrap();
        assert!(first
            .to_string_lossy()
            .ends_with("clip_lut_applied (2).mp4"));
        assert!(second
            .to_string_lossy()
            .ends_with("clip_lut_applied (3).mp4"));
        assert_eq!(std::fs::read(&resolved).unwrap(), b"existing export");
    }

    #[test]
    fn cancellation_remains_in_progress_until_running_encoder_exits() {
        let mut runtime = BatchRuntime {
            status: BatchRuntimeStatus::Running,
            cancel_requested: true,
            errors: vec![],
            processor: None,
            photo_controls: Vec::new(),
            item_states: vec![
                BatchItemRuntime {
                    input_path: "a".into(),
                    output_path: "a-out".into(),
                    status: BatchItemRuntimeStatus::Cancelled,
                    progress: 100.0,
                    task_id: None,
                    error: None,
                    encoder: None,
                    speed: None,
                    eta_seconds: None,
                    message: None,
                },
                BatchItemRuntime {
                    input_path: "b".into(),
                    output_path: "b-out".into(),
                    status: BatchItemRuntimeStatus::Running,
                    progress: 20.0,
                    task_id: None,
                    error: None,
                    encoder: None,
                    speed: None,
                    eta_seconds: None,
                    message: None,
                },
            ],
        };
        finalize_batch_status(&mut runtime);
        assert_eq!(runtime.status, BatchRuntimeStatus::Cancelling);
        runtime.item_states[1].status = BatchItemRuntimeStatus::Cancelled;
        finalize_batch_status(&mut runtime);
        assert_eq!(runtime.status, BatchRuntimeStatus::Cancelled);
        assert_eq!(overall_progress(&runtime.item_states), 100.0);
        runtime.cancel_requested = false;
        runtime.item_states[0].status = BatchItemRuntimeStatus::Completed;
        runtime.item_states[1].status = BatchItemRuntimeStatus::Failed;
        finalize_batch_status(&mut runtime);
        assert_eq!(
            runtime.status,
            BatchRuntimeStatus::Failed,
            "partial failures must not report all-success"
        );
    }

    #[test]
    fn common_directory_keeps_relative_subfolders() {
        let items = vec![
            item(Path::new("/shoot/day-1/clip.mov")),
            item(Path::new("/shoot/day-2/clip.mov")),
        ];
        assert_eq!(common_input_parent(&items), Some(PathBuf::from("/shoot")));
    }

    #[cfg(unix)]
    #[test]
    fn directory_scanning_does_not_follow_symlink_cycles() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("clip.mp4"), b"video").unwrap();
        std::os::unix::fs::symlink(dir.path(), dir.path().join("cycle")).unwrap();
        let (mut videos, mut luts, mut size) = (Vec::new(), Vec::new(), 0);
        scan_recursive(dir.path(), &["mp4"], &[], &mut videos, &mut luts, &mut size).unwrap();
        assert_eq!(videos.len(), 1);
        assert_eq!(size, 5);
    }
}
