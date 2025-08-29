//! FFmpeg滤镜模块
//! 提供视频滤镜和效果处理功能

use crate::types::{AppResult, AppError};
use crate::core::ffmpeg::{EncodingSettings, Resolution};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use tokio::process::Command as AsyncCommand;
use serde::{Serialize, Deserialize};
use std::fmt;

/// 视频滤镜管理器
pub struct FilterManager {
    /// FFmpeg可执行文件路径
    ffmpeg_path: PathBuf,
    /// 预定义滤镜
    predefined_filters: HashMap<String, FilterChain>,
}

impl FilterManager {
    /// 创建新的滤镜管理器
    pub fn new(ffmpeg_path: PathBuf) -> Self {
        let mut manager = Self {
            ffmpeg_path,
            predefined_filters: HashMap::new(),
        };
        
        // 初始化预定义滤镜
        manager.init_predefined_filters();
        manager
    }

    /// 初始化预定义滤镜
    fn init_predefined_filters(&mut self) {
        // LUT滤镜
        self.predefined_filters.insert("lut3d".to_string(), FilterChain {
            name: "3D LUT".to_string(),
            description: "应用3D LUT颜色查找表".to_string(),
            filters: vec![
                Filter::Lut3d {
                    file: "".to_string(),
                    interp: LutInterpolation::Trilinear,
                }
            ],
        });
        
        // 色彩校正滤镜
        self.predefined_filters.insert("color_correction".to_string(), FilterChain {
            name: "色彩校正".to_string(),
            description: "基本色彩校正".to_string(),
            filters: vec![
                Filter::ColorBalance {
                    rs: 0.0, gs: 0.0, bs: 0.0,
                    rm: 0.0, gm: 0.0, bm: 0.0,
                    rh: 0.0, gh: 0.0, bh: 0.0,
                    pl: false,
                },
                Filter::Curves {
                    preset: CurvePreset::None,
                    master: None,
                    red: None,
                    green: None,
                    blue: None,
                }
            ],
        });
        
        // 锐化滤镜
        self.predefined_filters.insert("sharpen".to_string(), FilterChain {
            name: "锐化".to_string(),
            description: "增强图像锐度".to_string(),
            filters: vec![
                Filter::UnsharpMask {
                    luma_msize_x: 5,
                    luma_msize_y: 5,
                    luma_amount: 1.0,
                    chroma_msize_x: 5,
                    chroma_msize_y: 5,
                    chroma_amount: 0.0,
                }
            ],
        });
        
        // 降噪滤镜
        self.predefined_filters.insert("denoise".to_string(), FilterChain {
            name: "降噪".to_string(),
            description: "减少视频噪点".to_string(),
            filters: vec![
                Filter::Hqdn3d {
                    luma_spatial: 4.0,
                    chroma_spatial: 3.0,
                    luma_tmp: 6.0,
                    chroma_tmp: 4.5,
                }
            ],
        });
        
        // 稳定化滤镜
        self.predefined_filters.insert("stabilize".to_string(), FilterChain {
            name: "稳定化".to_string(),
            description: "视频防抖".to_string(),
            filters: vec![
                Filter::Vidstab {
                    shakiness: 5,
                    accuracy: 15,
                    stepsize: 6,
                    mincontrast: 0.3,
                    tripod: false,
                }
            ],
        });
    }

