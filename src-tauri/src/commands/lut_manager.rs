//! LUT管理命令模块
//! 提供LUT文件操作、资料库管理和预览相关的Tauri命令

use crate::core::ffmpeg::processor::VideoProcessor;
use crate::core::lut::LutManager;
use crate::database::models::Lut as DbLut;
use crate::database::queries::lut as lut_queries;
use crate::database::runtime::upsert_lut_info;
use crate::database::DatabaseManager;
use crate::types::{LutInfo, LutValidationResult};
use crate::utils::logger;
use crate::utils::path_utils::get_cache_dir;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LutLibraryItem {
    pub id: Option<i64>,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub lut_type: String,
    pub format: String,
    pub category: String,
    pub is_valid: bool,
    pub error_message: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LutPreviewRequest {
    pub lut_path: String,
    #[serde(default)]
    pub video_path: Option<String>,
    #[serde(default = "default_preview_intensity")]
    pub intensity: f32,
}

fn default_preview_intensity() -> f32 {
    1.0
}

fn category_for(lut_type: &str, format: &str) -> String {
    match lut_type {
        "ThreeDimensional" => "3D LUT".to_string(),
        "OneDimensional" => "1D LUT".to_string(),
        _ if !format.is_empty() => format.to_uppercase(),
        _ => "Unclassified".to_string(),
    }
}

fn item_from_lut_info(id: Option<i64>, info: &LutInfo) -> LutLibraryItem {
    let lut_type = format!("{:?}", info.lut_type);
    let format = info.format.extension().to_uppercase();
    LutLibraryItem {
        id,
        path: info.path.to_string_lossy().to_string(),
        name: info.name.clone(),
        size: info.size,
        lut_type: lut_type.clone(),
        format: format.clone(),
        category: category_for(&lut_type, &format),
        is_valid: info.is_valid,
        error_message: info.error_message.clone(),
        updated_at: info.modified_at.to_rfc3339(),
    }
}

fn item_from_db_record(record: &DbLut, error_message: Option<String>) -> LutLibraryItem {
    let lut_type = record.lut_type.clone();
    let format = record.format.clone().unwrap_or_default().to_uppercase();
    LutLibraryItem {
        id: Some(record.id),
        path: record.file_path.clone(),
        name: record.file_name.clone(),
        size: record.file_size as u64,
        lut_type: lut_type.clone(),
        format: format.clone(),
        category: category_for(&lut_type, &format),
        is_valid: error_message.is_none(),
        error_message,
        updated_at: record.updated_at.to_rfc3339(),
    }
}

fn recursive_scan_luts(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        std::fs::read_dir(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            recursive_scan_luts(&entry_path, files)?;
            continue;
        }

        if entry_path.is_file() {
            let is_supported = entry_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(crate::types::LutFormat::is_supported)
                .unwrap_or(false);
            if is_supported {
                files.push(entry_path);
            }
        }
    }

    Ok(())
}

async fn remember_paths(
    paths: &[String],
    lut_manager: &LutManager,
    db: &DatabaseManager,
) -> Vec<LutLibraryItem> {
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();

    for path in paths {
        if !seen.insert(path.clone()) {
            continue;
        }

        match lut_manager.get_lut_info(path).await {
            Ok(info) => {
                if let Err(error) = upsert_lut_info(db, &info) {
                    logger::log_warn(&format!("Failed to persist LUT {}: {}", path, error));
                }
                items.push(item_from_lut_info(None, &info));
            }
            Err(error) => {
                let fallback_path = PathBuf::from(path);
                let format = fallback_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or_default()
                    .to_uppercase();
                let name = fallback_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                items.push(LutLibraryItem {
                    id: None,
                    path: path.clone(),
                    name,
                    size: 0,
                    lut_type: "Unknown".to_string(),
                    format: format.clone(),
                    category: category_for("Unknown", &format),
                    is_valid: false,
                    error_message: Some(error.to_string()),
                    updated_at: Utc::now().to_rfc3339(),
                });
            }
        }
    }

    items
}

/// 验证LUT文件（委托核心模块）
#[tauri::command]
pub async fn validate_lut_file(
    lut_path: String,
    lut_manager: State<'_, LutManager>,
) -> Result<LutValidationResult, String> {
    logger::log_info(&format!("Validating LUT file: {}", lut_path));
    lut_manager
        .validate_lut(&lut_path)
        .await
        .map_err(|e| e.to_string())
}

/// 获取LUT信息，并写入资料库。
#[tauri::command]
pub async fn get_lut_info(
    lut_path: String,
    lut_manager: State<'_, LutManager>,
    db: State<'_, DatabaseManager>,
) -> Result<LutInfo, String> {
    logger::log_info(&format!("Getting LUT info: {}", lut_path));
    let info = lut_manager
        .get_lut_info(&lut_path)
        .await
        .map_err(|e| e.to_string())?;

    if let Err(error) = upsert_lut_info(&db, &info) {
        logger::log_warn(&format!(
            "Failed to persist LUT info {}: {}",
            lut_path, error
        ));
    }

    Ok(info)
}

