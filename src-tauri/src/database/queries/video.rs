//! 视频相关数据库查询

use crate::database::models::Video;
use crate::types::AppResult;
use chrono::{DateTime, Utc};
use rusqlite::params;
use rusqlite::Connection;

/// 创建视频记录
pub fn create_video(conn: &Connection, video: &Video) -> AppResult<i64> {
    conn.execute(
        r#"
        INSERT INTO videos (
            file_path, file_name, file_size, duration, width, height,
            fps, codec, bitrate, format, created_at, updated_at, last_accessed
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            &video.file_path,
            &video.file_name,
            video.file_size,
            video.duration,
            video.width,
            video.height,
            video.fps,
            &video.codec,
            video.bitrate,
            &video.format,
            video.created_at.to_rfc3339(),
            video.updated_at.to_rfc3339(),
            video.last_accessed.map(|dt| dt.to_rfc3339()),
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

/// 根据ID获取视频
pub fn get_video_by_id(conn: &Connection, id: i64) -> AppResult<Option<Video>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_path, file_name, file_size, duration, width, height, fps, codec, bitrate, format, created_at, updated_at, last_accessed FROM videos WHERE id = ?1"
    )?;

    let video_iter = stmt.query_map([id], |row| {
        Ok(Video {
            id: row.get(0)?,
            file_path: row.get(1)?,
            file_name: row.get(2)?,
            file_size: row.get(3)?,
            duration: row.get::<_, Option<f64>>(4)?,
            width: row.get::<_, Option<i32>>(5)?,
            height: row.get::<_, Option<i32>>(6)?,
            fps: row.get::<_, Option<f64>>(7)?,
            codec: row.get::<_, Option<String>>(8)?,
            bitrate: row.get::<_, Option<i64>>(9)?,
            format: row.get::<_, Option<String>>(10)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(11)?)
                .unwrap()
                .with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                .unwrap()
                .with_timezone(&Utc),
            last_accessed: row.get::<_, Option<String>>(13)?.map(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .unwrap()
                    .with_timezone(&Utc)
            }),
        })
    })?;

    for video in video_iter {
        return Ok(Some(video?));
    }

    Ok(None)
}

/// 根据文件路径获取视频
pub fn get_video_by_path(conn: &Connection, path: &str) -> AppResult<Option<Video>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_path, file_name, file_size, duration, width, height, fps, codec, bitrate, format, created_at, updated_at, last_accessed FROM videos WHERE file_path = ?1"
    )?;

    let video_iter = stmt.query_map([path], |row| {
        Ok(Video {
            id: row.get(0)?,
            file_path: row.get(1)?,
            file_name: row.get(2)?,
            file_size: row.get(3)?,
            duration: row.get::<_, Option<f64>>(4)?,
            width: row.get::<_, Option<i32>>(5)?,
            height: row.get::<_, Option<i32>>(6)?,
            fps: row.get::<_, Option<f64>>(7)?,
            codec: row.get::<_, Option<String>>(8)?,
            bitrate: row.get::<_, Option<i64>>(9)?,
            format: row.get::<_, Option<String>>(10)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(11)?)
                .unwrap()
                .with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                .unwrap()
                .with_timezone(&Utc),
            last_accessed: row.get::<_, Option<String>>(13)?.map(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .unwrap()
                    .with_timezone(&Utc)
            }),
        })
    })?;

    for video in video_iter {
        return Ok(Some(video?));
    }

    Ok(None)
}

/// 获取所有视频
pub fn get_all_videos(conn: &Connection) -> AppResult<Vec<Video>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_path, file_name, file_size, duration, width, height, fps, codec, bitrate, format, created_at, updated_at, last_accessed FROM videos ORDER BY created_at DESC"
    )?;

    let video_iter = stmt.query_map([], |row| {
        Ok(Video {
            id: row.get(0)?,
            file_path: row.get(1)?,
            file_name: row.get(2)?,
            file_size: row.get(3)?,
            duration: row.get::<_, Option<f64>>(4)?,
            width: row.get::<_, Option<i32>>(5)?,
            height: row.get::<_, Option<i32>>(6)?,
            fps: row.get::<_, Option<f64>>(7)?,
            codec: row.get::<_, Option<String>>(8)?,
            bitrate: row.get::<_, Option<i64>>(9)?,
            format: row.get::<_, Option<String>>(10)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(11)?)
                .unwrap()
                .with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                .unwrap()
                .with_timezone(&Utc),
            last_accessed: row.get::<_, Option<String>>(13)?.map(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .unwrap()
                    .with_timezone(&Utc)
            }),
        })
    })?;

    let mut videos = Vec::new();
    for video in video_iter {
        videos.push(video?);
    }

    Ok(videos)
}

/// 更新视频记录
pub fn update_video(conn: &Connection, video: &Video) -> AppResult<()> {
    conn.execute(
        r#"
        UPDATE videos SET
            file_path = ?1, file_name = ?2, file_size = ?3, duration = ?4,
            width = ?5, height = ?6, fps = ?7, codec = ?8, bitrate = ?9,
            format = ?10, updated_at = ?11, last_accessed = ?12
        WHERE id = ?13
        "#,
        params![
            &video.file_path,
            &video.file_name,
            video.file_size,
            video.duration,
            video.width,
            video.height,
            video.fps,
            &video.codec,
            video.bitrate,
            &video.format,
            video.updated_at.to_rfc3339(),
            video.last_accessed.map(|dt| dt.to_rfc3339()),
            video.id,
        ],
    )?;

    Ok(())
}

/// 删除视频记录
pub fn delete_video(conn: &Connection, id: i64) -> AppResult<()> {
    let mut stmt = conn.prepare("DELETE FROM videos WHERE id = ?1")?;
    stmt.execute([id])?;
    Ok(())
}