    /// 应用滤镜链
    pub async fn apply_filter_chain(
        &self,
        input_path: &Path,
        output_path: &Path,
        filter_chain: &FilterChain,
        settings: Option<EncodingSettings>,
    ) -> AppResult<()> {
        let filter_string = self.build_filter_string(&filter_chain.filters)?;
        
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args(["-i", input_path.to_str().unwrap()]);
        
        // 添加滤镜
        cmd.args(["-vf", &filter_string]);
        
        // 添加编码设置
        if let Some(settings) = settings {
            self.add_encoding_args(&mut cmd, &settings);
        }
        
        // 输出文件
        cmd.args(["-y", output_path.to_str().unwrap()]);
        
        let output = cmd.output().await
            .map_err(|e| AppError::FFmpeg(format!("Failed to apply filters: {}", e)))?;
        
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::FFmpeg(format!("Filter application failed: {}", error_msg)));
        }
        
        Ok(())
    }

    /// 应用单个滤镜
    pub async fn apply_filter(
        &self,
        input_path: &Path,
        output_path: &Path,
        filter: &Filter,
        settings: Option<EncodingSettings>,
    ) -> AppResult<()> {
        let filter_chain = FilterChain {
            name: "Single Filter".to_string(),
            description: "Single filter application".to_string(),
            filters: vec![filter.clone()],
        };
        
        self.apply_filter_chain(input_path, output_path, &filter_chain, settings).await
    }

    /// 应用LUT滤镜
    pub async fn apply_lut(
        &self,
        input_path: &Path,
        output_path: &Path,
        lut_path: &Path,
        settings: Option<EncodingSettings>,
    ) -> AppResult<()> {
        let filter = Filter::Lut3d {
            file: lut_path.to_string_lossy().to_string(),
            interp: LutInterpolation::Trilinear,
        };
        
        self.apply_filter(input_path, output_path, &filter, settings).await
    }

    /// 批量应用滤镜
    pub async fn batch_apply_filters(
        &self,
        tasks: Vec<FilterTask>,
        max_concurrent: usize,
    ) -> AppResult<Vec<FilterResult>> {
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent));
        let mut handles = Vec::new();
        
        for task in tasks {
            let semaphore = semaphore.clone();
            let manager = self.clone_for_task();
            
            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                
                let result = manager.apply_filter_chain(
                    &task.input_path,
                    &task.output_path,
                    &task.filter_chain,
                    task.settings,
                ).await;
                
                let success = result.is_ok();
                let error = result.err().map(|e| e.to_string());
                
                FilterResult {
                    task_id: task.id,
                    success,
                    error,
                    output_path: if success { Some(task.output_path) } else { None },
                }
            });
            
            handles.push(handle);
        }
        
        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => {
                    results.push(FilterResult {
                        task_id: "unknown".to_string(),
                        success: false,
                        error: Some(format!("Task failed: {}", e)),
                        output_path: None,
                    });
                }
            }
        }
        
        Ok(results)
    }

    /// 构建滤镜字符串
    fn build_filter_string(&self, filters: &[Filter]) -> AppResult<String> {
        let filter_strings: Result<Vec<String>, AppError> = filters.iter()
            .map(|f| f.to_ffmpeg_string())
            .collect();
        
        Ok(filter_strings?.join(","))
    }

    /// 添加编码参数
    fn add_encoding_args(&self, cmd: &mut AsyncCommand, settings: &EncodingSettings) {
        cmd.args(["-c:v", &settings.video_codec]);
        cmd.args(["-c:a", &settings.audio_codec]);
        cmd.args(["-preset", &settings.preset]);
        
        if let Some(bitrate) = &settings.bitrate {
            cmd.args(["-b:v", bitrate]);
        } else {
            cmd.args(["-crf", &settings.crf.to_string()]);
        }
        
        if let Some(resolution) = &settings.resolution {
            cmd.args(["-s", &format!("{}x{}", resolution.width, resolution.height)]);
        }
        
        if let Some(fps) = settings.fps {
            cmd.args(["-r", &fps.to_string()]);
        }
        
        for (key, value) in &settings.extra_params {
            cmd.args([key, value]);
        }
    }

    /// 克隆管理器用于任务
    fn clone_for_task(&self) -> Self {
        Self {
            ffmpeg_path: self.ffmpeg_path.clone(),
            predefined_filters: self.predefined_filters.clone(),
        }
    }

    /// 获取预定义滤镜
    pub fn get_predefined_filter(&self, name: &str) -> Option<&FilterChain> {
        self.predefined_filters.get(name)
    }

    /// 获取所有预定义滤镜
    pub fn get_all_predefined_filters(&self) -> &HashMap<String, FilterChain> {
        &self.predefined_filters
    }

    /// 添加自定义滤镜
    pub fn add_custom_filter(&mut self, name: String, filter_chain: FilterChain) {
        self.predefined_filters.insert(name, filter_chain);
    }

    /// 移除滤镜
    pub fn remove_filter(&mut self, name: &str) -> Option<FilterChain> {
        self.predefined_filters.remove(name)
    }

    /// 验证滤镜链
    pub fn validate_filter_chain(&self, filter_chain: &FilterChain) -> AppResult<()> {
        for filter in &filter_chain.filters {
            self.validate_filter(filter)?;
        }
        Ok(())
    }

    /// 验证单个滤镜
    fn validate_filter(&self, filter: &Filter) -> AppResult<()> {
        match filter {
            Filter::Lut3d { file, .. } => {
                if !file.is_empty() && !Path::new(file).exists() {
                    return Err(AppError::FFmpeg(format!("LUT file not found: {}", file)));
                }
            }
            Filter::Scale { width, height, .. } => {
                if *width == 0 || *height == 0 {
                    return Err(AppError::FFmpeg("Invalid scale dimensions".to_string()));
                }
            }
            _ => {} // 其他滤镜暂时不验证
        }
        Ok(())
    }

    /// 获取滤镜信息
    pub async fn get_filter_info(&self) -> AppResult<Vec<FilterInfo>> {
        let mut cmd = AsyncCommand::new(&self.ffmpeg_path);
        cmd.args(["-filters"]);
        
        let output = cmd.output().await
            .map_err(|e| AppError::FFmpeg(format!("Failed to get filter info: {}", e)))?;
        
        if !output.status.success() {
            return Err(AppError::FFmpeg("Failed to get filter information".to_string()));
        }
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        self.parse_filter_info(&output_str)
    }

    /// 解析滤镜信息
    fn parse_filter_info(&self, output: &str) -> AppResult<Vec<FilterInfo>> {
        let mut filters = Vec::new();
        
        for line in output.lines().skip(8) { // 跳过头部信息
            if line.trim().is_empty() {
                continue;
            }
            
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let flags = parts[0];
                let name = parts[1];
                let description = parts[2..].join(" ");
                
                filters.push(FilterInfo {
                    name: name.to_string(),
                    description,
                    supports_timeline: flags.contains('T'),
                    supports_slice_threading: flags.contains('S'),
                    supports_command: flags.contains('C'),
                });
            }
        }
        
        Ok(filters)
    }
}

