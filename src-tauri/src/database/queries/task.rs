//! 任务相关的数据库查询操作

use crate::types::{AppError, AppResult};
use crate::database::models::Task;
use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension, Row};

/// 插入任务记录
pub fn insert_task(conn: &Connection, task: &Task) -> AppResult<String> {
    conn.execute(
        r#"
        INSERT INTO tasks (
            id, name, description, task_type, status, priority, progress,
            config, result, error_message, created_at, started_at, completed_at,
            estimated_duration, actual_duration
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        "#,
        params![
            &task.id,
            &task.name,
            &task.description,
            &task.task_type,
            &task.status,
            &task.priority,
            task.progress,
            &task.config,
            &task.result,
            &task.error_message,
            task.created_at.timestamp(),
            task.started_at.map(|t| t.timestamp()),
            task.completed_at.map(|t| t.timestamp()),
            task.estimated_duration,
            task.actual_duration
        ]
    )?;
    Ok(task.id.clone())
}

/// 根据ID获取任务
pub fn get_task_by_id(conn: &Connection, id: &str) -> AppResult<Option<Task>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, name, description, task_type, status, priority, progress,
               config, result, error_message, created_at, started_at, completed_at,
               estimated_duration, actual_duration
        FROM tasks
        WHERE id = ?1
        "#
    )?;

    let task = stmt
        .query_row(params![id], map_task_row)
        .optional()?;
    Ok(task)
}

/// 更新任务
pub fn update_task(conn: &Connection, task: &Task) -> AppResult<()> {
    conn.execute(
        r#"
        UPDATE tasks
        SET name = ?1,
            description = ?2,
            task_type = ?3,
            status = ?4,
            priority = ?5,
            progress = ?6,
            config = ?7,
            result = ?8,
            error_message = ?9,
            created_at = ?10,
            started_at = ?11,
            completed_at = ?12,
            estimated_duration = ?13,
            actual_duration = ?14
        WHERE id = ?15
        "#,
        params![
            &task.name,
            &task.description,
            &task.task_type,
            &task.status,
            &task.priority,
            task.progress,
            &task.config,
            &task.result,
            &task.error_message,
            task.created_at.timestamp(),
            task.started_at.map(|t| t.timestamp()),
            task.completed_at.map(|t| t.timestamp()),
            task.estimated_duration,
            task.actual_duration,
            &task.id,
        ]
    )?;
    Ok(())
}

/// 删除任务
pub fn delete_task(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
    Ok(())
}

/// 列出任务
pub fn list_tasks(
    conn: &Connection,
    status: Option<&str>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<Task>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, name, description, task_type, status, priority, progress,
               config, result, error_message, created_at, started_at, completed_at,
               estimated_duration, actual_duration
        FROM tasks
        ORDER BY created_at DESC
        "#
    )?;

    let task_iter = stmt.query_map([], map_task_row)?;
    let mut tasks = Vec::new();
    for task in task_iter {
        tasks.push(task?);
    }

    if let Some(status) = status {
        tasks.retain(|task| task.status.eq_ignore_ascii_case(status));
    }

    let offset = offset.unwrap_or(0).max(0) as usize;
    if offset >= tasks.len() {
        return Ok(Vec::new());
    }

    let mut tasks: Vec<Task> = tasks.into_iter().skip(offset).collect();
    if let Some(limit) = limit.filter(|value| *value >= 0) {
        tasks.truncate(limit as usize);
    }

    Ok(tasks)
}

/// 更新任务状态
pub fn update_task_status(
    conn: &Connection,
    id: &str,
    status: &str,
    progress: Option<f64>,
) -> AppResult<()> {
    if let Some(progress) = progress {
        conn.execute(
            "UPDATE tasks SET status = ?1, progress = ?2 WHERE id = ?3",
            params![status, progress, id]
        )?;
    } else {
        conn.execute(
            "UPDATE tasks SET status = ?1 WHERE id = ?2",
            params![status, id]
        )?;
    }
    Ok(())
}

