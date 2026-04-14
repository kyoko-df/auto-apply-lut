use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// LUT文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LutInfo {
    /// LUT文件路径
    pub path: PathBuf,
    /// 文件名
    pub name: String,
    /// 文件大小（字节）
    pub size: u64,
    /// LUT类型
    pub lut_type: LutType,
    /// LUT格式
    pub format: LutFormat,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 修改时间
    pub modified_at: chrono::DateTime<chrono::Utc>,
    /// 是否有效
    pub is_valid: bool,
    /// 错误信息（如果无效）
    pub error_message: Option<String>,
}

/// LUT类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum LutType {
    /// 1D LUT
    OneDimensional,
    /// 3D LUT
    ThreeDimensional,
    /// 未知类型
    Unknown,
}

/// LUT格式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum LutFormat {
    /// .cube格式
    Cube,
    /// .3dl格式
    ThreeDL,
    /// .lut格式
    Lut,
    /// .csp格式
    Csp,
    /// .m3d格式
    M3d,
    /// .look格式
    Look,
    /// .vlt格式
    Vlt,
    /// .mga格式
    Mga,
    /// 未知格式
    Unknown,
}

impl LutFormat {
    /// 从文件扩展名获取LUT格式
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "cube" => LutFormat::Cube,
            "3dl" => LutFormat::ThreeDL,
            "lut" => LutFormat::Lut,
            "csp" => LutFormat::Csp,
            "m3d" => LutFormat::M3d,
            "look" => LutFormat::Look,
            "vlt" => LutFormat::Vlt,
            "mga" => LutFormat::Mga,
            _ => LutFormat::Unknown,
        }
    }

    /// 获取格式对应的扩展名字符串
    pub fn extension(&self) -> &'static str {
        match self {
            LutFormat::Cube => "cube",
            LutFormat::ThreeDL => "3dl",
            LutFormat::Lut => "lut",
            LutFormat::Csp => "csp",
            LutFormat::M3d => "m3d",
            LutFormat::Look => "look",
            LutFormat::Vlt => "vlt",
            LutFormat::Mga => "mga",
            LutFormat::Unknown => "unknown",
        }
    }

    /// 获取支持的文件扩展名
    pub fn supported_extensions() -> Vec<&'static str> {
        vec!["cube", "3dl", "lut", "csp", "m3d", "look", "vlt", "mga"]
    }

    /// 检查是否为支持的格式
    pub fn is_supported(ext: &str) -> bool {
        Self::supported_extensions().contains(&ext.to_lowercase().as_str())
    }
}

/// LUT应用选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LutApplyOptions {
    /// LUT文件路径
    pub lut_path: PathBuf,
    /// 强度（0.0-1.0）
    pub intensity: f32,
    /// 是否启用插值
    pub interpolation: bool,
    /// 色彩空间转换
    pub color_space: Option<ColorSpace>,
}

/// 色彩空间
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ColorSpace {
    /// sRGB
    Srgb,
    /// Rec.709
    Rec709,
    /// Rec.2020
    Rec2020,
    /// DCI-P3
    DciP3,
    /// Adobe RGB
    AdobeRgb,
}

impl Default for LutApplyOptions {
    fn default() -> Self {
        Self {
            lut_path: PathBuf::new(),
            intensity: 1.0,
            interpolation: true,
            color_space: None,
        }
    }
}

/// LUT验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LutValidationResult {
    /// 是否有效
    pub is_valid: bool,
    /// LUT类型
    pub lut_type: LutType,
    /// LUT格式
    pub format: LutFormat,
    /// 错误信息
    pub errors: Vec<String>,
    /// 警告信息
    pub warnings: Vec<String>,
    /// LUT大小信息
    pub size_info: Option<LutSizeInfo>,
}

/// LUT大小信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LutSizeInfo {
    /// 网格大小（对于3D LUT）
    pub grid_size: Option<u32>,
    /// 输入范围
    pub input_range: Option<(f32, f32)>,
    /// 输出范围
    pub output_range: Option<(f32, f32)>,
}
