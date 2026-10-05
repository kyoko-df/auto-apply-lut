//! Recoverable local-storage startup. Failed user files are never discarded.
use crate::database::runtime::default_database_path;
use crate::database::DatabaseManager;
use crate::utils::config::ConfigManager;
use crate::utils::path_utils::get_app_data_dir;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

#[derive(Clone, Debug, Serialize)]
pub struct StartupIssue {
    pub component: String,
    pub message: String,
    pub recoverable: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct StartupStatus {
    pub issues: Vec<StartupIssue>,
    pub temporary_storage: bool,
    pub backup_paths: Vec<String>,
}

pub struct StartupState(pub Mutex<StartupStatus>);

/// A failure to open a user DB creates an isolated in-memory session. Failure to
/// allocate even that session is returned to the application, never panicked.
pub fn initialize_storage() -> Result<(DatabaseManager, ConfigManager, StartupState), String> {
    let mut status = StartupStatus::default();
    let database = match default_database_path().and_then(|path| {
        let manager = DatabaseManager::new(path)?;
        manager.initialize()?;
        Ok(manager)
    }) {
        Ok(database) => database,
        Err(error) => {
            status.issues.push(StartupIssue {
                component: "database".into(),
                message: format!("无法打开素材资料库，原文件已保留。当前资料库仅在内存中使用，退出后不会保留新记录：{error}"),
                recoverable: true,
            });
            status.temporary_storage = true;
            DatabaseManager::in_memory().map_err(|error| format!("无法启动临时资料库：{error}"))?
        }
    };
    let config = match ConfigManager::new() {
        Ok(config) => config,
        Err(error) => ConfigManager::temporary(format!("无法初始化设置目录：{error}")),
    };
    if let Some(error) = config.persistence_error() {
        status.issues.push(StartupIssue {
            component: "settings".into(),
            message: format!("当前使用临时默认设置，原配置已保留：{error}"),
            recoverable: true,
        });
        status.temporary_storage = true;
    }
    Ok((database, config, StartupState(Mutex::new(status))))
}

#[tauri::command]
pub fn startup_status(state: State<'_, StartupState>) -> Result<StartupStatus, String> {
    state
        .0
        .lock()
        .map(|value| value.clone())
        .map_err(|error| error.to_string())
}

fn copy_durable_no_clobber(source: &Path, destination: &Path) -> Result<(), String> {
    let mut source_file = fs::File::open(source)
        .map_err(|error| format!("无法读取原文件 {}：{error}", source.display()))?;
    let mut destination_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("无法创建备份 {}：{error}", destination.display()))?;
    std::io::copy(&mut source_file, &mut destination_file)
        .and_then(|_| destination_file.flush())
        .and_then(|_| destination_file.sync_all())
        .map_err(|error| format!("备份失败，原文件已保留：{error}"))?;
    Ok(())
}

