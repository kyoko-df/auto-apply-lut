use crate::core::lut::{LutManager, LutData};
use crate::core::ffmpeg::processor::VideoProcessor;
use crate::core::ffmpeg::processor::ProcessingProgress;
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
use tokio::fs;
use tokio::process::Command as AsyncCommand;
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
    let lut_clone = request.lut_path.clone();
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

        match vp
            .apply_lut_with_task_id(
                Path::new(&input_clone),
                Path::new(&final_output_clone),
                Path::new(&lut_clone),
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
        message: "Video processing started".to_string(),
        output_path: final_output_path,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct PreviewFrameResponse {
    pub png_path: String,
    pub timestamp: f64,
}

#[tauri::command]
pub async fn render_preview_frame(
    path: String,
    timestamp: f64,
    width: Option<u32>,
    config_manager: State<'_, Mutex<ConfigManager>>,
) -> Result<PreviewFrameResponse, String> {
    if fs::metadata(&path).await.is_err() {
        return Err("Input file does not exist".to_string());
    }

    let cfg_ffmpeg = config_manager
        .lock()
        .map_err(|e| format!("Config lock poisoned: {}", e))?
        .get_config()
        .ffmpeg_path
        .clone()
        .filter(|s| !s.trim().is_empty());

    let ffmpeg_path = if let Some(p) = cfg_ffmpeg {
        let pb = PathBuf::from(&p);
        if pb.exists() { pb } else { PathBuf::from("ffmpeg") }
    } else {
        crate::core::ffmpeg::discover_ffmpeg_path().unwrap_or_else(|_| PathBuf::from("ffmpeg"))
    };

    let out_dir = std::env::temp_dir().join("auto-apply-lut-preview");
    fs::create_dir_all(&out_dir)
        .await
        .map_err(|e| format!("Failed to create preview directory: {}", e))?;

    let t_ms = (timestamp.max(0.0) * 1000.0).round() as u64;
    let out_path = out_dir.join(format!("frame_{}_{}.png", Uuid::new_v4(), t_ms));

    let ts = format!("{:.3}", timestamp.max(0.0));
    let mut cmd = AsyncCommand::new(ffmpeg_path);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-ss")
        .arg(ts)
        .arg("-i")
        .arg(&path)
        .arg("-frames:v")
        .arg("1")
        .arg("-an");

    if let Some(w) = width.filter(|v| *v > 0) {
        cmd.arg("-vf").arg(format!("scale={}:-2", w));
    }

    cmd.arg(&out_path);

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let msg = if stderr.trim().is_empty() {
            "ffmpeg failed to render frame".to_string()
        } else {
            stderr
        };
        return Err(msg);
    }

    Ok(PreviewFrameResponse {
        png_path: out_path.to_string_lossy().to_string(),
        timestamp: timestamp.max(0.0),
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PreviewFrameItem {
    pub image_path: String,
    pub timestamp: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PreviewFramesResponse {
    pub start_time: f64,
    pub fps: f64,
    pub frames: Vec<PreviewFrameItem>,
}

#[tauri::command]
pub async fn prefetch_preview_frames(
    path: String,
    start_time: f64,
    duration: f64,
    fps: f64,
    width: Option<u32>,
    config_manager: State<'_, Mutex<ConfigManager>>,
) -> Result<PreviewFramesResponse, String> {
    if fs::metadata(&path).await.is_err() {
        return Err("Input file does not exist".to_string());
    }

    let fps = if fps.is_finite() && fps > 0.0 { fps } else { 12.0 };
    let duration = if duration.is_finite() && duration > 0.0 { duration } else { 3.0 };
    let start_time = start_time.max(0.0);

    let cfg_ffmpeg = config_manager
        .lock()
        .map_err(|e| format!("Config lock poisoned: {}", e))?
        .get_config()
        .ffmpeg_path
        .clone()
        .filter(|s| !s.trim().is_empty());

    let ffmpeg_path = if let Some(p) = cfg_ffmpeg {
        let pb = PathBuf::from(&p);
        if pb.exists() { pb } else { PathBuf::from("ffmpeg") }
    } else {
        crate::core::ffmpeg::discover_ffmpeg_path().unwrap_or_else(|_| PathBuf::from("ffmpeg"))
    };

    let out_root = std::env::temp_dir().join("auto-apply-lut-preview-segments");
    fs::create_dir_all(&out_root)
        .await
        .map_err(|e| format!("Failed to create preview directory: {}", e))?;
    let out_dir = out_root.join(Uuid::new_v4().to_string());
    fs::create_dir_all(&out_dir)
        .await
        .map_err(|e| format!("Failed to create segment directory: {}", e))?;

    let out_pattern = out_dir.join("frame_%05d.jpg");
    let out_pattern_str = out_pattern.to_string_lossy().to_string();

    logger::log_info(&format!(
        "Prefetch preview frames: input='{}', start_time={:.3}, duration={:.3}, fps={:.3}, width={:?}, ffmpeg='{}', out='{}'",
        path,
        start_time,
        duration,
        fps,
        width,
        ffmpeg_path.to_string_lossy(),
        out_pattern_str
    ));

    let mut cmd = AsyncCommand::new(ffmpeg_path);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-ss")
        .arg(format!("{:.3}", start_time))
        .arg("-i")
        .arg(&path)
        .arg("-t")
        .arg(format!("{:.3}", duration))
        .arg("-an")
        .arg("-vf");

    let mut vf = format!("fps={}", fps);
    if let Some(w) = width.filter(|v| *v > 0) {
        vf.push_str(&format!(",scale={}:-2", w));
    }
    cmd.arg(vf);

    cmd.arg("-q:v")
        .arg("4")
        .arg("-vsync")
        .arg("vfr")
        .arg("-y")
        .arg(out_pattern_str);

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let msg = if stderr.trim().is_empty() && stdout.trim().is_empty() {
            "ffmpeg failed to prefetch frames".to_string()
        } else if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        };
        logger::log_error(&format!(
            "Prefetch preview frames failed (status={}): {}",
            output.status,
            msg
        ));
        return Err(msg);
    }

    let mut frames = Vec::new();
    let mut entries = fs::read_dir(&out_dir)
        .await
        .map_err(|e| format!("Failed to read segment directory: {}", e))?;
    let mut image_paths: Vec<PathBuf> = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("Failed to read segment directory entry: {}", e))?
    {
        let p = entry.path();
        if p.is_file() {
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("frame_") && name.ends_with(".jpg") {
                    image_paths.push(p);
                }
            }
        }
    }
    image_paths.sort();
    for (i, p) in image_paths.into_iter().enumerate() {
        let ts = start_time + (i as f64) / fps;
        frames.push(PreviewFrameItem {
            image_path: p.to_string_lossy().to_string(),
            timestamp: ts,
        });
    }

    Ok(PreviewFramesResponse {
        start_time,
        fps,
        frames,
    })
}
