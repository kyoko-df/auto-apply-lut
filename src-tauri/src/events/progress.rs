//! 进度事件模块

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

/// 进度事件数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// 任务ID
    pub task_id: String,
    /// 当前进度 (0-100)
    pub progress: f64,
    /// 进度描述
    pub message: String,
    /// 当前步骤
    pub current_step: Option<String>,
    /// 总步骤数
    pub total_steps: Option<u32>,
    /// 当前步骤索引
    pub current_step_index: Option<u32>,
}

impl ProgressEvent {
    /// 创建新的进度事件
    pub fn new(task_id: String, progress: f64, message: String) -> Self {
        Self {
            task_id,
            progress,
            message,
            current_step: None,
            total_steps: None,
            current_step_index: None,
        }
    }

    /// 设置步骤信息
    pub fn with_steps(mut self, current_step: String, current_index: u32, total: u32) -> Self {
        self.current_step = Some(current_step);
        self.current_step_index = Some(current_index);
        self.total_steps = Some(total);
        self
    }
}

/// 发送进度事件
pub fn emit_progress<R: tauri::Runtime>(app: &tauri::AppHandle<R>, event: ProgressEvent) {
    if let Err(e) = app.emit("progress", &event) {
        eprintln!("Failed to emit progress event: {}", e);
    }
}
