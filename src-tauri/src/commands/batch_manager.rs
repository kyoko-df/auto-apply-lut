use crate::core::task::{TaskManager, TaskType};
use crate::core::ffmpeg::processor::VideoProcessor;
use crate::core::lut::LutManager;
use crate::utils::{logger};
use serde::{Deserialize, Serialize};
use std::path::{PathBuf, Path};
use tauri::{State, AppHandle};
use uuid::Uuid;
use std::collections::HashMap;
use tokio::sync::{mpsc, Semaphore, Mutex};
use std::sync::Arc;
use crate::events::batch::{BatchEvent, BatchEventType, BatchStatus as BatchEvtStatus, BatchItem as BatchEvtItem, emit_batch as emit_batch_event};

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
    app_handle: AppHandle,
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
    logger::log_info(&format!("Created batch {} with {} items", batch_id, total_items));

    // 发出开始事件
    emit_batch_event(&app_handle, BatchEvent::new(
        BatchEventType::Started,
        batch_id.clone(),
        total_items as u32,
        "Batch processing started".to_string(),
    ));

    // 后台执行批处理
    let ffmpeg_path = video_processor.ffmpeg_path().to_path_buf();
    let tm = task_manager.inner().clone();
    let app = app_handle.clone();
    let items = request.items.clone();
    let max_concurrent = if request.hardware_acceleration { 1 } else { 2 };
    // 克隆一份供后台任务使用，避免将外层 batch_id 移入异步闭包
    let batch_id_for_spawn = batch_id.clone();

    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        let mut handles = Vec::new();

        // 批处理聚合状态
        let completed_items: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
        let failed_items: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
        let progress_map: Arc<Mutex<HashMap<String, f64>>> = Arc::new(Mutex::new(HashMap::new()));

        for it in items {
            // 为每个条目创建 TaskManager 任务
            let desc = format!("Batch processing: {}", it.input_path);
            let task_id = match tm.create_task(TaskType::VideoProcessing, desc) {
                Ok(id) => id,
                Err(e) => {
                    logger::log_error(&format!("Failed to create task for {}: {}", it.input_path, e));
                    {
                        let mut f = failed_items.lock().await;
                        *f += 1;
                    }
                    // 发出进度事件
                    let (c_count, f_count, overall) = {
                        let c = *completed_items.lock().await;
                        let f = *failed_items.lock().await;
                        let overall = (c as f64 + f as f64) / total_items as f64 * 100.0;
                        (c, f, overall)
                    };
                    emit_batch_event(&app, BatchEvent::new(
                        BatchEventType::Progress,
                        batch_id_for_spawn.clone(),
                        total_items as u32,
                        format!("Task create failed: {}", it.input_path),
                    ).with_progress(c_count, f_count, overall));
                    continue;
                }
            };
            let _ = tm.start_task(&task_id);

            // 并发控制
            let permit = semaphore.clone().acquire_owned().await.unwrap();

            // 构建独立处理器 + 进度转发
            let mut vp = crate::core::ffmpeg::processor::VideoProcessor::new(ffmpeg_path.clone());
            let (tx, mut rx) = mpsc::unbounded_channel::<crate::core::ffmpeg::processor::ProcessingProgress>();
            vp.set_progress_sender(tx);

            // 进度转发到 TaskManager 与批处理聚合
            let tm_progress = tm.clone();
            let app_progress = app.clone();
            let batch_id_progress = batch_id_for_spawn.clone();
            let input_for_event = it.input_path.clone();
            let output_for_event = it.output_path.clone();
            {
            let pm = progress_map.clone();
            let total_items_progress = total_items as f64;
            tokio::spawn(async move {
                while let Some(p) = rx.recv().await {
                    let _ = tm_progress.update_progress(&p.task_id, (p.progress * 100.0) as f64);
                    let _ = tm_progress.update_description(&p.task_id, p.message.clone());

                    // 更新总体进度（当前简单聚合：所有任务进度相加 / 总任务数）
                    {
                        let mut map = pm.lock().await;
                        map.insert(p.task_id.clone(), p.progress);
                        let sum_progress: f64 = map.values().copied().sum();
                        let overall = (sum_progress / total_items_progress) * 100.0;

                        // 发出进度事件（当前项目）
                        let evt_item = BatchEvtItem {
                            id: p.task_id.clone(),
                            input_path: input_for_event.clone(),
                            output_path: output_for_event.clone(),
                            lut_path: String::new(),
                            status: BatchEvtStatus::Running,
                            progress: (p.progress * 100.0) as f64,
                            error: None,
                        };
                        emit_batch_event(&app_progress, BatchEvent::new(
                            BatchEventType::Progress,
                            batch_id_progress.clone(),
                            total_items_progress as u32,
                            p.message.clone(),
                        ).with_progress(0, 0, overall).with_current_item(evt_item));
                    }
                }
            });
            }

            // 启动实际处理
            let tm_done = tm.clone();
            let app_done = app.clone();
            let batch_id_done = batch_id_for_spawn.clone();
            let total_items_done = total_items as u32;
            let completed_items_done = completed_items.clone();
            let failed_items_done = failed_items.clone();
            let input_path = it.input_path.clone();
            let output_path = it.output_path.clone();
            let lut_path = it.lut_path.clone();
            let task_id_clone = task_id.clone();

            let handle = tokio::spawn(async move {
                let _permit = permit; // 保持生命周期
                let settings = crate::core::ffmpeg::EncodingSettings::default();
                match vp
                    .apply_lut_with_task_id(
                        Path::new(&input_path),
                        Path::new(&output_path),
                        Path::new(&lut_path),
                        &settings,
                        task_id_clone.clone(),
                    )
                    .await
                {
                    Ok(res) => {
                        if res.success {
                            let _ = tm_done.set_output_path(&task_id_clone, output_path.clone());
                            let _ = tm_done.update_progress(&task_id_clone, 100.0);
                            let _ = tm_done.complete_task(&task_id_clone);
                            {
                                let mut c = completed_items_done.lock().await;
                                *c += 1;
                            }
                            let (c_count, f_count, overall) = {
                                let c = *completed_items_done.lock().await;
                                let f = *failed_items_done.lock().await;
                                let overall = (c as f64 + f as f64) / total_items_done as f64 * 100.0;
                                (c, f, overall)
                            };
                            emit_batch_event(&app_done, BatchEvent::new(
                                BatchEventType::Progress,
                                batch_id_done.clone(),
                                total_items_done,
                                "Item completed".to_string(),
                            ).with_progress(c_count, f_count, overall));
                        } else {
                            let err_msg = res.error.unwrap_or_else(|| "Unknown error".to_string());
                            let first_line = err_msg.lines().next().unwrap_or("Unknown error").to_string();
                            let _ = tm_done.update_description(&task_id_clone, first_line.clone());
                            let _ = tm_done.fail_task(&task_id_clone, err_msg);
                            {
                                let mut f = failed_items_done.lock().await;
                                *f += 1;
                            }
                            let (c_count, f_count, overall) = {
                                let c = *completed_items_done.lock().await;
                                let f = *failed_items_done.lock().await;
                                let overall = (c as f64 + f as f64) / total_items_done as f64 * 100.0;
                                (c, f, overall)
                            };
                            emit_batch_event(&app_done, BatchEvent::new(
                                BatchEventType::Progress,
                                batch_id_done.clone(),
                                total_items_done,
                                first_line,
                            ).with_progress(c_count, f_count, overall));
                        }
                    }
                    Err(e) => {
                        logger::log_error(&format!("FFmpeg failed for task {}: {}", task_id_clone, e));
                        let _ = tm_done.fail_task(&task_id_clone, e.to_string());
                        {
                            let mut f = failed_items_done.lock().await;
                            *f += 1;
                        }
                        let (c_count, f_count, overall) = {
                            let c = *completed_items_done.lock().await;
                            let f = *failed_items_done.lock().await;
                            let overall = (c as f64 + f as f64) / total_items_done as f64 * 100.0;
                            (c, f, overall)
                        };
                        emit_batch_event(&app_done, BatchEvent::new(
                            BatchEventType::Progress,
                            batch_id_done.clone(),
                            total_items_done,
                            "Item failed".to_string(),
                        ).with_progress(c_count, f_count, overall));
                    }
                }
            });

            handles.push(handle);
        }

        // 等待全部任务完成后发出完成事件
        for h in handles {
            let _ = h.await;
        }

        let c = *completed_items.lock().await;
        let f = *failed_items.lock().await;
        let overall = (c as f64 + f as f64) / total_items as f64 * 100.0;
        let final_type = if f == 0 { BatchEventType::Completed } else { BatchEventType::Failed };
        emit_batch_event(&app, BatchEvent::new(
            final_type,
            batch_id_for_spawn.clone(),
            total_items as u32,
            "Batch finished".to_string(),
        ).with_progress(c, f, overall));
    });

    // 返回批处理响应（异步进行）
    Ok(BatchResponse {
        batch_id,
        total_items,
        status: "Started".to_string(),
        message: format!("Batch processing started with {} items", total_items),
    })
}