/// 滤镜链
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterChain {
    pub name: String,
    pub description: String,
    pub filters: Vec<Filter>,
}

/// 滤镜定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Filter {
    /// 3D LUT滤镜
    Lut3d {
        file: String,
        interp: LutInterpolation,
    },
    /// 1D LUT滤镜
    Lut1d {
        file: String,
        interp: LutInterpolation,
    },
    /// 缩放滤镜
    Scale {
        width: u32,
        height: u32,
        algorithm: ScaleAlgorithm,
    },
    /// 色彩平衡
    ColorBalance {
        rs: f64, gs: f64, bs: f64, // 阴影
        rm: f64, gm: f64, bm: f64, // 中间调
        rh: f64, gh: f64, bh: f64, // 高光
        pl: bool, // 保持亮度
    },
    /// 曲线调整
    Curves {
        preset: CurvePreset,
        master: Option<String>,
        red: Option<String>,
        green: Option<String>,
        blue: Option<String>,
    },
    /// 反锐化蒙版
    UnsharpMask {
        luma_msize_x: u32,
        luma_msize_y: u32,
        luma_amount: f64,
        chroma_msize_x: u32,
        chroma_msize_y: u32,
        chroma_amount: f64,
    },
    /// 高质量降噪
    Hqdn3d {
        luma_spatial: f64,
        chroma_spatial: f64,
        luma_tmp: f64,
        chroma_tmp: f64,
    },
    /// 视频稳定
    Vidstab {
        shakiness: u32,
        accuracy: u32,
        stepsize: u32,
        mincontrast: f64,
        tripod: bool,
    },
    /// 亮度/对比度
    Eq {
        contrast: f64,
        brightness: f64,
        saturation: f64,
        gamma: f64,
    },
    /// 色调/饱和度
    Hue {
        hue: f64,
        saturation: f64,
    },
}

