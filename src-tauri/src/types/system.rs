use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 系统信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// 操作系统信息
    pub os_info: OsInfo,
    /// CPU信息
    pub cpu_info: CpuInfo,
    /// 内存信息
    pub memory_info: MemoryInfo,
    /// 磁盘信息
    pub disk_info: Vec<DiskInfo>,
    /// GPU信息
    pub gpu_info: Vec<super::gpu::GpuInfo>,
    /// 网络信息
    pub network_info: NetworkInfo,
    /// 系统性能
    pub performance: SystemPerformance,
    /// 环境变量
    pub environment: HashMap<String, String>,
    /// 已安装的软件
    pub installed_software: Vec<SoftwareInfo>,
}

/// 操作系统信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    /// 操作系统名称
    pub name: String,
    /// 版本
    pub version: String,
    /// 架构
    pub arch: String,
    /// 内核版本
    pub kernel_version: String,
    /// 主机名
    pub hostname: String,
    /// 启动时间
    pub boot_time: DateTime<Utc>,
    /// 运行时间（秒）
    pub uptime: u64,
}

/// CPU信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    /// CPU品牌
    pub brand: String,
    /// 型号
    pub model: String,
    /// 核心数
    pub cores: usize,
    /// 线程数
    pub threads: usize,
    /// 基础频率（MHz）
    pub base_frequency: f64,
    /// 最大频率（MHz）
    pub max_frequency: Option<f64>,
    /// 缓存大小（KB）
    pub cache_size: Option<u64>,
    /// 支持的指令集
    pub features: Vec<String>,
    /// 当前使用率（0-100）
    pub usage: f32,
    /// 温度（摄氏度）
    pub temperature: Option<f32>,
}

/// 内存信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    /// 总内存（字节）
    pub total: u64,
    /// 可用内存（字节）
    pub available: u64,
    /// 已使用内存（字节）
    pub used: u64,
    /// 空闲内存（字节）
    pub free: u64,
    /// 缓存内存（字节）
    pub cached: Option<u64>,
    /// 缓冲区内存（字节）
    pub buffers: Option<u64>,
    /// 交换分区总大小（字节）
    pub swap_total: Option<u64>,
    /// 交换分区已使用（字节）
    pub swap_used: Option<u64>,
    /// 内存使用率（0-100）
    pub usage_percentage: f32,
}

/// 磁盘信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    /// 设备名称
    pub device: String,
    /// 挂载点
    pub mount_point: String,
    /// 文件系统类型
    pub file_system: String,
    /// 总容量（字节）
    pub total_space: u64,
    /// 可用空间（字节）
    pub available_space: u64,
    /// 已使用空间（字节）
    pub used_space: u64,
    /// 使用率（0-100）
    pub usage_percentage: f32,
    /// 是否为可移动设备
    pub is_removable: bool,
    /// 磁盘类型
    pub disk_type: DiskType,
    /// 读写速度
    pub io_stats: Option<DiskIoStats>,
}

/// 磁盘类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DiskType {
    /// 固态硬盘
    Ssd,
    /// 机械硬盘
    Hdd,
    /// 网络存储
    Network,
    /// 虚拟磁盘
    Virtual,
    /// 未知类型
    Unknown,
}

/// 磁盘IO统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskIoStats {
    /// 读取速度（字节/秒）
    pub read_speed: u64,
    /// 写入速度（字节/秒）
    pub write_speed: u64,
    /// 读取IOPS
    pub read_iops: u64,
    /// 写入IOPS
    pub write_iops: u64,
    /// 平均响应时间（毫秒）
    pub avg_response_time: f64,
}

/// 网络信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    /// 网络接口列表
    pub interfaces: Vec<NetworkInterface>,
    /// 总下载速度（字节/秒）
    pub total_download_speed: u64,
    /// 总上传速度（字节/秒）
    pub total_upload_speed: u64,
    /// 网络连接状态
    pub connectivity: NetworkConnectivity,
}

/// 网络接口
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    /// 接口名称
    pub name: String,
    /// 显示名称
    pub display_name: Option<String>,
    /// MAC地址
    pub mac_address: Option<String>,
    /// IP地址列表
    pub ip_addresses: Vec<String>,
    /// 接口类型
    pub interface_type: NetworkInterfaceType,
    /// 是否启用
    pub is_up: bool,
    /// 传输速度（Mbps）
    pub speed: Option<u64>,
    /// 网络统计
    pub stats: NetworkStats,
}

/// 网络接口类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkInterfaceType {
    /// 以太网
    Ethernet,
    /// WiFi
    Wifi,
    /// 蓝牙
    Bluetooth,
    /// 虚拟接口
    Virtual,
    /// 回环接口
    Loopback,
    /// 其他
    Other,
}

/// 网络统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    /// 接收字节数
    pub bytes_received: u64,
    /// 发送字节数
    pub bytes_sent: u64,
    /// 接收包数
    pub packets_received: u64,
    /// 发送包数
    pub packets_sent: u64,
    /// 接收错误数
    pub errors_received: u64,
    /// 发送错误数
    pub errors_sent: u64,
    /// 丢包数
    pub packets_dropped: u64,
}

/// 网络连接状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkConnectivity {
    /// 已连接
    Connected,
    /// 有限连接
    Limited,
    /// 未连接
    Disconnected,
    /// 未知状态
    Unknown,
}

/// 系统性能
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPerformance {
    /// CPU使用率（0-100）
    pub cpu_usage: f32,
    /// 内存使用率（0-100）
    pub memory_usage: f32,
    /// 磁盘使用率（0-100）
    pub disk_usage: f32,
    /// 网络使用率（0-100）
    pub network_usage: f32,
    /// 系统负载
    pub load_average: Option<LoadAverage>,
    /// 进程数
    pub process_count: usize,
    /// 线程数
    pub thread_count: usize,
    /// 文件描述符使用数
    pub file_descriptor_count: Option<usize>,
}

