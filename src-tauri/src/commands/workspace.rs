//! Bounded workspace snapshots and coordinated application shutdown.
use crate::core::task::{TaskManager, TaskStatus};
use crate::utils::path_utils::get_app_data_dir;
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

const MAX_WORKSPACE_BYTES: u64 = 8 * 1024 * 1024;

pub struct WorkspaceStore {
    path: Result<PathBuf, String>,
    // Serializes load/save as well as corrupt-file recovery.
    lock: Mutex<Option<String>>,
}

impl Default for WorkspaceStore {
    fn default() -> Self {
        Self {
            path: get_app_data_dir()
                .map(|directory| directory.join("workspace.json"))
                .map_err(|error| error.to_string()),
            lock: Mutex::new(None),
        }
    }
}

fn ensure_regular_file(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => Err(format!(
            "工作区路径不是普通文件，原路径已保留：{}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法检查工作区文件：{error}")),
    }
}

fn load_snapshot(path: &Path) -> Result<Option<Value>, String> {
    ensure_regular_file(path)?;
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("无法读取工作区，原文件已保留：{error}")),
    };
    let mut bytes = Vec::new();
    file.take(MAX_WORKSPACE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("无法读取工作区，原文件已保留：{error}"))?;
    if bytes.len() as u64 > MAX_WORKSPACE_BYTES {
        return Err("工作区文件超过 8 MB，原文件已保留，请先备份并移走该文件".into());
    }
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) if value.is_object() => Ok(Some(value)),
        result => {
            let reason = result
                .err()
                .map(|error| error.to_string())
                .unwrap_or_else(|| "工作区根数据必须是对象".into());
            let backup =
                path.with_file_name(format!("workspace.invalid-{}.json", uuid::Uuid::new_v4()));
            // A durable no-clobber backup must exist before the malformed source
            // is moved aside. Failure leaves the original completely intact.
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup)
                .map_err(|error| format!("工作区损坏且无法备份，原文件已保留：{error}"))?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|error| format!("工作区备份失败，原文件已保留：{error}"))?;
            fs::remove_file(path)
                .map_err(|error| format!("工作区已备份但无法恢复，原文件已保留：{error}"))?;
            Err(format!(
                "工作区数据损坏，已保留备份 {}：{reason}",
                backup.display()
            ))
        }
    }
}

fn save_snapshot(path: &Path, snapshot: &Value) -> Result<(), String> {
    if !snapshot.is_object() {
        return Err("工作区必须是 JSON 对象".into());
    }
    let content = serde_json::to_vec(snapshot).map_err(|error| error.to_string())?;
    if content.len() as u64 > MAX_WORKSPACE_BYTES {
        return Err("工作区超过 8 MB，无法保存；请减少素材数量".into());
    }
    ensure_regular_file(path)?;
    let parent = path.parent().ok_or("无法解析工作区目录")?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建工作区目录：{error}"))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".workspace-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|error| format!("无法创建工作区临时文件：{error}"))?;
    temporary
        .write_all(&content)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("工作区写入失败，原文件已保留：{error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("工作区保存失败，原文件已保留：{}", error.error))?;
    #[cfg(unix)]
    if let Err(error) = fs::File::open(parent).and_then(|directory| directory.sync_all()) {
        tracing::warn!("工作区已保存，但目录同步失败：{error}");
    }
    Ok(())
}

#[tauri::command]
pub async fn load_workspace(store: State<'_, WorkspaceStore>) -> Result<Option<Value>, String> {
    let mut blocked = store.lock.lock().map_err(|error| error.to_string())?;
    let path = store.path.as_ref().map_err(Clone::clone)?;
    let result = load_snapshot(path);
    // An unreadable, oversized or non-regular original cannot be overwritten by
    // a later autosave. Corrupt files successfully backed up no longer exist.
    *blocked = result
        .as_ref()
        .err()
        .filter(|_| path.symlink_metadata().is_ok())
        .cloned();
    result
}

#[tauri::command]
pub async fn save_workspace(
    snapshot: Value,
    store: State<'_, WorkspaceStore>,
) -> Result<(), String> {
    let blocked = store.lock.lock().map_err(|error| error.to_string())?;
    if let Some(reason) = &*blocked {
        return Err(format!(
            "工作区原文件需要手动恢复后重启应用，已停止自动保存：{reason}"
        ));
    }
    save_snapshot(store.path.as_ref().map_err(Clone::clone)?, &snapshot)
}