#[derive(Deserialize)]
pub struct GetBatchProgressArgs {
    // 兼容前端两种命名：camelCase `batchId` 与 snake_case `batch_id`
    #[serde(alias = "batch_id")]
    batchId: String,
}

#[tauri::command]
pub async fn get_batch_progress(
    args: GetBatchProgressArgs,
    task_manager: State<'_, TaskManager>,
) -> Result<BatchProgress, String> {
    // 简化实现：目前进度通过事件推送，此接口返回占位信息
    // 如需轮询精确进度，可改为聚合 TaskManager 中各任务的进度
    match task_manager.get_all_tasks() {
        Ok(tasks) => {
            let total = tasks.len();
            let completed = tasks.iter().filter(|t| t.progress >= 100.0).count();
            let failed = tasks.iter().filter(|t| t.description.as_deref().unwrap_or("").starts_with("Unknown error")).count();
            let avg_progress: f64 = if total > 0 {
                tasks.iter().map(|t| t.progress).sum::<f64>() / total as f64
            } else { 0.0 };
            Ok(BatchProgress {
                batch_id: args.batchId.clone(),
                total_items: total,
                completed_items: completed as usize,
                failed_items: failed as usize,
                current_item: None,
                overall_progress: avg_progress as f32,
                status: "Running".to_string(),
                errors: Vec::new(),
            })
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn cancel_batch(
    batch_id: String,
    task_manager: State<'_, TaskManager>,
) -> Result<String, String> {
    // 简化实现：获取所有任务并尝试取消
    logger::log_info(&format!("Batch cancel requested: {}", batch_id));
    match task_manager.get_all_tasks() {
        Ok(tasks) => {
            for t in tasks {
                let _ = task_manager.cancel_task(&t.id);
            }
            Ok("Batch cancelled successfully".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
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