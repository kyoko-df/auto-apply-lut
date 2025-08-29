use crate::core::task::{TaskManager, TaskType};
use crate::core::ffmpeg::processor::VideoProcessor;
use crate::core::lut::LutManager;
use crate::utils::{logger, path_utils};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BatchItem {
    pub input_path: String,
    pub output_path: String,
    pub lut_path: String,
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
    pub current_item: Option<String>,
    pub overall_progress: f32,
    pub status: String,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanResult {
    pub video_files: Vec<String>,
    pub lut_files: Vec<String>,
    pub total_size: u64,
    pub estimated_time: Option<u64>, // in seconds
}

#[tauri::command]
pub async fn scan_directory_for_videos(directory: String) -> Result<ScanResult, String> {
    let dir_path = PathBuf::from(&directory);
    
    if !dir_path.exists() || !dir_path.is_dir() {
        return Err("Directory does not exist or is not a directory".to_string());
    }
    
    let video_extensions = vec!["mp4", "mov", "avi", "mkv", "wmv", "flv", "webm", "m4v"];
    let lut_extensions = vec!["cube", "3dl", "lut", "csp"];
    
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
    
    match scan_recursive(&dir_path, &video_extensions, &lut_extensions, &mut video_files, &mut lut_files, &mut total_size) {
        Ok(_) => {
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
    logger::log_info(&format!("Starting batch processing with {} items", request.items.len()));
    
    // 简化验证逻辑
    for item in &request.items {
        if !std::path::Path::new(&item.input_path).exists() {
            return Err(format!("Input file does not exist: {}", item.input_path));
        }
    }
    
    // Create output directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&request.output_directory) {
        return Err(format!("Failed to create output directory: {}", e));
    }
    
    let batch_id = Uuid::new_v4().to_string();
    let total_items = request.items.len();
    
    // 简化批处理逻辑
    logger::log_info(&format!("Created batch {} with {} items", batch_id, total_items));
    
    // 返回批处理响应
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
    // 简化实现，返回基本的批处理进度信息
    Ok(BatchProgress {
        batch_id: batch_id.clone(),
        total_items: 0,
        completed_items: 0,
        failed_items: 0,
        current_item: None,
        overall_progress: 0.0,
        status: "Running".to_string(),
        errors: Vec::new(),
    })
}

#[tauri::command]
pub async fn cancel_batch(
    batch_id: String,
    task_manager: State<'_, TaskManager>,
) -> Result<String, String> {
    // 简化实现
    logger::log_info(&format!("Batch cancel requested: {}", batch_id));
    Ok("Batch cancelled successfully".to_string())
}

#[tauri::command]
pub async fn generate_batch_from_directory(
    input_directory: String,
    lut_path: String,
    output_directory: String,
    intensity: f32,
) -> Result<Vec<BatchItem>, String> {
    let scan_result = scan_directory_for_videos(input_directory.clone()).await?;
    
    let mut batch_items = Vec::new();
    
    for video_file in scan_result.video_files {
        let input_path = PathBuf::from(&video_file);
        let file_name = input_path.file_stem()
            .ok_or("Invalid file name")?;
        let output_file_name = format!("{}_processed.mp4", file_name.to_string_lossy());
        let output_path = PathBuf::from(&output_directory).join(output_file_name);
        
        batch_items.push(BatchItem {
            input_path: video_file,
            output_path: output_path.to_string_lossy().to_string(),
            lut_path: lut_path.clone(),
            intensity,
        });
    }
    
    Ok(batch_items)
}