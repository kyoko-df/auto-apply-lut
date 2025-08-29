//! 错误类型定义

use serde::{Deserialize, Serialize};
use std::fmt;

/// 应用程序错误类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppError {
    /// 文件系统错误
    FileSystem(String),
    /// 数据库错误
    Database(String),
    /// FFmpeg处理错误
    FFmpeg(String),
    /// LUT处理错误
    LutProcessing(String),
    /// GPU相关错误
    Gpu(String),
    /// 配置错误
    Config(String),
    /// 配置错误（别名）
    Configuration(String),
    /// 网络错误
    Network(String),
    /// 验证错误
    Validation(String),
    /// IO错误
    Io(String),
    /// 序列化错误
    Serialization(String),
    /// 解析错误
    Parse(String),
    /// 无效输入
    InvalidInput(String),
    /// 未找到
    NotFound(String),
    /// 内部错误
    Internal(String),
    /// 超时错误
    Timeout(String),
    /// 未知错误
    Unknown(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::FileSystem(msg) => write!(f, "文件系统错误: {}", msg),
            AppError::Database(msg) => write!(f, "数据库错误: {}", msg),
            AppError::FFmpeg(msg) => write!(f, "FFmpeg处理错误: {}", msg),
            AppError::LutProcessing(msg) => write!(f, "LUT处理错误: {}", msg),
            AppError::Gpu(msg) => write!(f, "GPU错误: {}", msg),
            AppError::Config(msg) => write!(f, "配置错误: {}", msg),
            AppError::Configuration(msg) => write!(f, "配置错误: {}", msg),
            AppError::Timeout(msg) => write!(f, "超时错误: {}", msg),
            AppError::Network(msg) => write!(f, "网络错误: {}", msg),
            AppError::Validation(msg) => write!(f, "验证错误: {}", msg),
            AppError::Io(msg) => write!(f, "IO错误: {}", msg),
            AppError::Serialization(msg) => write!(f, "序列化错误: {}", msg),
            AppError::Parse(msg) => write!(f, "解析错误: {}", msg),
            AppError::InvalidInput(msg) => write!(f, "无效输入错误: {}", msg),
            AppError::NotFound(msg) => write!(f, "未找到: {}", msg),
            AppError::Internal(msg) => write!(f, "内部错误: {}", msg),
            AppError::Unknown(msg) => write!(f, "未知错误: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

// 从标准库错误类型转换
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Serialization(err.to_string())
    }
}

/// 应用程序结果类型
pub type AppResult<T> = Result<T, AppError>;