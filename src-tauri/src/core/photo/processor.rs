use super::PhotoStage;
use super::{check_cancel, codec, color, metadata, PhotoOutput, PhotoSpace, SourceInterpretation};
use crate::core::ffmpeg::{lut::prepare_luts, photo_bridge::grade_rgb16};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Default)]
pub struct PhotoControl {
    pub token: CancellationToken,
    committed: Arc<Mutex<bool>>,
}
impl PhotoControl {
    pub fn cancel(&self) {
        if let Ok(committed) = self.committed.lock() {
            if !*committed {
                self.token.cancel();
            }
        }
    }
    pub fn publish(
        &self,
        mut temporary: tempfile::NamedTempFile,
        destination: &Path,
    ) -> Result<PathBuf, String> {
        let mut target = destination.to_path_buf();
        let mut reserved = std::collections::HashSet::new();
        for _ in 0..32 {
            let mut committed = self.committed.lock().map_err(|e| e.to_string())?;
            check_cancel(&self.token)?;
            match temporary.persist_noclobber(&target) {
                Ok(_) => {
                    *committed = true;
                    return Ok(target);
                }
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    temporary = error.file;
                    target = crate::core::output::available_path(destination, &mut reserved)?;
                }
                Err(error) if crate::core::output::publication_unsupported(&error.error) => {
                    temporary = error.file;
                    // Allow cancel() to set the token while a network copy runs.
                    drop(committed);
                    let pending =
                        crate::core::output::publish_without_rename(&temporary, &target, || {
                            check_cancel(&self.token).map_err(|message| {
                                std::io::Error::new(std::io::ErrorKind::Interrupted, message)
                            })
                        });
                    match pending {
                        Ok(pending) => {
                            let mut committed = self.committed.lock().map_err(|e| e.to_string())?;
                            check_cancel(&self.token)?;
                            pending.commit();
                            *committed = true;
                            return Ok(target);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                            target =
                                crate::core::output::available_path(destination, &mut reserved)?;
                        }
                        Err(error) => {
                            check_cancel(&self.token)?;
                            return Err(format!("照片发布失败，现有文件保持完整：{error}"));
                        }
                    }
                }
                Err(error) => {
                    return Err(format!("照片发布失败，现有文件保持完整：{}", error.error))
                }
            }
        }
        Err("输出名称被持续占用，请重新导出".into())
    }
}

pub struct PhotoJob {
    pub input: PathBuf,
    pub output: PathBuf,
    pub luts: Vec<PathBuf>,
    pub intensity: f32,
    pub lut_space: Option<PhotoSpace>,
    pub source: SourceInterpretation,
    pub format: PhotoOutput,
    pub preserve_metadata: bool,
    pub preserve_gps: bool,
    pub expected_version: Option<String>,
}

