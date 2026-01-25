use crate::core::lut::LutManager;
use crate::core::ffmpeg::processor::VideoProcessor;
use crate::core::ffmpeg::processor::ProcessingProgress;
use crate::core::ffmpeg::EncodingSettings;
use crate::core::task::{TaskManager, TaskType};
use crate::types::{TaskProgress, VideoInfo};
use crate::utils::logger;
use crate::utils::config::ConfigManager;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;
use tokio::sync::mpsc;
use tokio::fs;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LutErrorStrategy {
    StopOnError,
    SkipOnError,
}

impl Default for LutErrorStrategy {
    fn default() -> Self {
        Self::StopOnError
    }
}

fn apply_lut_error_strategy(
    strategy: LutErrorStrategy,
    valid_lut_paths: Vec<String>,
    invalid_lut_messages: Vec<String>,
) -> Result<(Vec<String>, Vec<String>), String> {
    match strategy {
        LutErrorStrategy::StopOnError => {
            if !invalid_lut_messages.is_empty() {
                return Err(invalid_lut_messages.join("\n"));
            }
            Ok((valid_lut_paths, invalid_lut_messages))
        }
        LutErrorStrategy::SkipOnError => {
            if valid_lut_paths.is_empty() {
                return Err(invalid_lut_messages.join("\n"));
            }
            Ok((valid_lut_paths, invalid_lut_messages))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRequest {
    pub input_path: String,
    pub output_path: String,
    #[serde(default)]
    pub lut_paths: Vec<String>,
    #[serde(default)]
    pub lut_path: Option<String>,
    pub intensity: f32,
    pub hardware_acceleration: bool,
    #[serde(default)]
    pub lut_error_strategy: LutErrorStrategy,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessResponse {
    pub task_id: String,
    pub status: String,
    pub message: String,
    pub output_path: String,
}

#[tauri::command]
pub async fn start_video_processing(
    request: ProcessRequest,
    task_manager: State<'_, TaskManager>,
    video_processor: State<'_, VideoProcessor>,
    lut_manager: State<'_, LutManager>,
) -> Result<ProcessResponse, String> {
    logger::log_info(&format!("Starting video processing: {:?}", request));

    let mut lut_paths = request.lut_paths.clone();
    if lut_paths.is_empty() {
        if let Some(single) = request.lut_path.clone() {
            if !single.trim().is_empty() {
                lut_paths.push(single);
            }
        }
    }
    lut_paths.retain(|p| !p.trim().is_empty());
    if lut_paths.is_empty() {
        return Err("No LUT files provided".to_string());
    }
    
    // Validate input file (async)
    if fs::metadata(&request.input_path).await.is_err() {
        return Err("Input file does not exist".to_string());
    }
    
    let mut valid_lut_paths: Vec<String> = Vec::new();
    let mut invalid_lut_messages: Vec<String> = Vec::new();
    for lut_path in &lut_paths {
        match lut_manager.validate_lut(lut_path).await {
            Ok(result) => {
                if result.is_valid {
                    valid_lut_paths.push(lut_path.clone());
                } else {
                    let msg = if result.errors.is_empty() {
                        format!("Invalid LUT file: {}", lut_path)
                    } else {
                        format!("Invalid LUT file: {} ({})", lut_path, result.errors.join("; "))
                    };
                    invalid_lut_messages.push(msg);
                }
            }
            Err(e) => {
                invalid_lut_messages.push(format!("Invalid LUT file: {} ({})", lut_path, e));
            }
        }
    }
    let (valid_lut_paths, invalid_lut_messages) =
        apply_lut_error_strategy(request.lut_error_strategy, valid_lut_paths, invalid_lut_messages)?;
    
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

    // 生成最终输出路径（如未提供）
    let final_output_path = if request.output_path.is_empty() {
        let input_path = Path::new(&request.input_path);
        let parent = input_path.parent().unwrap_or_else(|| Path::new("."));
        let file_stem = input_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let extension = input_path.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("mp4");
        parent.join(format!("{}_lut_applied.{}", file_stem, extension)).to_string_lossy().to_string()
    } else {
        request.output_path.clone()
    };

    // 确保最终输出路径的父目录存在且可写
    let out_parent = Path::new(&final_output_path)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    if let Err(e) = fs::create_dir_all(&out_parent).await {
        return Err(format!("Failed to create output directory: {}", e));
    }
    // 写权限快速检测
    let test_file = out_parent.join(".write_test.tmp");
    match fs::File::create(&test_file).await {
        Ok(_) => {
            let _ = fs::remove_file(&test_file).await;
        }
        Err(e) => {
            return Err(format!("Output directory not writable: {}", e));
        }
    }

    // 后台启动真实 FFmpeg 处理
    let task_id_clone = task_id.clone();
    let final_output_clone = final_output_path.clone();
    let input_clone = request.input_path.clone();
    let lut_clones = valid_lut_paths.clone();
    let tm = task_manager.inner().clone();

    // Extract FFmpeg path from VideoProcessor
    let ffmpeg_path = video_processor.ffmpeg_path().to_path_buf();
    let settings = EncodingSettings::default();

    tokio::spawn(async move {
        logger::log_info(&format!("Starting real FFmpeg processing for task: {}", task_id_clone));
        // 为后台任务创建独立的处理器实例，并接入进度事件
        let mut vp = crate::core::ffmpeg::processor::VideoProcessor::new(ffmpeg_path);
        let (tx, mut rx) = mpsc::unbounded_channel::<ProcessingProgress>();
        vp.set_progress_sender(tx);

        // 转发进度到TaskManager（实时更新进度与状态消息）
        let tm_progress = tm.clone();
        tokio::spawn(async move {
            while let Some(p) = rx.recv().await {
                let _ = tm_progress.update_progress(&p.task_id, (p.progress * 100.0) as f64);
                let _ = tm_progress.update_description(&p.task_id, p.message.clone());
            }
        });

        let luts: Vec<PathBuf> = lut_clones.iter().map(PathBuf::from).collect();
        match vp
            .apply_luts_with_task_id(
                Path::new(&input_clone),
                Path::new(&final_output_clone),
                &luts,
                &settings,
                task_id_clone.clone(),
            )
            .await
        {
            Ok(res) => {
                if res.success {
                    logger::log_info(&format!(
                        "FFmpeg processing completed successfully for task: {} -> {:?}",
                        task_id_clone, res.output_path
                    ));
                    let _ = tm.set_output_path(&task_id_clone, final_output_clone.clone());
                    let _ = tm.update_progress(&task_id_clone, 100.0);
                    let _ = tm.complete_task(&task_id_clone);
                } else {
                    let err_msg = res.error.unwrap_or_else(|| "Unknown error".to_string());
                    logger::log_error(&format!(
                        "FFmpeg processing failed for task {}: {}",
                        task_id_clone,
                        err_msg
                    ));
                    // 将失败摘要同步到任务描述，便于前端显示详细原因
                    let first_line = err_msg.lines().next().unwrap_or("Unknown error").to_string();
                    let _ = tm.update_description(&task_id_clone, first_line);
                    let _ = tm.fail_task(&task_id_clone, err_msg);
                }
            }
            Err(e) => {
                logger::log_error(&format!("Failed to execute FFmpeg for task {}: {}", task_id_clone, e));
                let _ = tm.fail_task(&task_id_clone, e.to_string());
            }
        }
    });
    
    Ok(ProcessResponse {
        task_id,
        status: "started".to_string(),
        message: if invalid_lut_messages.is_empty() {
            "Video processing started".to_string()
        } else {
            format!(
                "Video processing started ({} LUT skipped)",
                invalid_lut_messages.len()
            )
        },
        output_path: final_output_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lut_error_strategy_stop_on_error() {
        let res = apply_lut_error_strategy(
            LutErrorStrategy::StopOnError,
            vec!["/a.cube".to_string()],
            vec!["bad".to_string()],
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_lut_error_strategy_skip_on_error_keeps_valid() {
        let (valid, invalid) = apply_lut_error_strategy(
            LutErrorStrategy::SkipOnError,
            vec!["/a.cube".to_string(), "/b.cube".to_string()],
            vec!["bad".to_string()],
        )
        .unwrap();
        assert_eq!(valid, vec!["/a.cube".to_string(), "/b.cube".to_string()]);
        assert_eq!(invalid.len(), 1);
    }

    #[test]
    fn test_lut_error_strategy_skip_on_error_no_valid_fails() {
        let res = apply_lut_error_strategy(
            LutErrorStrategy::SkipOnError,
            vec![],
            vec!["bad".to_string()],
        );
        assert!(res.is_err());
    }
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
