//! Collision allocation and no-clobber publication shared by media processors.
use crate::types::{ui_err, ui_err_p};
use std::{
    collections::HashSet,
    fs::File,
    io,
    path::{Path, PathBuf},
};
#[cfg(unix)]
use std::{
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
};

/// An exclusively created output, still owned by the current export until commit.
/// Drop removes an incomplete copy, but never a replacement created by somebody else.
pub struct PendingPublication {
    file: Option<File>,
    path: PathBuf,
    committed: bool,
}
impl PendingPublication {
    pub fn commit(mut self) -> File {
        self.committed = true;
        self.file.take().expect("publication owns its output")
    }
}
impl Drop for PendingPublication {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let owned = self.file.as_ref().and_then(|file| file.metadata().ok());
            let current = std::fs::symlink_metadata(&self.path).ok();
            if let (Some(owned), Some(current)) = (owned, current) {
                if owned.dev() == current.dev() && owned.ino() == current.ino() {
                    let _ = std::fs::remove_file(&self.path);
                }
            }
        }
    }
}

/// macOS ENOTSUP (45) and EOPNOTSUPP are different errno values. Rust's
/// ErrorKind only maps the latter to Unsupported on some toolchain versions.
pub fn publication_unsupported(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(target_os = "macos")]
    if error.raw_os_error() == Some(libc::ENOTSUP) {
        return true;
    }
    false
}

/// Use only after persist_noclobber reports an unsupported operation. Some network volumes
/// support links but not exclusive rename; macOS SMB supports neither. On those
/// volumes create_new is the no-overwrite primitive. The completed temporary file
/// remains on the destination volume until copying, syncing and commit finish.
/// Copying is not atomic: a partial target is visible and a crash/disconnection can
/// leave it behind. Never emulate exclusive rename with exists() + rename().
pub fn publish_without_rename(
    temporary: &tempfile::NamedTempFile,
    destination: &Path,
    mut check_cancel: impl FnMut() -> io::Result<()>,
) -> io::Result<PendingPublication> {
    check_cancel()?;
    #[cfg(not(unix))]
    {
        let _ = (temporary, destination);
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "exclusive publication is unsupported",
        ));
    }
    #[cfg(unix)]
    {
        // Open before linking/creating so any failure here leaves no target.
        let mut source = temporary.reopen()?;
        source.seek(SeekFrom::Start(0))?;
        match std::fs::hard_link(temporary.path(), destination) {
            Ok(()) => {
                return Ok(PendingPublication {
                    file: Some(source),
                    path: destination.to_path_buf(),
                    committed: false,
                });
            }
            Err(error) if publication_unsupported(&error) => {}
            Err(error) => return Err(error),
        }
        copy_without_overwrite(&mut source, destination, &mut check_cancel)
    }
}

#[cfg(unix)]
fn copy_without_overwrite(
    source: &mut impl Read,
    destination: &Path,
    check_cancel: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<PendingPublication> {
    check_cancel()?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(destination)?;
    let mut pending = PendingPublication {
        file: Some(file),
        path: destination.to_path_buf(),
        committed: false,
    };
    let output = pending.file.as_mut().expect("publication owns its output");
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        check_cancel()?;
        let read = match source.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            break;
        }
        check_cancel()?;
        output.write_all(&buffer[..read])?;
    }
    check_cancel()?;
    output.sync_all()?;
    check_cancel()?;
    Ok(pending)
}

fn key(path: &Path) -> PathBuf {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path.into()
    }
}
pub fn available_path(path: &Path, reserved: &mut HashSet<PathBuf>) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = std::fs::canonicalize(parent).map_err(|e| {
        ui_err_p(
            "output.bad_dir",
            serde_json::json!({ "error": e.to_string() }),
            format!("无效输出目录：{e}"),
        )
    })?;
    let initial = parent.join(path.file_name().ok_or("无效输出文件名")?);
    let stem = path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("output");
    let ext = path.extension().and_then(|v| v.to_str()).unwrap_or("mp4");
    for index in 1..=10000 {
        let candidate = if index == 1 {
            initial.clone()
        } else {
            parent.join(format!("{stem} ({index}).{ext}"))
        };
        if std::fs::symlink_metadata(&candidate).is_ok() || reserved.contains(&key(&candidate)) {
            continue;
        }
        reserved.insert(key(&candidate));
        return Ok(candidate);
    }
    Err(ui_err(
        "output.name_taken",
        "同名输出过多，请更换导出目录或文件名",
    ))
}

