//! FFmpeg视频处理器
//! 提供视频处理的核心功能

use crate::core::ffmpeg::{BatchTask, EncodingSettings};
use crate::types::{AppError, AppResult};
use crate::utils::logger;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as AsyncCommand;
use tokio::sync::{mpsc, Mutex};

const INTERNAL_TWO_PASS_KEY: &str = "__two_pass__";
const INTERNAL_HARDWARE_KEY: &str = "__hardware__";

enum CancellationSlot {
    Requested,
    Running(tokio::sync::oneshot::Sender<()>),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct EncoderAvailabilityKey {
    executable: PathBuf,
    bytes: Option<u64>,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    identity: Option<(u64, u64)>,
    codec: String,
}

fn encoder_availability_key(executable: &Path, codec: &str) -> EncoderAvailabilityKey {
    // Explicit configuration may use a command name resolved through PATH.
    let path = crate::core::ffmpeg::resolve_executable_path(executable);
    let executable = path.canonicalize().unwrap_or(path);
    let metadata = std::fs::metadata(&executable).ok();
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    EncoderAvailabilityKey {
        executable,
        bytes: metadata.as_ref().map(std::fs::Metadata::len),
        modified: metadata
            .as_ref()
            .and_then(|metadata| metadata.modified().ok()),
        #[cfg(unix)]
        identity: metadata
            .as_ref()
            .map(|metadata| (metadata.dev(), metadata.ino())),
        codec: codec.to_owned(),
    }
}

/// 视频处理器
pub struct VideoProcessor {
    /// FFmpeg可执行文件路径
    ffmpeg_path: PathBuf,
    /// 当前处理任务
    current_tasks: Arc<Mutex<Vec<ProcessingTask>>>,
    /// 进度发送器
    progress_sender: Option<mpsc::UnboundedSender<ProcessingProgress>>,
    /// 取消信号发送器映射（task_id -> oneshot sender）
    cancel_senders: Arc<Mutex<HashMap<String, CancellationSlot>>>,
    unavailable_encoders: Arc<Mutex<HashSet<EncoderAvailabilityKey>>>,
}

impl VideoProcessor {
    pub(crate) fn build_lut_filter(lut_paths: &[PathBuf], intensity: f32) -> AppResult<String> {
        crate::core::ffmpeg::lut::build_lut_filter(lut_paths, intensity)
    }