impl Filter {
    /// 转换为FFmpeg滤镜字符串
    pub fn to_ffmpeg_string(&self) -> AppResult<String> {
        match self {
            Filter::Lut3d { file, interp } => {
                Ok(format!("lut3d=file={}:interp={}", file, interp.to_string()))
            }
            Filter::Lut1d { file, interp } => {
                Ok(format!("lut=file={}:interp={}", file, interp.to_string()))
            }
            Filter::Scale { width, height, algorithm } => {
                Ok(format!("scale={}:{}:flags={}", width, height, algorithm.to_string()))
            }
            Filter::ColorBalance { rs, gs, bs, rm, gm, bm, rh, gh, bh, pl } => {
                Ok(format!(
                    "colorbalance=rs={}:gs={}:bs={}:rm={}:gm={}:bm={}:rh={}:gh={}:bh={}:pl={}",
                    rs, gs, bs, rm, gm, bm, rh, gh, bh, if *pl { "true" } else { "false" }
                ))
            }
            Filter::Curves { preset, master, red, green, blue } => {
                let mut params = Vec::new();
                
                if *preset != CurvePreset::None {
                    params.push(format!("preset={}", preset.to_string()));
                }
                
                if let Some(m) = master {
                    params.push(format!("master={}", m));
                }
                if let Some(r) = red {
                    params.push(format!("red={}", r));
                }
                if let Some(g) = green {
                    params.push(format!("green={}", g));
                }
                if let Some(b) = blue {
                    params.push(format!("blue={}", b));
                }
                
                Ok(format!("curves={}", params.join(":")))
            }
            Filter::UnsharpMask { luma_msize_x, luma_msize_y, luma_amount, chroma_msize_x, chroma_msize_y, chroma_amount } => {
                Ok(format!(
                    "unsharp={}:{}:{}:{}:{}:{}",
                    luma_msize_x, luma_msize_y, luma_amount,
                    chroma_msize_x, chroma_msize_y, chroma_amount
                ))
            }
            Filter::Hqdn3d { luma_spatial, chroma_spatial, luma_tmp, chroma_tmp } => {
                Ok(format!(
                    "hqdn3d={}:{}:{}:{}",
                    luma_spatial, chroma_spatial, luma_tmp, chroma_tmp
                ))
            }
            Filter::Vidstab { shakiness, accuracy, stepsize, mincontrast, tripod } => {
                Ok(format!(
                    "vidstabdetect=shakiness={}:accuracy={}:stepsize={}:mincontrast={}:tripod={}",
                    shakiness, accuracy, stepsize, mincontrast,
                    if *tripod { "1" } else { "0" }
                ))
            }
            Filter::Eq { contrast, brightness, saturation, gamma } => {
                Ok(format!(
                    "eq=contrast={}:brightness={}:saturation={}:gamma={}",
                    contrast, brightness, saturation, gamma
                ))
            }
            Filter::Hue { hue, saturation } => {
                Ok(format!("hue=h={}:s={}", hue, saturation))
            }
        }
    }
}

/// LUT插值方法
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LutInterpolation {
    Nearest,
    Trilinear,
    Tetrahedral,
}

impl fmt::Display for LutInterpolation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LutInterpolation::Nearest => write!(f, "nearest"),
            LutInterpolation::Trilinear => write!(f, "trilinear"),
            LutInterpolation::Tetrahedral => write!(f, "tetrahedral"),
        }
    }
}

/// 缩放算法
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScaleAlgorithm {
    FastBilinear,
    Bilinear,
    Bicubic,
    Experimental,
    Neighbor,
    Area,
    Bicublin,
    Gauss,
    Sinc,
    Lanczos,
    Spline,
}

