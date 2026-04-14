use crate::core::task::Task as RuntimeTask;
use crate::database::models::{Lut as DbLut, Task as DbTask, Video as DbVideo};
use crate::database::queries::{lut as lut_queries, task as task_queries, video as video_queries};
use crate::database::DatabaseManager;
use crate::types::{AppError, AppResult, LutInfo, VideoInfo};
use crate::utils::path_utils::get_app_data_dir;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

pub fn default_database_path() -> AppResult<PathBuf> {
    let mut path = get_app_data_dir()?;
    std::fs::create_dir_all(&path)
        .map_err(|e| AppError::Io(format!("创建数据库目录失败: {}", e)))?;
    path.push("auto-apply-lut.sqlite3");
    Ok(path)
}

pub fn upsert_task_snapshot(db: &DatabaseManager, task: &RuntimeTask) -> AppResult<()> {
    let db_task = DbTask {
        id: task.id.clone(),
        name: task.name.clone(),
        description: task.description.clone(),
        task_type: format!("{:?}", task.task_type),
        status: format!("{:?}", task.status).to_lowercase(),
        priority: "medium".to_string(),
        progress: task.progress,
        config: None,
        result: task.output_path.clone(),
        error_message: task.error.clone(),
        created_at: timestamp_to_datetime(task.created_at)?,
        started_at: task.started_at.map(timestamp_to_datetime).transpose()?,
        completed_at: task.completed_at.map(timestamp_to_datetime).transpose()?,
        estimated_duration: None,
        actual_duration: task
            .started_at
            .zip(task.completed_at)
            .map(|(started, completed)| completed.saturating_sub(started)),
    };

    let connection = db.connection();
    let conn = connection
        .lock()
        .map_err(|e| AppError::Database(format!("数据库连接锁失败: {}", e)))?;

    if task_queries::get_task_by_id(&conn, &db_task.id)?.is_some() {
        task_queries::update_task(&conn, &db_task)?;
    } else {
        task_queries::insert_task(&conn, &db_task)?;
    }

    Ok(())
}

pub fn upsert_video_info(db: &DatabaseManager, info: &VideoInfo) -> AppResult<()> {
    let file_path = info.path.to_string_lossy().to_string();
    let now = Utc::now();
    let connection = db.connection();
    let conn = connection
        .lock()
        .map_err(|e| AppError::Database(format!("数据库连接锁失败: {}", e)))?;

    let existing = video_queries::get_video_by_path(&conn, &file_path)?;
    let db_video = DbVideo {
        id: existing.as_ref().map(|video| video.id).unwrap_or_default(),
        file_path,
        file_name: info.filename.clone(),
        file_size: info.size as i64,
        duration: info.duration,
        width: info.width.map(|value| value as i32),
        height: info.height.map(|value| value as i32),
        fps: info.fps,
        codec: info.codec.clone(),
        bitrate: info.bitrate.map(|value| value as i64),
        format: info
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase()),
        created_at: info.created_at.unwrap_or(now),
        updated_at: now,
        last_accessed: Some(now),
    };

    if existing.is_some() {
        video_queries::update_video(&conn, &db_video)?;
    } else {
        video_queries::create_video(&conn, &db_video)?;
    }

    Ok(())
}

pub fn upsert_lut_info(db: &DatabaseManager, info: &LutInfo) -> AppResult<()> {
    let file_path = info.path.to_string_lossy().to_string();
    let now = Utc::now();
    let connection = db.connection();
    let conn = connection
        .lock()
        .map_err(|e| AppError::Database(format!("数据库连接锁失败: {}", e)))?;

    let existing = lut_queries::get_lut_by_path(&conn, &file_path)?;
    let db_lut = DbLut {
        id: existing.as_ref().map(|lut| lut.id).unwrap_or_default(),
        file_path,
        file_name: info.name.clone(),
        file_size: info.size as i64,
        lut_type: format!("{:?}", info.lut_type),
        format: Some(info.format.extension().to_string()),
        description: info.error_message.clone(),
        created_at: info.created_at,
        updated_at: now,
        last_accessed: Some(now),
    };

    if existing.is_some() {
        lut_queries::update_lut(&conn, &db_lut)?;
    } else {
        lut_queries::create_lut(&conn, &db_lut)?;
    }

    Ok(())
}

fn timestamp_to_datetime(timestamp: i64) -> AppResult<DateTime<Utc>> {
    DateTime::<Utc>::from_timestamp(timestamp, 0)
        .ok_or_else(|| AppError::Database(format!("无效时间戳: {}", timestamp)))
}
