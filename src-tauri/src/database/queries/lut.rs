//! LUT相关数据库查询

use crate::database::models::Lut;
use crate::types::AppResult;
use rusqlite::Connection;
use chrono::{DateTime, Utc};

/// 创建LUT记录
pub fn create_lut(conn: &Connection, lut: &Lut) -> AppResult<i64> {
    let mut stmt = conn.prepare(
        r#"
        INSERT INTO luts (
            file_path, file_name, file_size, lut_type, format, description,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#
    )?;
    
    stmt.execute([
        &lut.file_path,
        &lut.file_name,
        &lut.file_size.to_string(),
        &lut.lut_type,
        &lut.format.as_deref().unwrap_or("").to_string(),
        &lut.description.as_deref().unwrap_or("").to_string(),
        &lut.created_at.to_rfc3339(),
        &lut.updated_at.to_rfc3339(),
    ])?;
    
    Ok(conn.last_insert_rowid())
}

/// 根据ID获取LUT
pub fn get_lut_by_id(conn: &Connection, id: i64) -> AppResult<Option<Lut>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_path, file_name, file_size, lut_type, format, description, created_at, updated_at, last_accessed FROM luts WHERE id = ?1"
    )?;
    
    let lut_iter = stmt.query_map([id], |row| {
        Ok(Lut {
            id: row.get(0)?,
            file_path: row.get(1)?,
            file_name: row.get(2)?,
            file_size: row.get(3)?,
            lut_type: row.get(4)?,
            format: row.get::<_, Option<String>>(5)?,
            description: row.get::<_, Option<String>>(6)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?).unwrap().with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?).unwrap().with_timezone(&Utc),
            last_accessed: row.get::<_, Option<String>>(9)?.map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
        })
    })?;
    
    for lut in lut_iter {
        return Ok(Some(lut?));
    }
    
    Ok(None)
}

/// 根据文件路径获取LUT
pub fn get_lut_by_path(conn: &Connection, path: &str) -> AppResult<Option<Lut>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_path, file_name, file_size, lut_type, format, description, created_at, updated_at, last_accessed FROM luts WHERE file_path = ?1"
    )?;
    
    let lut_iter = stmt.query_map([path], |row| {
        Ok(Lut {
            id: row.get(0)?,
            file_path: row.get(1)?,
            file_name: row.get(2)?,
            file_size: row.get(3)?,
            lut_type: row.get(4)?,
            format: row.get::<_, Option<String>>(5)?,
            description: row.get::<_, Option<String>>(6)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?).unwrap().with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?).unwrap().with_timezone(&Utc),
            last_accessed: row.get::<_, Option<String>>(9)?.map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
        })
    })?;
    
    for lut in lut_iter {
        return Ok(Some(lut?));
    }
    
    Ok(None)
}

/// 获取所有LUT
pub fn get_all_luts(conn: &Connection) -> AppResult<Vec<Lut>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_path, file_name, file_size, lut_type, format, description, created_at, updated_at, last_accessed FROM luts ORDER BY created_at DESC"
    )?;
    
    let lut_iter = stmt.query_map([], |row| {
        Ok(Lut {
            id: row.get(0)?,
            file_path: row.get(1)?,
            file_name: row.get(2)?,
            file_size: row.get(3)?,
            lut_type: row.get(4)?,
            format: row.get::<_, Option<String>>(5)?,
            description: row.get::<_, Option<String>>(6)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?).unwrap().with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?).unwrap().with_timezone(&Utc),
            last_accessed: row.get::<_, Option<String>>(9)?.map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
        })
    })?;
    
    let mut luts = Vec::new();
    for lut in lut_iter {
        luts.push(lut?);
    }
    
    Ok(luts)
}

/// 根据类型获取LUT
pub fn get_luts_by_type(conn: &Connection, lut_type: &str) -> AppResult<Vec<Lut>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_path, file_name, file_size, lut_type, format, description, created_at, updated_at, last_accessed FROM luts WHERE lut_type = ?1 ORDER BY created_at DESC"
    )?;
    
    let lut_iter = stmt.query_map([lut_type], |row| {
        Ok(Lut {
            id: row.get(0)?,
            file_path: row.get(1)?,
            file_name: row.get(2)?,
            file_size: row.get(3)?,
            lut_type: row.get(4)?,
            format: row.get::<_, Option<String>>(5)?,
            description: row.get::<_, Option<String>>(6)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?).unwrap().with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?).unwrap().with_timezone(&Utc),
            last_accessed: row.get::<_, Option<String>>(9)?.map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
        })
    })?;
    
    let mut luts = Vec::new();
    for lut in lut_iter {
        luts.push(lut?);
    }
    
    Ok(luts)
}

/// 更新LUT记录
pub fn update_lut(conn: &Connection, lut: &Lut) -> AppResult<()> {
    let mut stmt = conn.prepare(
        r#"
        UPDATE luts SET
            file_path = ?1, file_name = ?2, file_size = ?3, lut_type = ?4,
            format = ?5, description = ?6, updated_at = ?7
        WHERE id = ?8
        "#
    )?;
    
    stmt.execute([
        &lut.file_path,
        &lut.file_name,
        &lut.file_size.to_string(),
        &lut.lut_type,
        &lut.format.as_deref().unwrap_or("").to_string(),
        &lut.description.as_deref().unwrap_or("").to_string(),
        &lut.updated_at.to_rfc3339(),
        &lut.id.to_string(),
    ])?;
    
    Ok(())
}

/// 删除LUT记录
pub fn delete_lut(conn: &Connection, id: i64) -> AppResult<()> {
    let mut stmt = conn.prepare("DELETE FROM luts WHERE id = ?1")?;
    stmt.execute([id])?;
    Ok(())
}