/// 更新任务进度
pub fn update_task_progress(conn: &Connection, id: &str, progress: f64) -> AppResult<()> {
    conn.execute(
        "UPDATE tasks SET progress = ?1 WHERE id = ?2",
        params![progress, id]
    )?;
    Ok(())
}

/// 设置任务错误
pub fn set_task_error(conn: &Connection, id: &str, error_message: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE tasks SET status = 'failed', error_message = ?1 WHERE id = ?2",
        params![error_message, id]
    )?;
    Ok(())
}

/// 任务统计信息
#[derive(Debug)]
pub struct TaskStats {
    pub total_count: i64,
    pub pending_count: i64,
    pub running_count: i64,
    pub completed_count: i64,
    pub failed_count: i64,
    pub cancelled_count: i64,
    pub avg_duration: Option<f64>,
    pub oldest_created: Option<chrono::DateTime<chrono::Utc>>,
    pub newest_created: Option<chrono::DateTime<chrono::Utc>>,
}

/// 获取任务统计
pub fn get_task_stats(conn: &Connection) -> AppResult<TaskStats> {
    let tasks = list_tasks(conn, None, None, None)?;
    let total_count = tasks.len() as i64;
    let pending_count = tasks
        .iter()
        .filter(|task| task.status.eq_ignore_ascii_case("pending"))
        .count() as i64;
    let running_count = tasks
        .iter()
        .filter(|task| task.status.eq_ignore_ascii_case("running"))
        .count() as i64;
    let completed_count = tasks
        .iter()
        .filter(|task| task.status.eq_ignore_ascii_case("completed"))
        .count() as i64;
    let failed_count = tasks
        .iter()
        .filter(|task| task.status.eq_ignore_ascii_case("failed"))
        .count() as i64;
    let cancelled_count = tasks
        .iter()
        .filter(|task| task.status.eq_ignore_ascii_case("cancelled"))
        .count() as i64;

    let durations: Vec<f64> = tasks
        .iter()
        .filter_map(|task| {
            task.actual_duration
                .map(|value| value as f64)
                .or_else(|| match (task.started_at, task.completed_at) {
                    (Some(started_at), Some(completed_at)) => {
                        Some((completed_at - started_at).num_seconds() as f64)
                    }
                    _ => None,
                })
        })
        .collect();

    let avg_duration = if durations.is_empty() {
        None
    } else {
        Some(durations.iter().sum::<f64>() / durations.len() as f64)
    };

    let oldest_created = tasks.iter().map(|task| task.created_at).min();
    let newest_created = tasks.iter().map(|task| task.created_at).max();

    Ok(TaskStats {
        total_count,
        pending_count,
        running_count,
        completed_count,
        failed_count,
        cancelled_count,
        avg_duration,
        oldest_created,
        newest_created,
    })
}

fn map_task_row(row: &Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        task_type: row.get(3)?,
        status: row.get(4)?,
        priority: row.get(5)?,
        progress: row.get(6)?,
        config: row.get(7)?,
        result: row.get(8)?,
        error_message: row.get(9)?,
        created_at: read_required_datetime(row, 10)?,
        started_at: read_optional_datetime(row, 11)?,
        completed_at: read_optional_datetime(row, 12)?,
        estimated_duration: row.get(13)?,
        actual_duration: row.get(14)?,
    })
}

fn read_required_datetime(row: &Row<'_>, index: usize) -> rusqlite::Result<DateTime<Utc>> {
    read_optional_datetime(row, index)?.ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Null,
            Box::new(AppError::Database("任务时间字段为空".to_string())),
        )
    })
}

fn read_optional_datetime(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<DateTime<Utc>>> {
    let value = row.get_ref(index)?;
    match value {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(ts) => DateTime::<Utc>::from_timestamp(ts, 0)
            .map(Some)
            .ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Integer,
                    Box::new(AppError::Database(format!("无效时间戳: {}", ts))),
                )
            }),
        ValueRef::Real(ts) => {
            let seconds = ts.trunc() as i64;
            DateTime::<Utc>::from_timestamp(seconds, 0)
                .map(Some)
                .ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        index,
                        rusqlite::types::Type::Real,
                        Box::new(AppError::Database(format!("无效时间戳: {}", ts))),
                    )
                })
        }
        ValueRef::Text(text) => parse_datetime_text(index, text),
        ValueRef::Blob(_) => Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Blob,
            Box::new(AppError::Database("不支持的时间字段格式".to_string())),
        )),
    }
}

