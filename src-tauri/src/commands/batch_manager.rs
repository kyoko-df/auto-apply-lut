use crate::commands::encoding_options::{build_encoding_settings, ProcessingOptions};
use crate::core::ffmpeg::processor::{ProcessingProgress, VideoProcessor};
use crate::core::lut::LutManager;
use crate::core::task::{TaskManager, TaskType};
use crate::types::LutFormat;
use crate::utils::config::ConfigManager;
use crate::utils::logger;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tauri::State;
use tokio::fs;
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BatchItem {
    pub input_path: String,
    pub output_path: String,
    #[serde(default)]
    pub lut_paths: Vec<String>,
    #[serde(default)]
    pub lut_path: Option<String>,
    pub intensity: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BatchRequest {
    pub items: Vec<BatchItem>,
    pub output_directory: String,
    pub preserve_structure: bool,
    #[serde(flatten)]
    pub options: ProcessingOptions,
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
}

struct BatchRuntime {
    status: BatchRuntimeStatus,
    cancel_requested: bool,
    errors: Vec<String>,
    item_states: Vec<BatchItemRuntime>,
    processor: Option<Arc<VideoProcessor>>,
}

type BatchStateMap = HashMap<String, Arc<AsyncMutex<BatchRuntime>>>;
static BATCH_STATES: OnceLock<AsyncMutex<BatchStateMap>> = OnceLock::new();

fn batch_states() -> &'static AsyncMutex<BatchStateMap> {
    BATCH_STATES.get_or_init(|| AsyncMutex::new(HashMap::new()))
}

fn resolve_output_path(item: &BatchItem, output_directory: &str) -> String {
    if !item.output_path.trim().is_empty() {
        return item.output_path.clone();
    }

    let input_path = Path::new(&item.input_path);
    let file_stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let extension = input_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("mp4");

    let target_dir = if !output_directory.trim().is_empty() {
        PathBuf::from(output_directory)
    } else {
        input_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };

    target_dir
        .join(format!("{}_lut_applied.{}", file_stem, extension))
        .to_string_lossy()
        .to_string()
}

async fn ensure_output_parent_exists(output_path: &str) -> Result<(), String> {
    if let Some(parent) = Path::new(output_path).parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }
    Ok(())
}

fn lut_extensions() -> Vec<String> {
    LutFormat::supported_extensions()
        .into_iter()
        .map(|ext| ext.to_string())
        .collect()
}

fn scan_recursive(
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

        if entry_path.is_dir() {
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

    runtime.status = if runtime.cancel_requested && cancelled > 0 {
        BatchRuntimeStatus::Cancelled
    } else if completed == total && total > 0 {
        BatchRuntimeStatus::Completed
    } else if failed == total && total > 0 {
        BatchRuntimeStatus::Failed
    } else if runtime.item_states.iter().any(|state| {
        matches!(
            state.status,
            BatchItemRuntimeStatus::Running | BatchItemRuntimeStatus::Pending
        )
    }) {
        if runtime.cancel_requested {
            BatchRuntimeStatus::Cancelling
        } else {
            BatchRuntimeStatus::Running
        }
    } else if completed > 0 {
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
) -> Result<BatchResponse, String> {
    logger::log_info(&format!(
        "Starting batch processing with {} items",
        request.items.len()
    ));

    if request.items.is_empty() {
        return Err("Batch request must contain at least one item".to_string());
    }

    if !request.output_directory.trim().is_empty() {
        fs::create_dir_all(&request.output_directory)
            .await
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    let mut normalized_items = Vec::with_capacity(request.items.len());
    for item in request.items {
        if fs::metadata(&item.input_path).await.is_err() {
            return Err(format!("Input file does not exist: {}", item.input_path));
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
        if lut_paths.is_empty() {
            return Err(format!(
                "No LUT files provided for input: {}",
                item.input_path
            ));
        }

        for lut_path in &lut_paths {
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
        }

        let output_path = resolve_output_path(&item, &request.output_directory);
        ensure_output_parent_exists(&output_path).await?;

        normalized_items.push(BatchItem {
            lut_paths,
            output_path,
            ..item
        });
    }

    let batch_id = Uuid::new_v4().to_string();
    let total_items = normalized_items.len();
    let max_concurrent = config_manager
        .lock()
        .map_err(|e| format!("Config lock poisoned: {}", e))?
        .get_config()
        .max_concurrent_tasks
        .max(1);
    let settings = build_encoding_settings(&request.options)?;

    let mut processor = VideoProcessor::new(video_processor.ffmpeg_path().to_path_buf());
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
            })
            .collect(),
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
            let _ =
                task_manager_for_progress.update_description(&progress.task_id, progress.message);
            let _ = task_manager_for_progress.update_progress(&progress.task_id, percent);

            let mut rt = runtime_for_progress.lock().await;
            if let Some(state) = rt
                .item_states
                .iter_mut()
                .find(|state| state.task_id.as_deref() == Some(progress.task_id.as_str()))
            {
                state.progress = ((progress.progress * 100.0) as f32).clamp(0.0, 99.0);
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
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
        let mut handles = Vec::with_capacity(normalized_items.len());

        for (index, item) in normalized_items.into_iter().enumerate() {
            let permit_pool = semaphore.clone();
            let runtime_for_item = runtime_for_worker.clone();
            let task_manager_for_item = task_manager_ref.clone();
            let processor_for_item = processor_for_worker.clone();
            let settings_for_item = settings_for_worker.clone();
            let batch_label = batch_id_for_worker.clone();

            handles.push(tokio::spawn(async move {
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
                    TaskType::VideoProcessing,
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

                let result = processor_for_item
                    .apply_luts_with_task_id(
                        Path::new(&item.input_path),
                        Path::new(&item.output_path),
                        &lut_paths,
                        &settings_for_item,
                        task_id.clone(),
                        item.intensity,
                    )
                    .await;

                let mut rt = runtime_for_item.lock().await;
                let cancel_requested = rt.cancel_requested;

                if let Some(state) = rt.item_states.get_mut(index) {
                    state.task_id = Some(task_id.clone());
                    match result {
                        Ok(processing_result) if processing_result.success => {
                            let _ = task_manager_for_item.update_progress(&task_id, 100.0);
                            let _ = task_manager_for_item.complete_task(&task_id);
                            state.status = BatchItemRuntimeStatus::Completed;
                            state.progress = 100.0;
                            state.error = None;
                        }
                        Ok(processing_result) if cancel_requested => {
                            let _ = task_manager_for_item.cancel_task(&task_id);
                            state.status = BatchItemRuntimeStatus::Cancelled;
                            state.progress = 100.0;
                            state.error = None;
                            if let Some(processor) = &rt.processor {
                                let _ = processor.cancel_task(&task_id).await;
                            }
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
                            if let Some(processor) = &rt.processor {
                                let _ = processor.cancel_task(&task_id).await;
                            }
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
            let _ = handle.await;
        }

        let mut rt = runtime_for_worker.lock().await;
        if rt.cancel_requested {
            for state in &mut rt.item_states {
                if state.status == BatchItemRuntimeStatus::Pending {
                    state.status = BatchItemRuntimeStatus::Cancelled;
                    state.progress = 100.0;
                    state.error = None;
                }
            }
        }
        finalize_batch_status(&mut rt);
        rt.processor = None;

        let cleanup_batch_id = batch_id_for_worker.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
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
        rt.cancel_requested = true;
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
        });
    }

    Ok(batch_items)
}
