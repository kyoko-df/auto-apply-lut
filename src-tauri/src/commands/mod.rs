//! Tauri命令模块
//! 处理前端与后端的通信接口

pub mod batch_manager;
pub(crate) mod encoding_options;
pub mod file_manager;
pub mod gpu_manager;
pub mod lut_manager;
pub mod processor;
pub mod system_manager;

// 重新导出命令函数
pub use system_manager::get_available_codecs;

#[cfg(test)]
mod command_interfaces_tests;
