//! 批处理相关数据库查询

use crate::types::AppResult;
use rusqlite::Connection;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Batch {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub total_videos: i32,
    pub processed_videos: i32,
    pub failed_videos: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// 创建批处理记录
pub fn create_batch(conn: &Connection, batch: &Batch) -> AppResult<i64> {
    let mut stmt = conn.prepare(
        r#"
        INSERT INTO batches (
            name, description, status, total_videos, processed_videos, failed_videos,
            created_at, updated_at, completed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#
    )?;
    
    stmt.execute([
        &batch.name,
        &batch.description.as_deref().unwrap_or("").to_string(),
        &batch.status,
        &batch.total_videos.to_string(),
        &batch.processed_videos.to_string(),
        &batch.failed_videos.to_string(),
        &batch.created_at.to_rfc3339(),
        &batch.updated_at.to_rfc3339(),
        &batch.completed_at.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
    ])?;
    
    Ok(conn.last_insert_rowid())
}

/// 根据ID获取批处理记录
pub fn get_batch_by_id(conn: &Connection, id: i64) -> AppResult<Option<Batch>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, name, description, status, total_videos, processed_videos, failed_videos,
               created_at, updated_at, completed_at
        FROM batches WHERE id = ?1
        "#
    )?;
    
    let batch_iter = stmt.query_map([id], |row| {
        Ok(Batch {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get::<_, Option<String>>(2)?,
            status: row.get(3)?,
            total_videos: row.get::<_, String>(4)?.parse().unwrap_or(0),
            processed_videos: row.get::<_, String>(5)?.parse().unwrap_or(0),
            failed_videos: row.get::<_, String>(6)?.parse().unwrap_or(0),
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?).unwrap().with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?).unwrap().with_timezone(&Utc),
            completed_at: {
                let completed_str: String = row.get(9)?;
                if completed_str.is_empty() {
                    None
                } else {
                    Some(DateTime::parse_from_rfc3339(&completed_str).unwrap().with_timezone(&Utc))
                }
            },
        })
    })?;
    
    for batch in batch_iter {
        return Ok(Some(batch?));
    }
    
    Ok(None)
}

/// 获取所有批处理记录
pub fn get_all_batches(conn: &Connection) -> AppResult<Vec<Batch>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, name, description, status, total_videos, processed_videos, failed_videos,
               created_at, updated_at, completed_at
        FROM batches ORDER BY created_at DESC
        "#
    )?;
    
    let batch_iter = stmt.query_map([], |row| {
        Ok(Batch {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get::<_, Option<String>>(2)?,
            status: row.get(3)?,
            total_videos: row.get::<_, String>(4)?.parse().unwrap_or(0),
            processed_videos: row.get::<_, String>(5)?.parse().unwrap_or(0),
            failed_videos: row.get::<_, String>(6)?.parse().unwrap_or(0),
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?).unwrap().with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?).unwrap().with_timezone(&Utc),
            completed_at: {
                let completed_str: String = row.get(9)?;
                if completed_str.is_empty() {
                    None
                } else {
                    Some(DateTime::parse_from_rfc3339(&completed_str).unwrap().with_timezone(&Utc))
                }
            },
        })
    })?;
    
    let mut batches = Vec::new();
    for batch in batch_iter {
        batches.push(batch?);
    }
    
    Ok(batches)
}

/// 更新批处理记录
pub fn update_batch(conn: &Connection, batch: &Batch) -> AppResult<()> {
    let mut stmt = conn.prepare(
        r#"
        UPDATE batches SET
            name = ?1, description = ?2, status = ?3, total_videos = ?4,
            processed_videos = ?5, failed_videos = ?6, updated_at = ?7, completed_at = ?8
        WHERE id = ?9
        "#
    )?;
    
    stmt.execute([
        &batch.name,
        &batch.description.as_deref().unwrap_or("").to_string(),
        &batch.status,
        &batch.total_videos.to_string(),
        &batch.processed_videos.to_string(),
        &batch.failed_videos.to_string(),
        &batch.updated_at.to_rfc3339(),
        &batch.completed_at.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
        &batch.id.to_string(),
    ])?;
    
    Ok(())
}

/// 删除批处理记录
pub fn delete_batch(conn: &Connection, id: i64) -> AppResult<()> {
    let mut stmt = conn.prepare("DELETE FROM batches WHERE id = ?1")?;
    stmt.execute([id])?;
    Ok(())
}

/// 更新批处理状态
pub fn update_batch_status(conn: &Connection, id: i64, status: &str) -> AppResult<()> {
    let now = Utc::now();
    let mut stmt = conn.prepare(
        "UPDATE batches SET status = ?1, updated_at = ?2 WHERE id = ?3"
    )?;
    
    stmt.execute([
        status,
        &now.to_rfc3339(),
        &id.to_string(),
    ])?;
    
    Ok(())
}

/// 更新批处理进度
pub fn update_batch_progress(
    conn: &Connection,
    id: i64,
    processed_videos: i32,
    failed_videos: i32,
) -> AppResult<()> {
    let now = Utc::now();
    let mut stmt = conn.prepare(
        "UPDATE batches SET processed_videos = ?1, failed_videos = ?2, updated_at = ?3 WHERE id = ?4"
    )?;
    
    stmt.execute([
        &processed_videos.to_string(),
        &failed_videos.to_string(),
        &now.to_rfc3339(),
        &id.to_string(),
    ])?;
    
    Ok(())
}

/// 完成批处理
pub fn complete_batch(conn: &Connection, id: i64) -> AppResult<()> {
    let now = Utc::now();
    let mut stmt = conn.prepare(
        "UPDATE batches SET status = 'completed', completed_at = ?1, updated_at = ?2 WHERE id = ?3"
    )?;
    
    stmt.execute([
        &now.to_rfc3339(),
        &now.to_rfc3339(),
        &id.to_string(),
    ])?;
    
    Ok(())
}