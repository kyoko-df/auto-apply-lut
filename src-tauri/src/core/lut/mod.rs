//! LUT处理核心模块
//! 提供LUT文件的解析、验证和应用功能

pub mod parser;
pub mod processor;
pub mod converter;
pub mod validator;
pub mod cache;
pub mod data;
pub mod utils;

// Re-export commonly used types
pub use data::{LutData, LutData1D, LutStatistics};
pub use utils::{LutUtils, LutFileInfo};

use crate::types::{AppResult, LutInfo, LutType, LutFormat, LutValidationResult, LutSizeInfo};
use std::path::Path;
use tokio::fs;
use chrono::{DateTime, Utc};

/// LUT管理器
#[derive(Debug)]
pub struct LutManager {
    /// 支持的LUT格式
    supported_formats: Vec<LutFormat>,
}

impl LutManager {
    /// 创建新的LUT管理器
    pub fn new() -> Self {
        Self {
            supported_formats: vec![
                LutFormat::Cube,
                LutFormat::ThreeDL,
                LutFormat::Lut,
                LutFormat::Csp,
            ],
        }
    }

    /// 获取LUT文件信息
    pub async fn get_lut_info<P: AsRef<Path>>(&self, path: P) -> AppResult<LutInfo> {
        let path = path.as_ref();
        
        // 检查文件是否存在 (async)
        if fs::metadata(path).await.is_err() {
            return Err(crate::types::AppError::FileSystem(
                format!("LUT file not found: {}", path.display())
            ));
        }

        // 获取文件基本信息
        let metadata = fs::metadata(path).await
            .map_err(|e| crate::types::AppError::FileSystem(e.to_string()))?;
        
        let size = metadata.len();
        let created_at = metadata.created()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());
        let modified_at = metadata.modified()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());

        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let format = path.extension()
            .and_then(|ext| ext.to_str())
            .map(LutFormat::from_extension)
            .unwrap_or(LutFormat::Unknown);

        // 验证LUT文件
        let validation_result = self.validate_lut(path).await?;
        
        Ok(LutInfo {
            path: path.to_path_buf(),
            name,
            size,
            lut_type: validation_result.lut_type,
            format,
            created_at,
            modified_at,
            is_valid: validation_result.is_valid,
            error_message: if validation_result.errors.is_empty() {
                None
            } else {
                Some(validation_result.errors.join("; "))
            },
        })
    }

    /// 验证LUT文件
    pub async fn validate_lut<P: AsRef<Path>>(&self, path: P) -> AppResult<LutValidationResult> {
        let path = path.as_ref();
        
        let format = path.extension()
            .and_then(|ext| ext.to_str())
            .map(LutFormat::from_extension)
            .unwrap_or(LutFormat::Unknown);

        if !self.is_format_supported(&format) {
            return Ok(LutValidationResult {
                is_valid: false,
                lut_type: LutType::Unknown,
                format,
                errors: vec![format!("Unsupported LUT format: {:?}", format)],
                warnings: vec![],
                size_info: None,
            });
        }

        // 读取文件内容
        let content = fs::read_to_string(path).await
            .map_err(|e| crate::types::AppError::FileSystem(e.to_string()))?;

        // 根据格式验证
        match format {
            LutFormat::Cube => self.validate_cube_lut(&content).await,
            LutFormat::ThreeDL => self.validate_3dl_lut(&content).await,
            LutFormat::Lut => self.validate_lut_format(&content).await,
            LutFormat::Csp => self.validate_csp_lut(&content).await,
            LutFormat::M3d | LutFormat::Look | LutFormat::Vlt | LutFormat::Mga => {
                // 这些格式暂时不支持，返回未实现错误
                Ok(LutValidationResult {
                    is_valid: false,
                    lut_type: LutType::Unknown,
                    format,
                    errors: vec![format!("LUT format {:?} is not yet implemented", format)],
                    warnings: vec![],
                    size_info: None,
                })
            },
            LutFormat::Unknown => Ok(LutValidationResult {
                is_valid: false,
                lut_type: LutType::Unknown,
                format,
                errors: vec!["Unknown LUT format".to_string()],
                warnings: vec![],
                size_info: None,
            }),
        }
    }

    /// 检查LUT文件是否有效
    pub async fn is_valid_lut<P: AsRef<Path>>(&self, path: P) -> bool {
        match self.validate_lut(path).await {
            Ok(result) => result.is_valid,
            Err(_) => false,
        }
    }

    /// 获取支持的LUT格式
    pub fn get_supported_formats(&self) -> &[LutFormat] {
        &self.supported_formats
    }

    /// 检查格式是否支持
    pub fn is_format_supported(&self, format: &LutFormat) -> bool {
        self.supported_formats.contains(format) && !matches!(format, LutFormat::Unknown)
    }

    /// 扫描目录中的LUT文件
    pub async fn scan_lut_directory<P: AsRef<Path>>(&self, dir_path: P) -> AppResult<Vec<LutInfo>> {
        let dir_path = dir_path.as_ref();
        
        if !dir_path.is_dir() {
            return Err(crate::types::AppError::FileSystem(
                format!("Path is not a directory: {}", dir_path.display())
            ));
        }

        let mut lut_files = Vec::new();
        let mut entries = fs::read_dir(dir_path).await
            .map_err(|e| crate::types::AppError::FileSystem(e.to_string()))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| crate::types::AppError::FileSystem(e.to_string()))? {
            
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if LutFormat::is_supported(ext) {
                        match self.get_lut_info(&path).await {
                            Ok(lut_info) => lut_files.push(lut_info),
                            Err(_) => continue, // 跳过无效文件
                        }
                    }
                }
            }
        }

        Ok(lut_files)
    }

    /// 验证CUBE格式LUT
    async fn validate_cube_lut(&self, content: &str) -> AppResult<LutValidationResult> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut grid_size: Option<u32> = None;
        let mut domain_min: Option<(f32, f32, f32)> = None;
        let mut domain_max: Option<(f32, f32, f32)> = None;
        let mut data_points = 0;

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();
            
            // 跳过注释和空行
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // 解析LUT_3D_SIZE
            if line.starts_with("LUT_3D_SIZE") {
                if let Some(size_str) = line.split_whitespace().nth(1) {
                    match size_str.parse::<u32>() {
                        Ok(size) => {
                            if size < 2 || size > 256 {
                                warnings.push(format!("Line {}: Unusual grid size: {}", line_num + 1, size));
                            }
                            grid_size = Some(size);
                        }
                        Err(_) => {
                            errors.push(format!("Line {}: Invalid grid size format", line_num + 1));
                        }
                    }
                }
            }
            // 解析DOMAIN_MIN
            else if line.starts_with("DOMAIN_MIN") {
                if let Some(values) = Self::parse_rgb_values(line) {
                    domain_min = Some(values);
                } else {
                    errors.push(format!("Line {}: Invalid DOMAIN_MIN format", line_num + 1));
                }
            }
            // 解析DOMAIN_MAX
            else if line.starts_with("DOMAIN_MAX") {
                if let Some(values) = Self::parse_rgb_values(line) {
                    domain_max = Some(values);
                } else {
                    errors.push(format!("Line {}: Invalid DOMAIN_MAX format", line_num + 1));
                }
            }
            // 解析数据行
            else if Self::parse_rgb_values(line).is_some() {
                data_points += 1;
            }
            else {
                warnings.push(format!("Line {}: Unrecognized line format", line_num + 1));
            }
        }

        // 验证必需的元素
        if grid_size.is_none() {
            errors.push("Missing LUT_3D_SIZE declaration".to_string());
        }

        // 验证数据点数量
        if let Some(size) = grid_size {
            let expected_points = (size * size * size) as usize;
            if data_points != expected_points {
                errors.push(format!(
                    "Data point count mismatch: expected {}, found {}",
                    expected_points, data_points
                ));
            }
        }

        let size_info = grid_size.map(|size| LutSizeInfo {
            grid_size: Some(size),
            input_range: domain_min.zip(domain_max).map(|(min, max)| (min.0.min(min.1).min(min.2), max.0.max(max.1).max(max.2))),
            output_range: None, // CUBE格式通常不指定输出范围
        });

        Ok(LutValidationResult {
            is_valid: errors.is_empty(),
            lut_type: LutType::ThreeDimensional,
            format: LutFormat::Cube,
            errors,
            warnings,
            size_info,
        })
    }

    /// 验证3DL格式LUT
    async fn validate_3dl_lut(&self, content: &str) -> AppResult<LutValidationResult> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut data_points = 0;

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();
            
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if Self::parse_rgb_values(line).is_some() {
                data_points += 1;
            } else {
                warnings.push(format!("Line {}: Invalid data format", line_num + 1));
            }
        }

        // 3DL格式通常是32x32x32的网格
        let expected_points = 32 * 32 * 32;
        if data_points != expected_points {
            warnings.push(format!(
                "Unexpected data point count: expected {}, found {}",
                expected_points, data_points
            ));
        }

        let size_info = Some(LutSizeInfo {
            grid_size: Some(32),
            input_range: Some((0.0, 1.0)),
            output_range: Some((0.0, 1.0)),
        });

        Ok(LutValidationResult {
            is_valid: errors.is_empty(),
            lut_type: LutType::ThreeDimensional,
            format: LutFormat::ThreeDL,
            errors,
            warnings,
            size_info,
        })
    }

    /// 验证LUT格式
    async fn validate_lut_format(&self, content: &str) -> AppResult<LutValidationResult> {
        // 简单的LUT格式验证
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut data_points = 0;

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();
            
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if Self::parse_rgb_values(line).is_some() {
                data_points += 1;
            } else {
                warnings.push(format!("Line {}: Invalid data format", line_num + 1));
            }
        }

        if data_points == 0 {
            errors.push("No valid data points found".to_string());
        }

        Ok(LutValidationResult {
            is_valid: errors.is_empty(),
            lut_type: if data_points > 256 { LutType::ThreeDimensional } else { LutType::OneDimensional },
            format: LutFormat::Lut,
            errors,
            warnings,
            size_info: None,
        })
    }

    /// 验证CSP格式LUT
    async fn validate_csp_lut(&self, content: &str) -> AppResult<LutValidationResult> {
        // CSP格式验证（简化版本）
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if content.is_empty() {
            errors.push("Empty CSP file".to_string());
        }

        // CSP格式通常包含二进制数据，这里只做基本检查
        if content.len() < 100 {
            warnings.push("CSP file seems too small".to_string());
        }

        Ok(LutValidationResult {
            is_valid: errors.is_empty(),
            lut_type: LutType::ThreeDimensional,
            format: LutFormat::Csp,
            errors,
            warnings,
            size_info: None,
        })
    }

    /// 解析RGB值
    fn parse_rgb_values(line: &str) -> Option<(f32, f32, f32)> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                parts[0].parse::<f32>(),
                parts[1].parse::<f32>(),
                parts[2].parse::<f32>(),
            ) {
                return Some((r, g, b));
            }
        }
        None
    }
}

impl Default for LutManager {
    fn default() -> Self {
        Self::new()
    }
}