//! FFmpeg视频处理器
//! 提供视频处理的核心功能

use crate::types::{AppResult, AppError};
use crate::core::ffmpeg::{EncodingSettings, VideoInfo, BatchTask, BatchResult};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command as AsyncCommand;
use tokio::io::{AsyncBufReadExt, BufReader};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use serde::{Serialize, Deserialize};
use std::time::{Duration, Instant};
use std::collections::HashMap;

/// 视频处理器
pub struct VideoProcessor {
    /// FFmpeg可执行文件路径
    ffmpeg_path: PathBuf,
    /// 当前处理任务
    current_tasks: Arc<Mutex<Vec<ProcessingTask>>>,
    /// 进度发送器
    progress_sender: Option<mpsc::UnboundedSender<ProcessingProgress>>,
    /// 取消信号发送器映射（task_id -> oneshot sender）
    cancel_senders: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>>,
}

impl VideoProcessor {
    /// 创建新的视频处理器
    pub fn new(ffmpeg_path: PathBuf) -> Self {
        Self {
            ffmpeg_path,
            current_tasks: Arc::new(Mutex::new(Vec::new())),
            progress_sender: None,
            cancel_senders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 设置进度发送器
    pub fn set_progress_sender(&mut self, sender: mpsc::UnboundedSender<ProcessingProgress>) {
        self.progress_sender = Some(sender);
    }

    /// 应用LUT到视频
    pub async fn apply_lut(
        &self,
        input_path: &Path,
        output_path: &Path,
        lut_path: &Path,
        settings: &EncodingSettings,
    ) -> AppResult<ProcessingResult> {
        let task_id = uuid::Uuid::new_v4().to_string();
        let start_time = Instant::now();
        
        // 创建处理任务
        let task = ProcessingTask {
            id: task_id.clone(),
            task_type: TaskType::ApplyLut,
            input_path: input_path.to_path_buf(),
            output_path: output_path.to_path_buf(),
            lut_path: Some(lut_path.to_path_buf()),
            settings: settings.clone(),
            start_time,
            status: TaskStatus::Running,
        };
        
        // 添加到当前任务列表
        {
            let mut tasks = self.current_tasks.lock().await;
            tasks.push(task);
        }
        
        // 发送开始进度
        self.send_progress(ProcessingProgress {
            task_id: task_id.clone(),
            progress: 0.0,
            stage: ProcessingStage::Starting,
            message: "开始应用LUT".to_string(),
            elapsed: Duration::from_secs(0),
        }).await;
        
        // 构建FFmpeg命令
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args([
            "-i", input_path.to_str().unwrap(),
            "-vf", &format!("lut3d={}", lut_path.to_str().unwrap()),
            "-c:v", &settings.video_codec,
            "-preset", &settings.preset,
            "-crf", &settings.crf.to_string(),
            "-c:a", &settings.audio_codec,
            "-progress", "pipe:2", // 输出进度到stderr
            "-y", // 覆盖输出文件
            output_path.to_str().unwrap(),
        ]);
        
        // 添加额外参数
        for (key, value) in &settings.extra_params {
            cmd.args([key, value]);
        }
        
        // 启动进程
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::FFmpeg(format!("Failed to start ffmpeg: {}", e)))?;

        // 监控进度
        let task_id_clone = task_id.clone();
        let progress_sender = self.progress_sender.clone();
        let start_time_clone = start_time;
        
        if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(progress) = Self::parse_ffmpeg_progress(&line) {
                        if let Some(sender) = &progress_sender {
                            let _ = sender.send(ProcessingProgress {
                                task_id: task_id_clone.clone(),
                                progress,
                                stage: ProcessingStage::Processing,
                                message: format!("处理中... {:.1}%", progress * 100.0),
                                elapsed: start_time_clone.elapsed(),
                            });
                        }
                    }
                }
            });
        }
        
        // 等待进程完成
        let status = child.wait().await
            .map_err(|e| AppError::FFmpeg(format!("FFmpeg process failed: {}", e)))?;
        
        let elapsed = start_time.elapsed();
        
        // 更新任务状态
        {
            let mut tasks = self.current_tasks.lock().await;
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                task.status = if status.success() {
                    TaskStatus::Completed
                } else {
                    TaskStatus::Failed
                };
            }
        }
        
        if status.success() {
            // 发送完成进度
            self.send_progress(ProcessingProgress {
                task_id: task_id.clone(),
                progress: 1.0,
                stage: ProcessingStage::Completed,
                message: "LUT应用完成".to_string(),
                elapsed,
            }).await;
            
            Ok(ProcessingResult {
                task_id,
                success: true,
                output_path: Some(output_path.to_path_buf()),
                error: None,
                elapsed,
                file_size: self.get_file_size(output_path).await.unwrap_or(0),
            })
        } else {
            // 发送失败进度
            self.send_progress(ProcessingProgress {
                task_id: task_id.clone(),
                progress: 0.0,
                stage: ProcessingStage::Failed,
                message: "LUT应用失败".to_string(),
                elapsed,
            }).await;
            
            Ok(ProcessingResult {
                task_id,
                success: false,
                output_path: None,
                error: Some("FFmpeg encoding failed".to_string()),
                elapsed,
                file_size: 0,
            })
        }
    }

    /// 应用LUT到视频（使用外部提供的 task_id）
    pub async fn apply_lut_with_task_id(
        &self,
        input_path: &Path,
        output_path: &Path,
        lut_path: &Path,
        settings: &EncodingSettings,
        task_id: String,
    ) -> AppResult<ProcessingResult> {
        let start_time = Instant::now();
        let mut cancelled = false;

        // 创建处理任务
        let task = ProcessingTask {
            id: task_id.clone(),
            task_type: TaskType::ApplyLut,
            input_path: input_path.to_path_buf(),
            output_path: output_path.to_path_buf(),
            lut_path: Some(lut_path.to_path_buf()),
            settings: settings.clone(),
            start_time,
            status: TaskStatus::Running,
        };
        {
            let mut tasks = self.current_tasks.lock().await;
            tasks.push(task);
        }

        // 发送开始进度
        self.send_progress(ProcessingProgress {
            task_id: task_id.clone(),
            progress: 0.0,
            stage: ProcessingStage::Starting,
            message: "开始应用LUT".to_string(),
            elapsed: Duration::from_secs(0),
        }).await;

        // 构建FFmpeg命令
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args([
            "-i", input_path.to_str().unwrap(),
            "-vf", &format!("lut3d={}", lut_path.to_str().unwrap()),
            "-c:v", &settings.video_codec,
            "-preset", &settings.preset,
            "-crf", &settings.crf.to_string(),
            "-c:a", &settings.audio_codec,
            "-progress", "pipe:2",
            "-y",
            output_path.to_str().unwrap(),
        ]);
        for (key, value) in &settings.extra_params {
            cmd.args([key, value]);
        }

        // 启动进程
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::FFmpeg(format!("Failed to start ffmpeg: {}", e)))?;

        // 注册取消通道
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        {
            let mut map = self.cancel_senders.lock().await;
            map.insert(task_id.clone(), cancel_tx);
        }

        // 监控进度
        let task_id_clone = task_id.clone();
        let progress_sender = self.progress_sender.clone();
        let start_time_clone = start_time;
        if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(progress) = Self::parse_ffmpeg_progress(&line) {
                        if let Some(sender) = &progress_sender {
                            let _ = sender.send(ProcessingProgress {
                                task_id: task_id_clone.clone(),
                                progress,
                                stage: ProcessingStage::Processing,
                                message: format!("处理中... {:.1}%", progress * 100.0),
                                elapsed: start_time_clone.elapsed(),
                            });
                        }
                    }
                }
            });
        }

        // 等待进程完成或接收取消
        let status = tokio::select! {
            res = child.wait() => {
                res.map_err(|e| AppError::FFmpeg(format!("FFmpeg process failed: {}", e)))?
            }
            _ = &mut cancel_rx => {
                // 收到取消信号
                cancelled = true;
                let _ = child.kill().await; // 终止子进程
                child.wait().await.map_err(|e| AppError::FFmpeg(format!("FFmpeg process failed after cancel: {}", e)))?
            }
        };

        // 完成后清理取消映射
        {
            let mut map = self.cancel_senders.lock().await;
            map.remove(&task_id);
        }

        let elapsed = start_time.elapsed();

        {
            let mut tasks = self.current_tasks.lock().await;
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                task.status = if cancelled {
                    TaskStatus::Cancelled
                } else if status.success() {
                    TaskStatus::Completed
                } else {
                    TaskStatus::Failed
                };
            }
        }

        if status.success() && !cancelled {
            // 发送完成进度
            self.send_progress(ProcessingProgress {
                task_id: task_id.clone(),
                progress: 1.0,
                stage: ProcessingStage::Completed,
                message: "LUT应用完成".to_string(),
                elapsed,
            }).await;
            
            Ok(ProcessingResult {
                task_id,
                success: true,
                output_path: Some(output_path.to_path_buf()),
                error: None,
                elapsed,
                file_size: self.get_file_size(output_path).await.unwrap_or(0),
            })
        } else if cancelled {
            // 发送取消进度（使用 Failed 阶段但消息为已取消）
            self.send_progress(ProcessingProgress {
                task_id: task_id.clone(),
                progress: 0.0,
                stage: ProcessingStage::Failed,
                message: "LUT处理已取消".to_string(),
                elapsed,
            }).await;
            Ok(ProcessingResult {
                task_id,
                success: false,
                output_path: None,
                error: Some("Cancelled".to_string()),
                elapsed,
                file_size: 0,
            })
        } else {
            // 发送失败进度
            self.send_progress(ProcessingProgress {
                task_id: task_id.clone(),
                progress: 0.0,
                stage: ProcessingStage::Failed,
                message: "LUT应用失败".to_string(),
                elapsed,
            }).await;
            
            Ok(ProcessingResult {
                task_id,
                success: false,
                output_path: None,
                error: Some("FFmpeg encoding failed".to_string()),
                elapsed,
                file_size: 0,
            })
        }
    }

    /// 批量处理视频
    pub async fn batch_process(
        &self,
        tasks: Vec<BatchTask>,
        settings: &EncodingSettings,
        max_concurrent: usize,
    ) -> AppResult<Vec<ProcessingResult>> {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
        let mut handles = Vec::new();
        
        for task in tasks {
            let semaphore = semaphore.clone();
            let settings = settings.clone();
            let processor = self.clone_for_task();
            
            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                
                processor.apply_lut(
                    &task.input_path,
                    &task.output_path,
                    &task.lut_path,
                    &settings,
                ).await
            });
            
            handles.push(handle);
        }
        
        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result?),
                Err(e) => {
                    results.push(ProcessingResult {
                        task_id: uuid::Uuid::new_v4().to_string(),
                        success: false,
                        output_path: None,
                        error: Some(format!("Task failed: {}", e)),
                        elapsed: Duration::from_secs(0),
                        file_size: 0,
                    });
                }
            }
        }
        
        Ok(results)
    }

    /// 转换视频格式
    pub async fn convert_format(
        &self,
        input_path: &Path,
        output_path: &Path,
        settings: &EncodingSettings,
    ) -> AppResult<ProcessingResult> {
        let task_id = uuid::Uuid::new_v4().to_string();
        let start_time = Instant::now();
        
        // 发送开始进度
        self.send_progress(ProcessingProgress {
            task_id: task_id.clone(),
            progress: 0.0,
            stage: ProcessingStage::Starting,
            message: "开始格式转换".to_string(),
            elapsed: Duration::from_secs(0),
        }).await;
        
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args([
            "-i", input_path.to_str().unwrap(),
            "-c:v", &settings.video_codec,
            "-preset", &settings.preset,
            "-crf", &settings.crf.to_string(),
            "-c:a", &settings.audio_codec,
        ]);
        
        // 添加分辨率设置
        if let Some(resolution) = &settings.resolution {
            cmd.args(["-s", &format!("{}x{}", resolution.width, resolution.height)]);
        }
        
        // 添加帧率设置
        if let Some(fps) = settings.fps {
            cmd.args(["-r", &fps.to_string()]);
        }
        
        // 添加额外参数
        for (key, value) in &settings.extra_params {
            cmd.args([key, value]);
        }
        
        cmd.args(["-y", output_path.to_str().unwrap()]);
        
        let status = cmd.status().await
            .map_err(|e| AppError::FFmpeg(format!("Failed to run ffmpeg: {}", e)))?;
        
        let elapsed = start_time.elapsed();
        
        if status.success() {
            self.send_progress(ProcessingProgress {
                task_id: task_id.clone(),
                progress: 1.0,
                stage: ProcessingStage::Completed,
                message: "格式转换完成".to_string(),
                elapsed,
            }).await;
            
            Ok(ProcessingResult {
                task_id,
                success: true,
                output_path: Some(output_path.to_path_buf()),
                error: None,
                elapsed,
                file_size: self.get_file_size(output_path).await.unwrap_or(0),
            })
        } else {
            self.send_progress(ProcessingProgress {
                task_id: task_id.clone(),
                progress: 0.0,
                stage: ProcessingStage::Failed,
                message: "格式转换失败".to_string(),
                elapsed,
            }).await;
            
            Ok(ProcessingResult {
                task_id,
                success: false,
                output_path: None,
                error: Some("Video conversion failed".to_string()),
                elapsed,
                file_size: 0,
            })
        }
    }

    /// 提取视频片段
    pub async fn extract_segment(
        &self,
        input_path: &Path,
        output_path: &Path,
        start_time: f64,
        duration: f64,
        settings: &EncodingSettings,
    ) -> AppResult<ProcessingResult> {
        let task_id = uuid::Uuid::new_v4().to_string();
        let start_instant = Instant::now();
        
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args([
            "-i", input_path.to_str().unwrap(),
            "-ss", &start_time.to_string(),
            "-t", &duration.to_string(),
            "-c:v", &settings.video_codec,
            "-c:a", &settings.audio_codec,
            "-y",
            output_path.to_str().unwrap(),
        ]);
        
        let status = cmd.status().await
            .map_err(|e| AppError::FFmpeg(format!("Failed to extract segment: {}", e)))?;
        
        let elapsed = start_instant.elapsed();
        
        if status.success() {
            Ok(ProcessingResult {
                task_id,
                success: true,
                output_path: Some(output_path.to_path_buf()),
                error: None,
                elapsed,
                file_size: self.get_file_size(output_path).await.unwrap_or(0),
            })
        } else {
            Ok(ProcessingResult {
                task_id,
                success: false,
                output_path: None,
                error: Some("Segment extraction failed".to_string()),
                elapsed,
                file_size: 0,
            })
        }
    }

    /// 合并视频文件
    pub async fn merge_videos(
        &self,
        input_paths: Vec<PathBuf>,
        output_path: &Path,
        settings: &EncodingSettings,
    ) -> AppResult<ProcessingResult> {
        let task_id = uuid::Uuid::new_v4().to_string();
        let start_time = Instant::now();
        
        // 创建临时文件列表
        let temp_dir = tempfile::tempdir()
            .map_err(AppError::from)?;
        let file_list_path = temp_dir.path().join("file_list.txt");
        
        let mut file_list_content = String::new();
        for path in &input_paths {
            file_list_content.push_str(&format!("file '{}'\n", path.to_str().unwrap()));
        }
        
        tokio::fs::write(&file_list_path, file_list_content).await
            .map_err(AppError::from)?;
        
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args([
            "-f", "concat",
            "-safe", "0",
            "-i", file_list_path.to_str().unwrap(),
            "-c:v", &settings.video_codec,
            "-c:a", &settings.audio_codec,
            "-y",
            output_path.to_str().unwrap(),
        ]);
        
        let status = cmd.status().await
            .map_err(|e| AppError::FFmpeg(format!("Failed to merge videos: {}", e)))?;
        
        let elapsed = start_time.elapsed();
        
        if status.success() {
            Ok(ProcessingResult {
                task_id,
                success: true,
                output_path: Some(output_path.to_path_buf()),
                error: None,
                elapsed,
                file_size: self.get_file_size(output_path).await.unwrap_or(0),
            })
        } else {
            Ok(ProcessingResult {
                task_id,
                success: false,
                output_path: None,
                error: Some("Video merge failed".to_string()),
                elapsed,
                file_size: 0,
            })
        }
    }

    /// 获取当前任务列表
    pub async fn get_current_tasks(&self) -> Vec<ProcessingTask> {
        let tasks = self.current_tasks.lock().await;
        tasks.clone()
    }

    /// 取消任务
    pub async fn cancel_task(&self, task_id: &str) -> AppResult<bool> {
        // 向任务发送取消信号
        let sender_opt = {
            let mut map = self.cancel_senders.lock().await;
            map.remove(task_id)
        };

        if let Some(sender) = sender_opt {
            let _ = sender.send(());
            // 同时更新当前任务状态
            let mut tasks = self.current_tasks.lock().await;
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                task.status = TaskStatus::Cancelled;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 清理完成的任务
    pub async fn cleanup_completed_tasks(&self) {
        let mut tasks = self.current_tasks.lock().await;
        tasks.retain(|task| {
            !matches!(task.status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled)
        });
    }

    /// 解析FFmpeg进度输出
    fn parse_ffmpeg_progress(line: &str) -> Option<f64> {
        // FFmpeg进度输出格式：out_time_ms=123456
        if line.starts_with("out_time_ms=") {
            // 这里需要根据总时长计算进度百分比
            // 简化实现，返回固定进度
            Some(0.5)
        } else if line.contains("time=") {
            // 解析时间格式：time=00:00:05.00
            if let Some(time_part) = line.split("time=").nth(1) {
                if let Some(time_str) = time_part.split_whitespace().next() {
                    // 简化的时间解析
                    return Some(0.5);
                }
            }
            None
        } else {
            None
        }
    }

    /// 发送进度更新
    async fn send_progress(&self, progress: ProcessingProgress) {
        if let Some(sender) = &self.progress_sender {
            let _ = sender.send(progress);
        }
    }

    /// 获取文件大小
    async fn get_file_size(&self, path: &Path) -> AppResult<u64> {
        let metadata = tokio::fs::metadata(path).await
            .map_err(AppError::from)?;
        Ok(metadata.len())
    }

    /// 克隆处理器用于任务
    pub fn clone_for_task(&self) -> Self {
        Self {
            ffmpeg_path: self.ffmpeg_path.clone(),
            current_tasks: self.current_tasks.clone(),
            progress_sender: self.progress_sender.clone(),
            cancel_senders: self.cancel_senders.clone(),
        }
    }
}

/// 处理任务
#[derive(Debug, Clone)]
pub struct ProcessingTask {
    pub id: String,
    pub task_type: TaskType,
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub lut_path: Option<PathBuf>,
    pub settings: EncodingSettings,
    pub start_time: Instant,
    pub status: TaskStatus,
}

/// 任务类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    ApplyLut,
    ConvertFormat,
    ExtractSegment,
    MergeVideos,
    ExtractFrames,
}