#[derive(Clone)]
pub struct PhotoProcessor {
    permits: Arc<Semaphore>,
    budget_mib: u32,
}
impl Default for PhotoProcessor {
    fn default() -> Self {
        let memory = sysinfo::System::new_all().total_memory();
        Self::with_budget((memory / 3).clamp(256 * 1024 * 1024, 4 * 1024 * 1024 * 1024))
    }
}
impl PhotoProcessor {
    pub fn with_budget(bytes: u64) -> Self {
        let budget_mib = (bytes / (1024 * 1024)).clamp(1, 4096) as u32;
        Self {
            permits: Arc::new(Semaphore::new(budget_mib as usize)),
            budget_mib,
        }
    }
    pub async fn reserve(
        &self,
        bytes: u64,
        token: &CancellationToken,
    ) -> Result<Arc<tokio::sync::OwnedSemaphorePermit>, String> {
        let required = bytes.div_ceil(1024 * 1024);
        if required > u64::from(self.budget_mib) {
            return Err(
                "照片处理超过内存预算，请使用更小的源照片或内存更多的设备；应用不会缩小导出尺寸"
                    .into(),
            );
        }
        tokio::select! {
            biased;
            _ = token.cancelled() => Err("照片处理已取消".into()),
            permit = self.permits.clone().acquire_many_owned(required as u32) => permit.map(Arc::new).map_err(|e| e.to_string()),
        }
    }
    pub async fn inspect(
        &self,
        path: &Path,
        token: &CancellationToken,
    ) -> Result<metadata::PhotoHeader, String> {
        let bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        let lease = self.reserve(128 * 1024 * 1024 + bytes, token).await?;
        let path = path.to_path_buf();
        let token = token.clone();
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            check_cancel(&token)?;
            let result = metadata::header(&path)?;
            check_cancel(&token)?;
            Ok(result)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub async fn process(
        &self,
        engine: &Path,
        job: PhotoJob,
        control: PhotoControl,
        progress: Arc<dyn Fn(&str) + Send + Sync>,
    ) -> Result<PathBuf, String> {
        check_cancel(&control.token)?;
        if !job.intensity.is_finite() || !(0.0..=1.0).contains(&job.intensity) {
            return Err("照片 LUT 强度必须在 0–1 之间".into());
        }
        if !job.luts.is_empty() && job.intensity > 0.0 && job.lut_space != Some(PhotoSpace::Srgb) {
            return Err("请明确按 sRGB 输入和输出使用此照片 LUT".into());
        }
        if job.output.exists() {
            return Err("照片输出已存在，禁止覆盖".into());
        }
        progress(PhotoStage::Read.as_str());
        let header = self.inspect(&job.input, &control.token).await?;
        check_cancel(&control.token)?;
        if job
            .expected_version
            .as_ref()
            .is_some_and(|v| *v != header.info.source_version)
        {
            return Err("源照片已变化，请重新读取后导出".into());
        }
        job.format.validate(header.info.has_alpha)?;
        let required = (128 * 1024 * 1024
            + u64::from(header.info.width) * u64::from(header.info.height) * 64
            + header.info.size)
            .div_ceil(1024 * 1024);
        if required > u64::from(self.budget_mib) {
            return Err(
                "照片处理超过内存预算，请使用更小的源照片或内存更多的设备；应用不会缩小导出尺寸"
                    .into(),
            );
        }
        let permit = self.reserve(required * 1024 * 1024, &control.token).await?;
        progress(PhotoStage::Normalize.as_str());
        let path = job.input.clone();
        let source = job.source.clone();
        let token = control.token.clone();
        let lease = permit.clone();
        let (mut frame, metadata) = tokio::task::spawn_blocking(move || {
            let _lease = lease;
            let frame = codec::decode(&path, &source, &token)?;
            check_cancel(&token)?;
            let metadata = codec::capture_metadata(&path, job.preserve_metadata, job.preserve_gps)?;
            Ok::<_, String>((frame, metadata))
        })
        .await
        .map_err(|e| e.to_string())??;
        check_cancel(&control.token)?;
        let luts_directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let luts = if job.intensity > 0.0 {
            prepare_luts(&job.luts, luts_directory.path())
                .await
                .map_err(|e| e.to_string())?
        } else {
            Vec::new()
        };
        check_cancel(&control.token)?;
        progress(PhotoStage::Lut.as_str());
        frame.rgb = grade_rgb16(
            engine,
            frame.width,
            frame.height,
            frame.rgb,
            &luts,
            job.intensity,
            &control.token,
        )
        .await?;
        check_cancel(&control.token)?;
        progress(PhotoStage::Write.as_str());
        let path = job.input.clone();
        let destination = job.output.clone();
        let format = job.format;
        let lease = permit.clone();
        let version = header.info.source_version;
        let token = control.token.clone();
        let temporary = tokio::task::spawn_blocking(move || {
            let _lease = lease;
            color::apply_alpha(&mut frame, format.alpha_policy(), &token)?;
            let parent = destination.parent().ok_or("无效照片输出目录")?;
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            let mut temporary = tempfile::Builder::new()
                .prefix(".lutlab-photo-")
                .suffix(&format!(".{}", format.extension()))
                .tempfile_in(parent)
                .map_err(|e| e.to_string())?;
            codec::encode(&frame, &format, temporary.as_file_mut(), &token)?;
            check_cancel(&token)?;
            codec::write_metadata(temporary.path(), metadata, frame.width, frame.height)?;
            check_cancel(&token)?;
            let actual = metadata::header(temporary.path())?;
            if actual.info.width != frame.width
                || actual.info.height != frame.height
                || actual.info.bit_depth != format.bit_depth()
                || actual.info.color_status != "embedded"
                || actual.info.has_alpha != frame.alpha.is_some()
                || actual.info.orientation != 1
            {
                return Err(format!(
                    "照片输出验证失败：{} {}×{}，{} 位，ICC {}，透明 {}，方向 {}",
                    format.extension(),
                    actual.info.width,
                    actual.info.height,
                    actual.info.bit_depth,
                    actual.info.color_status,
                    actual.info.has_alpha,
                    actual.info.orientation
                ));
            }
            let _ = metadata::reader(temporary.path())?
                .decode()
                .map_err(|e| format!("照片输出不可读取：{e}"))?;
            if metadata::version(&path)? != version {
                return Err("源照片在处理期间变化，未发布输出".into());
            }
            temporary.as_file().sync_all().map_err(|e| e.to_string())?;
            check_cancel(&token)?;
            Ok::<_, String>(temporary)
        })
        .await
        .map_err(|e| e.to_string())??;
        tokio::task::spawn_blocking(move || control.publish(temporary, &job.output))
            .await
            .map_err(|e| e.to_string())?
    }
}
