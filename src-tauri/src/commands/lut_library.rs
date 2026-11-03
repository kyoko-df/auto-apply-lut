//! Short, serialized database mutations; no engine discovery or filesystem writes.
use crate::database::{models::LutCategory, queries::lut_library as queries, DatabaseManager};
use serde::Deserialize;
use tauri::State;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategoryNameRequest {
    pub name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameCategoryRequest {
    pub id: String,
    pub name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FavoriteRequest {
    pub lut_path: String,
    pub is_favorite: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignCategoryRequest {
    pub lut_paths: Vec<String>,
    pub category_id: Option<String>,
}

#[tauri::command]
pub async fn list_lut_categories(
    db: State<'_, DatabaseManager>,
) -> Result<Vec<LutCategory>, String> {
    let connection = db.connection();
    let conn = connection.lock().map_err(|e| e.to_string())?;
    queries::list_categories(&conn)
}
#[tauri::command]
pub async fn create_lut_category(
    request: CategoryNameRequest,
    db: State<'_, DatabaseManager>,
) -> Result<LutCategory, String> {
    let connection = db.connection();
    let conn = connection.lock().map_err(|e| e.to_string())?;
    queries::create_category(&conn, &request.name)
}
#[tauri::command]
pub async fn rename_lut_category(
    request: RenameCategoryRequest,
    db: State<'_, DatabaseManager>,
) -> Result<LutCategory, String> {
    let connection = db.connection();
    let conn = connection.lock().map_err(|e| e.to_string())?;
    queries::rename_category(&conn, &request.id, &request.name)
}
#[tauri::command]
pub async fn delete_lut_category(
    category_id: String,
    db: State<'_, DatabaseManager>,
) -> Result<(), String> {
    let connection = db.connection();
    let conn = connection.lock().map_err(|e| e.to_string())?;
    queries::delete_category(&conn, &category_id)
}
#[tauri::command]
pub async fn set_lut_favorite(
    request: FavoriteRequest,
    db: State<'_, DatabaseManager>,
) -> Result<(), String> {
    let connection = db.connection();
    let conn = connection.lock().map_err(|e| e.to_string())?;
    queries::set_favorite(&conn, &request.lut_path, request.is_favorite)
}
#[tauri::command]
pub async fn assign_lut_category(
    request: AssignCategoryRequest,
    db: State<'_, DatabaseManager>,
) -> Result<(), String> {
    let connection = db.connection();
    let mut conn = connection.lock().map_err(|e| e.to_string())?;
    queries::assign_category(
        &mut conn,
        &request.lut_paths,
        request.category_id.as_deref(),
    )
}
