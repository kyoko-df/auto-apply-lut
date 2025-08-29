//! 设置相关数据库查询

use crate::database::models::Setting;
use crate::types::AppResult;
use rusqlite::Connection;
use chrono::{DateTime, Utc};

/// 创建设置记录
pub fn create_setting(conn: &Connection, setting: &Setting) -> AppResult<i64> {
    let mut stmt = conn.prepare(
        r#"
        INSERT INTO settings (
            key, value, description, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#
    )?;
    
    stmt.execute([
        &setting.key,
        &setting.value,
        &setting.description.as_deref().unwrap_or("").to_string(),
        &setting.created_at.to_rfc3339(),
        &setting.updated_at.to_rfc3339(),
    ])?;
    
    Ok(conn.last_insert_rowid())
}

/// 根据键获取设置
pub fn get_setting_by_key(conn: &Connection, key: &str) -> AppResult<Option<Setting>> {
    let mut stmt = conn.prepare(
        "SELECT id, key, value, description, created_at, updated_at FROM settings WHERE key = ?1"
    )?;
    
    let setting_iter = stmt.query_map([key], |row| {
        Ok(Setting {
            id: row.get(0)?,
            key: row.get(1)?,
            value: row.get(2)?,
            description: row.get::<_, Option<String>>(3)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?).unwrap().with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?).unwrap().with_timezone(&Utc),
        })
    })?;
    
    for setting in setting_iter {
        return Ok(Some(setting?));
    }
    
    Ok(None)
}

/// 获取所有设置
pub fn get_all_settings(conn: &Connection) -> AppResult<Vec<Setting>> {
    let mut stmt = conn.prepare(
        "SELECT id, key, value, description, created_at, updated_at FROM settings ORDER BY key"
    )?;
    
    let setting_iter = stmt.query_map([], |row| {
        Ok(Setting {
            id: row.get(0)?,
            key: row.get(1)?,
            value: row.get(2)?,
            description: row.get::<_, Option<String>>(3)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?).unwrap().with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?).unwrap().with_timezone(&Utc),
        })
    })?;
    
    let mut settings = Vec::new();
    for setting in setting_iter {
        settings.push(setting?);
    }
    
    Ok(settings)
}

/// 更新设置
pub fn update_setting(conn: &Connection, setting: &Setting) -> AppResult<()> {
    let mut stmt = conn.prepare(
        r#"
        UPDATE settings SET
            value = ?1, description = ?2, updated_at = ?3
        WHERE key = ?4
        "#
    )?;
    
    stmt.execute([
        &setting.value,
        &setting.description.as_deref().unwrap_or("").to_string(),
        &setting.updated_at.to_rfc3339(),
        &setting.key,
    ])?;
    
    Ok(())
}

/// 设置或更新设置值
pub fn set_setting(conn: &Connection, key: &str, value: &str, description: Option<&str>) -> AppResult<()> {
    let now = Utc::now();
    
    // 尝试更新现有设置
    let mut stmt = conn.prepare(
        "UPDATE settings SET value = ?1, description = ?2, updated_at = ?3 WHERE key = ?4"
    )?;
    
    let rows_affected = stmt.execute([
        value,
        &description.unwrap_or("").to_string(),
        &now.to_rfc3339(),
        key,
    ])?;
    
    // 如果没有更新任何行，则插入新记录
    if rows_affected == 0 {
        let mut insert_stmt = conn.prepare(
            "INSERT INTO settings (key, value, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)"
        )?;
        
        insert_stmt.execute([
            key,
            value,
            &description.unwrap_or("").to_string(),
            &now.to_rfc3339(),
            &now.to_rfc3339(),
        ])?;
    }
    
    Ok(())
}

/// 删除设置
pub fn delete_setting(conn: &Connection, key: &str) -> AppResult<()> {
    let mut stmt = conn.prepare("DELETE FROM settings WHERE key = ?1")?;
    stmt.execute([key])?;
    Ok(())
}

/// 获取设置值（字符串）
pub fn get_setting_value(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    if let Some(setting) = get_setting_by_key(conn, key)? {
        Ok(Some(setting.value))
    } else {
        Ok(None)
    }
}

/// 获取设置值（布尔）
pub fn get_setting_bool(conn: &Connection, key: &str, default: bool) -> AppResult<bool> {
    if let Some(value) = get_setting_value(conn, key)? {
        Ok(value.parse().unwrap_or(default))
    } else {
        Ok(default)
    }
}

/// 获取设置值（整数）
pub fn get_setting_i64(conn: &Connection, key: &str, default: i64) -> AppResult<i64> {
    if let Some(value) = get_setting_value(conn, key)? {
        Ok(value.parse().unwrap_or(default))
    } else {
        Ok(default)
    }
}

/// 获取设置值（浮点数）
pub fn get_setting_f64(conn: &Connection, key: &str, default: f64) -> AppResult<f64> {
    if let Some(value) = get_setting_value(conn, key)? {
        Ok(value.parse().unwrap_or(default))
    } else {
        Ok(default)
    }
}