impl fmt::Display for ScaleAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ScaleAlgorithm::FastBilinear => write!(f, "fast_bilinear"),
            ScaleAlgorithm::Bilinear => write!(f, "bilinear"),
            ScaleAlgorithm::Bicubic => write!(f, "bicubic"),
            ScaleAlgorithm::Experimental => write!(f, "experimental"),
            ScaleAlgorithm::Neighbor => write!(f, "neighbor"),
            ScaleAlgorithm::Area => write!(f, "area"),
            ScaleAlgorithm::Bicublin => write!(f, "bicublin"),
            ScaleAlgorithm::Gauss => write!(f, "gauss"),
            ScaleAlgorithm::Sinc => write!(f, "sinc"),
            ScaleAlgorithm::Lanczos => write!(f, "lanczos"),
            ScaleAlgorithm::Spline => write!(f, "spline"),
        }
    }
}

/// 曲线预设
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CurvePreset {
    None,
    Color_negative,
    Cross_process,
    Darker,
    Increase_contrast,
    Lighter,
    Linear_contrast,
    Medium_contrast,
    Negative,
    Strong_contrast,
    Vintage,
}

impl fmt::Display for CurvePreset {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CurvePreset::None => write!(f, "none"),
            CurvePreset::Color_negative => write!(f, "color_negative"),
            CurvePreset::Cross_process => write!(f, "cross_process"),
            CurvePreset::Darker => write!(f, "darker"),
            CurvePreset::Increase_contrast => write!(f, "increase_contrast"),
            CurvePreset::Lighter => write!(f, "lighter"),
            CurvePreset::Linear_contrast => write!(f, "linear_contrast"),
            CurvePreset::Medium_contrast => write!(f, "medium_contrast"),
            CurvePreset::Negative => write!(f, "negative"),
            CurvePreset::Strong_contrast => write!(f, "strong_contrast"),
            CurvePreset::Vintage => write!(f, "vintage"),
        }
    }
}

/// 滤镜任务
#[derive(Debug, Clone)]
pub struct FilterTask {
    pub id: String,
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub filter_chain: FilterChain,
    pub settings: Option<EncodingSettings>,
}

/// 滤镜结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterResult {
    pub task_id: String,
    pub success: bool,
    pub error: Option<String>,
    pub output_path: Option<PathBuf>,
}

