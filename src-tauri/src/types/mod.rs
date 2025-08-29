//! 类型定义模块
//! 定义项目中使用的数据结构和类型

use serde::{Deserialize, Serialize};

// 数据结构和类型定义
pub mod video;
pub mod lut;
pub mod task;
pub mod error;
pub mod gpu;
pub mod batch;
pub mod system;

// 重新导出常用类型
pub use error::{AppError, AppResult};
pub use video::{VideoInfo, VideoFormat, VideoProcessOptions};
pub use lut::{LutInfo, LutType, LutFormat, LutApplyOptions, LutValidationResult, LutSizeInfo};
pub use task::{TaskInfo, TaskStatus, TaskType, TaskProgress};
pub use gpu::{GpuInfo, GpuAcceleration, GpuPerformanceConfig};
pub use batch::{BatchTask, BatchStatus, BatchConfig};
pub use system::{SystemInfo, SystemRequirements, CompatibilityCheck};

/// 通用响应结果类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
            error: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            data: None,
            message: None,
            error: Some(error),
        }
    }

    pub fn message(message: String) -> Self {
        Self {
            success: true,
            data: None,
            message: Some(message),
            error: None,
        }
    }
}