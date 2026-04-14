//! FFmpeg视频处理器
//! 提供视频处理的核心功能

use crate::core::ffmpeg::{BatchResult, BatchTask, EncodingSettings, VideoInfo};
use crate::types::{AppError, AppResult};
use crate::utils::logger;
use crate::utils::path_utils::get_app_data_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as AsyncCommand;
use tokio::sync::{mpsc, Mutex};

const INTERNAL_TWO_PASS_KEY: &str = "__two_pass__";

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
    fn escape_filter_path(path: &Path) -> String {
        path.to_string_lossy().replace('\'', "\\'")
    }

    fn build_lut_filter(lut_paths: &[PathBuf], intensity: f32) -> AppResult<String> {
        if lut_paths.is_empty() {
            return Err(AppError::InvalidInput("No LUT files provided".to_string()));
        }

        let mut lut_chain: Vec<String> = Vec::with_capacity(lut_paths.len());
        for lut_path in lut_paths {
            let escaped = Self::escape_filter_path(lut_path.as_path());
            lut_chain.push(format!("lut3d=file='{}'", escaped));
        }

        let clamped = intensity.clamp(0.0, 1.0);
        if clamped >= 1.0 {
            lut_chain.push("format=yuv422p".to_string());
            Ok(lut_chain.join(","))
        } else {
            // Use split + lut3d + mix to blend original with LUT-applied at the given intensity
            let lut_part = lut_chain.join(",");
            Ok(format!(
                "split[orig][lut];[lut]{},format=yuv422p[lutted];[orig]format=yuv422p[origfmt];[origfmt][lutted]mix=weights={:.4} {:.4}",
                lut_part,
                1.0 - clamped,
                clamped
            ))
        }
    }

    fn add_encoding_args(cmd: &mut AsyncCommand, settings: &EncodingSettings) {
        cmd.args(["-c:v", &settings.video_codec]);
        cmd.args(["-c:a", &settings.audio_codec]);
        cmd.args(["-preset", &settings.preset]);

        if let Some(bitrate) = &settings.bitrate {
            cmd.args(["-b:v", bitrate]);
        } else {
            cmd.args(["-crf", &settings.crf.to_string()]);
        }

        if let Some(resolution) = &settings.resolution {
            cmd.args(["-s", &format!("{}x{}", resolution.width, resolution.height)]);
        }

        if let Some(fps) = settings.fps {
            cmd.args(["-r", &fps.to_string()]);
        }

        for (key, value) in &settings.extra_params {
            if key.starts_with("__") {
                continue;
            }
            cmd.args([key, value]);
        }
    }

    fn is_truthy(value: &str) -> bool {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    }

    async fn probe_duration_seconds(ffmpeg_path: &Path, input_path: &Path) -> Option<f64> {
        let ffprobe_name = if cfg!(target_os = "windows") {
            "ffprobe.exe"
        } else {
            "ffprobe"
        };
        let ffprobe_path = ffmpeg_path
            .parent()
            .map(|p| p.join(ffprobe_name))
            .filter(|p| p.exists())
            .unwrap_or_else(|| PathBuf::from(ffprobe_name));

        let output = AsyncCommand::new(ffprobe_path)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
                input_path.to_str().unwrap_or_default(),
            ])
            .output()
            .await
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8(output.stdout).ok()?;
        let duration = stdout.trim().parse::<f64>().ok()?;
        if duration.is_finite() && duration > 0.0 {
            Some(duration)
        } else {
            None
        }
    }

    /// 创建新的视频处理器
    pub fn new(ffmpeg_path: PathBuf) -> Self {
        Self {
            ffmpeg_path,
            current_tasks: Arc::new(Mutex::new(Vec::new())),
            progress_sender: None,
            cancel_senders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 获取FFmpeg路径
    pub fn ffmpeg_path(&self) -> &Path {
        &self.ffmpeg_path
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
        self.apply_luts_with_task_id(
            input_path,
            output_path,
            &[lut_path.to_path_buf()],
            settings,
            task_id,
            1.0,
        )
        .await
    }

    pub async fn apply_luts_with_task_id(
        &self,
        input_path: &Path,
        output_path: &Path,
        lut_paths: &[PathBuf],
        settings: &EncodingSettings,
        task_id: String,
        intensity: f32,
    ) -> AppResult<ProcessingResult> {
        let start_time = Instant::now();
        let mut cancelled = false;

        let task = ProcessingTask {
            id: task_id.clone(),
            task_type: TaskType::ApplyLut,
            input_path: input_path.to_path_buf(),
            output_path: output_path.to_path_buf(),
            lut_paths: lut_paths.to_vec(),
            settings: settings.clone(),
            start_time,
            status: TaskStatus::Running,
        };
        {
            let mut tasks = self.current_tasks.lock().await;
            tasks.push(task);
        }

        self.send_progress(ProcessingProgress {
            task_id: task_id.clone(),
            progress: 0.0,
            stage: ProcessingStage::Starting,
            message: format!("开始应用 LUT（{} 个）", lut_paths.len()),
            elapsed: Duration::from_secs(0),
        })
        .await;

        let lut_filter = Self::build_lut_filter(lut_paths, intensity)?;
        let total_duration_sec = Self::probe_duration_seconds(&self.ffmpeg_path, input_path).await;
        let two_pass_requested = settings
            .extra_params
            .get(INTERNAL_TWO_PASS_KEY)
            .map(|v| Self::is_truthy(v))
            .unwrap_or(false);
        let use_two_pass =
            two_pass_requested && settings.bitrate.is_some() && settings.video_codec != "copy";

        let mut log_dir = get_app_data_dir()?;
        log_dir.push("logs");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_file_path = log_dir.join(format!("ffmpeg_{}.log", task_id));
        let open_log_file = |append: bool| -> AppResult<std::fs::File> {
            OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(&log_file_path)
                .map_err(|e| AppError::Io(format!("Failed to open FFmpeg log file: {}", e)))
        };

        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        {
            let mut map = self.cancel_senders.lock().await;
            map.insert(task_id.clone(), cancel_tx);
        }

        if two_pass_requested && !use_two_pass {
            logger::log_warn(
                "two_pass_encoding requested but ignored (requires bitrate and non-copy video codec)",
            );
        }

        let status = if use_two_pass {
            self.send_progress(ProcessingProgress {
                task_id: task_id.clone(),
                progress: 0.02,
                stage: ProcessingStage::Processing,
                message: "双通道编码：第一遍分析中...".to_string(),
                elapsed: start_time.elapsed(),
            })
            .await;

            let passlog_file = log_dir.join(format!("ffmpeg_passlog_{}", task_id));
            let passlog_str = passlog_file.to_string_lossy().to_string();
            let null_sink = if cfg!(target_os = "windows") {
                "NUL"
            } else {
                "/dev/null"
            };

            let mut pass1_cmd = AsyncCommand::new(&self.ffmpeg_path);
            pass1_cmd.args(["-i", input_path.to_str().unwrap(), "-vf", &lut_filter]);
            Self::add_encoding_args(&mut pass1_cmd, settings);
            pass1_cmd.args([
                "-pass",
                "1",
                "-passlogfile",
                &passlog_str,
                "-an",
                "-f",
                "null",
                "-y",
                null_sink,
            ]);
            let pass1_log = open_log_file(false)?;
            let mut pass1_child = pass1_cmd
                .stdout(Stdio::null())
                .stderr(Stdio::from(pass1_log))
                .spawn()
                .map_err(|e| AppError::FFmpeg(format!("Failed to start ffmpeg pass 1: {}", e)))?;

            let pass1_status = tokio::select! {
                res = pass1_child.wait() => {
                    res.map_err(|e| AppError::FFmpeg(format!("FFmpeg pass 1 failed: {}", e)))?
                }
                _ = &mut cancel_rx => {
                    cancelled = true;
                    let _ = pass1_child.kill().await;
                    pass1_child.wait().await.map_err(|e| AppError::FFmpeg(format!("FFmpeg process failed after cancel: {}", e)))?
                }
            };

            let status = if cancelled || !pass1_status.success() {
                pass1_status
            } else if cancel_rx.try_recv().is_ok() {
                // Cancel arrived between pass 1 completing and pass 2 starting
                cancelled = true;
                pass1_status
            } else {
                self.send_progress(ProcessingProgress {
                    task_id: task_id.clone(),
                    progress: 0.1,
                    stage: ProcessingStage::Processing,
                    message: "双通道编码：第二遍处理中...".to_string(),
                    elapsed: start_time.elapsed(),
                })
                .await;

                let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
                cmd.args(["-i", input_path.to_str().unwrap(), "-vf", &lut_filter]);
                Self::add_encoding_args(&mut cmd, settings);
                cmd.args([
                    "-pass",
                    "2",
                    "-passlogfile",
                    &passlog_str,
                    "-loglevel",
                    "debug",
                    "-progress",
                    "pipe:1",
                    "-y",
                    output_path.to_str().unwrap(),
                ]);

                let log_file = open_log_file(true)?;
                let mut child = cmd
                    .stdout(Stdio::piped())
                    .stderr(Stdio::from(log_file))
                    .spawn()
                    .map_err(|e| {
                        AppError::FFmpeg(format!("Failed to start ffmpeg pass 2: {}", e))
                    })?;

                let task_id_clone = task_id.clone();
                let progress_sender = self.progress_sender.clone();
                let start_time_clone = start_time;
                let duration_for_progress = total_duration_sec;
                let mut progress_handle: Option<tokio::task::JoinHandle<()>> = None;
                if let Some(stdout) = child.stdout.take() {
                    let reader = BufReader::new(stdout);
                    let mut lines = reader.lines();
                    let handle = tokio::spawn(async move {
                        while let Ok(Some(line)) = lines.next_line().await {
                            if let Some(progress) =
                                Self::parse_ffmpeg_progress(&line, duration_for_progress)
                            {
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
                    progress_handle = Some(handle);
                }

                let status = tokio::select! {
                    res = child.wait() => {
                        res.map_err(|e| AppError::FFmpeg(format!("FFmpeg process failed: {}", e)))?
                    }
                    _ = &mut cancel_rx => {
                        cancelled = true;
                        let _ = child.kill().await;
                        child.wait().await.map_err(|e| AppError::FFmpeg(format!("FFmpeg process failed after cancel: {}", e)))?
                    }
                };

                if let Some(handle) = progress_handle.take() {
                    handle.abort();
                }
                status
            };

            let _ = std::fs::remove_file(passlog_file.clone());
            let _ = std::fs::remove_file(passlog_file.with_extension("log"));
            let _ = std::fs::remove_file(passlog_file.with_extension("mbtree"));
            status
        } else {
            let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
            cmd.args(["-i", input_path.to_str().unwrap(), "-vf", &lut_filter]);
            Self::add_encoding_args(&mut cmd, settings);
            cmd.args([
                "-loglevel",
                "debug",
                "-progress",
                "pipe:1",
                "-y",
                output_path.to_str().unwrap(),
            ]);

            let log_file = open_log_file(false)?;
            let mut child = cmd
                .stdout(Stdio::piped())
                .stderr(Stdio::from(log_file))
                .spawn()
                .map_err(|e| AppError::FFmpeg(format!("Failed to start ffmpeg: {}", e)))?;

            let task_id_clone = task_id.clone();
            let progress_sender = self.progress_sender.clone();
            let start_time_clone = start_time;
            let duration_for_progress = total_duration_sec;
            let mut progress_handle: Option<tokio::task::JoinHandle<()>> = None;
            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                let handle = tokio::spawn(async move {
                    while let Ok(Some(line)) = lines.next_line().await {
                        if let Some(progress) =
                            Self::parse_ffmpeg_progress(&line, duration_for_progress)
                        {
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
                progress_handle = Some(handle);
            }

            let status = tokio::select! {
                res = child.wait() => {
                    res.map_err(|e| AppError::FFmpeg(format!("FFmpeg process failed: {}", e)))?
                }
                _ = &mut cancel_rx => {
                    cancelled = true;
                    let _ = child.kill().await;
                    child.wait().await.map_err(|e| AppError::FFmpeg(format!("FFmpeg process failed after cancel: {}", e)))?
                }
            };

            if let Some(handle) = progress_handle.take() {
                handle.abort();
            }
            status
        };

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
            self.send_progress(ProcessingProgress {
                task_id: task_id.clone(),
                progress: 1.0,
                stage: ProcessingStage::Completed,
                message: "LUT应用完成".to_string(),
                elapsed,
            })
            .await;

            Ok(ProcessingResult {
                task_id,
                success: true,
                output_path: Some(output_path.to_path_buf()),
                error: None,
                elapsed,
                file_size: self.get_file_size(output_path).await.unwrap_or(0),
            })
        } else if cancelled {
            self.send_progress(ProcessingProgress {
                task_id: task_id.clone(),
                progress: 0.0,
                stage: ProcessingStage::Failed,
                message: "LUT处理已取消".to_string(),
                elapsed,
            })
            .await;
            Ok(ProcessingResult {
                task_id,
                success: false,
                output_path: None,
                error: Some("Cancelled".to_string()),
                elapsed,
                file_size: 0,
            })
        } else {
            let mut err_summary = String::new();
            let exit_code = status.code().unwrap_or(-1);
            err_summary.push_str(&format!("FFmpeg 失败，退出码: {}", exit_code));
            let log_snippet = std::fs::read_to_string(&log_file_path)
                .map(|content| {
                    let max_chars = 2000usize;
                    if content.len() > max_chars {
                        content[content.len() - max_chars..].to_string()
                    } else {
                        content
                    }
                })
                .unwrap_or_else(|_| "(无法读取FFmpeg日志)".to_string());
            err_summary.push_str("\n日志片段：\n");
            err_summary.push_str(&log_snippet);

            self.send_progress(ProcessingProgress {
                task_id: task_id.clone(),
                progress: 0.0,
                stage: ProcessingStage::Failed,
                message: format!(
                    "LUT应用失败：{}",
                    err_summary.lines().next().unwrap_or("未知错误")
                ),
                elapsed,
            })
            .await;

            Ok(ProcessingResult {
                task_id,
                success: false,
                output_path: None,
                error: Some(err_summary),
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
        self.apply_luts_with_task_id(
            input_path,
            output_path,
            &[lut_path.to_path_buf()],
            settings,
            task_id,
            1.0,
        )
        .await
    }

    /// 生成 LUT 预览图。
    pub async fn generate_lut_preview_image(
        &self,
        lut_paths: &[PathBuf],
        output_path: &Path,
        video_path: Option<&Path>,
        intensity: f32,
    ) -> AppResult<()> {
        let lut_filter = Self::build_lut_filter(lut_paths, intensity)?;

        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(AppError::from)?;
        }

        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args(["-hide_banner", "-loglevel", "error"]);

        if let Some(input_path) = video_path.filter(|path| path.exists()) {
            cmd.args(["-ss", "1.0", "-i", input_path.to_str().unwrap()]);
        } else {
            cmd.args(["-f", "lavfi", "-i", "testsrc2=size=960x540:rate=1"]);
        }

        cmd.args([
            "-vf",
            &lut_filter,
            "-frames:v",
            "1",
            "-y",
            output_path.to_str().unwrap(),
        ]);

        let output = cmd
            .output()
            .await
            .map_err(|e| AppError::FFmpeg(format!("Failed to generate LUT preview: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::FFmpeg(format!(
                "Failed to generate LUT preview: {}",
                stderr.trim()
            )));
        }

        Ok(())
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

                processor
                    .apply_lut(
                        &task.input_path,
                        &task.output_path,
                        &task.lut_path,
                        &settings,
                    )
                    .await
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
        })
        .await;

        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args([
            "-i",
            input_path.to_str().unwrap(),
            "-c:v",
            &settings.video_codec,
            "-preset",
            &settings.preset,
            "-crf",
            &settings.crf.to_string(),
            "-c:a",
            &settings.audio_codec,
        ]);

        // 添加分辨率设置
        if let Some(resolution) = &settings.resolution {
            cmd.args(["-s", &format!("{}x{}", resolution.width, resolution.height)]);
        }

        // 添加帧率设置
        if let Some(fps) = settings.fps {
            cmd.args(["-r", &fps.to_string()]);
        }

        // 添加额外参数（跳过内部标记键）
        for (key, value) in &settings.extra_params {
            if !key.starts_with("__") {
                cmd.args([key, value]);
            }
        }

        cmd.args(["-y", output_path.to_str().unwrap()]);

        let status = cmd
            .status()
            .await
            .map_err(|e| AppError::FFmpeg(format!("Failed to run ffmpeg: {}", e)))?;

        let elapsed = start_time.elapsed();

        if status.success() {
            self.send_progress(ProcessingProgress {
                task_id: task_id.clone(),
                progress: 1.0,
                stage: ProcessingStage::Completed,
                message: "格式转换完成".to_string(),
                elapsed,
            })
            .await;

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
            })
            .await;

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
            "-i",
            input_path.to_str().unwrap(),
            "-ss",
            &start_time.to_string(),
            "-t",
            &duration.to_string(),
            "-c:v",
            &settings.video_codec,
            "-c:a",
            &settings.audio_codec,
            "-y",
            output_path.to_str().unwrap(),
        ]);

        let status = cmd
            .status()
            .await
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
        let temp_dir = tempfile::tempdir().map_err(AppError::from)?;
        let file_list_path = temp_dir.path().join("file_list.txt");

        let mut file_list_content = String::new();
        for path in &input_paths {
            file_list_content.push_str(&format!("file '{}'\n", path.to_str().unwrap()));
        }

        tokio::fs::write(&file_list_path, file_list_content)
            .await
            .map_err(AppError::from)?;

        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args([
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            file_list_path.to_str().unwrap(),
            "-c:v",
            &settings.video_codec,
            "-c:a",
            &settings.audio_codec,
            "-y",
            output_path.to_str().unwrap(),
        ]);

        let status = cmd
            .status()
            .await
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
            !matches!(
                task.status,
                TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
            )
        });
    }

    /// 解析FFmpeg进度输出
    fn parse_ffmpeg_progress(line: &str, total_duration_sec: Option<f64>) -> Option<f64> {
        let total = total_duration_sec.filter(|v| *v > 0.0)?;

        // Prefer out_time= (HH:MM:SS.ms) as the authoritative source
        if let Some(time_str) = line.strip_prefix("out_time=") {
            let seconds = Self::parse_ffmpeg_time_to_seconds(time_str.trim())?;
            if seconds >= 0.0 {
                return Some((seconds / total).clamp(0.0, 0.99));
            }
        }

        // Fallback: out_time_ms (note: FFmpeg labels this "ms" but older builds
        // output microseconds; newer builds output milliseconds. We try both
        // heuristics: if the value yields > 10x total duration when treated as
        // milliseconds, assume microseconds instead.)
        if let Some(ms_str) = line.strip_prefix("out_time_ms=") {
            let raw = ms_str.trim().parse::<f64>().ok()?;
            let seconds_as_ms = raw / 1_000.0;
            let seconds_as_us = raw / 1_000_000.0;
            let seconds = if seconds_as_ms > total * 10.0 {
                seconds_as_us
            } else {
                seconds_as_ms
            };
            if seconds >= 0.0 {
                return Some((seconds / total).clamp(0.0, 0.99));
            }
        }

        // Fallback: inline "time=HH:MM:SS.ms" in verbose FFmpeg output
        if let Some(time_part) = line.split("time=").nth(1) {
            if let Some(time_str) = time_part.split_whitespace().next() {
                let seconds = Self::parse_ffmpeg_time_to_seconds(time_str.trim())?;
                return Some((seconds / total).clamp(0.0, 0.99));
            }
        }

        None
    }

    fn parse_ffmpeg_time_to_seconds(time_str: &str) -> Option<f64> {
        let mut parts = time_str.trim().split(':');
        let h = parts.next()?.parse::<f64>().ok()?;
        let m = parts.next()?.parse::<f64>().ok()?;
        let s = parts.next()?.parse::<f64>().ok()?;
        Some(h * 3600.0 + m * 60.0 + s)
    }

    /// 发送进度更新
    async fn send_progress(&self, progress: ProcessingProgress) {
        if let Some(sender) = &self.progress_sender {
            let _ = sender.send(progress);
        }
    }

    /// 获取文件大小
    async fn get_file_size(&self, path: &Path) -> AppResult<u64> {
        let metadata = tokio::fs::metadata(path).await.map_err(AppError::from)?;
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
    pub lut_paths: Vec<PathBuf>,
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
    use std::collections::HashMap;
    use tempfile::TempDir;

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
            lut_paths: vec![PathBuf::from("/luts/test.cube")],
            settings: create_test_settings(),
            start_time: Instant::now(),
            status: TaskStatus::Pending,
        };

        assert_eq!(task.id, "test_task");
        assert!(matches!(task.task_type, TaskType::ApplyLut));
        assert!(matches!(task.status, TaskStatus::Pending));
    }

    #[test]
    fn test_build_lut_filter_preserves_order() {
        let filter = VideoProcessor::build_lut_filter(
            &[PathBuf::from("/luts/a.cube"), PathBuf::from("/luts/b.cube")],
            1.0,
        )
        .unwrap();
        assert_eq!(
            filter,
            "lut3d=file='/luts/a.cube',lut3d=file='/luts/b.cube',format=yuv422p"
        );

        // Test partial intensity produces a mix filter
        let filter_half =
            VideoProcessor::build_lut_filter(&[PathBuf::from("/luts/a.cube")], 0.5).unwrap();
        assert!(filter_half.contains("mix="));
        assert!(filter_half.contains("split"));
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
        // out_time= is the preferred source (HH:MM:SS.ms)
        let line_time = "out_time=00:00:05.000000";
        let progress_time = VideoProcessor::parse_ffmpeg_progress(line_time, Some(20.0));
        assert!(progress_time.is_some());
        assert!((progress_time.unwrap() - 0.25).abs() < 0.001);

        // out_time_ms with large value (microseconds heuristic: 5000000 as ms = 5000s >> 20*10)
        let line1 = "out_time_ms=5000000";
        let progress1 = VideoProcessor::parse_ffmpeg_progress(line1, Some(20.0));
        assert!(progress1.is_some());
        assert!((progress1.unwrap() - 0.25).abs() < 0.001);

        // out_time_ms with true millisecond value (5000 ms = 5s, 5s/20s = 0.25)
        let line_ms = "out_time_ms=5000";
        let progress_ms = VideoProcessor::parse_ffmpeg_progress(line_ms, Some(20.0));
        assert!(progress_ms.is_some());
        assert!((progress_ms.unwrap() - 0.25).abs() < 0.001);

        let line2 =
            "frame=  123 fps= 25 q=28.0 size=    1024kB time=00:00:10.00 bitrate=1677.7kbits/s";
        let progress2 = VideoProcessor::parse_ffmpeg_progress(line2, Some(20.0));
        assert!(progress2.is_some());
        assert!((progress2.unwrap() - 0.5).abs() < 0.001);

        let line3 = "invalid line";
        let progress3 = VideoProcessor::parse_ffmpeg_progress(line3, Some(20.0));
        assert!(progress3.is_none());
    }
}