    fn add_encoding_args(cmd: &mut AsyncCommand, settings: &EncodingSettings) {
        let codec = settings.video_codec.as_str();
        cmd.args(["-c:v", codec, "-c:a", &settings.audio_codec]);
        if matches!(codec, "libx264" | "libx265") {
            cmd.args(["-preset", &settings.preset]);
        } else if codec.ends_with("_nvenc") {
            cmd.args(["-preset", "p4"]);
        } else if codec == "libvpx-vp9" {
            cmd.args(["-deadline", "good", "-cpu-used", "3", "-row-mt", "1"]);
        } else if codec == "prores_ks" {
            cmd.args(["-profile:v", "3"]);
        }

        if let Some(bitrate) = &settings.bitrate {
            cmd.args(["-b:v", bitrate]);
        } else if codec.ends_with("_videotoolbox") {
            // Apple quality mode is 1–100, unlike x264 CRF (lower is better).
            let quality = (100 - settings.crf * 2).clamp(1, 100).to_string();
            cmd.args(["-q:v", &quality]);
        } else if codec.ends_with("_nvenc") {
            cmd.args(["-rc", "vbr", "-cq", &settings.crf.to_string(), "-b:v", "0"]);
        } else if codec.ends_with("_qsv") {
            cmd.args(["-global_quality", &settings.crf.to_string()]);
        } else if codec != "prores_ks" {
            cmd.args(["-crf", &settings.crf.to_string()]);
            if codec == "libvpx-vp9" {
                cmd.args(["-b:v", "0"]);
            }
        }
        cmd.args([
            "-pix_fmt",
            if codec == "prores_ks" {
                "yuv422p10le"
            } else if settings
                .extra_params
                .get("__bit_depth__")
                .is_some_and(|v| v == "10")
            {
                if codec.ends_with("_videotoolbox")
                    || codec.ends_with("_qsv")
                    || codec.ends_with("_nvenc")
                {
                    "p010le"
                } else {
                    "yuv420p10le"
                }
            } else {
                "yuv420p"
            },
        ]);
        if codec.starts_with("hevc") || codec == "libx265" {
            cmd.args(["-tag:v", "hvc1"]);
        }
        if let Some(fps) = settings.fps {
            cmd.args(["-r", &fps.to_string()]);
        }
        for (key, value) in &settings.extra_params {
            if !key.starts_with("__") {
                cmd.args([key, value]);
            }
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

        let mut command = AsyncCommand::new(ffprobe_path);
        command
            .kill_on_drop(true)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(input_path);
        let output = tokio::time::timeout(Duration::from_secs(10), command.output())
            .await
            .ok()?
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
            unavailable_encoders: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// 获取FFmpeg路径
    pub fn ffmpeg_path(&self) -> &Path {
        &self.ffmpeg_path
    }

    pub fn set_ffmpeg_path(&mut self, path: PathBuf) {
        self.ffmpeg_path = path;
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
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        {
            // Register before any I/O. A cancel request arriving before launch is
            // retained, so the queued task cannot escape cancellation.
            let mut map = self.cancel_senders.lock().await;
            match map.remove(&task_id) {
                Some(CancellationSlot::Requested) => {
                    let _ = cancel_tx.send(());
                }
                _ => {
                    map.insert(task_id.clone(), CancellationSlot::Running(cancel_tx));
                }
            }
        }
        self.current_tasks.lock().await.push(ProcessingTask {
            id: task_id.clone(),
            task_type: TaskType::ApplyLut,
            input_path: input_path.to_path_buf(),
            output_path: output_path.to_path_buf(),
            lut_paths: lut_paths.to_vec(),
            settings: settings.clone(),
            start_time,
            status: TaskStatus::Running,
        });
        self.send_progress(ProcessingProgress {
            task_id: task_id.clone(),
            progress: 0.0,
            stage: ProcessingStage::Starting,
            message: "正在准备导出".to_string(),
            elapsed: start_time.elapsed(),
            encoder: None,
            speed: None,
            eta_seconds: None,
        })
        .await;

        let result = self
            .run_lut_export(
                input_path,
                output_path,
                lut_paths,
                settings,
                &task_id,
                intensity,
                start_time,
                &mut cancel_rx,
            )
            .await;
        self.cancel_senders.lock().await.remove(&task_id);
        let cancelled = matches!(&result, Err(AppError::FFmpeg(message)) if message == "Cancelled");
        let success = result.is_ok();
        if let Some(task) = self
            .current_tasks
            .lock()
            .await
            .iter_mut()
            .find(|task| task.id == task_id)
        {
            task.status = if cancelled {
                TaskStatus::Cancelled
            } else if success {
                TaskStatus::Completed
            } else {
                TaskStatus::Failed
            };
        }
        let error = result.err().map(|e| {
            if cancelled {
                "Cancelled".to_string()
            } else {
                e.to_string()
            }
        });
        self.send_progress(ProcessingProgress {
            task_id: task_id.clone(),
            progress: if success { 1.0 } else { 0.0 },
            stage: if success {
                ProcessingStage::Completed
            } else {
                ProcessingStage::Failed
            },
            message: if success {
                "导出完成".to_string()
            } else if cancelled {
                "导出已取消".to_string()
            } else {
                error.clone().unwrap_or_default()
            },
            elapsed: start_time.elapsed(),
            encoder: None,
            speed: None,
            eta_seconds: None,
        })
        .await;
        Ok(ProcessingResult {
            task_id,
            success,
            output_path: success.then(|| output_path.to_path_buf()),
            error,
            elapsed: start_time.elapsed(),
            file_size: if success {
                self.get_file_size(output_path).await.unwrap_or(0)
            } else {
                0
            },
        })
    }

    async fn run_lut_export(
        &self,
        input_path: &Path,
        output_path: &Path,
        lut_paths: &[PathBuf],
        settings: &EncodingSettings,
        task_id: &str,
        intensity: f32,
        start_time: Instant,
        cancel_rx: &mut tokio::sync::oneshot::Receiver<()>,
    ) -> AppResult<()> {
        Self::check_cancel(cancel_rx)?;
        if !input_path.is_file() {
            return Err(AppError::InvalidInput(
                "Input video does not exist".to_string(),
            ));
        }
        if output_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "Output already exists: {}",
                output_path.display()
            )));
        }
        let parent = output_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(AppError::from)?;
        // Temp files live on the destination volume. Dropping them on errors or
        // cancellation removes incomplete output. Prefer persist_noclobber;
        // network volumes without exclusive rename use an exclusive-copy fallback.
        let extension = output_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp4");
        let temporary_output = tempfile::Builder::new()
            .prefix(".lut-export-")
            .suffix(&format!(".{}", extension))
            .tempfile_in(parent)
            .map_err(AppError::from)?;
        let workspace = tempfile::Builder::new()
            .prefix("lut-work-")
            .tempdir()
            .map_err(AppError::from)?;
        let prepared_luts =
            crate::core::ffmpeg::lut::prepare_luts(lut_paths, workspace.path()).await?;
        let input_space = settings
            .extra_params
            .get("__input_color_space__")
            .map(String::as_str)
            .unwrap_or("auto");
        let color = crate::core::ffmpeg::color::input_filter(input_space)?;
        let mut filter = format!(
            "{},{}",
            color,
            Self::build_lut_filter(&prepared_luts, intensity)?
        );
        if let Some(resolution) = &settings.resolution {
            // Fit the display dimensions, not stored pixels: anamorphic footage
            // must become square pixels before padding, without an intermediate
            // full-resolution upscale (same geometry as the preview pipeline).
            let scale = format!(
                "min({}/(iw*sar),{}/ih)",
                resolution.width, resolution.height
            );
            filter.push_str(&format!(
                ",scale=w='max(2,trunc(iw*sar*({scale})/2)*2)':h='max(2,trunc(ih*({scale})/2)*2)',setsar=1,pad={}:{}:(ow-iw)/2:(oh-ih)/2",
                resolution.width, resolution.height
            ));
        } else {
            // 4:2:0 encoders need even dimensions, including portrait/odd source frames.
            filter.push_str(",pad=ceil(iw/2)*2:ceil(ih/2)*2");
        }
        let duration = tokio::select! {
            biased;
            _ = &mut *cancel_rx => return Err(AppError::FFmpeg("Cancelled".to_string())),
            duration = Self::probe_duration_seconds(&self.ffmpeg_path, input_path) => duration,
        };
        Self::check_cancel(cancel_rx)?;
        let two_pass = settings
            .extra_params
            .get(INTERNAL_TWO_PASS_KEY)
            .is_some_and(|v| Self::is_truthy(v));
        let accelerated = settings
            .extra_params
            .get(INTERNAL_HARDWARE_KEY)
            .is_some_and(|v| Self::is_truthy(v))
            && !two_pass;
        let mut codecs: Vec<String> = Vec::new();
        if accelerated && matches!(settings.video_codec.as_str(), "libx264" | "libx265") {
            let family = if settings.video_codec == "libx265" {
                "hevc"
            } else {
                "h264"
            };
            if cfg!(target_os = "macos") {
                codecs.push(format!("{}_videotoolbox", family));
            } else {
                codecs.push(format!("{}_nvenc", family));
                codecs.push(format!("{}_qsv", family));
            }
        }
        codecs.push(settings.video_codec.clone());
        let mut last_error = None;
        for (attempt, codec) in codecs.iter().enumerate() {
            let availability_key = encoder_availability_key(&self.ffmpeg_path, codec);
            if codec != &settings.video_codec
                && self
                    .unavailable_encoders
                    .lock()
                    .await
                    .contains(&availability_key)
            {
                continue;
            }
            Self::check_cancel(cancel_rx)?;
            let mut active_settings = settings.clone();
            active_settings.video_codec = codec.clone();
            self.send_progress(ProcessingProgress {
                task_id: task_id.to_string(),
                progress: 0.0,
                stage: ProcessingStage::Processing,
                message: if attempt == 0 {
                    format!("正在导出 · {}", codec)
                } else {
                    format!("硬件编码回退，使用 {} 重试", codec)
                },
                elapsed: start_time.elapsed(),
                encoder: Some(codec.clone()),
                speed: None,
                eta_seconds: None,
            })
            .await;
            let passlog = workspace.path().join("pass");
            let mut result = Ok(());
            for pass in 0..if two_pass { 2 } else { 1 } {
                result = self
                    .encode_pass(
                        input_path,
                        temporary_output.path(),
                        &filter,
                        &active_settings,
                        task_id,
                        start_time,
                        duration,
                        cancel_rx,
                        if two_pass {
                            Some((pass + 1, &passlog))
                        } else {
                            None
                        },
                        workspace.path(),
                    )
                    .await;
                if result.is_err() {
                    break;
                }
            }
            match result {
                Ok(()) => {
                    Self::check_cancel(cancel_rx)?;
                    let destination = output_path.to_path_buf();
                    let (_, replacement) = tokio::sync::oneshot::channel();
                    let mut publication_cancel = std::mem::replace(cancel_rx, replacement);
                    // A NAS copy can take time. Keep cancellation registered and
                    // await its cleanup without blocking the async scheduler.
                    tokio::task::spawn_blocking(move || {
                        let mut cancelled = false;
                        let result = (|| {
                            let mut check = || {
                                if publication_cancel.try_recv().is_ok() {
                                    cancelled = true;
                                    return Err(std::io::Error::new(
                                        std::io::ErrorKind::Interrupted,
                                        "Cancelled",
                                    ));
                                }
                                Ok(())
                            };
                            check()?;
                            match temporary_output.persist_noclobber(&destination) {
                                Ok(_) => Ok(()),
                                Err(error)
                                    if crate::core::output::publication_unsupported(
                                        &error.error,
                                    ) =>
                                {
                                    let pending = crate::core::output::publish_without_rename(
                                        &error.file,
                                        &destination,
                                        &mut check,
                                    )?;
                                    check()?;
                                    pending.commit();
                                    Ok(())
                                }
                                Err(error) => Err(error.error),
                            }
                        })();
                        result.map_err(|error| {
                            if cancelled {
                                AppError::FFmpeg("Cancelled".into())
                            } else {
                                AppError::Io(format!(
                                    "Cannot publish output without overwriting: {error}"
                                ))
                            }
                        })
                    })
                    .await
                    .map_err(|error| AppError::Internal(error.to_string()))??;
                    return Ok(());
                }
                Err(AppError::FFmpeg(message)) if message == "Cancelled" => {
                    return Err(AppError::FFmpeg(message))
                }
                Err(error) => {
                    logger::log_warn(&format!("Encoder {} failed: {}", codec, error));
                    let message = error.to_string();
                    let hardware = codec != &settings.video_codec;
                    if hardware && permanently_unavailable_encoder(&message) {
                        let mut unavailable = self.unavailable_encoders.lock().await;
                        if unavailable.len() >= 64 {
                            unavailable.clear();
                        }
                        unavailable.insert(availability_key);
                    } else if !hardware || !retryable_hardware_error(&message) {
                        return Err(error);
                    }
                    last_error = Some(error);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| AppError::FFmpeg("No encoder available".to_string())))
    }

