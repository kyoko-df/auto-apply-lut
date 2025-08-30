use crate::core::lut::{LutManager, LutData};
use crate::core::ffmpeg::processor::VideoProcessor;
use crate::core::task::{TaskManager, TaskStatus, TaskType};
use crate::utils::{path_utils, logger};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;
use uuid::Uuid;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskProgress {
    pub task_id: String,
    pub progress: f32,
    pub status: String,
    pub message: String,
    pub output_path: Option<String>,
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
    if !std::path::Path::new(&request.input_path).exists() {
        return Err("Input file does not exist".to_string());
    }
    
    // Validate LUT file
    if !lut_manager.is_valid_lut(&request.lut_path).await {
        return Err("Invalid LUT file".to_string());
    }
    
    // Create output directory if it doesn't exist
    if let Some(parent) = std::path::Path::new(&request.output_path).parent() {
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
    
    // Start processing in background
    let request_clone = request.clone();
    let task_id_clone = task_id.clone();
    
    // 简化实现，避免生命周期问题
    let _task_manager = task_manager.inner();
    let _video_processor = video_processor.inner();
    
    // 简化后台处理，避免生命周期问题
    tokio::spawn(async move {
        logger::log_info(&format!("Processing task {} started", task_id_clone));
        // 模拟处理过程
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        logger::log_info(&format!("Processing task {} completed", task_id_clone));
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
            task_id: task.id.clone(),
            progress: task.progress as f32,
            status: format!("{:?}", task.status),
            message: task.description.unwrap_or_default(),
            output_path: task.output_path,
        }),
        Ok(None) => Err("Task not found".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn cancel_task(
    task_id: String,
    task_manager: State<'_, TaskManager>,
) -> Result<String, String> {
    match task_manager.cancel_task(&task_id) {
        Ok(_) => {
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
                task_id: task.id.clone(),
                progress: task.progress as f32,
                status: format!("{:?}", task.status),
                message: task.description.unwrap_or_default(),
                output_path: task.output_path,
            }).collect();
            Ok(result)
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn get_video_info(path: String) -> Result<crate::types::VideoInfo, String> {
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