fn parse_datetime_text(index: usize, text: &[u8]) -> rusqlite::Result<Option<DateTime<Utc>>> {
    let text = std::str::from_utf8(text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(error))
    })?;

    if text.trim().is_empty() {
        return Ok(None);
    }

    if let Ok(timestamp) = text.parse::<i64>() {
        return DateTime::<Utc>::from_timestamp(timestamp, 0)
            .map(Some)
            .ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Text,
                    Box::new(AppError::Database(format!("无效时间戳: {}", text))),
                )
            });
    }

    if let Ok(datetime) = DateTime::parse_from_rfc3339(text) {
        return Ok(Some(datetime.with_timezone(&Utc)));
    }

    if let Ok(naive) = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S") {
        return Ok(Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)));
    }

    Err(rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Text,
        Box::new(AppError::Database(format!("无法解析时间字段: {}", text))),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations::run_migrations;
    use chrono::Duration;

    fn sample_task(id: &str, status: &str, created_at: DateTime<Utc>) -> Task {
        Task {
            id: id.to_string(),
            name: format!("task-{id}"),
            description: Some("desc".to_string()),
            task_type: "video_processing".to_string(),
            status: status.to_string(),
            priority: "medium".to_string(),
            progress: 25.0,
            config: Some("{\"demo\":true}".to_string()),
            result: None,
            error_message: None,
            created_at,
            started_at: Some(created_at + Duration::seconds(5)),
            completed_at: Some(created_at + Duration::seconds(25)),
            estimated_duration: Some(30),
            actual_duration: Some(20),
        }
    }

    #[test]
    fn task_queries_roundtrip_and_stats() {
        let conn = Connection::open_in_memory().expect("open db");
        run_migrations(&conn).expect("run migrations");

        let first_created = Utc::now() - Duration::minutes(10);
        let second_created = Utc::now() - Duration::minutes(5);
        let third_created = Utc::now();

        let first = sample_task("task-1", "pending", first_created);
        let second = sample_task("task-2", "completed", second_created);
        let mut third = sample_task("task-3", "failed", third_created);
        third.actual_duration = Some(40);
        third.error_message = Some("boom".to_string());

        insert_task(&conn, &first).expect("insert first");
        insert_task(&conn, &second).expect("insert second");
        insert_task(&conn, &third).expect("insert third");

        let fetched = get_task_by_id(&conn, "task-2")
            .expect("get task by id")
            .expect("task exists");
        assert_eq!(fetched.status, "completed");
        assert_eq!(fetched.name, "task-task-2");

        let completed_only = list_tasks(&conn, Some("completed"), None, None).expect("list completed");
        assert_eq!(completed_only.len(), 1);
        assert_eq!(completed_only[0].id, "task-2");

        let paged = list_tasks(&conn, None, Some(1), Some(1)).expect("list paged");
        assert_eq!(paged.len(), 1);

        let mut updated = fetched.clone();
        updated.status = "cancelled".to_string();
        updated.progress = 100.0;
        update_task(&conn, &updated).expect("update task");

        let fetched_after_update = get_task_by_id(&conn, "task-2")
            .expect("get updated task")
            .expect("updated task exists");
        assert_eq!(fetched_after_update.status, "cancelled");
        assert_eq!(fetched_after_update.progress, 100.0);

        let stats = get_task_stats(&conn).expect("task stats");
        assert_eq!(stats.total_count, 3);
        assert_eq!(stats.pending_count, 1);
        assert_eq!(stats.failed_count, 1);
        assert_eq!(stats.cancelled_count, 1);
        assert_eq!(stats.completed_count, 0);
        assert!(stats.avg_duration.is_some());
        assert_eq!(
            stats.oldest_created.map(|value| value.timestamp()),
            Some(first_created.timestamp())
        );
        assert_eq!(
            stats.newest_created.map(|value| value.timestamp()),
            Some(third_created.timestamp())
        );
    }
}