/// 滤镜信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterInfo {
    pub name: String,
    pub description: String,
    pub supports_timeline: bool,
    pub supports_slice_threading: bool,
    pub supports_command: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_filter_chain() {
        let chain = FilterChain {
            name: "Test Chain".to_string(),
            description: "Test description".to_string(),
            filters: vec![
                Filter::Scale {
                    width: 1920,
                    height: 1080,
                    algorithm: ScaleAlgorithm::Bicubic,
                }
            ],
        };
        
        assert_eq!(chain.name, "Test Chain");
        assert_eq!(chain.filters.len(), 1);
    }

    #[test]
    fn test_lut_interpolation_display() {
        assert_eq!(LutInterpolation::Nearest.to_string(), "nearest");
        assert_eq!(LutInterpolation::Trilinear.to_string(), "trilinear");
        assert_eq!(LutInterpolation::Tetrahedral.to_string(), "tetrahedral");
    }

    #[test]
    fn test_scale_algorithm_display() {
        assert_eq!(ScaleAlgorithm::FastBilinear.to_string(), "fast_bilinear");
        assert_eq!(ScaleAlgorithm::Bicubic.to_string(), "bicubic");
        assert_eq!(ScaleAlgorithm::Lanczos.to_string(), "lanczos");
    }

    #[test]
    fn test_curve_preset_display() {
        assert_eq!(CurvePreset::None.to_string(), "none");
        assert_eq!(CurvePreset::Vintage.to_string(), "vintage");
        assert_eq!(CurvePreset::Cross_process.to_string(), "cross_process");
    }

    #[test]
    fn test_filter_to_ffmpeg_string() {
        let filter = Filter::Scale {
            width: 1920,
            height: 1080,
            algorithm: ScaleAlgorithm::Bicubic,
        };
        
        let result = filter.to_ffmpeg_string().unwrap();
        assert_eq!(result, "scale=1920:1080:flags=bicubic");
    }

    #[test]
    fn test_lut3d_filter_string() {
        let filter = Filter::Lut3d {
            file: "/path/to/lut.cube".to_string(),
            interp: LutInterpolation::Trilinear,
        };
        
        let result = filter.to_ffmpeg_string().unwrap();
        assert_eq!(result, "lut3d=file=/path/to/lut.cube:interp=trilinear");
    }

    #[test]
    fn test_color_balance_filter_string() {
        let filter = Filter::ColorBalance {
            rs: 0.1, gs: 0.0, bs: -0.1,
            rm: 0.05, gm: 0.0, bm: 0.0,
            rh: 0.0, gh: 0.0, bh: 0.0,
            pl: true,
        };
        
        let result = filter.to_ffmpeg_string().unwrap();
        assert!(result.contains("colorbalance"));
        assert!(result.contains("rs=0.1"));
        assert!(result.contains("pl=true"));
    }

    #[test]
    fn test_unsharp_mask_filter_string() {
        let filter = Filter::UnsharpMask {
            luma_msize_x: 5,
            luma_msize_y: 5,
            luma_amount: 1.0,
            chroma_msize_x: 5,
            chroma_msize_y: 5,
            chroma_amount: 0.0,
        };
        
        let result = filter.to_ffmpeg_string().unwrap();
        assert_eq!(result, "unsharp=5:5:1:5:5:0");
    }

    #[test]
    fn test_filter_task() {
        let task = FilterTask {
            id: "test_task".to_string(),
            input_path: PathBuf::from("/input/video.mp4"),
            output_path: PathBuf::from("/output/video.mp4"),
            filter_chain: FilterChain {
                name: "Test".to_string(),
                description: "Test".to_string(),
                filters: Vec::new(),
            },
            settings: None,
        };
        
        assert_eq!(task.id, "test_task");
    }

    #[test]
    fn test_filter_result() {
        let result = FilterResult {
            task_id: "test_task".to_string(),
            success: true,
            error: None,
            output_path: Some(PathBuf::from("/output/video.mp4")),
        };
        
        assert!(result.success);
        assert!(result.error.is_none());
        assert!(result.output_path.is_some());
    }

    #[test]
    fn test_filter_info() {
        let info = FilterInfo {
            name: "scale".to_string(),
            description: "Scale the input video size and/or convert the image format.".to_string(),
            supports_timeline: true,
            supports_slice_threading: true,
            supports_command: false,
        };
        
        assert_eq!(info.name, "scale");
        assert!(info.supports_timeline);
        assert!(info.supports_slice_threading);
        assert!(!info.supports_command);
    }

    #[tokio::test]
    async fn test_filter_manager_creation() {
        let manager = FilterManager::new(PathBuf::from("/usr/bin/ffmpeg"));
        
        // 检查预定义滤镜是否已加载
        assert!(manager.get_predefined_filter("lut3d").is_some());
        assert!(manager.get_predefined_filter("color_correction").is_some());
        assert!(manager.get_predefined_filter("sharpen").is_some());
        assert!(manager.get_predefined_filter("denoise").is_some());
        assert!(manager.get_predefined_filter("stabilize").is_some());
    }

    #[test]
    fn test_custom_filter_management() {
        let mut manager = FilterManager::new(PathBuf::from("/usr/bin/ffmpeg"));
        
        let custom_chain = FilterChain {
            name: "Custom Filter".to_string(),
            description: "Custom description".to_string(),
            filters: vec![
                Filter::Eq {
                    contrast: 1.2,
                    brightness: 0.1,
                    saturation: 1.1,
                    gamma: 1.0,
                }
            ],
        };
        
        // 添加自定义滤镜
        manager.add_custom_filter("custom".to_string(), custom_chain.clone());
        assert!(manager.get_predefined_filter("custom").is_some());
        
        // 移除滤镜
        let removed = manager.remove_filter("custom");
        assert!(removed.is_some());
        assert!(manager.get_predefined_filter("custom").is_none());
    }
}