#[cfg(all(test, unix))]
mod publication_tests {
    use super::*;

    #[test]
    fn only_unsupported_operations_enable_the_fallback() {
        assert!(publication_unsupported(&io::Error::from(
            io::ErrorKind::Unsupported
        )));
        #[cfg(target_os = "macos")]
        assert!(publication_unsupported(&io::Error::from_raw_os_error(
            libc::ENOTSUP
        )));
        for kind in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::AlreadyExists,
            io::ErrorKind::NotFound,
            io::ErrorKind::StorageFull,
            io::ErrorKind::InvalidInput,
        ] {
            assert!(!publication_unsupported(&io::Error::from(kind)));
        }
    }

    #[test]
    fn exclusive_copy_preserves_bytes_and_cannot_overwrite_files_or_directories() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("完成 ' [1].mp4");
        let bytes = vec![123; 2 * 1024 * 1024 + 17];
        let pending =
            copy_without_overwrite(&mut bytes.as_slice(), &destination, &mut || Ok(())).unwrap();
        pending.commit();
        assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        let error =
            copy_without_overwrite(&mut b"replacement".as_slice(), &destination, &mut || Ok(()))
                .err()
                .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        let directory = dir.path().join("directory");
        std::fs::create_dir(&directory).unwrap();
        assert!(copy_without_overwrite(&mut b"x".as_slice(), &directory, &mut || Ok(())).is_err());
        assert!(directory.is_dir());
        let symlink = dir.path().join("symlink");
        std::os::unix::fs::symlink(dir.path().join("missing"), &symlink).unwrap();
        assert!(copy_without_overwrite(&mut b"x".as_slice(), &symlink, &mut || Ok(())).is_err());
        assert!(std::fs::symlink_metadata(symlink).unwrap().is_symlink());
    }

    #[test]
    fn cancellation_before_during_and_after_copy_removes_only_this_output() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("cancel.mp4");
        let bytes = vec![42; 3 * 1024 * 1024];
        let cancel = || io::Error::new(io::ErrorKind::Interrupted, "Cancelled");
        assert!(
            copy_without_overwrite(&mut bytes.as_slice(), &destination, &mut || Err(cancel()))
                .is_err()
        );
        assert!(!destination.exists());
        let mut saw_partial = false;
        let result = copy_without_overwrite(&mut bytes.as_slice(), &destination, &mut || {
            if std::fs::metadata(&destination).is_ok_and(|m| m.len() > 0) {
                saw_partial = true;
                Err(cancel())
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert!(saw_partial);
        assert!(!destination.exists());
        let pending =
            copy_without_overwrite(&mut bytes.as_slice(), &destination, &mut || Ok(())).unwrap();
        drop(pending); // Cancellation after copying, before the commit point.
        assert!(!destination.exists());
    }

    #[test]
    fn failed_read_cleans_partial_output_and_cleanup_preserves_replacements() {
        struct BrokenReader(bool);
        impl Read for BrokenReader {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                if self.0 {
                    return Err(io::Error::other("network read failed"));
                }
                self.0 = true;
                buffer[..3].copy_from_slice(b"abc");
                Ok(3)
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("failed.mp4");
        assert!(
            copy_without_overwrite(&mut BrokenReader(false), &destination, &mut || Ok(())).is_err()
        );
        assert!(!destination.exists());
        let pending =
            copy_without_overwrite(&mut b"abc".as_slice(), &destination, &mut || Ok(())).unwrap();
        std::fs::rename(&destination, dir.path().join("moved.mp4")).unwrap();
        std::fs::write(&destination, b"somebody else's output").unwrap();
        drop(pending);
        assert_eq!(
            std::fs::read(destination).unwrap(),
            b"somebody else's output"
        );
    }

    #[test]
    fn concurrent_exclusive_copies_have_one_winner() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("race.mp4");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = [b'a', b'b']
            .into_iter()
            .map(|byte| {
                let destination = destination.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let bytes = vec![byte; 2 * 1024 * 1024];
                    match copy_without_overwrite(
                        &mut bytes.as_slice(),
                        &destination,
                        &mut || Ok(()),
                    ) {
                        Ok(pending) => {
                            pending.commit();
                            true
                        }
                        Err(error) => {
                            assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
                            false
                        }
                    }
                })
            })
            .collect();
        assert_eq!(
            handles
                .into_iter()
                .map(|h| usize::from(h.join().unwrap()))
                .sum::<usize>(),
            1
        );
        let bytes = std::fs::read(destination).unwrap();
        assert_eq!(bytes.len(), 2 * 1024 * 1024);
        assert!(bytes.iter().all(|byte| *byte == bytes[0]));
    }

    #[test]
    fn link_fallback_keeps_temporary_until_commit_and_does_not_clobber() {
        let dir = tempfile::tempdir().unwrap();
        let mut temporary = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        temporary.write_all(b"completed media").unwrap();
        let destination = dir.path().join("linked.mp4");
        let pending = publish_without_rename(&temporary, &destination, || Ok(())).unwrap();
        assert!(temporary.path().exists());
        pending.commit();
        assert!(publish_without_rename(&temporary, &destination, || Ok(())).is_err());
        drop(temporary);
        assert_eq!(std::fs::read(destination).unwrap(), b"completed media");
    }

    // Explicit opt-in: generated media stays inside a unique, automatically
    // removed test directory on the mounted volume. No existing NAS media is used.
    #[test]
    fn mounted_output_volume_publication_roundtrip() {
        let Some(parent) = std::env::var_os("LUTLAB_TEST_OUTPUT_DIR") else {
            return;
        };
        let dir = tempfile::Builder::new()
            .prefix(".lutlab-publication-test-")
            .tempdir_in(parent)
            .unwrap();
        let mut temporary = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        let bytes = vec![93; 1024 * 1024 + 29];
        temporary.write_all(&bytes).unwrap();
        let destination = dir.path().join("NAS 输出.mp4");
        match temporary.persist_noclobber(&destination) {
            Ok(_) => {}
            Err(error) => {
                assert!(publication_unsupported(&error.error), "{}", error.error);
                eprintln!(
                    "exclusive rename rejected by mounted volume: {}",
                    error.error
                );
                let pending = publish_without_rename(&error.file, &destination, || Ok(())).unwrap();
                pending.commit();
            }
        }
        assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        let control = crate::core::photo::processor::PhotoControl::default();
        let mut photo = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        photo.write_all(b"completed photo").unwrap();
        let photo_destination = dir.path().join("photo.png");
        assert_eq!(
            control.publish(photo, &photo_destination).unwrap(),
            photo_destination
        );
        assert_eq!(
            std::fs::read(&photo_destination).unwrap(),
            b"completed photo"
        );
        let mut collision = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        collision.write_all(b"another photo").unwrap();
        let allocated = crate::core::photo::processor::PhotoControl::default()
            .publish(collision, &photo_destination)
            .unwrap();
        assert_ne!(allocated, photo_destination);
        assert_eq!(
            std::fs::read(&photo_destination).unwrap(),
            b"completed photo"
        );
        assert_eq!(std::fs::read(allocated).unwrap(), b"another photo");
        // The actual network fallback must clean a partial target on cancellation.
        let mut source = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        source.write_all(&vec![7; 3 * 1024 * 1024]).unwrap();
        let cancelled = dir.path().join("cancelled.mp4");
        let result = publish_without_rename(&source, &cancelled, || {
            if std::fs::metadata(&cancelled).is_ok_and(|m| m.len() > 0) {
                Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled"))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert!(!cancelled.exists());
    }
}
