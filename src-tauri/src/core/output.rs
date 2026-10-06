//! Collision allocation shared by media processors; publishing remains no-clobber.
use crate::types::{ui_err, ui_err_p};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

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