#[derive(Default)]
pub struct ExitGuard {
    exporting: AtomicBool,
    approved: AtomicBool,
}

impl ExitGuard {
    pub fn approved(&self) -> bool {
        self.approved.load(Ordering::SeqCst)
    }

    pub fn exporting(&self, task_manager: &TaskManager) -> bool {
        self.exporting.load(Ordering::SeqCst) || tasks_running(task_manager)
    }
}

fn tasks_running(task_manager: &TaskManager) -> bool {
    super::batch_manager::has_unfinished_batches()
        || task_manager
            .get_all_tasks()
            .map(|tasks| tasks.iter().any(|task| task.status == TaskStatus::Running))
            .unwrap_or(true)
}

/// Both native window-close and OS quit use the same coordination path. An
/// ordinary close is also delayed until the frontend flushes its last snapshot.
pub fn notify_close_requested(app: &tauri::AppHandle) {
    let guard = app.state::<ExitGuard>();
    let task_manager = app.state::<TaskManager>();
    let event = if guard.exporting(&task_manager) {
        "export-close-requested"
    } else {
        "workspace-close-requested"
    };
    if let Err(error) = app.emit(event, ()) {
        tracing::error!("无法通知窗口保存并退出：{error}");
    }
}

#[tauri::command]
pub fn set_export_guard(active: bool, guard: State<'_, ExitGuard>) -> Result<(), String> {
    // This flag covers frontend startup. Backend jobs hold their independent
    // lifetime guard, so clearing this flag cannot permit an early process exit.
    guard.exporting.store(active, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn request_app_exit(
    app: tauri::AppHandle,
    guard: State<'_, ExitGuard>,
    task_manager: State<'_, TaskManager>,
) -> Result<(), String> {
    if guard.exporting(&task_manager) {
        return Err("请先取消并等待导出结束，再退出应用".into());
    }
    guard.approved.store(true, Ordering::SeqCst);
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::task::TaskType;
    use serde_json::json;

    #[test]
    fn snapshots_roundtrip_atomically_and_reject_oversized_or_invalid_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.json");
        assert!(load_snapshot(&path).unwrap().is_none());
        let first = json!({"version": 1, "clips": [{"path": "视频.mp4"}]});
        save_snapshot(&path, &first).unwrap();
        assert_eq!(load_snapshot(&path).unwrap(), Some(first.clone()));
        assert!(save_snapshot(&path, &json!([])).is_err());
        assert!(save_snapshot(
            &path,
            &json!({"huge": "x".repeat(MAX_WORKSPACE_BYTES as usize)})
        )
        .is_err());
        assert_eq!(load_snapshot(&path).unwrap(), Some(first));
    }

    #[test]
    fn corrupt_snapshot_preserves_exact_bytes_before_allowing_a_new_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.json");
        let bytes = b"{bad\xff";
        fs::write(&path, bytes).unwrap();
        assert!(load_snapshot(&path).unwrap_err().contains("已保留备份"));
        let backup = fs::read_dir(directory.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(fs::read(backup).unwrap(), bytes);
        assert!(!path.exists());
        save_snapshot(&path, &json!({"version": 1})).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn snapshots_never_follow_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original");
        let path = directory.path().join("workspace.json");
        fs::write(&original, b"untouched").unwrap();
        std::os::unix::fs::symlink(&original, &path).unwrap();
        assert!(load_snapshot(&path).is_err());
        assert!(save_snapshot(&path, &json!({})).is_err());
        assert_eq!(fs::read(&original).unwrap(), b"untouched");
    }

    #[test]
    fn actual_running_tasks_keep_the_exit_guard_active() {
        let manager = TaskManager::default();
        let guard = ExitGuard::default();
        assert!(!guard.exporting(&manager));
        let task = manager
            .create_task(TaskType::VideoProcessing, "test".into())
            .unwrap();
        manager.start_task(&task).unwrap();
        assert!(guard.exporting(&manager));
        manager.cancel_task(&task).unwrap();
        assert!(!guard.exporting(&manager));
        guard.exporting.store(true, Ordering::SeqCst);
        assert!(guard.exporting(&manager));
    }
}
