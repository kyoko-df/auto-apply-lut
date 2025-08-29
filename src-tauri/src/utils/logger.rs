//! 日志管理模块

use crate::types::error::{AppError, AppResult};
use crate::utils::path_utils::get_app_data_dir;
use std::fs;
use std::path::PathBuf;

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "error" => Some(LogLevel::Error),
            "warn" => Some(LogLevel::Warn),
            "info" => Some(LogLevel::Info),
            "debug" => Some(LogLevel::Debug),
            "trace" => Some(LogLevel::Trace),
            _ => None,
        }
    }
}

/// 日志记录器
pub struct Logger {
    log_file_path: PathBuf,
    level: LogLevel,
}

impl Logger {
    /// 创建新的日志记录器
    pub fn new(level: LogLevel) -> AppResult<Self> {
        let mut log_dir = get_app_data_dir()?;
        log_dir.push("logs");
        
        // 确保日志目录存在
        if !log_dir.exists() {
            fs::create_dir_all(&log_dir)
                .map_err(|e| AppError::Io(format!("创建日志目录失败: {}", e)))?;
        }
        
        let log_file_path = log_dir.join("app.log");
        
        Ok(Self {
            log_file_path,
            level,
        })
    }
    
    /// 设置日志级别
    pub fn set_level(&mut self, level: LogLevel) {
        self.level = level;
    }
    
    /// 记录日志
    pub fn log(&self, level: LogLevel, message: &str) -> AppResult<()> {
        if !self.should_log(level) {
            return Ok(());
        }
        
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let log_entry = format!("[{}] [{}] {}\n", timestamp, level.as_str(), message);
        
        // 写入文件
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file_path)
            .and_then(|mut file| {
                use std::io::Write;
                file.write_all(log_entry.as_bytes())
            })
            .map_err(|e| AppError::Io(format!("写入日志失败: {}", e)))?;
        
        // 同时输出到控制台
        match level {
            LogLevel::Error => eprintln!("{}", log_entry.trim()),
            _ => println!("{}", log_entry.trim()),
        }
        
        Ok(())
    }
    
    /// 检查是否应该记录该级别的日志
    fn should_log(&self, level: LogLevel) -> bool {
        match self.level {
            LogLevel::Error => matches!(level, LogLevel::Error),
            LogLevel::Warn => matches!(level, LogLevel::Error | LogLevel::Warn),
            LogLevel::Info => matches!(level, LogLevel::Error | LogLevel::Warn | LogLevel::Info),
            LogLevel::Debug => matches!(level, LogLevel::Error | LogLevel::Warn | LogLevel::Info | LogLevel::Debug),
            LogLevel::Trace => true,
        }
    }
    
    /// 记录错误日志
    pub fn error(&self, message: &str) -> AppResult<()> {
        self.log(LogLevel::Error, message)
    }
    
    /// 记录警告日志
    pub fn warn(&self, message: &str) -> AppResult<()> {
        self.log(LogLevel::Warn, message)
    }
    
    /// 记录信息日志
    pub fn info(&self, message: &str) -> AppResult<()> {
        self.log(LogLevel::Info, message)
    }
    
    /// 记录调试日志
    pub fn debug(&self, message: &str) -> AppResult<()> {
        self.log(LogLevel::Debug, message)
    }
    
    /// 记录跟踪日志
    pub fn trace(&self, message: &str) -> AppResult<()> {
        self.log(LogLevel::Trace, message)
    }
    
    /// 清理旧日志文件
    pub fn cleanup_old_logs(&self, days: u64) -> AppResult<()> {
        let log_dir = self.log_file_path.parent()
            .ok_or_else(|| AppError::Internal("无法获取日志目录".to_string()))?;
        
        let cutoff_time = std::time::SystemTime::now() - std::time::Duration::from_secs(days * 24 * 60 * 60);
        
        for entry in fs::read_dir(log_dir)
            .map_err(|e| AppError::Io(format!("读取日志目录失败: {}", e)))? {
            let entry = entry.map_err(|e| AppError::Io(format!("读取目录项失败: {}", e)))?;
            let path = entry.path();
            
            if path.is_file() && path.extension().map_or(false, |ext| ext == "log") {
                if let Ok(metadata) = fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        if modified < cutoff_time {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// 获取日志文件路径
    pub fn get_log_file_path(&self) -> &PathBuf {
        &self.log_file_path
    }
}

/// 全局日志记录器实例
static mut GLOBAL_LOGGER: Option<Logger> = None;
static LOGGER_INIT: std::sync::Once = std::sync::Once::new();

/// 初始化全局日志记录器
pub fn init_logger(level: LogLevel) -> AppResult<()> {
    LOGGER_INIT.call_once(|| {
        unsafe {
            GLOBAL_LOGGER = Logger::new(level).ok();
        }
    });
    Ok(())
}

/// 获取全局日志记录器
fn get_global_logger() -> Option<&'static Logger> {
    unsafe { GLOBAL_LOGGER.as_ref() }
}

/// 记录错误日志
pub fn log_error(message: &str) {
    if let Some(logger) = get_global_logger() {
        let _ = logger.error(message);
    }
}

/// 记录警告日志
pub fn log_warn(message: &str) {
    if let Some(logger) = get_global_logger() {
        let _ = logger.warn(message);
    }
}

/// 记录信息日志
pub fn log_info(message: &str) {
    if let Some(logger) = get_global_logger() {
        let _ = logger.info(message);
    }
}

/// 记录调试日志
pub fn log_debug(message: &str) {
    if let Some(logger) = get_global_logger() {
        let _ = logger.debug(message);
    }
}

/// 记录跟踪日志
pub fn log_trace(message: &str) {
    if let Some(logger) = get_global_logger() {
        let _ = logger.trace(message);
    }
}

/// 日志宏
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::utils::logger::log_error(&format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::utils::logger::log_warn(&format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::utils::logger::log_info(&format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::utils::logger::log_debug(&format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        $crate::utils::logger::log_trace(&format!($($arg)*));
    };
}