    fn check_cancel(cancel_rx: &mut tokio::sync::oneshot::Receiver<()>) -> AppResult<()> {
        if cancel_rx.try_recv().is_ok() {
            Err(AppError::FFmpeg("Cancelled".to_string()))
        } else {
            Ok(())
        }
    }

    async fn encode_pass(
        &self,
        input_path: &Path,
        output_path: &Path,
        filter: &str,
        settings: &EncodingSettings,
        task_id: &str,
        start_time: Instant,
        duration: Option<f64>,
        cancel_rx: &mut tokio::sync::oneshot::Receiver<()>,
        pass: Option<(usize, &Path)>,
        workspace: &Path,
    ) -> AppResult<()> {
        Self::check_cancel(cancel_rx)?;
        let log_path = workspace.join("ffmpeg.log");
        let log_file = std::fs::File::create(&log_path).map_err(AppError::from)?;
        let mut command = AsyncCommand::new(&self.ffmpeg_path);
        command.kill_on_drop(true).args([
            "-hide_banner",
            "-nostdin",
            "-loglevel",
            "error",
            "-nostats",
            "-stats_period",
            "0.25",
        ]);
        let threads = settings
            .extra_params
            .get("__threads__")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(4)
            .clamp(1, 16)
            .to_string();
        command.args(["-threads", &threads, "-filter_threads", &threads]);
        command
            .arg("-i")
            .arg(input_path)
            .args(["-map", "0:v:0", "-map", "0:a?", "-vf", filter]);
        Self::add_encoding_args(&mut command, settings);
        command.args(["-threads", &threads]);
        if let Some((number, passlog)) = pass {
            command
                .args(["-pass", &number.to_string(), "-passlogfile"])
                .arg(passlog);
            if number == 1 {
                command.args(["-an", "-f", "null"]);
            }
        }
        command.args(["-progress", "pipe:1", "-y"]).arg(output_path);
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log_file))
            .spawn()
            .map_err(|e| AppError::FFmpeg(format!("Cannot start FFmpeg: {}", e)))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::FFmpeg("FFmpeg progress pipe unavailable".to_string()))?;
        let mut lines = BufReader::new(stdout).lines();
        let mut lines_open = true;
        let mut progress_value = 0.0;
        let mut speed: Option<f64> = None;
        let status = loop {
            tokio::select! {
                biased;
                _ = &mut *cancel_rx => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    return Err(AppError::FFmpeg("Cancelled".to_string()));
                }
                status = child.wait() => { break status.map_err(|e| AppError::FFmpeg(format!("FFmpeg wait failed: {}", e)))?; }
                line = lines.next_line(), if lines_open => {
                    match line {
                        Ok(Some(line)) => {
                            if let Some(value) = Self::parse_ffmpeg_progress(&line, duration) {
                                progress_value = if let Some((number, _)) = pass { ((number - 1) as f64 + value) / 2.0 } else { value };
                            }
                            if let Some(sample) = line.strip_prefix("speed=").and_then(|v| v.trim().trim_end_matches('x').parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.0) {
                                speed = Some(speed.map_or(sample, |previous| previous * 0.7 + sample * 0.3));
                            }
                            if line.starts_with("progress=") {
                                let eta = duration.zip(speed).map(|(seconds, rate)| seconds * (if pass.is_some() {2.0} else {1.0}) * (1.0-progress_value) / rate).filter(|v| v.is_finite());
                                self.send_progress(ProcessingProgress {
                                    task_id: task_id.to_string(), progress: progress_value,
                                    stage: ProcessingStage::Processing,
                                    message: format!("正在导出 · {}", settings.video_codec),
                                    elapsed: start_time.elapsed(), encoder: Some(settings.video_codec.clone()), speed, eta_seconds: eta,
                                }).await;
                            }
                        },
                        _ => lines_open = false,
                    }
                }
            }
        };
        if !status.success() {
            let log = tokio::fs::read(&log_path).await.unwrap_or_default();
            let tail = String::from_utf8_lossy(&log[log.len().saturating_sub(3000)..]);
            return Err(AppError::FFmpeg(format!(
                "{} 编码失败（{}）：{}",
                settings.video_codec,
                status,
                tail.trim()
            )));
        }
        Ok(())
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
        let workspace = tempfile::tempdir().map_err(AppError::from)?;
        let prepared = crate::core::ffmpeg::lut::prepare_luts(lut_paths, workspace.path()).await?;
        let lut_filter = Self::build_lut_filter(&prepared, intensity)?;

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
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent.max(1)));
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
            encoder: None,
            speed: None,
            eta_seconds: None,
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
                encoder: None,
                speed: None,
                eta_seconds: None,
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
                encoder: None,
                speed: None,
                eta_seconds: None,
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
        let mut map = self.cancel_senders.lock().await;
        match map.remove(task_id) {
            Some(CancellationSlot::Running(sender)) => {
                let _ = sender.send(());
            }
            _ => {
                map.insert(task_id.to_string(), CancellationSlot::Requested);
            }
        }
        if let Some(task) = self
            .current_tasks
            .lock()
            .await
            .iter_mut()
            .find(|t| t.id == task_id)
        {
            if matches!(task.status, TaskStatus::Running | TaskStatus::Pending) {
                task.status = TaskStatus::Cancelled;
            }
        }
        Ok(true)
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

        // FFmpeg's historical out_time_ms key is microseconds (same as out_time_us).
        // Treating early microsecond samples as milliseconds made long exports jump to 99%.
        if let Some(raw) = line
            .strip_prefix("out_time_us=")
            .or_else(|| line.strip_prefix("out_time_ms="))
        {
            let seconds = raw.trim().parse::<f64>().ok()? / 1_000_000.0;
            if seconds.is_finite() && seconds >= 0.0 {
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
            unavailable_encoders: self.unavailable_encoders.clone(),
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
    pub encoder: Option<String>,
    pub speed: Option<f64>,
    pub eta_seconds: Option<f64>,
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
    use std::collections::{HashMap, HashSet};

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
        assert!(filter.find("a.cube").unwrap() < filter.find("b.cube").unwrap());
        assert!(filter.contains("tetrahedral"));

        // Test partial intensity produces a mix filter
        let filter_half =
            VideoProcessor::build_lut_filter(&[PathBuf::from("/luts/a.cube")], 0.5).unwrap();
        assert!(filter_half.contains("blend="));
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
            encoder: None,
            speed: None,
            eta_seconds: None,
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
        assert!((progress_ms.unwrap() - 0.00025).abs() < 0.00001);

        let line2 =
            "frame=  123 fps= 25 q=28.0 size=    1024kB time=00:00:10.00 bitrate=1677.7kbits/s";
        let progress2 = VideoProcessor::parse_ffmpeg_progress(line2, Some(20.0));
        assert!(progress2.is_some());
        assert!((progress2.unwrap() - 0.5).abs() < 0.001);

        let line3 = "invalid line";
        let progress3 = VideoProcessor::parse_ffmpeg_progress(line3, Some(20.0));
        assert!(progress3.is_none());
    }
    fn integration_ffmpeg() -> Option<PathBuf> {
        crate::core::ffmpeg::discover_ffmpeg_path().ok()
    }

    async fn fixture(ffmpeg: &Path, path: &Path) {
        let result = AsyncCommand::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=64x48:rate=12:duration=0.5",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.5",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=880:duration=0.5",
                "-map",
                "0:v",
                "-map",
                "1:a",
                "-map",
                "2:a",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-y",
            ])
            .arg(path)
            .output()
            .await
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[tokio::test]
    async fn export_blends_lut_preserves_audio_and_never_overwrites() {
        let Some(ffmpeg) = integration_ffmpeg() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("source.mp4");
        fixture(&ffmpeg, &input).await;
        let lut = temp.path().join("cinema ' tone,[1].cube");
        // CUBE order: red changes fastest. Inversion makes the halfway blend neutral.
        std::fs::write(
            &lut,
            "LUT_3D_SIZE 2\n1 1 1\n0 1 1\n1 0 1\n0 0 1\n1 1 0\n0 1 0\n1 0 0\n0 0 0\n",
        )
        .unwrap();
        let output_directory = std::env::var_os("LUTLAB_TEST_OUTPUT_DIR").map(|parent| {
            tempfile::Builder::new()
                .prefix(".lutlab-video-test-")
                .tempdir_in(parent)
                .unwrap()
        });
        let output_parent = output_directory
            .as_ref()
            .map_or(temp.path(), |dir| dir.path());
        let output = output_parent.join("finished.mp4");
        let processor = VideoProcessor::new(ffmpeg.clone());
        let result = processor
            .apply_luts_with_task_id(
                &input,
                &output,
                &[lut],
                &create_test_settings(),
                "blend-test".into(),
                0.5,
            )
            .await
            .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert!(result.file_size > 0);
        let decoded = AsyncCommand::new(&ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args([
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "pipe:1",
            ])
            .output()
            .await
            .unwrap();
        assert!(decoded.status.success());
        assert_eq!(decoded.stdout.len(), 64 * 48 * 3);
        let mean_error: f64 = decoded
            .stdout
            .iter()
            .map(|sample| (*sample as f64 - 128.0).abs())
            .sum::<f64>()
            / decoded.stdout.len() as f64;
        assert!(
            mean_error < 5.0,
            "half-strength inversion must be neutral: mean error {mean_error}"
        );
        let ffprobe = crate::core::ffmpeg::discover_ffprobe_path().unwrap();
        let audio = AsyncCommand::new(ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "a",
                "-show_entries",
                "stream=index",
                "-of",
                "csv=p=0",
            ])
            .arg(&output)
            .output()
            .await
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&audio.stdout).lines().count(),
            2,
            "all audio tracks must survive"
        );
        let original = std::fs::read(&output).unwrap();
        let rejected = processor
            .apply_luts_with_task_id(
                &input,
                &output,
                &[],
                &create_test_settings(),
                "collision-test".into(),
                1.0,
            )
            .await
            .unwrap();
        assert!(!rejected.success);
        assert_eq!(std::fs::read(&output).unwrap(), original);
        assert!(std::fs::read_dir(temp.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".lut-export-")));
        assert!(std::fs::read_dir(output_parent).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".lut-export-")));
    }

    #[tokio::test]
    async fn cancellation_before_registration_is_not_lost() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("cancelled.mp4");
        let processor = VideoProcessor::new(PathBuf::from("missing-ffmpeg"));
        processor.cancel_task("cancel-first").await.unwrap();
        let result = processor
            .apply_luts_with_task_id(
                Path::new("missing.mp4"),
                &output,
                &[],
                &create_test_settings(),
                "cancel-first".into(),
                1.0,
            )
            .await
            .unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Cancelled"));
        assert!(!output.exists());
        assert!(matches!(
            processor.get_current_tasks().await[0].status,
            TaskStatus::Cancelled
        ));
        assert!(processor.cancel_senders.lock().await.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn hardware_failure_retries_software_and_running_cancel_stops_process() {
        use std::os::unix::fs::PermissionsExt;
        let Some(ffmpeg) = integration_ffmpeg() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("source.mp4");
        fixture(&ffmpeg, &input).await;
        let wrapper = temp.path().join("ffmpeg-wrapper");
        let script = format!("#!/bin/sh\ncase \"$*\" in *videotoolbox*|*nvenc*|*qsv*) echo 'Cannot create compression session' >&2; exit 1;; esac\nexec '{}' \"$@\"\n", ffmpeg.to_string_lossy().replace('\'', "'\\''"));
        std::fs::write(&wrapper, script).unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let processor = VideoProcessor::new(wrapper.clone());
        let mut settings = create_test_settings();
        settings
            .extra_params
            .insert(INTERNAL_HARDWARE_KEY.into(), "1".into());
        let output = temp.path().join("fallback.mp4");
        let result = processor
            .apply_luts_with_task_id(&input, &output, &[], &settings, "fallback-test".into(), 1.0)
            .await
            .unwrap();
        assert!(result.success, "{:?}", result.error);
        // A stand-in encoder blocks until killed. Its parent process gets an exec,
        // so termination cannot leave a shell child working in the background.
        std::fs::write(&wrapper, "#!/bin/sh\necho 'out_time_us=1'\nexec sleep 30\n").unwrap();
        let processor = Arc::new(VideoProcessor::new(wrapper));
        let worker = processor.clone();
        let cancelled_output = temp.path().join("cancelled.mp4");
        let worker_output = cancelled_output.clone();
        let handle = tokio::spawn(async move {
            worker
                .apply_luts_with_task_id(
                    &input,
                    &worker_output,
                    &[],
                    &create_test_settings(),
                    "running-cancel".into(),
                    1.0,
                )
                .await
        });
        tokio::time::sleep(Duration::from_millis(150)).await;
        processor.cancel_task("running-cancel").await.unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("cancel should terminate encoder promptly")
            .unwrap()
            .unwrap();
        assert_eq!(result.error.as_deref(), Some("Cancelled"));
        assert!(!cancelled_output.exists());
        assert!(std::fs::read_dir(temp.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".lut-export-")));
    }
    #[tokio::test]
    async fn parallel_video_exports_and_two_pass_encoding_complete() {
        let Some(ffmpeg) = integration_ffmpeg() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input.mp4");
        fixture(&ffmpeg, &input).await;
        let output_directory = std::env::var_os("LUTLAB_TEST_OUTPUT_DIR").map(|parent| {
            tempfile::Builder::new()
                .prefix(".lutlab-parallel-test-")
                .tempdir_in(parent)
                .unwrap()
        });
        let output_parent = output_directory
            .as_ref()
            .map_or(temp.path(), |dir| dir.path());
        let processor = Arc::new(VideoProcessor::new(ffmpeg));
        let mut settings = create_test_settings();
        settings.bitrate = Some("500k".into());
        settings
            .extra_params
            .insert(INTERNAL_TWO_PASS_KEY.into(), "1".into());
        let mut handles = Vec::new();
        for index in 0..2 {
            let processor = processor.clone();
            let input = input.clone();
            let settings = settings.clone();
            let output = output_parent.join(format!("output-{index}.mp4"));
            handles.push(tokio::spawn(async move {
                processor
                    .apply_luts_with_task_id(
                        &input,
                        &output,
                        &[],
                        &settings,
                        format!("parallel-{index}"),
                        1.0,
                    )
                    .await
                    .unwrap()
            }));
        }
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.success, "{:?}", result.error);
            assert!(result.output_path.unwrap().exists());
        }
    }
    #[tokio::test]
    async fn anamorphic_export_uses_display_aspect_ratio_without_pillarboxing() {
        let Some(ffmpeg) = integration_ffmpeg() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("anamorphic.mp4");
        let fixture = AsyncCommand::new(&ffmpeg)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=red:size=1440x1080:rate=10:duration=0.1",
                "-vf",
                "setsar=4/3",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&input)
            .output()
            .await
            .unwrap();
        assert!(
            fixture.status.success(),
            "{}",
            String::from_utf8_lossy(&fixture.stderr)
        );
        let mut settings = create_test_settings();
        settings.resolution = Some(crate::core::ffmpeg::Resolution {
            width: 1920,
            height: 1080,
        });
        let output = temp.path().join("square-pixels.mp4");
        let processor = VideoProcessor::new(ffmpeg.clone());
        let result = processor
            .apply_luts_with_task_id(
                &input,
                &output,
                &[],
                &settings,
                "anamorphic-test".into(),
                1.0,
            )
            .await
            .unwrap();
        assert!(result.success, "{:?}", result.error);
        let decoded = AsyncCommand::new(&ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args([
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "pipe:1",
            ])
            .output()
            .await
            .unwrap();
        assert!(decoded.status.success());
        assert_eq!(decoded.stdout.len(), 1920 * 1080 * 3);
        // The old 1440/1080 calculation introduced 240px black bars on each side.
        // Sample all four near-corners and the center to verify full-frame content.
        for (x, y) in [(2, 2), (1917, 2), (2, 1077), (1917, 1077), (960, 540)] {
            let offset = (y * 1920 + x) * 3;
            assert!(
                decoded.stdout[offset] > 200
                    && decoded.stdout[offset + 1] < 20
                    && decoded.stdout[offset + 2] < 20,
                "red source must fill 16:9 output at ({x}, {y}), got {:?}",
                &decoded.stdout[offset..offset + 3]
            );
        }
        let ffprobe = crate::core::ffmpeg::discover_ffprobe_path().unwrap();
        let probe = AsyncCommand::new(ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=sample_aspect_ratio,display_aspect_ratio",
                "-of",
                "json",
            ])
            .arg(&output)
            .output()
            .await
            .unwrap();
        let metadata: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        assert_eq!(metadata["streams"][0]["sample_aspect_ratio"], "1:1");
        assert_eq!(metadata["streams"][0]["display_aspect_ratio"], "16:9");
    }
}

