use crate::core::ffmpeg::processor::{ProcessingProgress, VideoProcessor};
use crate::core::ffmpeg::EncodingSettings;
use crate::core::lut::LutManager;
use crate::core::task::{TaskManager, TaskType};
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
    pub hardware_acceleration: bool,
    pub output_directory: String,
    pub preserve_structure: bool,
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
    pub estimated_time: Option<u64>, // in seconds
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
    error: Option<String>,
}

struct BatchRuntime {
    status: BatchRuntimeStatus,
    total_items: usize,
    completed_items: usize,
    failed_items: usize,
    cancelled_items: usize,
    current_item: Option<String>,
    current_task_id: Option<String>,
    errors: Vec<String>,
    cancel_requested: bool,
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

#[tauri::command]
pub async fn scan_directory_for_videos(directory: String) -> Result<ScanResult, String> {
    let dir_path = PathBuf::from(&directory);

    if !dir_path.exists() || !dir_path.is_dir() {
        return Err("Directory does not exist or is not a directory".to_string());
    }

    let video_extensions: Vec<&str> = vec!["mp4", "mov", "avi", "mkv", "wmv", "flv", "webm", "m4v"];
    let lut_extensions: Vec<&str> = vec!["cube", "3dl", "lut", "csp"];

    // Run blocking I/O on a dedicated thread pool to avoid starving the async executor
    let scan_result = tokio::task::spawn_blocking(move || {
        let mut video_files = Vec::new();
        let mut lut_files = Vec::new();
        let mut total_size = 0u64;

        fn scan_recursive(
            path: &PathBuf,
            video_exts: &[&str],
            lut_exts: &[&str],
            videos: &mut Vec<String>,
            luts: &mut Vec<String>,
            size: &mut u64,
        ) -> Result<(), std::io::Error> {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let entry_path = entry.path();

                if entry_path.is_dir() {
                    scan_recursive(&entry_path, video_exts, lut_exts, videos, luts, size)?;
                } else if entry_path.is_file() {
                    if let Some(extension) = entry_path.extension() {
                        if let Some(ext_str) = extension.to_str() {
                            let ext_lower = ext_str.to_lowercase();

                            if video_exts.contains(&ext_lower.as_str()) {
                                videos.push(entry_path.to_string_lossy().to_string());
                                if let Ok(metadata) = entry.metadata() {
                                    *size += metadata.len();
                                }
                            } else if lut_exts.contains(&ext_lower.as_str()) {
                                luts.push(entry_path.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
            Ok(())
        }

        let result = scan_recursive(
            &dir_path,
            &video_extensions,
            &lut_extensions,
            &mut video_files,
            &mut lut_files,
            &mut total_size,
        );

        result.map(|_| (video_files, lut_files, total_size))
    })
    .await
    .map_err(|e| format!("Directory scan task failed: {}", e))?;

    match scan_result {
        Ok((video_files, lut_files, total_size)) => {
            // Estimate processing time (rough calculation: 1GB per 5 minutes)
            let estimated_time = if total_size > 0 {
                Some((total_size / (1024 * 1024 * 1024)) * 300) // 5 minutes per GB
            } else {
                None
            };

            Ok(ScanResult {
                video_files,
                lut_files,
                total_size,
                estimated_time,
            })
        }
        Err(e) => Err(format!("Failed to scan directory: {}", e)),
    }
}

#[tauri::command]
pub async fn start_batch_processing(
    request: BatchRequest,
    task_manager: State<'_, TaskManager>,
    video_processor: State<'_, VideoProcessor>,
    lut_manager: State<'_, LutManager>,
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
        lut_paths.retain(|p| !p.trim().is_empty());
        if lut_paths.is_empty() {
            return Err(format!("No LUT files provided for input: {}", item.input_path));
        }

        for lut_path in &lut_paths {
            if fs::metadata(lut_path).await.is_err() {
                return Err(format!("LUT file does not exist: {}", lut_path));
            }

            let lut_validation = lut_manager
                .validate_lut(lut_path)
                .await
                .map_err(|e| format!("Failed to validate LUT {}: {}", lut_path, e))?;
            if !lut_validation.is_valid {
                return Err(format!(
                    "Invalid LUT file {}: {}",
                    lut_path,
                    lut_validation.errors.join("; ")
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

    let mut processor = VideoProcessor::new(video_processor.ffmpeg_path().to_path_buf());
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<ProcessingProgress>();
    processor.set_progress_sender(progress_tx);
    let processor = Arc::new(processor);

    let runtime = Arc::new(AsyncMutex::new(BatchRuntime {
        status: BatchRuntimeStatus::Running,
        total_items,
        completed_items: 0,
        failed_items: 0,
        cancelled_items: 0,
        current_item: None,
        current_task_id: None,
        errors: Vec::new(),
        cancel_requested: false,
        item_states: normalized_items
            .iter()
            .map(|item| BatchItemRuntime {
                input_path: item.input_path.clone(),
                output_path: item.output_path.clone(),
                status: BatchItemRuntimeStatus::Pending,
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
    let task_manager_for_progress = task_manager_ref.clone();
    tokio::spawn(async move {
        while let Some(p) = progress_rx.recv().await {
            let _ = task_manager_for_progress.update_progress(&p.task_id, (p.progress * 100.0) as f64);
            let _ = task_manager_for_progress.update_description(&p.task_id, p.message);
        }
    });

    let runtime_for_worker = runtime.clone();
    let batch_id_for_worker = batch_id.clone();
    let processor_for_worker = processor.clone();
    let hardware_acceleration = request.hardware_acceleration;

    tokio::spawn(async move {
        let mut settings = EncodingSettings::default();
        if hardware_acceleration {
            settings
                .extra_params
                .insert("-hwaccel".to_string(), "auto".to_string());
        }

        for (index, item) in normalized_items.into_iter().enumerate() {
            {
                let mut rt = runtime_for_worker.lock().await;
                if rt.cancel_requested {
                    break;
                }
                rt.status = BatchRuntimeStatus::Running;
                rt.current_item = Some(item.input_path.clone());
                if let Some(state) = rt.item_states.get_mut(index) {
                    state.status = BatchItemRuntimeStatus::Running;
                    state.error = None;
                }
            }

            let task_id = match task_manager_ref.create_task(
                TaskType::VideoProcessing,
                format!("Batch {}: {}", batch_id_for_worker, item.input_path),
            ) {
                Ok(id) => id,
                Err(e) => {
                    let err = format!("Failed to create task for {}: {}", item.input_path, e);
                    let mut rt = runtime_for_worker.lock().await;
                    rt.failed_items += 1;
                    rt.errors.push(err);
                    if let Some(state) = rt.item_states.get_mut(index) {
                        state.status = BatchItemRuntimeStatus::Failed;
                        state.error = Some("Failed to create task".to_string());
                    }
                    rt.current_item = None;
                    rt.current_task_id = None;
                    continue;
                }
            };

            let _ = task_manager_ref.start_task(&task_id);
            let _ = task_manager_ref.set_output_path(&task_id, item.output_path.clone());

            {
                let mut rt = runtime_for_worker.lock().await;
                rt.current_task_id = Some(task_id.clone());
            }

            let process_result = processor_for_worker
                .apply_luts_with_task_id(
                    Path::new(&item.input_path),
                    Path::new(&item.output_path),
                    &item
                        .lut_paths
                        .iter()
                        .map(PathBuf::from)
                        .collect::<Vec<_>>(),
                    &settings,
                    task_id.clone(),
                    item.intensity,
                )
                .await;

            let mut rt = runtime_for_worker.lock().await;
            let cancel_requested = rt.cancel_requested;

            match process_result {
                Ok(result) if result.success => {
                    let _ = task_manager_ref.update_progress(&task_id, 100.0);
                    let _ = task_manager_ref.complete_task(&task_id);
                    rt.completed_items += 1;
                    if let Some(state) = rt.item_states.get_mut(index) {
                        state.status = BatchItemRuntimeStatus::Completed;
                        state.error = None;
                    }
                }
                Ok(result) => {
                    if cancel_requested {
                        let _ = task_manager_ref.cancel_task(&task_id);
                        rt.cancelled_items += 1;
                        if let Some(state) = rt.item_states.get_mut(index) {
                            state.status = BatchItemRuntimeStatus::Cancelled;
                            state.error = None;
                        }
                    } else {
                        let error = result.error.unwrap_or_else(|| "Unknown batch item failure".to_string());
                        let _ = task_manager_ref.fail_task(&task_id, error.clone());
                        rt.failed_items += 1;
                        rt.errors.push(format!("{}: {}", item.input_path, error.clone()));
                        if let Some(state) = rt.item_states.get_mut(index) {
                            state.status = BatchItemRuntimeStatus::Failed;
                            state.error = Some(error);
                        }
                    }
                }
                Err(e) => {
                    if cancel_requested {
                        let _ = task_manager_ref.cancel_task(&task_id);
                        rt.cancelled_items += 1;
                        if let Some(state) = rt.item_states.get_mut(index) {
                            state.status = BatchItemRuntimeStatus::Cancelled;
                            state.error = None;
                        }
                    } else {
                        let message = e.to_string();
                        let _ = task_manager_ref.fail_task(&task_id, message.clone());
                        rt.failed_items += 1;
                        rt.errors.push(format!("{}: {}", item.input_path, message.clone()));
                        if let Some(state) = rt.item_states.get_mut(index) {
                            state.status = BatchItemRuntimeStatus::Failed;
                            state.error = Some(message);
                        }
                    }
                }
            }

            rt.current_item = None;
            rt.current_task_id = None;

            if rt.cancel_requested {
                break;
            }
        }

        let mut rt = runtime_for_worker.lock().await;
        if rt.cancel_requested {
            let mut newly_cancelled = 0usize;
            for state in &mut rt.item_states {
                if state.status == BatchItemRuntimeStatus::Pending {
                    state.status = BatchItemRuntimeStatus::Cancelled;
                    state.error = None;
                    newly_cancelled += 1;
                }
            }
            rt.cancelled_items += newly_cancelled;
            rt.status = BatchRuntimeStatus::Cancelled;
        } else if rt.completed_items == rt.total_items {
            rt.status = BatchRuntimeStatus::Completed;
        } else if rt.completed_items == 0 && rt.failed_items > 0 {
            rt.status = BatchRuntimeStatus::Failed;
        } else {
            rt.status = BatchRuntimeStatus::Completed;
        }

        rt.current_item = None;
        rt.current_task_id = None;
        rt.processor = None;

        // Schedule cleanup of this batch entry from BATCH_STATES after a delay,
        // giving the frontend time to poll the final status.
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
        message: format!("Batch processing started with {} items", total_items),
    })
}

#[tauri::command]
pub async fn get_batch_progress(
    batch_id: String,
    task_manager: State<'_, TaskManager>,
) -> Result<BatchProgress, String> {
    let runtime = {
        let states = batch_states().lock().await;
        states.get(&batch_id).cloned()
    }
    .ok_or_else(|| "Batch not found".to_string())?;

    let (
        total_items,
        completed_items,
        failed_items,
        cancelled_items,
        current_item,
        current_task_id,
        status,
        errors,
        item_states,
    ) = {
        let rt = runtime.lock().await;
        (
            rt.total_items,
            rt.completed_items,
            rt.failed_items,
            rt.cancelled_items,
            rt.current_item.clone(),
            rt.current_task_id.clone(),
            rt.status,
            rt.errors.clone(),
            rt.item_states.clone(),
        )
    };

    let mut current_fraction = 0.0f64;
    if let Some(task_id) = current_task_id {
        if let Ok(Some(task)) = task_manager.get_task(&task_id) {
            current_fraction = (task.progress / 100.0).clamp(0.0, 0.99);
        }
    }

    let done_units = completed_items + failed_items + cancelled_items;
    let overall_progress = if total_items == 0 {
        0.0
    } else {
        (((done_units as f64 + current_fraction) / total_items as f64) * 100.0).clamp(0.0, 100.0)
            as f32
    };

    let running_input = current_item.clone();
    let running_progress = (current_fraction * 100.0) as f32;
    let items = item_states
        .into_iter()
        .map(|state| {
            let progress = match state.status {
                BatchItemRuntimeStatus::Pending => 0.0,
                BatchItemRuntimeStatus::Running => {
                    if running_input.as_deref() == Some(state.input_path.as_str()) {
                        running_progress.clamp(0.0, 99.0)
                    } else {
                        0.0
                    }
                }
                BatchItemRuntimeStatus::Completed
                | BatchItemRuntimeStatus::Failed
                | BatchItemRuntimeStatus::Cancelled => 100.0,
            };

            BatchItemProgress {
                input_path: state.input_path,
                output_path: state.output_path,
                status: state.status.as_str().to_string(),
                progress,
                error: state.error,
            }
        })
        .collect();

    Ok(BatchProgress {
        batch_id,
        total_items,
        completed_items,
        failed_items,
        cancelled_items,
        current_item,
        overall_progress,
        status: status.as_str().to_string(),
        errors,
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

    let (current_task_id, processor) = {
        let mut rt = runtime.lock().await;
        rt.cancel_requested = true;
        if matches!(rt.status, BatchRuntimeStatus::Running) {
            rt.status = BatchRuntimeStatus::Cancelling;
        }
        (rt.current_task_id.clone(), rt.processor.clone())
    };

    if let Some(task_id) = current_task_id {
        let _ = task_manager.cancel_task(&task_id);
        if let Some(processor) = processor {
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
        let extension = input_path.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
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
