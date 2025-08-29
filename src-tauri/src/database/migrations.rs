//! 数据库迁移模块

use crate::types::AppResult;
use rusqlite::Connection;

/// 运行数据库迁移
pub fn run_migrations(conn: &Connection) -> AppResult<()> {
    // 创建迁移表
    create_migration_table(conn)?;
    
    // 运行各个迁移
    if !is_migration_applied(conn, "create_videos_table")? {
        create_videos_table(conn)?;
        mark_migration_applied(conn, "create_videos_table")?;
    }
    
    if !is_migration_applied(conn, "create_luts_table")? {
        create_luts_table(conn)?;
        mark_migration_applied(conn, "create_luts_table")?;
    }
    
    if !is_migration_applied(conn, "create_tasks_table")? {
        create_tasks_table(conn)?;
        mark_migration_applied(conn, "create_tasks_table")?;
    }
    
    if !is_migration_applied(conn, "create_settings_table")? {
        create_settings_table(conn)?;
        mark_migration_applied(conn, "create_settings_table")?;
    }
    
    Ok(())
}

/// 创建迁移记录表
fn create_migration_table(conn: &Connection) -> AppResult<()> {
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS migrations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
        []
    )?;
    Ok(())
}

/// 检查迁移是否已应用
fn is_migration_applied(conn: &Connection, name: &str) -> AppResult<bool> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM migrations WHERE name = ?1")?;
    let count: i64 = stmt.query_row([name], |row| row.get(0))?;
    Ok(count > 0)
}

/// 标记迁移为已应用
fn mark_migration_applied(conn: &Connection, name: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO migrations (name) VALUES (?1)",
        [name]
    )?;
    Ok(())
}

/// 创建视频表
fn create_videos_table(conn: &Connection) -> AppResult<()> {
    conn.execute(
        r#"
        CREATE TABLE videos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_path TEXT NOT NULL UNIQUE,
            file_name TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            duration REAL,
            width INTEGER,
            height INTEGER,
            fps REAL,
            codec TEXT,
            bitrate INTEGER,
            format TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            last_accessed DATETIME
        )
        "#,
        []
    )?;
    Ok(())
}

/// 创建LUT表
fn create_luts_table(conn: &Connection) -> AppResult<()> {
    conn.execute(
        r#"
        CREATE TABLE luts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_path TEXT NOT NULL UNIQUE,
            file_name TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            lut_type TEXT NOT NULL,
            format TEXT,
            description TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            last_accessed DATETIME
        )
        "#,
        []
    )?;
    Ok(())
}

/// 创建任务表
fn create_tasks_table(conn: &Connection) -> AppResult<()> {
    conn.execute(
        r#"
        CREATE TABLE tasks (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            task_type TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            priority TEXT NOT NULL DEFAULT 'medium',
            progress REAL DEFAULT 0.0,
            config TEXT,
            result TEXT,
            error_message TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            started_at DATETIME,
            completed_at DATETIME,
            estimated_duration INTEGER,
            actual_duration INTEGER
        )
        "#,
        []
    )?;
    Ok(())
}

/// 创建设置表
fn create_settings_table(conn: &Connection) -> AppResult<()> {
    conn.execute(
        r#"
        CREATE TABLE settings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            key TEXT NOT NULL UNIQUE,
            value TEXT NOT NULL,
            description TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
        []
    )?;
    Ok(())
}