/// Retry only hardware initialization/capability failures; corrupt input, bad
/// filters, incompatible audio and full disks must not restart the entire job.
fn non_encoder_failure(text: &str) -> bool {
    [
        "no space left",
        "permission denied",
        "read-only file system",
        "disk quota exceeded",
        "input/output error",
        "invalid data found",
        "moov atom not found",
        "error opening input",
        "error reinitializing filters",
        "error while filtering",
        "no such filter",
        "error initializing filter",
        "error applying option",
        "error parsing a filter",
        "error initializing complex filters",
        "could not find tag for codec",
        "codec not currently supported in container",
    ]
    .iter()
    .any(|failure| text.contains(failure))
}

fn permanently_unavailable_encoder(message: &str) -> bool {
    let text = message.to_ascii_lowercase();
    if non_encoder_failure(&text) {
        return false;
    }
    [
        "unknown encoder",
        "no nvenc capable devices",
        "cannot load libcuda",
        "cannot load nvcuda",
        "no device available",
        "no capable devices",
        "failed to initialise vaapi connection",
    ]
    .iter()
    .any(|value| text.contains(value))
}
fn retryable_hardware_error(message: &str) -> bool {
    let text = message.to_ascii_lowercase();
    if non_encoder_failure(&text) {
        return false;
    }
    permanently_unavailable_encoder(message)
        || [
            "error while opening encoder",
            "error initializing output stream",
            "cannot create compression session",
            "unsupported device",
            "unsupported pixel format",
            "mfx session",
            "failed to open nvenc",
        ]
        .iter()
        .any(|value| text.contains(value))
}

