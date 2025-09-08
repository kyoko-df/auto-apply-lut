use crate::core::lut::{LutManager, LutData};
use crate::core::ffmpeg::processor::VideoProcessor;
use crate::core::ffmpeg::EncodingSettings;
use crate::core::task::{TaskManager, TaskStatus, TaskType};
use crate::types::{TaskProgress, VideoInfo};
use crate::utils::{path_utils, logger};
use crate::utils::config::ConfigManager;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;
use tokio::sync::mpsc;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRequest {
    pub input_path: String,
    pub output_path: String,
    pub lut_path: String,
    pub intensity: f32,
    pub hardware_acceleration: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessResponse {
    pub task_id: String,
    pub status: String,
    pub message: String,
}

#[tauri::command]
pub async fn start_video_processing(
    request: ProcessRequest,
    task_manager: State<'_, TaskManager>,
    video_processor: State<'_, VideoProcessor>,
    lut_manager: State<'_, LutManager>,
) -> Result<ProcessResponse, String> {
    logger::log_info(&format!("Starting video processing: {:?}", request));
    
    // Validate input file
    if !Path::new(&request.input_path).exists() {
        return Err("Input file does not exist".to_string());
    }
    
    // Validate LUT file
    if !lut_manager.is_valid_lut(&request.lut_path).await {
        return Err("Invalid LUT file".to_string());
    }
    
    // Create output directory if it doesn't exist
    if let Some(parent) = Path::new(&request.output_path).parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Err(format!("Failed to create output directory: {}", e));
        }
    }
    
    // Create task
    let task_id = match task_manager.create_task(
        TaskType::VideoProcessing,
        format!("Processing video: {}", request.input_path),
    ) {
        Ok(id) => id,
        Err(e) => return Err(format!("Failed to create task: {}", e)),
    };
    // Start the task lifecycle
    if let Err(e) = task_manager.start_task(&task_id) {
        logger::log_error(&format!("Failed to start task {}: {}", task_id, e));
    }

    // Prepare processor with progress channel
    let (tx, mut rx) = mpsc::unbounded_channel::<crate::core::ffmpeg::processor::ProcessingProgress>();
    let mut processor = video_processor.inner().clone_for_task();
    processor.set_progress_sender(tx);

    // Clone handles for async tasks
    let tm_for_progress = task_manager.inner().clone();
    let task_id_for_progress = task_id.clone();
    tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            // Map 0.0-1.0 to 0-100.0
            let progress = (p.progress * 100.0).clamp(0.0, 100.0);
            if let Err(e) = tm_for_progress.update_progress(&task_id_for_progress, progress) {
                logger::log_error(&format!("Failed to update progress for {}: {}", task_id_for_progress, e));
            }
        }
    });

    // Spawn the processing work
    let tm_for_task = task_manager.inner().clone();
    let task_id_for_task = task_id.clone();
    let input = PathBuf::from(&request.input_path);
    let output = PathBuf::from(&request.output_path);
    let lut = PathBuf::from(&request.lut_path);
    tokio::spawn(async move {
        let settings = EncodingSettings::default();
        match processor.apply_lut_with_task_id(&input, &output, &lut, &settings, task_id_for_task.clone()).await {
            Ok(res) => {
                if res.success {
                    if let Err(e) = tm_for_task.complete_task(&task_id_for_task) {
                        logger::log_error(&format!("Failed to complete task {}: {}", task_id_for_task, e));
                    }
                } else {
                    if let Err(e) = tm_for_task.fail_task(&task_id_for_task, res.error.unwrap_or_else(|| "Unknown error".to_string())) {
                        logger::log_error(&format!("Failed to fail task {}: {}", task_id_for_task, e));
                    }
                }
            }
            Err(e) => {
                if let Err(e2) = tm_for_task.fail_task(&task_id_for_task, e.to_string()) {
                    logger::log_error(&format!("Failed to fail task {}: {}", task_id_for_task, e2));
                }
            }
        }
    });
    
    Ok(ProcessResponse {
        task_id,
        status: "started".to_string(),
        message: "Video processing started".to_string(),
    })
}

#[tauri::command]
pub async fn get_task_progress(
    task_id: String,
    task_manager: State<'_, TaskManager>,
) -> Result<TaskProgress, String> {
    match task_manager.get_task(&task_id) {
        Ok(Some(task)) => Ok(TaskProgress {
            task_id: uuid::Uuid::parse_str(&task.id).unwrap_or_else(|_| Uuid::new_v4()),
            progress: task.progress as f32,
            current_file: task.input_path.map(PathBuf::from),
            processed_count: 0,
            total_count: 1,
            estimated_remaining: None,
            processing_speed: None,
            status_message: task.description.clone(),
        }),
        Ok(None) => Err("Task not found".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn cancel_task(
    task_id: String,
    task_manager: State<'_, TaskManager>,
    video_processor: State<'_, VideoProcessor>,
) -> Result<String, String> {
    match task_manager.cancel_task(&task_id) {
        Ok(_) => {
            // 尝试通知底层处理器取消（可能只是标记，后续可扩展为实际终止进程）
            if let Err(e) = video_processor.cancel_task(&task_id).await {
                logger::log_error(&format!("Failed to cancel processor task {}: {}", task_id, e));
            }
            logger::log_info(&format!("Task cancelled: {}", task_id));
            Ok("Task cancelled successfully".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn get_all_tasks(
    task_manager: State<'_, TaskManager>,
) -> Result<Vec<TaskProgress>, String> {
    match task_manager.get_all_tasks() {
        Ok(tasks) => {
            let result = tasks.into_iter().map(|task| TaskProgress {
                task_id: uuid::Uuid::parse_str(&task.id).unwrap_or_else(|_| Uuid::new_v4()),
                progress: task.progress as f32,
                current_file: task.input_path.map(PathBuf::from),
                processed_count: 0,
                total_count: 1,
                estimated_remaining: None,
                processing_speed: None,
                status_message: task.description.clone(),
            }).collect();
            Ok(result)
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn get_video_info(path: String, config_manager: State<'_, Mutex<ConfigManager>>) -> Result<VideoInfo, String> {
    logger::log_info(&format!("Getting video info: {}", path));
    // 优先使用配置中的 ffmpeg 路径
    let cfg_path = config_manager
        .lock().map_err(|e| format!("Config lock poisoned: {}", e))?
        .get_config()
        .ffmpeg_path
        .clone()
        .filter(|s| !s.trim().is_empty());

    let manager = if let Some(ffmpeg_path) = cfg_path {
        // 推断 ffprobe 路径：同目录下可执行名替换
        let mut ffprobe_path = std::path::PathBuf::from(&ffmpeg_path);
        let probe_name = if cfg!(target_os = "windows") { "ffprobe.exe" } else { "ffprobe" };
        if ffprobe_path.is_file() {
            ffprobe_path.pop();
            ffprobe_path.push(probe_name);
        } else if ffprobe_path.ends_with("ffmpeg") || ffmpeg_path.ends_with("ffmpeg.exe") {
            ffprobe_path.pop();
            ffprobe_path.push(probe_name);
        } else {
            ffprobe_path = std::path::PathBuf::from(probe_name);
        }
        crate::core::video::VideoManager::with_paths(ffmpeg_path, ffprobe_path.to_string_lossy().to_string())
    } else {
        match crate::core::video::VideoManager::new() {
            Ok(m) => m,
            Err(e) => return Err(e.to_string()),
        }
    };
    manager
        .get_video_info(&std::path::Path::new(&path))
        .await
        .map_err(|e| e.to_string())
}