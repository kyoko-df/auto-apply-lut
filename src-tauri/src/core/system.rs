//! 系统信息模块
//! 提供系统资源监控和信息查询功能

use crate::types::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sysinfo::System;

/// 系统信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub kernel_version: String,
    pub total_memory: u64,
    pub available_memory: u64,
    pub cpu_count: usize,
    pub cpu_usage: f32,
}

/// 系统管理器
pub struct SystemManager {
    system: System,
}

impl SystemManager {
    /// 创建新的系统管理器
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
        }
    }

    /// 获取系统信息
    pub fn get_system_info(&mut self) -> AppResult<SystemInfo> {
        self.system.refresh_all();

        Ok(SystemInfo {
            os: System::name().unwrap_or_else(|| "Unknown".to_string()),
            kernel_version: System::kernel_version().unwrap_or_else(|| "Unknown".to_string()),
            total_memory: self.system.total_memory(),
            available_memory: self.system.available_memory(),
            cpu_count: self.system.cpus().len(),
            cpu_usage: self.system.global_cpu_usage(),
        })
    }

    /// 获取内存使用率
    pub fn get_memory_usage(&mut self) -> f32 {
        self.system.refresh_memory();
        let total = self.system.total_memory();
        let used = total - self.system.available_memory();
        if total > 0 {
            (used as f32 / total as f32) * 100.0
        } else {
            0.0
        }
    }

    /// 获取CPU使用率
    pub fn get_cpu_usage(&mut self) -> f32 {
        self.system.refresh_cpu_usage();
        self.system.global_cpu_usage()
    }
}

impl Default for SystemManager {
    fn default() -> Self {
        Self::new()
    }
}