/// Move the whole SQLite set aside only after every member is durably copied.
/// Keeping sidecars under their original adjacent names in the backup directory
/// allows SQLite recovery tools to replay WAL data later.
fn backup_files(directory: &Path, names: &[&str]) -> Result<(PathBuf, Vec<PathBuf>), String> {
    fs::create_dir_all(directory).map_err(|error| format!("无法创建数据目录：{error}"))?;
    let backup_directory = directory.join(format!("recovery-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&backup_directory).map_err(|error| format!("无法创建恢复备份目录：{error}"))?;
    let mut sources = Vec::new();
    for name in names {
        let source = directory.join(name);
        match fs::symlink_metadata(&source) {
            Ok(metadata) if metadata.file_type().is_file() => {
                copy_durable_no_clobber(&source, &backup_directory.join(name))?;
                sources.push(source);
            }
            Ok(_) => {
                return Err(format!(
                    "{} 不是普通文件，已保留原路径；请手动修复访问权限或路径",
                    source.display()
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法读取原文件信息，原文件已保留：{error}")),
        }
    }
    #[cfg(unix)]
    fs::File::open(&backup_directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("无法同步备份目录，原文件已保留：{error}"))?;
    Ok((backup_directory, sources))
}

fn recover_database(path: &Path, manager: &DatabaseManager) -> Result<PathBuf, String> {
    let directory = path.parent().ok_or("无法确定数据库目录")?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    // Validate a fresh DB first. The failed database has not been moved yet.
    let temporary = tempfile::Builder::new()
        .prefix(".database-recovery-")
        .suffix(".sqlite3")
        .tempfile_in(directory)
        .map_err(|error| error.to_string())?;
    let replacement = DatabaseManager::new(temporary.path()).map_err(|error| error.to_string())?;
    replacement
        .initialize()
        .map_err(|error| error.to_string())?;
    drop(replacement);
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("数据库文件名无效")?;
    let wal = format!("{name}-wal");
    let shm = format!("{name}-shm");
    let journal = format!("{name}-journal");
    let (backup, sources) = backup_files(directory, &[name, &wal, &shm, &journal])?;
    // Durable full copies exist. Remove sidecars before replacing the main file;
    // any I/O error leaves the original main DB and its backup accessible.
    for source in sources.iter().filter(|source| source.as_path() != path) {
        fs::remove_file(source).map_err(|error| {
            format!(
                "原资料库已备份到 {}，但无法移走侧文件：{error}",
                backup.display()
            )
        })?;
    }
    temporary.persist(path).map_err(|error| {
        format!(
            "原资料库已备份到 {}，无法创建新资料库：{}",
            backup.display(),
            error.error
        )
    })?;
    let replacement = DatabaseManager::new(path).map_err(|error| {
        format!(
            "原资料库已备份到 {}，无法打开新资料库：{error}",
            backup.display()
        )
    })?;
    replacement.initialize().map_err(|error| {
        format!(
            "原资料库已备份到 {}，无法初始化新资料库：{error}",
            backup.display()
        )
    })?;
    manager
        .replace_connection(replacement)
        .map_err(|error| error.to_string())?;
    Ok(backup)
}

#[tauri::command]
pub async fn recover_startup_storage(
    state: State<'_, StartupState>,
    database: State<'_, DatabaseManager>,
    config: State<'_, Mutex<ConfigManager>>,
    guard: State<'_, super::workspace::ExitGuard>,
    tasks: State<'_, crate::core::task::TaskManager>,
) -> Result<StartupStatus, String> {
    if guard.exporting(&tasks) {
        return Err("请等待导出完成后再恢复存储".into());
    }
    let mut status = state.0.lock().map_err(|error| error.to_string())?;
    if status
        .issues
        .iter()
        .any(|issue| issue.component == "database")
    {
        let path = default_database_path().map_err(|error| error.to_string())?;
        let backup = recover_database(&path, &database)?;
        status
            .backup_paths
            .push(backup.to_string_lossy().into_owned());
        status.issues.retain(|issue| issue.component != "database");
        status.temporary_storage = !status.issues.is_empty();
    }
    if status
        .issues
        .iter()
        .any(|issue| issue.component == "settings")
    {
        let directory = get_app_data_dir().map_err(|error| error.to_string())?;
        let (backup, sources) = backup_files(&directory, &["config.json"])?;
        for source in sources {
            fs::remove_file(source)
                .map_err(|error| format!("设置已备份到 {}，无法重建：{error}", backup.display()))?;
        }
        let recovered = ConfigManager::new().map_err(|error| error.to_string())?;
        recovered.save().map_err(|error| {
            format!(
                "设置原文件已备份到 {}，保存默认设置失败：{error}",
                backup.display()
            )
        })?;
        *config.lock().map_err(|error| error.to_string())? = recovered;
        status
            .backup_paths
            .push(backup.to_string_lossy().into_owned());
        status.issues.retain(|issue| issue.component != "settings");
        status.temporary_storage = !status.issues.is_empty();
    }
    Ok(status.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_recovery_preserves_exact_original_and_sidecars() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("broken.sqlite3");
        fs::write(&path, b"invalid database\xff").unwrap();
        fs::write(directory.path().join("broken.sqlite3-wal"), b"original wal").unwrap();
        let manager = DatabaseManager::in_memory().unwrap();
        let backup = recover_database(&path, &manager).unwrap();
        assert_eq!(
            fs::read(backup.join("broken.sqlite3")).unwrap(),
            b"invalid database\xff"
        );
        assert_eq!(
            fs::read(backup.join("broken.sqlite3-wal")).unwrap(),
            b"original wal"
        );
        assert!(!directory.path().join("broken.sqlite3-wal").exists());
        let connection = manager.connection();
        let connection = connection.lock().unwrap();
        let result: i32 = connection
            .query_row("SELECT count(*) FROM luts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(result, 0);
        let reopened = DatabaseManager::new(&path).unwrap();
        reopened.initialize().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn failed_backup_keeps_original_database_and_does_not_replace_memory_session() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("valuable-file");
        let path = directory.path().join("broken.sqlite3");
        fs::write(&original, b"valuable").unwrap();
        std::os::unix::fs::symlink(&original, &path).unwrap();
        let manager = DatabaseManager::in_memory().unwrap();
        assert!(recover_database(&path, &manager).is_err());
        assert_eq!(fs::read(&original).unwrap(), b"valuable");
        assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
    }
}