/// 任务状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// 处理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingResult {
    pub task_id: String,
    pub success: bool,
    pub output_path: Option<PathBuf>,
    pub error: Option<String>,
    pub elapsed: Duration,
    pub file_size: u64,
}

/// 处理进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingProgress {
    pub task_id: String,
    pub progress: f64, // 0.0 - 1.0
    pub stage: ProcessingStage,
    pub message: String,
    pub elapsed: Duration,
}

/// 处理阶段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessingStage {
    Starting,
    Processing,
    Completed,
    Failed,
}

/// 处理统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingStats {
    pub total_tasks: u64,
    pub completed_tasks: u64,
    pub failed_tasks: u64,
    pub cancelled_tasks: u64,
    pub total_processing_time: Duration,
    pub average_processing_time: Duration,
    pub total_input_size: u64,
    pub total_output_size: u64,
}

impl ProcessingStats {
    pub fn new() -> Self {
        Self {
            total_tasks: 0,
            completed_tasks: 0,
            failed_tasks: 0,
            cancelled_tasks: 0,
            total_processing_time: Duration::from_secs(0),
            average_processing_time: Duration::from_secs(0),
            total_input_size: 0,
            total_output_size: 0,
        }
    }

    pub fn add_result(&mut self, result: &ProcessingResult) {
        self.total_tasks += 1;
        
        if result.success {
            self.completed_tasks += 1;
            self.total_output_size += result.file_size;
        } else {
            self.failed_tasks += 1;
        }
        
        self.total_processing_time += result.elapsed;
        self.average_processing_time = self.total_processing_time / self.total_tasks as u32;
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_tasks == 0 {
            0.0
        } else {
            self.completed_tasks as f64 / self.total_tasks as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::collections::HashMap;

    fn create_test_settings() -> EncodingSettings {
        EncodingSettings {
            video_codec: "libx264".to_string(),
            audio_codec: "aac".to_string(),
            preset: "fast".to_string(),
            crf: 23,
            resolution: None,
            fps: None,
            bitrate: None,
            extra_params: HashMap::new(),
        }
    }

    #[test]
    fn test_processing_task_creation() {
        let task = ProcessingTask {
            id: "test_task".to_string(),
            task_type: TaskType::ApplyLut,
            input_path: PathBuf::from("/input/video.mp4"),
            output_path: PathBuf::from("/output/video.mp4"),
            lut_path: Some(PathBuf::from("/luts/test.cube")),
            settings: create_test_settings(),
            start_time: Instant::now(),
            status: TaskStatus::Pending,
        };
        
        assert_eq!(task.id, "test_task");
        assert!(matches!(task.task_type, TaskType::ApplyLut));
        assert!(matches!(task.status, TaskStatus::Pending));
    }

    #[test]
    fn test_processing_result() {
        let result = ProcessingResult {
            task_id: "test_task".to_string(),
            success: true,
            output_path: Some(PathBuf::from("/output/video.mp4")),
            error: None,
            elapsed: Duration::from_secs(10),
            file_size: 1024 * 1024, // 1MB
        };
        
        assert!(result.success);
        assert!(result.error.is_none());
        assert_eq!(result.file_size, 1024 * 1024);
    }

    #[test]
    fn test_processing_progress() {
        let progress = ProcessingProgress {
            task_id: "test_task".to_string(),
            progress: 0.5,
            stage: ProcessingStage::Processing,
            message: "Processing...".to_string(),
            elapsed: Duration::from_secs(5),
        };
        
        assert_eq!(progress.progress, 0.5);
        assert!(matches!(progress.stage, ProcessingStage::Processing));
    }

    #[test]
    fn test_processing_stats() {
        let mut stats = ProcessingStats::new();
        
        let result1 = ProcessingResult {
            task_id: "task1".to_string(),
            success: true,
            output_path: Some(PathBuf::from("/output1.mp4")),
            error: None,
            elapsed: Duration::from_secs(10),
            file_size: 1024,
        };
        
        let result2 = ProcessingResult {
            task_id: "task2".to_string(),
            success: false,
            output_path: None,
            error: Some("Error".to_string()),
            elapsed: Duration::from_secs(5),
            file_size: 0,
        };
        
        stats.add_result(&result1);
        stats.add_result(&result2);
        
        assert_eq!(stats.total_tasks, 2);
        assert_eq!(stats.completed_tasks, 1);
        assert_eq!(stats.failed_tasks, 1);
        assert_eq!(stats.success_rate(), 0.5);
        assert_eq!(stats.total_output_size, 1024);
    }

    #[test]
    fn test_task_type_serialization() {
        let task_type = TaskType::ApplyLut;
        let serialized = serde_json::to_string(&task_type).unwrap();
        let deserialized: TaskType = serde_json::from_str(&serialized).unwrap();
        
        assert!(matches!(deserialized, TaskType::ApplyLut));
    }

    #[test]
    fn test_task_status_serialization() {
        let status = TaskStatus::Running;
        let serialized = serde_json::to_string(&status).unwrap();
        let deserialized: TaskStatus = serde_json::from_str(&serialized).unwrap();
        
        assert!(matches!(deserialized, TaskStatus::Running));
    }

    #[test]
    fn test_processing_stage_serialization() {
        let stage = ProcessingStage::Processing;
        let serialized = serde_json::to_string(&stage).unwrap();
        let deserialized: ProcessingStage = serde_json::from_str(&serialized).unwrap();
        
        assert!(matches!(deserialized, ProcessingStage::Processing));
    }

    #[tokio::test]
    async fn test_video_processor_creation() {
        let processor = VideoProcessor::new(PathBuf::from("/usr/bin/ffmpeg"));
        
        let tasks = processor.get_current_tasks().await;
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_ffmpeg_progress_parsing() {
        let line1 = "out_time_ms=123456";
        let progress1 = VideoProcessor::parse_ffmpeg_progress(line1);
        assert!(progress1.is_some());
        
        let line2 = "frame=  123 fps= 25 q=28.0 size=    1024kB time=00:00:05.00 bitrate=1677.7kbits/s";
        let progress2 = VideoProcessor::parse_ffmpeg_progress(line2);
        assert!(progress2.is_some());
        
        let line3 = "invalid line";
        let progress3 = VideoProcessor::parse_ffmpeg_progress(line3);
        assert!(progress3.is_none());
    }
}