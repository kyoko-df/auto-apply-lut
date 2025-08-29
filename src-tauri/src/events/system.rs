//! 系统事件模块

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

/// 系统事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemEventType {
    /// 应用启动
    AppStarted,
    /// 应用关闭
    AppShutdown,
    /// 系统资源更新
    ResourceUpdate,
    /// 配置更新
    ConfigUpdate,
    /// 缓存清理
    CacheCleaned,
}

/// 系统事件数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    /// 事件类型
    pub event_type: SystemEventType,
    /// 事件消息
    pub message: String,
    /// 附加数据
    pub data: Option<serde_json::Value>,
    /// 时间戳
    pub timestamp: i64,
}

impl SystemEvent {
    /// 创建新的系统事件
    pub fn new(event_type: SystemEventType, message: String) -> Self {
        Self {
            event_type,
            message,
            data: None,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
    
    /// 设置附加数据
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// 发送系统事件
pub fn emit_system<R: tauri::Runtime>(app: &tauri::AppHandle<R>, event: SystemEvent) {
    if let Err(e) = app.emit("system", &event) {
        eprintln!("Failed to emit system event: {}", e);
    }
}

/// 发送应用启动事件
pub fn emit_app_started<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let event = SystemEvent::new(
        SystemEventType::AppStarted,
        "应用已启动".to_string(),
    );
    emit_system(app, event);
}

/// 发送应用关闭事件
pub fn emit_app_shutdown<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let event = SystemEvent::new(
        SystemEventType::AppShutdown,
        "应用正在关闭".to_string(),
    );
    emit_system(app, event);
}