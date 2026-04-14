//! 事件系统模块
//! 处理应用程序内的事件通信

use serde::{Deserialize, Serialize};

pub mod batch;
pub mod error;
pub mod gpu;
pub mod progress;
pub mod system;

/// 事件类型枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AppEvent {
    /// 处理进度事件
    Progress {
        task_id: String,
        progress: f64,
        message: String,
    },
    /// 错误事件
    Error {
        task_id: Option<String>,
        error: String,
        details: Option<String>,
    },
    /// 任务完成事件
    TaskCompleted { task_id: String, result: String },
    /// 系统状态事件
    SystemStatus {
        cpu_usage: f64,
        memory_usage: f64,
        gpu_usage: Option<f64>,
    },
    /// GPU状态事件
    GpuStatus {
        available: bool,
        device_name: Option<String>,
        memory_usage: Option<f64>,
    },
    /// 批处理状态事件
    BatchStatus {
        batch_id: String,
        completed: usize,
        total: usize,
        current_file: Option<String>,
    },
}
