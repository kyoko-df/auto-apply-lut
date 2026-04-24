//! 类型定义模块
//! 定义项目中使用的数据结构和类型

use serde::{Deserialize, Serialize};

// 数据结构和类型定义
pub mod batch;
pub mod error;
pub mod gpu;
pub mod lut;
pub mod lut_conversion;
pub mod system;
pub mod task;
pub mod video;

// 重新导出常用类型
pub use batch::{BatchConfig, BatchStatus, BatchTask};
pub use error::{AppError, AppResult};
pub use gpu::{GpuAcceleration, GpuInfo, GpuPerformanceConfig};
pub use lut::{LutApplyOptions, LutFormat, LutInfo, LutSizeInfo, LutType, LutValidationResult};
pub use lut_conversion::{
    BatchConvertLutItemResult, BatchConvertLutsRequest, BatchConvertLutsResponse,
};
pub use system::{CompatibilityCheck, SystemInfo, SystemRequirements};
pub use task::{TaskInfo, TaskProgress, TaskStatus, TaskType};
pub use video::{VideoFormat, VideoInfo, VideoProcessOptions};

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