#[cfg(test)]
mod color_depth_integration_tests {
    use super::*;
    use crate::commands::encoding_options::{build_encoding_settings, ProcessingOptions};

    #[tokio::test]
    async fn hevc_ten_bit_hdr_transform_preserves_audio_and_encodes_sdr_pixels() {
        let Ok((ffmpeg, ffprobe)) = crate::core::ffmpeg::discover_ffmpeg_pair() else {
            eprintln!("FFmpeg unavailable; skipping HEVC 10-bit integration test");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        for (input_space, transfer) in
            [("rec2020-pq", "smpte2084"), ("rec2020-hlg", "arib-std-b67")]
        {
            let source = directory.path().join(format!("{input_space}.mkv"));
            let generated = AsyncCommand::new(&ffmpeg)
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=0x804020:s=64x48:r=10:d=0.5",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=0.5",
                    "-map",
                    "0:v",
                    "-map",
                    "1:a",
                    "-pix_fmt",
                    "yuv420p10le",
                    "-color_primaries",
                    "bt2020",
                    "-color_trc",
                    transfer,
                    "-colorspace",
                    "bt2020nc",
                    "-c:v",
                    "ffv1",
                    "-threads",
                    "1",
                    "-c:a",
                    "aac",
                ])
                .arg(&source)
                .output()
                .await
                .unwrap();
            assert!(
                generated.status.success(),
                "{}",
                String::from_utf8_lossy(&generated.stderr)
            );
            let options: ProcessingOptions = serde_json::from_value(serde_json::json!({
                "video_codec": "libx265", "output_format": "mp4", "output_bit_depth": "10",
                "input_color_space": input_space, "hardware_acceleration": false,
                "audio_codec": "aac", "quality_preset": "high"
            }))
            .unwrap();
            let mut settings = build_encoding_settings(&options).unwrap();
            settings.preset = "ultrafast".into();
            settings.extra_params.insert(
                "-x265-params".into(),
                "lossless=1:pools=1:frame-threads=1".into(),
            );
            let output = directory.path().join(format!("{input_space}-sdr.mp4"));
            let mut processor = VideoProcessor::new(ffmpeg.clone());
            let (sender, mut progress) = mpsc::unbounded_channel();
            processor.set_progress_sender(sender);
            let result = processor
                .apply_luts_with_task_id(&source, &output, &[], &settings, input_space.into(), 1.0)
                .await
                .unwrap();
            assert!(result.success, "{:?}", result.error);
            let mut encoder_seen = false;
            while let Ok(update) = progress.try_recv() {
                encoder_seen |= update.encoder.as_deref() == Some("libx265");
            }
            assert!(
                encoder_seen,
                "progress must identify the encoder actually used"
            );
            let probed = AsyncCommand::new(&ffprobe)
                .args(["-v", "error", "-show_streams", "-of", "json"])
                .arg(&output)
                .output()
                .await
                .unwrap();
            assert!(probed.status.success());
            let probe: serde_json::Value = serde_json::from_slice(&probed.stdout).unwrap();
            let streams = probe["streams"].as_array().unwrap();
            let video = streams
                .iter()
                .find(|stream| stream["codec_type"] == "video")
                .unwrap();
            assert_eq!(video["codec_name"], "hevc");
            assert_eq!(video["pix_fmt"], "yuv420p10le");
            assert_eq!(video["profile"], "Main 10");
            assert_eq!(video["color_primaries"], "bt709");
            assert_eq!(video["color_transfer"], "bt709");
            assert_eq!(video["color_space"], "bt709");
            assert!(streams.iter().any(|stream| stream["codec_type"] == "audio"));
            let encoded = AsyncCommand::new(&ffmpeg)
                .args(["-v", "error", "-i"])
                .arg(&output)
                .args([
                    "-frames:v",
                    "1",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgb24",
                    "pipe:1",
                ])
                .output()
                .await
                .unwrap();
            let reference = AsyncCommand::new(&ffmpeg)
                .args(["-v", "error", "-i"])
                .arg(&source)
                .arg("-vf")
                .arg(crate::core::ffmpeg::color::input_filter(input_space).unwrap())
                .args([
                    "-frames:v",
                    "1",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgb24",
                    "pipe:1",
                ])
                .output()
                .await
                .unwrap();
            assert!(encoded.status.success() && reference.status.success());
            assert_eq!(encoded.stdout.len(), reference.stdout.len());
            let error = encoded
                .stdout
                .iter()
                .zip(&reference.stdout)
                .map(|(a, b)| (*a as f64 - *b as f64).abs())
                .sum::<f64>()
                / encoded.stdout.len() as f64;
            assert!(error < 4.0, "{input_space}: SDR tags must describe the actual RGB pixels; mean roundtrip error={error}");
        }
    }

    #[test]
    fn hardware_failures_are_classified_without_restarting_bad_input_or_full_disks() {
        for message in [
            "Unknown encoder 'h264_nvenc'",
            "Cannot load libcuda.1",
            "No NVENC capable devices found",
        ] {
            assert!(permanently_unavailable_encoder(message));
            assert!(retryable_hardware_error(message));
        }
        for message in [
            "Cannot create compression session",
            "Error while opening encoder: MFX session failed",
        ] {
            assert!(retryable_hardware_error(message));
            assert!(!permanently_unavailable_encoder(message));
        }
        for failure in [
            "No space left on device", "Permission denied", "Invalid data found when processing input",
            "Error reinitializing filters", "No such filter: missing", "Error applying option to filter lut3d",
            "Could not find tag for codec pcm_s16le in stream #1, codec not currently supported in container",
        ] {
            let message = format!("{failure}\nError while opening encoder: unknown encoder");
            assert!(!retryable_hardware_error(&message), "must fail immediately: {failure}");
            assert!(!permanently_unavailable_encoder(&message), "must not poison engine cache: {failure}");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unavailable_encoder_cache_tracks_executable_and_retries_transient_sessions() {
        use std::os::unix::fs::PermissionsExt;
        let Ok(ffmpeg) = crate::core::ffmpeg::discover_ffmpeg_path() else {
            eprintln!("FFmpeg unavailable; skipping hardware cache integration test");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.mp4");
        let generated = AsyncCommand::new(&ffmpeg)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=red:size=64x48:rate=10:duration=0.2",
                "-c:v",
                "libx264",
                "-threads",
                "1",
            ])
            .arg(&source)
            .output()
            .await
            .unwrap();
        assert!(generated.status.success());
        let log = directory.path().join("attempts.txt");
        let first = directory.path().join("ffmpeg-first");
        let second = directory.path().join("ffmpeg-second");
        let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"));
        let wrapper = |path: &Path, error: &str| {
            std::fs::write(path, format!("#!/bin/sh\ncase \"$*\" in *videotoolbox*|*nvenc*|*qsv*) echo attempted >> {}; echo '{}' >&2; exit 1;; esac\nexec {} \"$@\"\n", quote(&log), error, quote(&ffmpeg))).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        };
        wrapper(&first, "Unknown encoder");
        wrapper(&second, "Cannot create compression session");
        let mut settings = EncodingSettings::default();
        settings.video_codec = "libx264".into();
        settings.preset = "ultrafast".into();
        settings
            .extra_params
            .insert(INTERNAL_HARDWARE_KEY.into(), "1".into());
        let mut processor = VideoProcessor::new(first.clone());
        let hardware_count = if cfg!(target_os = "macos") { 1 } else { 2 };
        for (index, expected) in [
            hardware_count,
            hardware_count,
            hardware_count * 2,
            hardware_count * 3,
            hardware_count * 4,
        ]
        .into_iter()
        .enumerate()
        {
            if index == 2 {
                processor.set_ffmpeg_path(second.clone());
            }
            if index == 4 {
                // Replacing an executable at the same path must invalidate its failures.
                wrapper(&first, "Unknown encoder after engine update");
                processor.set_ffmpeg_path(first.clone());
            }
            let output = directory.path().join(format!("output-{index}.mp4"));
            let result = processor
                .apply_luts_with_task_id(
                    &source,
                    &output,
                    &[],
                    &settings,
                    format!("cache-{index}"),
                    1.0,
                )
                .await
                .unwrap();
            assert!(result.success, "{:?}", result.error);
            assert_eq!(
                std::fs::read_to_string(&log).unwrap().lines().count(),
                expected,
                "run {index}: cache only permanent errors for the same executable version"
            );
        }
    }
}
