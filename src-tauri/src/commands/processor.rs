use crate::core::lut::{LutManager, LutData};
use crate::core::ffmpeg::processor::VideoProcessor;
use crate::core::ffmpeg::EncodingSettings;
use crate::core::task::{TaskManager, TaskStatus, TaskType};
use crate::types::{TaskProgress, VideoInfo};
use crate::utils::{path_utils, logger};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;
use tokio::sync::mpsc;
use tokio::fs;

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
    
    // Validate input file (async)
    if fs::metadata(&request.input_path).await.is_err() {
        return Err("Input file does not exist".to_string());
    }
    
    // Validate LUT file
    if !lut_manager.is_valid_lut(&request.lut_path).await {
        return Err("Invalid LUT file".to_string());
    }
    
    // Create output directory if it doesn't exist (async)
    if let Some(parent) = Path::new(&request.output_path).parent() {
        if let Err(e) = fs::create_dir_all(parent).await {
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

    // For now, create a simple synchronous processing that returns immediately
    // In a real implementation, this would spawn a background task

    // Generate output path if empty
    let final_output_path = if request.output_path.is_empty() {
        let input_path = Path::new(&request.input_path);
        let parent = input_path.parent().unwrap_or_else(|| Path::new("."));
        let file_stem = input_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let extension = input_path.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("mp4");
        parent.join(format!("{}_processed.{}", file_stem, extension)).to_string_lossy().to_string()
    } else {
        request.output_path.clone()
    };

    // Start processing in background (simplified for now)
    let task_id_clone = task_id.clone();
    let final_output_clone = final_output_path.clone();
    let input_clone = request.input_path.clone();
    let lut_clone = request.lut_path.clone();

    tokio::spawn(async move {
        logger::log_info(&format!("Starting background processing for task: {}", task_id_clone));

        // Simulate processing for now
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // TODO: Implement actual FFmpeg processing here
        logger::log_info(&format!("Background processing completed for task: {}", task_id_clone));
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
pub async fn get_video_info(path: String) -> Result<VideoInfo, String> {
    logger::log_info(&format!("Getting video info: {}", path));
    let manager = match crate::core::video::VideoManager::new() {
        Ok(m) => m,
        Err(e) => return Err(e.to_string()),
    };
    manager
        .get_video_info(&std::path::Path::new(&path))
        .await
        .map_err(|e| e.to_string())
}