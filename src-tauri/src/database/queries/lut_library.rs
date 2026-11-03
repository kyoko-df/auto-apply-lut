//! User organization only: never mutate LUT files, processing settings or content timestamps.
use crate::database::models::LutCategory;
use crate::types::ui_err;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeSet;

fn name(value: &str) -> Result<String, String> {
    let name = value.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(ui_err(
            "lut_library.bad_name",
            "分类名称必须为 1–80 个字符，不能包含控制字符",
        ));
    }
    Ok(name.to_string())
}

fn check_name(conn: &Connection, name: &str, except: &str) -> Result<(), String> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM lut_categories WHERE name_key = ?1 AND id != ?2)",
            params![name.to_lowercase(), except],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists {
        return Err(ui_err(
            "lut_library.duplicate_name",
            "已有同名分类，请使用其他名称",
        ));
    }
    Ok(())
}

fn require_category(conn: &Connection, id: &str) -> Result<(), String> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM lut_categories WHERE id = ?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err(ui_err(
            "lut_library.missing_category",
            "分类已不存在，请刷新资料库后重试",
        ));
    }
    Ok(())
}

pub fn list_categories(conn: &Connection) -> Result<Vec<LutCategory>, String> {
    let mut statement = conn
        .prepare("SELECT id, name FROM lut_categories ORDER BY name_key, id")
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| {
            Ok(LutCategory {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn create_category(conn: &Connection, value: &str) -> Result<LutCategory, String> {
    let name = name(value)?;
    check_name(conn, &name, "")?;
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM lut_categories", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if count >= 100 {
        return Err(ui_err("lut_library.category_limit", "最多创建 100 个分类"));
    }
    let category = LutCategory {
        id: uuid::Uuid::new_v4().to_string(),
        name,
    };
    conn.execute(
        "INSERT INTO lut_categories (id, name, name_key) VALUES (?1, ?2, ?3)",
        params![category.id, category.name, category.name.to_lowercase()],
    )
    .map_err(|e| e.to_string())?;
    Ok(category)
}

pub fn rename_category(conn: &Connection, id: &str, value: &str) -> Result<LutCategory, String> {
    require_category(conn, id)?;
    let name = name(value)?;
    check_name(conn, &name, id)?;
    conn.execute(
        "UPDATE lut_categories SET name = ?1, name_key = ?2 WHERE id = ?3",
        params![name, name.to_lowercase(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(LutCategory {
        id: id.to_string(),
        name,
    })
}

pub fn delete_category(conn: &Connection, id: &str) -> Result<(), String> {
    require_category(conn, id)?;
    // The foreign key returns members to Uncategorized; it never deletes a LUT.
    conn.execute("DELETE FROM lut_categories WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn check_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() || path.chars().count() > 4096 || path.contains('\0') {
        return Err(ui_err("lut_library.bad_path", "LUT 资料库路径无效"));
    }
    Ok(())
}

pub fn set_favorite(conn: &Connection, path: &str, favorite: bool) -> Result<(), String> {
    check_path(path)?;
    if conn
        .execute(
            "UPDATE luts SET is_favorite = ?1 WHERE file_path = ?2",
            params![favorite, path],
        )
        .map_err(|e| e.to_string())?
        == 0
    {
        return Err(ui_err(
            "lut_library.missing_lut",
            "所选 LUT 已不在资料库中，请刷新后重试",
        ));
    }
    Ok(())
}

pub fn assign_category(
    conn: &mut Connection,
    paths: &[String],
    category: Option<&str>,
) -> Result<(), String> {
    if paths.is_empty() || paths.len() > 10_000 {
        return Err(ui_err(
            "lut_library.bad_selection",
            "请选择 1–10000 个 LUT 进行归类",
        ));
    }
    let transaction = conn.transaction().map_err(|e| e.to_string())?;
    if let Some(id) = category {
        require_category(&transaction, id)?;
    }
    let unique: BTreeSet<_> = paths.iter().collect();
    for path in &unique {
        check_path(path)?;
        let exists: Option<i64> = transaction
            .query_row("SELECT id FROM luts WHERE file_path = ?1", [path], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())?;
        if exists.is_none() {
            return Err(ui_err(
                "lut_library.missing_lut",
                "所选 LUT 已不在资料库中，请刷新后重试",
            ));
        }
    }
    for path in unique {
        transaction
            .execute(
                "UPDATE luts SET category_id = ?1 WHERE file_path = ?2",
                params![category, path],
            )
            .map_err(|e| e.to_string())?;
    }
    transaction.commit().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{queries::lut, DatabaseManager};

    fn add(conn: &Connection, path: &str) {
        conn.execute("INSERT INTO luts (file_path, file_name, file_size, lut_type, created_at, updated_at) VALUES (?1, ?1, 10, 'ThreeDimensional', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')", [path]).unwrap();
    }

    #[test]
    fn names_are_bounded_unique_and_renames_keep_category_identity() {
        let db = DatabaseManager::in_memory().unwrap();
        let connection = db.connection();
        let conn = connection.lock().unwrap();
        let first = create_category(&conn, "  Night  ").unwrap();
        assert_eq!(first.name, "Night");
        assert!(create_category(&conn, "night").is_err());
        for invalid in [
            " ".to_string(),
            "x".repeat(81),
            "a\nb".to_string(),
            "a\0b".to_string(),
        ] {
            assert!(create_category(&conn, &invalid).is_err());
        }
        let unicode = create_category(&conn, &"🎞".repeat(80)).unwrap();
        assert!(rename_category(&conn, &unicode.id, "NIGHT").is_err());
        let renamed = rename_category(&conn, &first.id, " 日常's / 分类 ").unwrap();
        assert_eq!(renamed.id, first.id);
        assert_eq!(renamed.name, "日常's / 分类");
        assert!(rename_category(&conn, "missing", "missing").is_err());
        for i in 0..98 {
            create_category(&conn, &format!("category-{i}")).unwrap();
        }
        assert_eq!(list_categories(&conn).unwrap().len(), 100);
        assert!(create_category(&conn, "one too many").is_err());
    }

    #[test]
    fn batch_assignment_is_atomic_for_missing_members_and_mid_write_failures() {
        let db = DatabaseManager::in_memory().unwrap();
        let connection = db.connection();
        let mut conn = connection.lock().unwrap();
        add(&conn, "/a.cube");
        add(&conn, "/b.cube");
        let first = create_category(&conn, "first").unwrap();
        let second = create_category(&conn, "second").unwrap();
        let paths = vec!["/a.cube".into(), "/b.cube".into()];
        assign_category(&mut conn, &paths, Some(&first.id)).unwrap();
        set_favorite(&conn, "/a.cube", true).unwrap();
        assert!(assign_category(
            &mut conn,
            &[paths[0].clone(), "/missing.cube".into()],
            Some(&second.id)
        )
        .is_err());
        assert!(assign_category(&mut conn, &paths, Some("missing")).is_err());
        conn.execute_batch("CREATE TRIGGER fail_assignment BEFORE UPDATE OF category_id ON luts WHEN NEW.file_path = '/b.cube' BEGIN SELECT RAISE(ABORT, 'simulated write failure'); END;").unwrap();
        assert!(assign_category(&mut conn, &paths, Some(&second.id)).is_err());
        for record in lut::get_all_luts(&conn).unwrap() {
            assert_eq!(record.category_id.as_deref(), Some(first.id.as_str()));
        }
        conn.execute("DROP TRIGGER fail_assignment", []).unwrap();
        delete_category(&conn, &first.id).unwrap();
        let a = lut::get_lut_by_path(&conn, "/a.cube").unwrap().unwrap();
        assert!(a.is_favorite);
        assert!(a.category_id.is_none());
        assert_eq!(a.updated_at.to_rfc3339(), "2026-01-01T00:00:00+00:00");
        assert_eq!(lut::get_all_luts(&conn).unwrap().len(), 2);
        assert!(assign_category(&mut conn, &[], None).is_err());
        assert!(assign_category(&mut conn, &vec![paths[0].clone(); 10_001], None).is_err());
        assert!(set_favorite(&conn, "/missing.cube", true).is_err());
    }

    #[test]
    fn failed_database_writes_leave_existing_organization_intact() {
        let db = DatabaseManager::in_memory().unwrap();
        let connection = db.connection();
        let mut conn = connection.lock().unwrap();
        add(&conn, "/a.cube");
        let category = create_category(&conn, "Keep").unwrap();
        assign_category(&mut conn, &["/a.cube".into()], Some(&category.id)).unwrap();
        conn.execute("PRAGMA query_only = ON", []).unwrap();
        assert!(set_favorite(&conn, "/a.cube", true).is_err());
        assert!(rename_category(&conn, &category.id, "Lost").is_err());
        assert!(create_category(&conn, "Lost").is_err());
        assert!(delete_category(&conn, &category.id).is_err());
        assert!(assign_category(&mut conn, &["/a.cube".into()], None).is_err());
        assert_eq!(list_categories(&conn).unwrap(), vec![category.clone()]);
        let record = lut::get_lut_by_path(&conn, "/a.cube").unwrap().unwrap();
        assert!(!record.is_favorite);
        assert_eq!(record.category_id, Some(category.id));
    }
}
