//! 批处理事件模块

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

/// 批处理事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchEventType {
    /// 批处理开始
    Started,
    /// 批处理进度更新
    Progress,
    /// 批处理完成
    Completed,
    /// 批处理失败
    Failed,
    /// 批处理暂停
    Paused,
    /// 批处理恢复
    Resumed,
    /// 批处理取消
    Cancelled,
}

/// 批处理状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchStatus {
    /// 等待中
    Pending,
    /// 运行中
    Running,
    /// 已完成
    Completed,
    /// 已失败
    Failed,
    /// 已暂停
    Paused,
    /// 已取消
    Cancelled,
}

/// 批处理项目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItem {
    /// 项目ID
    pub id: String,
    /// 输入文件路径
    pub input_path: String,
    /// 输出文件路径
    pub output_path: String,
    /// LUT文件路径
    pub lut_path: String,
    /// 项目状态
    pub status: BatchStatus,
    /// 进度 (0-100)
    pub progress: f64,
    /// 错误信息
    pub error: Option<String>,
}

/// 批处理事件数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchEvent {
    /// 事件类型
    pub event_type: BatchEventType,
    /// 批处理ID
    pub batch_id: String,
    /// 总项目数
    pub total_items: u32,
    /// 已完成项目数
    pub completed_items: u32,
    /// 失败项目数
    pub failed_items: u32,
    /// 整体进度 (0-100)
    pub overall_progress: f64,
    /// 当前处理的项目
    pub current_item: Option<BatchItem>,
    /// 事件消息
    pub message: String,
    /// 时间戳
    pub timestamp: i64,
}

impl BatchEvent {
    /// 创建新的批处理事件
    pub fn new(
        event_type: BatchEventType,
        batch_id: String,
        total_items: u32,
        message: String,
    ) -> Self {
        Self {
            event_type,
            batch_id,
            total_items,
            completed_items: 0,
            failed_items: 0,
            overall_progress: 0.0,
            current_item: None,
            message,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 设置进度信息
    pub fn with_progress(
        mut self,
        completed_items: u32,
        failed_items: u32,
        overall_progress: f64,
    ) -> Self {
        self.completed_items = completed_items;
        self.failed_items = failed_items;
        self.overall_progress = overall_progress;
        self
    }

    /// 设置当前项目
    pub fn with_current_item(mut self, current_item: BatchItem) -> Self {
        self.current_item = Some(current_item);
        self
    }
}

/// 发送批处理事件
pub fn emit_batch<R: tauri::Runtime>(app: &tauri::AppHandle<R>, event: BatchEvent) {
    if let Err(e) = app.emit("batch", &event) {
        eprintln!("Failed to emit batch event: {}", e);
    }
}