/// 获取支持的LUT格式
#[tauri::command]
pub async fn get_supported_lut_formats(
    lut_manager: State<'_, LutManager>,
) -> Result<Vec<String>, String> {
    Ok(lut_manager
        .get_supported_formats()
        .iter()
        .map(|format| format.extension().to_string())
        .collect())
}

/// 将当前选择的 LUT 文件记入资料库。
#[tauri::command]
pub async fn remember_lut_files(
    paths: Vec<String>,
    lut_manager: State<'_, LutManager>,
    db: State<'_, DatabaseManager>,
) -> Result<Vec<LutLibraryItem>, String> {
    Ok(remember_paths(&paths, &lut_manager, &db).await)
}

/// 返回资料库中的 LUT 列表。
#[tauri::command]
pub async fn list_lut_library(
    lut_manager: State<'_, LutManager>,
    db: State<'_, DatabaseManager>,
) -> Result<Vec<LutLibraryItem>, String> {
    let connection = db.connection();
    let records = {
        let conn = connection
            .lock()
            .map_err(|e| format!("Database lock poisoned: {}", e))?;
        lut_queries::get_all_luts(&conn).map_err(|e| e.to_string())?
    };

    let mut items = Vec::with_capacity(records.len());
    for record in records {
        if !Path::new(&record.file_path).exists() {
            items.push(item_from_db_record(&record, Some("文件不存在".to_string())));
            continue;
        }

        match lut_manager.get_lut_info(&record.file_path).await {
            Ok(info) => {
                if let Err(error) = upsert_lut_info(&db, &info) {
                    logger::log_warn(&format!(
                        "Failed to refresh LUT {}: {}",
                        record.file_path, error
                    ));
                }
                items.push(item_from_lut_info(Some(record.id), &info));
            }
            Err(error) => {
                items.push(item_from_db_record(&record, Some(error.to_string())));
            }
        }
    }

    Ok(items)
}

/// 从目录递归导入 LUT 到资料库。
#[tauri::command]
pub async fn import_lut_directory(
    directory: String,
    lut_manager: State<'_, LutManager>,
    db: State<'_, DatabaseManager>,
) -> Result<Vec<LutLibraryItem>, String> {
    let directory_path = PathBuf::from(&directory);
    if !directory_path.exists() || !directory_path.is_dir() {
        return Err("Directory does not exist or is not a directory".to_string());
    }

    let files = tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        recursive_scan_luts(&directory_path, &mut files)?;
        Ok::<Vec<PathBuf>, String>(files)
    })
    .await
    .map_err(|e| format!("LUT import task failed: {}", e))??;

    let paths = files
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    Ok(remember_paths(&paths, &lut_manager, &db).await)
}

/// 从资料库移除指定 LUT。
#[tauri::command]
pub async fn remove_lut_from_library(
    lut_path: String,
    db: State<'_, DatabaseManager>,
) -> Result<String, String> {
    let connection = db.connection();
    let conn = connection
        .lock()
        .map_err(|e| format!("Database lock poisoned: {}", e))?;

    if let Some(record) =
        lut_queries::get_lut_by_path(&conn, &lut_path).map_err(|e| e.to_string())?
    {
        lut_queries::delete_lut(&conn, record.id).map_err(|e| e.to_string())?;
    }

    Ok("Removed from LUT library".to_string())
}

/// 生成 LUT 预览图。
#[tauri::command]
pub async fn generate_lut_preview(
    request: LutPreviewRequest,
    lut_manager: State<'_, LutManager>,
    video_processor: State<'_, VideoProcessor>,
    db: State<'_, DatabaseManager>,
) -> Result<String, String> {
    let info = lut_manager
        .get_lut_info(&request.lut_path)
        .await
        .map_err(|e| e.to_string())?;
    if let Err(error) = upsert_lut_info(&db, &info) {
        logger::log_warn(&format!(
            "Failed to persist LUT info before preview {}: {}",
            request.lut_path, error
        ));
    }

    let mut hasher = Sha1::new();
    hasher.update(request.lut_path.as_bytes());
    hasher.update(format!("{:.3}", request.intensity).as_bytes());
    if let Some(video_path) = &request.video_path {
        hasher.update(video_path.as_bytes());
    }
    if let Ok(metadata) = std::fs::metadata(&request.lut_path) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                hasher.update(duration.as_secs().to_le_bytes());
                hasher.update(duration.subsec_nanos().to_le_bytes());
            }
        }
    }

    let cache_key = format!("{:x}", hasher.finalize());
    let preview_dir = get_cache_dir()
        .map_err(|e| e.to_string())?
        .join("lut-previews");
    std::fs::create_dir_all(&preview_dir)
        .map_err(|e| format!("Failed to create LUT preview cache: {}", e))?;
    let preview_path = preview_dir.join(format!("{}.png", cache_key));

    if !preview_path.exists() {
        video_processor
            .generate_lut_preview_image(
                &[PathBuf::from(&request.lut_path)],
                &preview_path,
                request.video_path.as_deref().map(Path::new),
                request.intensity,
            )
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(preview_path.to_string_lossy().to_string())
}