/// 系统负载
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAverage {
    /// 1分钟负载
    pub one_minute: f64,
    /// 5分钟负载
    pub five_minute: f64,
    /// 15分钟负载
    pub fifteen_minute: f64,
}

/// 软件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareInfo {
    /// 软件名称
    pub name: String,
    /// 版本
    pub version: String,
    /// 安装路径
    pub install_path: Option<String>,
    /// 软件类型
    pub software_type: SoftwareType,
    /// 是否可用
    pub is_available: bool,
}

/// 软件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SoftwareType {
    /// FFmpeg
    Ffmpeg,
    /// 视频编解码器
    VideoCodec,
    /// 图像处理
    ImageProcessing,
    /// 系统工具
    SystemTool,
    /// 开发工具
    DevelopmentTool,
    /// 其他
    Other,
}

/// 系统要求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemRequirements {
    /// 最小CPU核心数
    pub min_cpu_cores: usize,
    /// 最小内存（GB）
    pub min_memory_gb: f64,
    /// 最小可用磁盘空间（GB）
    pub min_disk_space_gb: f64,
    /// 支持的操作系统
    pub supported_os: Vec<String>,
    /// 必需的软件
    pub required_software: Vec<String>,
    /// 推荐的GPU
    pub recommended_gpu: Option<String>,
}

/// 系统兼容性检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityCheck {
    /// 是否兼容
    pub is_compatible: bool,
    /// 检查项目
    pub checks: Vec<CompatibilityCheckItem>,
    /// 警告信息
    pub warnings: Vec<String>,
    /// 建议
    pub recommendations: Vec<String>,
}

/// 兼容性检查项目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityCheckItem {
    /// 检查项目名称
    pub name: String,
    /// 是否通过
    pub passed: bool,
    /// 当前值
    pub current_value: String,
    /// 要求值
    pub required_value: String,
    /// 描述
    pub description: Option<String>,
}

impl Default for SystemRequirements {
    fn default() -> Self {
        Self {
            min_cpu_cores: 2,
            min_memory_gb: 4.0,
            min_disk_space_gb: 10.0,
            supported_os: vec![
                "Windows 10".to_string(),
                "Windows 11".to_string(),
                "macOS 10.15".to_string(),
                "Ubuntu 18.04".to_string(),
            ],
            required_software: vec!["FFmpeg".to_string()],
            recommended_gpu: None,
        }
    }
}

impl MemoryInfo {
    /// 获取内存使用率
    pub fn get_usage_percentage(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f64 / self.total as f64 * 100.0) as f32
        }
    }

    /// 获取可用内存百分比
    pub fn get_available_percentage(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.available as f64 / self.total as f64 * 100.0) as f32
        }
    }
}

impl DiskInfo {
    /// 获取磁盘使用率
    pub fn get_usage_percentage(&self) -> f32 {
        if self.total_space == 0 {
            0.0
        } else {
            (self.used_space as f64 / self.total_space as f64 * 100.0) as f32
        }
    }

    /// 检查是否有足够空间
    pub fn has_enough_space(&self, required_bytes: u64) -> bool {
        self.available_space >= required_bytes
    }
}

impl SystemInfo {
    /// 检查系统兼容性
    pub fn check_compatibility(&self, requirements: &SystemRequirements) -> CompatibilityCheck {
        let mut checks = Vec::new();
        let mut warnings = Vec::new();
        let mut recommendations = Vec::new();

        // 检查CPU核心数
        let cpu_check = CompatibilityCheckItem {
            name: "CPU Cores".to_string(),
            passed: self.cpu_info.cores >= requirements.min_cpu_cores,
            current_value: self.cpu_info.cores.to_string(),
            required_value: requirements.min_cpu_cores.to_string(),
            description: Some("Minimum CPU cores required".to_string()),
        };
        checks.push(cpu_check);

        // 检查内存
        let memory_gb = self.memory_info.total as f64 / 1024.0 / 1024.0 / 1024.0;
        let memory_check = CompatibilityCheckItem {
            name: "Memory".to_string(),
            passed: memory_gb >= requirements.min_memory_gb,
            current_value: format!("{:.1} GB", memory_gb),
            required_value: format!("{:.1} GB", requirements.min_memory_gb),
            description: Some("Minimum memory required".to_string()),
        };
        checks.push(memory_check);

        // 检查磁盘空间
        let total_disk_space: u64 = self.disk_info.iter().map(|d| d.available_space).sum();
        let disk_space_gb = total_disk_space as f64 / 1024.0 / 1024.0 / 1024.0;
        let disk_check = CompatibilityCheckItem {
            name: "Disk Space".to_string(),
            passed: disk_space_gb >= requirements.min_disk_space_gb,
            current_value: format!("{:.1} GB", disk_space_gb),
            required_value: format!("{:.1} GB", requirements.min_disk_space_gb),
            description: Some("Minimum available disk space required".to_string()),
        };
        checks.push(disk_check);

        let is_compatible = checks.iter().all(|check| check.passed);

        // 生成警告和建议
        if self.memory_info.usage_percentage > 80.0 {
            warnings.push("High memory usage detected".to_string());
            recommendations
                .push("Consider closing other applications to free up memory".to_string());
        }

        if self.gpu_info.is_empty() {
            warnings.push("No GPU detected".to_string());
            recommendations
                .push("Consider using a dedicated GPU for better performance".to_string());
        }

        CompatibilityCheck {
            is_compatible,
            checks,
            warnings,
            recommendations,
        }
    }
}
