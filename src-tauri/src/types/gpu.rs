use serde::{Deserialize, Serialize};

/// GPU信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    /// GPU ID
    pub id: u32,
    /// GPU名称
    pub name: String,
    /// GPU厂商
    pub vendor: GpuVendor,
    /// 显存大小（MB）
    pub memory_total: u64,
    /// 可用显存（MB）
    pub memory_available: u64,
    /// 已使用显存（MB）
    pub memory_used: u64,
    /// GPU使用率（0-100）
    pub utilization: f32,
    /// 温度（摄氏度）
    pub temperature: Option<f32>,
    /// 功耗（瓦特）
    pub power_usage: Option<f32>,
    /// 是否支持硬件编码
    pub supports_encoding: bool,
    /// 是否支持硬件解码
    pub supports_decoding: bool,
    /// 支持的编码格式
    pub supported_codecs: Vec<VideoCodec>,
    /// 驱动版本
    pub driver_version: Option<String>,
    /// CUDA版本（NVIDIA GPU）
    pub cuda_version: Option<String>,
    /// OpenCL版本
    pub opencl_version: Option<String>,
}

/// GPU厂商
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GpuVendor {
    /// NVIDIA
    Nvidia,
    /// AMD
    Amd,
    /// Intel
    Intel,
    /// Apple
    Apple,
    /// 其他
    Other(String),
}

/// 视频编解码器
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VideoCodec {
    /// H.264/AVC
    H264,
    /// H.265/HEVC
    H265,
    /// VP9
    Vp9,
    /// AV1
    Av1,
    /// ProRes
    ProRes,
    /// DNxHD
    DnxHd,
    /// 其他
    Other(String),
}

/// GPU加速类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GpuAcceleration {
    /// 无GPU加速
    None,
    /// NVIDIA NVENC
    Nvenc,
    /// AMD VCE
    Vce,
    /// Intel Quick Sync
    QuickSync,
    /// Apple VideoToolbox
    VideoToolbox,
    /// CUDA
    Cuda,
    /// OpenCL
    OpenCl,
    /// Metal
    Metal,
}

/// GPU性能配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuPerformanceConfig {
    /// 是否启用GPU加速
    pub enabled: bool,
    /// 首选GPU ID
    pub preferred_gpu_id: Option<u32>,
    /// 加速类型
    pub acceleration_type: GpuAcceleration,
    /// 最大并发任务数
    pub max_concurrent_tasks: usize,
    /// 内存使用限制（MB）
    pub memory_limit: Option<u64>,
    /// 是否启用自动降级
    pub auto_fallback: bool,
}

/// GPU状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuStatus {
    /// 是否可用
    pub available: bool,
    /// 当前使用率
    pub utilization: f32,
    /// 内存使用情况
    pub memory_usage: GpuMemoryUsage,
    /// 温度
    pub temperature: Option<f32>,
    /// 当前运行的任务数
    pub active_tasks: usize,
    /// 错误信息
    pub error_message: Option<String>,
}

/// GPU内存使用情况
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMemoryUsage {
    /// 总内存（MB）
    pub total: u64,
    /// 已使用内存（MB）
    pub used: u64,
    /// 可用内存（MB）
    pub available: u64,
    /// 使用率（0-100）
    pub usage_percentage: f32,
}

/// GPU编码设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuEncodingSettings {
    /// 编码器
    pub encoder: String,
    /// 编码预设
    pub preset: EncodingPreset,
    /// 质量设置
    pub quality: EncodingQuality,
    /// 比特率控制模式
    pub rate_control: RateControlMode,
    /// 目标比特率（kbps）
    pub bitrate: Option<u32>,
    /// 最大比特率（kbps）
    pub max_bitrate: Option<u32>,
    /// B帧数量
    pub b_frames: Option<u32>,
    /// 参考帧数量
    pub ref_frames: Option<u32>,
}

/// 编码预设
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EncodingPreset {
    /// 最快速度
    UltraFast,
    /// 超快速度
    SuperFast,
    /// 很快速度
    VeryFast,
    /// 快速
    Faster,
    /// 快
    Fast,
    /// 中等
    Medium,
    /// 慢
    Slow,
    /// 更慢
    Slower,
    /// 很慢
    VerySlow,
}

/// 编码质量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EncodingQuality {
    /// CRF模式
    Crf(u32),
    /// 恒定比特率
    Cbr(u32),
    /// 可变比特率
    Vbr { min: u32, max: u32 },
    /// 恒定质量
    ConstantQuality(u32),
}

/// 比特率控制模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RateControlMode {
    /// 恒定比特率
    Cbr,
    /// 可变比特率
    Vbr,
    /// 恒定质量
    Cq,
    /// 恒定率因子
    Crf,
}

impl Default for GpuPerformanceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            preferred_gpu_id: None,
            acceleration_type: GpuAcceleration::None,
            max_concurrent_tasks: 1,
            memory_limit: None,
            auto_fallback: true,
        }
    }
}

impl Default for GpuEncodingSettings {
    fn default() -> Self {
        Self {
            encoder: "libx264".to_string(),
            preset: EncodingPreset::Medium,
            quality: EncodingQuality::Crf(23),
            rate_control: RateControlMode::Crf,
            bitrate: None,
            max_bitrate: None,
            b_frames: Some(3),
            ref_frames: Some(3),
        }
    }
}

impl GpuVendor {
    /// 从字符串解析GPU厂商
    pub fn from_string(vendor: &str) -> Self {
        match vendor.to_lowercase().as_str() {
            s if s.contains("nvidia") => GpuVendor::Nvidia,
            s if s.contains("amd") || s.contains("ati") => GpuVendor::Amd,
            s if s.contains("intel") => GpuVendor::Intel,
            s if s.contains("apple") => GpuVendor::Apple,
            _ => GpuVendor::Other(vendor.to_string()),
        }
    }

    /// 获取推荐的加速类型
    pub fn recommended_acceleration(&self) -> GpuAcceleration {
        match self {
            GpuVendor::Nvidia => GpuAcceleration::Nvenc,
            GpuVendor::Amd => GpuAcceleration::Vce,
            GpuVendor::Intel => GpuAcceleration::QuickSync,
            GpuVendor::Apple => GpuAcceleration::VideoToolbox,
            GpuVendor::Other(_) => GpuAcceleration::None,
        }
    }
}

impl VideoCodec {
    /// 获取编解码器字符串
    pub fn to_string(&self) -> &str {
        match self {
            VideoCodec::H264 => "h264",
            VideoCodec::H265 => "hevc",
            VideoCodec::Vp9 => "vp9",
            VideoCodec::Av1 => "av1",
            VideoCodec::ProRes => "prores",
            VideoCodec::DnxHd => "dnxhd",
            VideoCodec::Other(s) => s,
        }
    }

    /// 从字符串解析编解码器
    pub fn from_string(codec: &str) -> Self {
        match codec.to_lowercase().as_str() {
            "h264" | "avc" => VideoCodec::H264,
            "h265" | "hevc" => VideoCodec::H265,
            "vp9" => VideoCodec::Vp9,
            "av1" => VideoCodec::Av1,
            "prores" => VideoCodec::ProRes,
            "dnxhd" => VideoCodec::DnxHd,
            _ => VideoCodec::Other(codec.to_string()),
        }
    }
}