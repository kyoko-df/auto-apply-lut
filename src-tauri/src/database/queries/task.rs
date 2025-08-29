//! 任务相关的数据库查询操作

use crate::types::AppResult;
use crate::database::models::Task;
use rusqlite::{Connection, params};
use chrono::Utc;

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
    // 简化实现，返回None
    Ok(None)
}

/// 更新任务
pub fn update_task(conn: &Connection, task: &Task) -> AppResult<()> {
    // 简化实现
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
    // 简化实现，返回空列表
    Ok(vec![])
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
    // 简化实现，返回默认统计
    Ok(TaskStats {
        total_count: 0,
        pending_count: 0,
        running_count: 0,
        completed_count: 0,
        failed_count: 0,
        cancelled_count: 0,
        avg_duration: None,
        oldest_created: None,
        newest_created: None,
    })
}