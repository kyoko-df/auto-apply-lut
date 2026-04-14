//! 数据库模型定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 视频文件模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Video {
    pub id: i64,
    pub file_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub duration: Option<f64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub fps: Option<f64>,
    pub codec: Option<String>,
    pub bitrate: Option<i64>,
    pub format: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_accessed: Option<DateTime<Utc>>,
}

/// LUT文件模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lut {
    pub id: i64,
    pub file_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub lut_type: String,
    pub format: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_accessed: Option<DateTime<Utc>>,
}

/// 任务模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub task_type: String,
    pub status: String,
    pub priority: String,
    pub progress: f64,
    pub config: Option<String>,
    pub result: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub estimated_duration: Option<i64>,
    pub actual_duration: Option<i64>,
}

/// 设置模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub id: i64,
    pub key: String,
    pub value: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 任务状态枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl ToString for TaskStatus {
    fn to_string(&self) -> String {
        match self {
            TaskStatus::Pending => "pending".to_string(),
            TaskStatus::Running => "running".to_string(),
            TaskStatus::Completed => "completed".to_string(),
            TaskStatus::Failed => "failed".to_string(),
            TaskStatus::Cancelled => "cancelled".to_string(),
        }
    }
}

/// 任务优先级枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Critical,
}

impl ToString for TaskPriority {
    fn to_string(&self) -> String {
        match self {
            TaskPriority::Low => "low".to_string(),
            TaskPriority::Medium => "medium".to_string(),
            TaskPriority::High => "high".to_string(),
            TaskPriority::Critical => "critical".to_string(),
        }
    }
}
