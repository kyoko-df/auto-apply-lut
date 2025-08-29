//! GPU事件模块

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

/// GPU事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GpuEventType {
    /// GPU状态更新
    StatusUpdate,
    /// GPU使用率更新
    UsageUpdate,
    /// GPU错误
    Error,
    /// GPU加速启用
    AccelerationEnabled,
    /// GPU加速禁用
    AccelerationDisabled,
}

/// GPU状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GpuStatus {
    /// 可用
    Available,
    /// 不可用
    Unavailable,
    /// 使用中
    InUse,
    /// 错误
    Error,
}

/// GPU信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    /// GPU名称
    pub name: String,
    /// 驱动版本
    pub driver_version: Option<String>,
    /// 内存大小(MB)
    pub memory_size: Option<u64>,
    /// 是否支持硬件加速
    pub supports_acceleration: bool,
}

/// GPU事件数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuEvent {
    /// 事件类型
    pub event_type: GpuEventType,
    /// GPU状态
    pub status: Option<GpuStatus>,
    /// GPU信息
    pub gpu_info: Option<GpuInfo>,
    /// 使用率 (0-100)
    pub usage_percent: Option<f64>,
    /// 内存使用率 (0-100)
    pub memory_usage_percent: Option<f64>,
    /// 事件消息
    pub message: String,
    /// 时间戳
    pub timestamp: i64,
}

impl GpuEvent {
    /// 创建新的GPU事件
    pub fn new(event_type: GpuEventType, message: String) -> Self {
        Self {
            event_type,
            status: None,
            gpu_info: None,
            usage_percent: None,
            memory_usage_percent: None,
            message,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
    
    /// 设置GPU状态
    pub fn with_status(mut self, status: GpuStatus) -> Self {
        self.status = Some(status);
        self
    }
    
    /// 设置GPU信息
    pub fn with_gpu_info(mut self, gpu_info: GpuInfo) -> Self {
        self.gpu_info = Some(gpu_info);
        self
    }
    
    /// 设置使用率
    pub fn with_usage(mut self, usage_percent: f64, memory_usage_percent: f64) -> Self {
        self.usage_percent = Some(usage_percent);
        self.memory_usage_percent = Some(memory_usage_percent);
        self
    }
}

/// 发送GPU事件
pub fn emit_gpu<R: tauri::Runtime>(app: &tauri::AppHandle<R>, event: GpuEvent) {
    if let Err(e) = app.emit("gpu", &event) {
        eprintln!("Failed to emit GPU event: {}", e);
    }
}