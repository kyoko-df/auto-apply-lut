//! Single-flight successful engine discovery. No background processes or timers.

use crate::types::{AppError, AppResult};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

type Pair = (PathBuf, PathBuf);

#[derive(Debug, PartialEq, Eq)]
struct FileVersion {
    target: PathBuf,
    bytes: u64,
    modified: SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, u32),
}

impl FileVersion {
    fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        if !metadata.is_file() {
            return None;
        }
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Some(Self {
            target: path.canonicalize().ok()?,
            bytes: metadata.len(),
            modified: metadata.modified().ok()?,
            #[cfg(unix)]
            identity: (metadata.dev(), metadata.ino(), metadata.mode()),
        })
    }
}

struct CachedPair {
    context: Vec<Option<OsString>>,
    paths: Pair,
    versions: (FileVersion, FileVersion),
}

#[derive(Default)]
struct DiscoveryCache {
    pair: Option<CachedPair>,
}

impl DiscoveryCache {
    fn resolve(
        &mut self,
        context: Vec<Option<OsString>>,
        discover: impl FnOnce() -> AppResult<Pair>,
    ) -> AppResult<Pair> {
        if let Some(cached) = &self.pair {
            if cached.context == context
                && FileVersion::read(&cached.paths.0).as_ref() == Some(&cached.versions.0)
                && FileVersion::read(&cached.paths.1).as_ref() == Some(&cached.versions.1)
            {
                return Ok(cached.paths.clone());
            }
        }
        self.pair = None;
        let paths = discover()?;
        if let (Some(ffmpeg), Some(ffprobe)) =
            (FileVersion::read(&paths.0), FileVersion::read(&paths.1))
        {
            self.pair = Some(CachedPair {
                context,
                paths: paths.clone(),
                versions: (ffmpeg, ffprobe),
            });
        }
        Ok(paths)
    }
}

static CACHE: OnceLock<Mutex<DiscoveryCache>> = OnceLock::new();
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub(super) fn discover_cached(discover: impl FnOnce() -> AppResult<Pair>) -> AppResult<Pair> {
    let mut context = vec![
        std::env::var_os("PATH"),
        std::env::var_os("FFMPEG_PATH"),
        std::env::var_os("FFPROBE_PATH"),
        std::env::current_exe().ok().map(PathBuf::into_os_string),
    ];
    // Discovery callers already use spawn_blocking. Serializing the initial
    // probe avoids a burst of duplicate -version subprocesses on app startup.
    let mut cache = CACHE
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|error| AppError::FFmpeg(format!("引擎缓存锁异常：{error}")))?;
    context.push(Some(GENERATION.load(Ordering::Acquire).to_string().into()));
    cache.resolve(context, discover)
}

pub(super) fn invalidate() {
    // Settings/diagnostics can invalidate from an async command without waiting
    // on an initial blocking discovery. An in-flight old result expires next use.
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn reuses_validation_and_invalidates_file_or_environment_changes() {
        let directory = tempfile::tempdir().unwrap();
        let pair = (
            directory.path().join("ffmpeg"),
            directory.path().join("ffprobe"),
        );
        std::fs::write(&pair.0, "first").unwrap();
        std::fs::write(&pair.1, "probe").unwrap();
        let checks = Cell::new(0);
        let discover = || {
            checks.set(checks.get() + 1);
            Ok(pair.clone())
        };
        let mut cache = DiscoveryCache::default();
        for _ in 0..20 {
            assert_eq!(cache.resolve(vec![], discover).unwrap(), pair);
        }
        assert_eq!(checks.get(), 1);
        std::fs::write(&pair.1, "new probe version").unwrap();
        cache.resolve(vec![], discover).unwrap();
        assert_eq!(checks.get(), 2);
        cache
            .resolve(vec![Some("new PATH".into())], discover)
            .unwrap();
        assert_eq!(checks.get(), 3);
        cache.pair = None;
        cache
            .resolve(vec![Some("new PATH".into())], discover)
            .unwrap();
        assert_eq!(checks.get(), 4);
    }

    #[test]
    fn missing_files_and_failed_discovery_are_never_cached() {
        let directory = tempfile::tempdir().unwrap();
        let pair = (
            directory.path().join("ffmpeg"),
            directory.path().join("ffprobe"),
        );
        std::fs::write(&pair.0, "first").unwrap();
        std::fs::write(&pair.1, "probe").unwrap();
        let mut cache = DiscoveryCache::default();
        cache.resolve(vec![], || Ok(pair.clone())).unwrap();
        std::fs::remove_file(&pair.1).unwrap();
        assert!(cache
            .resolve(vec![], || Err(AppError::FFmpeg("missing".into())))
            .is_err());
        assert!(cache.pair.is_none());
        std::fs::write(&pair.1, "restored").unwrap();
        assert_eq!(cache.resolve(vec![], || Ok(pair.clone())).unwrap(), pair);
    }
}
