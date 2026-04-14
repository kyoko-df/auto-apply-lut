//! 错误事件模块

use crate::types::error::AppError;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

/// 错误事件数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    /// 错误ID
    pub error_id: String,
    /// 错误类型
    pub error_type: String,
    /// 错误消息
    pub message: String,
    /// 详细信息
    pub details: Option<String>,
    /// 相关任务ID
    pub task_id: Option<String>,
    /// 时间戳
    pub timestamp: i64,
}

impl ErrorEvent {
    /// 从AppError创建错误事件
    pub fn from_app_error(error: &AppError, task_id: Option<String>) -> Self {
        let error_type = match error {
            AppError::FileSystem(_) => "FileSystem",
            AppError::Database(_) => "Database",
            AppError::FFmpeg(_) => "FFmpeg",
            AppError::LutProcessing(_) => "LutProcessing",
            AppError::Gpu(_) => "Gpu",
            AppError::Config(_) => "Config",
            AppError::Configuration(_) => "Configuration",
            AppError::Timeout(_) => "Timeout",
            AppError::Network(_) => "Network",
            AppError::Validation(_) => "Validation",
            AppError::Io(_) => "IO",
            AppError::Serialization(_) => "Serialization",
            AppError::Parse(_) => "Parse",
            AppError::InvalidInput(_) => "InvalidInput",
            AppError::NotFound(_) => "NotFound",
            AppError::Internal(_) => "Internal",
            AppError::Unknown(_) => "Unknown",
        }
        .to_string();

        Self {
            error_id: uuid::Uuid::new_v4().to_string(),
            error_type,
            message: error.to_string(),
            details: None,
            task_id,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 创建新的错误事件
    pub fn new(error_type: String, message: String) -> Self {
        Self {
            error_id: uuid::Uuid::new_v4().to_string(),
            error_type,
            message,
            details: None,
            task_id: None,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 设置详细信息
    pub fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }

    /// 设置任务ID
    pub fn with_task_id(mut self, task_id: String) -> Self {
        self.task_id = Some(task_id);
        self
    }
}

/// 发送错误事件
pub fn emit_error<R: tauri::Runtime>(app: &tauri::AppHandle<R>, event: ErrorEvent) {
    if let Err(e) = app.emit("error", &event) {
        eprintln!("Failed to emit error event: {}", e);
    }
}
