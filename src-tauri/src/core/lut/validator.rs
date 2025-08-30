//! LUT验证器模块
//! 提供LUT文件和数据的验证功能

use crate::types::{AppResult, AppError};
use crate::core::lut::{LutData, LutType, LutFormat};
use std::collections::HashMap;
use std::path::Path;

/// LUT验证器
pub struct LutValidator {
    /// 验证规则配置
    config: ValidationConfig,
}

impl LutValidator {
    /// 创建新的LUT验证器
    pub fn new() -> Self {
        Self {
            config: ValidationConfig::default(),
        }
    }

    /// 创建带配置的LUT验证器
    pub fn with_config(config: ValidationConfig) -> Self {
        Self { config }
    }

    /// 验证LUT数据
    pub fn validate(&self, lut_data: &LutData) -> ValidationResult {
        let mut result = ValidationResult::new();
        
        // 基本结构验证
        self.validate_basic_structure(lut_data, &mut result);
        
        // 数据完整性验证
        self.validate_data_integrity(lut_data, &mut result);
        
        // 数值范围验证
        self.validate_value_ranges(lut_data, &mut result);
        
        // 格式特定验证
        self.validate_format_specific(lut_data, &mut result);
        
        // 性能验证
        if self.config.check_performance {
            self.validate_performance(lut_data, &mut result);
        }
        
        // 兼容性验证
        if self.config.check_compatibility {
            self.validate_compatibility(lut_data, &mut result);
        }
        
        result
    }

    /// 验证基本结构
    fn validate_basic_structure(&self, lut_data: &LutData, result: &mut ValidationResult) {
        // 检查LUT大小
        if lut_data.size == 0 {
            result.add_error(ValidationError::InvalidSize {
                size: lut_data.size,
                message: "LUT size cannot be zero".to_string(),
            });
        }
        
        if lut_data.size > self.config.max_lut_size {
            result.add_warning(ValidationWarning::LargeSize {
                size: lut_data.size,
                max_recommended: self.config.max_lut_size,
            });
        }
        
        // 检查数据长度
        let expected_length = match lut_data.lut_type {
            LutType::OneDimensional => lut_data.size,
            LutType::ThreeDimensional => lut_data.size * lut_data.size * lut_data.size,
            _ => {
                result.add_error(ValidationError::UnsupportedType {
                    lut_type: lut_data.lut_type,
                });
                return;
            }
        };
        
        // 检查数据长度
        match lut_data.lut_type {
            LutType::ThreeDimensional => {
                if let Some(data_3d) = &lut_data.data_3d {
                    let actual_length = data_3d.len() * data_3d.get(0).map_or(0, |v| v.len()) * data_3d.get(0).and_then(|v| v.get(0)).map_or(0, |v| v.len());
                    if actual_length != expected_length {
                        result.add_error(ValidationError::DataLengthMismatch {
                            expected: expected_length,
                            actual: actual_length,
                        });
                    }
                } else {
                    result.add_error(ValidationError::DataLengthMismatch {
                        expected: expected_length,
                        actual: 0,
                    });
                }
            },
            LutType::OneDimensional => {
                if let Some(data_1d) = &lut_data.data_1d {
                    let actual_length = data_1d.red.len();
                    if actual_length != expected_length {
                        result.add_error(ValidationError::DataLengthMismatch {
                            expected: expected_length,
                            actual: actual_length,
                        });
                    }
                    
                    // 检查输入输出范围
                    if data_1d.input_range.0 >= data_1d.input_range.1 {
                        result.add_error(ValidationError::InvalidRange {
                            range_type: "input".to_string(),
                            min: data_1d.input_range.0,
                            max: data_1d.input_range.1,
                        });
                    }
                    
                    if data_1d.output_range.0 >= data_1d.output_range.1 {
                        result.add_error(ValidationError::InvalidRange {
                            range_type: "output".to_string(),
                            min: data_1d.output_range.0,
                            max: data_1d.output_range.1,
                        });
                    }
                } else {
                    result.add_error(ValidationError::DataLengthMismatch {
                        expected: expected_length,
                        actual: 0,
                    });
                }
            },
            _ => {}
        }
    }

    /// 验证数据完整性
    fn validate_data_integrity(&self, lut_data: &LutData, result: &mut ValidationResult) {
        let mut nan_count = 0;
        let mut inf_count = 0;
        let mut out_of_range_count = 0;
        
        match lut_data.lut_type {
            LutType::OneDimensional => {
                if let Some(data_1d) = &lut_data.data_1d {
                    let channels = [&data_1d.red, &data_1d.green, &data_1d.blue];
                    for (channel, values) in channels.iter().enumerate() {
                        for (index, &value) in values.iter().enumerate() {
                            // 检查NaN值
                            if value.is_nan() {
                                nan_count += 1;
                                if self.config.strict_validation {
                                    result.add_error(ValidationError::InvalidValue {
                                        index,
                                        channel,
                                        value,
                                        reason: "NaN value detected".to_string(),
                                    });
                                }
                            }
                            
                            // 检查无穷值
                            if value.is_infinite() {
                                inf_count += 1;
                                if self.config.strict_validation {
                                    result.add_error(ValidationError::InvalidValue {
                                        index,
                                        channel,
                                        value,
                                        reason: "Infinite value detected".to_string(),
                                    });
                                }
                            }
                            
                            // 检查值范围
                            if value < data_1d.output_range.0 || value > data_1d.output_range.1 {
                                out_of_range_count += 1;
                                if self.config.check_value_ranges {
                                    result.add_warning(ValidationWarning::ValueOutOfRange {
                                        index,
                                        channel,
                                        value,
                                        expected_min: data_1d.output_range.0,
                                        expected_max: data_1d.output_range.1,
                                    });
                                }
                            }
                        }
                    }
                }
            },
            LutType::ThreeDimensional => {
                if let Some(data_3d) = &lut_data.data_3d {
                    let mut index = 0;
                    for r_slice in data_3d {
                        for g_slice in r_slice {
                            for color in g_slice {
                                for (channel, &value) in color.iter().enumerate() {
                                    // 检查NaN值
                                    if value.is_nan() {
                                        nan_count += 1;
                                        if self.config.strict_validation {
                                            result.add_error(ValidationError::InvalidValue {
                                                index,
                                                channel,
                                                value,
                                                reason: "NaN value detected".to_string(),
                                            });
                                        }
                                    }
                                    
                                    // 检查无穷值
                                    if value.is_infinite() {
                                        inf_count += 1;
                                        if self.config.strict_validation {
                                            result.add_error(ValidationError::InvalidValue {
                                                index,
                                                channel,
                                                value,
                                                reason: "Infinite value detected".to_string(),
                                            });
                                        }
                                    }
                                    
                                    // 对于3D LUT，使用默认范围 [0.0, 1.0]
                                    if value < 0.0 || value > 1.0 {
                                        out_of_range_count += 1;
                                        if self.config.check_value_ranges {
                                            result.add_warning(ValidationWarning::ValueOutOfRange {
                                                index,
                                                channel,
                                                value,
                                                expected_min: 0.0,
                                                expected_max: 1.0,
                                            });
                                        }
                                    }
                                }
                                index += 1;
                            }
                        }
                    }
                }
            },
            _ => {}
        }
        
        // 添加统计信息
        if nan_count > 0 {
            result.add_info(ValidationInfo::StatisticInfo {
                category: "data_quality".to_string(),
                message: format!("Found {} NaN values", nan_count),
            });
        }
        
        if inf_count > 0 {
            result.add_info(ValidationInfo::StatisticInfo {
                category: "data_quality".to_string(),
                message: format!("Found {} infinite values", inf_count),
            });
        }
        
        if out_of_range_count > 0 {
            result.add_info(ValidationInfo::StatisticInfo {
                category: "data_quality".to_string(),
                message: format!("Found {} out-of-range values", out_of_range_count),
            });
        }
    }

    /// 验证数值范围
    fn validate_value_ranges(&self, lut_data: &LutData, result: &mut ValidationResult) {
        let mut min_values = [f32::INFINITY; 3];
        let mut max_values = [f32::NEG_INFINITY; 3];
        
        // 计算实际的最小最大值
        match lut_data.lut_type {
            LutType::OneDimensional => {
                if let Some(data_1d) = &lut_data.data_1d {
                    for &value in &data_1d.red {
                        if value.is_finite() {
                            min_values[0] = min_values[0].min(value);
                            max_values[0] = max_values[0].max(value);
                        }
                    }
                    for &value in &data_1d.green {
                        if value.is_finite() {
                            min_values[1] = min_values[1].min(value);
                            max_values[1] = max_values[1].max(value);
                        }
                    }
                    for &value in &data_1d.blue {
                        if value.is_finite() {
                            min_values[2] = min_values[2].min(value);
                            max_values[2] = max_values[2].max(value);
                        }
                    }
                }
            },
            LutType::ThreeDimensional => {
                if let Some(data_3d) = &lut_data.data_3d {
                    for r_slice in data_3d {
                        for g_slice in r_slice {
                            for color in g_slice {
                                for (i, &value) in color.iter().enumerate() {
                                    if value.is_finite() {
                                        min_values[i] = min_values[i].min(value);
                                        max_values[i] = max_values[i].max(value);
                                    }
                                }
                            }
                        }
                    }
                }
            },
            _ => {}
        }
        
        // 检查是否有未使用的输出范围
        if let Some(data_1d) = &lut_data.data_1d {
            let output_range_size = data_1d.output_range.1 - data_1d.output_range.0;
            for i in 0..3 {
                if min_values[i].is_finite() && max_values[i].is_finite() {
                    let actual_range_size = max_values[i] - min_values[i];
                    let utilization = actual_range_size / output_range_size;
                    
                    if utilization < self.config.min_range_utilization {
                        result.add_warning(ValidationWarning::UnderutilizedRange {
                            channel: i,
                            utilization,
                            min_value: min_values[i],
                            max_value: max_values[i],
                        });
                    }
                }
            }
        }
        
        // 检查域范围
        for i in 0..3 {
            if lut_data.domain_min[i] >= lut_data.domain_max[i] {
                result.add_error(ValidationError::InvalidDomainRange {
                    channel: i,
                    min: lut_data.domain_min[i],
                    max: lut_data.domain_max[i],
                });
            }
        }
    }

    /// 验证格式特定规则
    fn validate_format_specific(&self, lut_data: &LutData, result: &mut ValidationResult) {
        match lut_data.format {
            LutFormat::Cube => self.validate_cube_format(lut_data, result),
            LutFormat::ThreeDL => self.validate_3dl_format(lut_data, result),
            LutFormat::Lut => self.validate_lut_format(lut_data, result),
            LutFormat::Csp => self.validate_csp_format(lut_data, result),
            _ => {
                result.add_info(ValidationInfo::FormatInfo {
                    message: format!("No specific validation rules for {:?} format", lut_data.format),
                });
            }
        }
    }

    /// 验证Cube格式
    fn validate_cube_format(&self, lut_data: &LutData, result: &mut ValidationResult) {
        // Cube格式应该是3D LUT
        if lut_data.lut_type != LutType::ThreeDimensional {
            result.add_error(ValidationError::FormatTypeMismatch {
                format: lut_data.format,
                expected_type: LutType::ThreeDimensional,
                actual_type: lut_data.lut_type,
            });
        }
        
        // 检查常见的Cube格式大小
        let common_sizes = [8, 16, 17, 32, 33, 64, 65];
        if !common_sizes.contains(&lut_data.size) {
            result.add_warning(ValidationWarning::UncommonSize {
                size: lut_data.size,
                common_sizes: common_sizes.to_vec(),
            });
        }
        
        // 检查标题
        if lut_data.title.is_none() {
            result.add_info(ValidationInfo::MissingOptionalData {
                field: "title".to_string(),
            });
        }
    }

    /// 验证3DL格式
    fn validate_3dl_format(&self, lut_data: &LutData, result: &mut ValidationResult) {
        // 3DL格式应该是3D LUT
        if lut_data.lut_type != LutType::ThreeDimensional {
            result.add_error(ValidationError::FormatTypeMismatch {
                format: lut_data.format,
                expected_type: LutType::ThreeDimensional,
                actual_type: lut_data.lut_type,
            });
        }
        
        // 3DL格式通常不支持元数据
        if !lut_data.metadata.is_empty() {
            result.add_warning(ValidationWarning::UnsupportedFeature {
                format: lut_data.format,
                feature: "metadata".to_string(),
            });
        }
    }

    /// 验证LUT格式
    fn validate_lut_format(&self, lut_data: &LutData, result: &mut ValidationResult) {
        // LUT格式应该是1D LUT
        if lut_data.lut_type != LutType::OneDimensional {
            result.add_error(ValidationError::FormatTypeMismatch {
                format: lut_data.format,
                expected_type: LutType::OneDimensional,
                actual_type: lut_data.lut_type,
            });
        }
        
        // 检查常见的1D LUT大小
        let common_sizes = [256, 512, 1024, 4096];
        if !common_sizes.contains(&lut_data.size) {
            result.add_warning(ValidationWarning::UncommonSize {
                size: lut_data.size,
                common_sizes: common_sizes.to_vec(),
            });
        }
    }

    /// 验证CSP格式
    fn validate_csp_format(&self, lut_data: &LutData, result: &mut ValidationResult) {
        // CSP格式应该是3D LUT
        if lut_data.lut_type != LutType::ThreeDimensional {
            result.add_error(ValidationError::FormatTypeMismatch {
                format: lut_data.format,
                expected_type: LutType::ThreeDimensional,
                actual_type: lut_data.lut_type,
            });
        }
        
        // CSP格式支持元数据
        if lut_data.metadata.is_empty() {
            result.add_info(ValidationInfo::MissingOptionalData {
                field: "metadata".to_string(),
            });
        }
    }

    /// 验证性能特性
    fn validate_performance(&self, lut_data: &LutData, result: &mut ValidationResult) {
        // 检查LUT大小对性能的影响
        let performance_impact = self.estimate_performance_impact(lut_data);
        
        match performance_impact {
            PerformanceImpact::Low => {
                result.add_info(ValidationInfo::PerformanceInfo {
                    message: "LUT size is optimal for performance".to_string(),
                });
            }
            PerformanceImpact::Medium => {
                result.add_info(ValidationInfo::PerformanceInfo {
                    message: "LUT size may have moderate performance impact".to_string(),
                });
            }
            PerformanceImpact::High => {
                result.add_warning(ValidationWarning::PerformanceImpact {
                    size: lut_data.size,
                    impact: performance_impact,
                });
            }
            PerformanceImpact::VeryHigh => {
                result.add_error(ValidationError::PerformanceIssue {
                    size: lut_data.size,
                    issue: "LUT size is too large for real-time processing".to_string(),
                });
            }
        }
        
        // 检查内存使用
        let memory_usage = self.estimate_memory_usage(lut_data);
        if memory_usage > self.config.max_memory_usage_mb {
            result.add_warning(ValidationWarning::HighMemoryUsage {
                usage_mb: memory_usage,
                max_recommended_mb: self.config.max_memory_usage_mb,
            });
        }
    }

    /// 验证兼容性
    fn validate_compatibility(&self, lut_data: &LutData, result: &mut ValidationResult) {
        // 检查与常见软件的兼容性
        let compatibility_issues = self.check_software_compatibility(lut_data);
        
        for issue in compatibility_issues {
            result.add_warning(ValidationWarning::CompatibilityIssue {
                software: issue.software,
                issue: issue.description,
                suggestion: issue.suggestion,
            });
        }
        
        // 检查标准合规性
        self.check_standard_compliance(lut_data, result);
    }

    /// 估算性能影响
    fn estimate_performance_impact(&self, lut_data: &LutData) -> PerformanceImpact {
        let data_points = match lut_data.lut_type {
            LutType::OneDimensional => lut_data.size,
            LutType::ThreeDimensional => lut_data.size * lut_data.size * lut_data.size,
            _ => 0,
        };
        
        match data_points {
            0..=1000 => PerformanceImpact::Low,
            1001..=10000 => PerformanceImpact::Medium,
            10001..=100000 => PerformanceImpact::High,
            _ => PerformanceImpact::VeryHigh,
        }
    }

    /// 估算内存使用
    fn estimate_memory_usage(&self, lut_data: &LutData) -> f32 {
        // 每个颜色值3个f32，每个f32占4字节
        let data_size = match lut_data.lut_type {
            LutType::OneDimensional => {
                if let Some(data_1d) = &lut_data.data_1d {
                    data_1d.red.len() * 3 * 4
                } else {
                    0
                }
            },
            LutType::ThreeDimensional => {
                if let Some(data_3d) = &lut_data.data_3d {
                    let total_points = data_3d.len() * data_3d.get(0).map_or(0, |v| v.len()) * data_3d.get(0).and_then(|v| v.get(0)).map_or(0, |v| v.len());
                    total_points * 3 * 4
                } else {
                    0
                }
            },
            _ => 0,
        };
        
        // 加上结构体开销和元数据
        let metadata_size = lut_data.metadata.iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>();
        
        let total_bytes = data_size + metadata_size + 1024; // 1KB结构体开销
        total_bytes as f32 / 1024.0 / 1024.0 // 转换为MB
    }

    /// 检查软件兼容性
    fn check_software_compatibility(&self, lut_data: &LutData) -> Vec<CompatibilityIssue> {
        let mut issues = Vec::new();
        
        // 检查DaVinci Resolve兼容性
        if lut_data.format == LutFormat::Cube && lut_data.size > 65 {
            issues.push(CompatibilityIssue {
                software: "DaVinci Resolve".to_string(),
                description: "Cube LUTs larger than 65x65x65 may not be supported".to_string(),
                suggestion: "Consider using a 65x65x65 or smaller LUT".to_string(),
            });
        }
        
        // 检查Adobe Premiere兼容性
        if lut_data.format == LutFormat::ThreeDL && lut_data.size != 32 {
            issues.push(CompatibilityIssue {
                software: "Adobe Premiere".to_string(),
                description: "3DL format works best with 32x32x32 size".to_string(),
                suggestion: "Use 32x32x32 size for optimal compatibility".to_string(),
            });
        }
        
        issues
    }

    /// 检查标准合规性
    fn check_standard_compliance(&self, lut_data: &LutData, result: &mut ValidationResult) {
        // 检查颜色空间标准
        if let Some(data_1d) = &lut_data.data_1d {
            if data_1d.input_range != (0.0, 1.0) {
                result.add_info(ValidationInfo::StandardCompliance {
                    standard: "sRGB".to_string(),
                    message: "Input range is not standard 0.0-1.0".to_string(),
                });
            }
            
            if data_1d.output_range != (0.0, 1.0) {
                result.add_info(ValidationInfo::StandardCompliance {
                    standard: "sRGB".to_string(),
                    message: "Output range is not standard 0.0-1.0".to_string(),
                });
            }
        }
    }

    /// 快速验证（仅基本检查）
    pub fn quick_validate(&self, lut_data: &LutData) -> bool {
        // 基本结构检查
        if lut_data.size == 0 {
            return false;
        }
        
        // 数据检查
        match lut_data.lut_type {
            LutType::OneDimensional => {
                if let Some(data_1d) = &lut_data.data_1d {
                    if data_1d.red.is_empty() || data_1d.green.is_empty() || data_1d.blue.is_empty() {
                        return false;
                    }
                    
                    // 范围检查
                    if data_1d.input_range.0 >= data_1d.input_range.1 ||
                       data_1d.output_range.0 >= data_1d.output_range.1 {
                        return false;
                    }
                } else {
                    return false;
                }
            },
            LutType::ThreeDimensional => {
                if let Some(data_3d) = &lut_data.data_3d {
                    let s = lut_data.size as usize;
                    if s == 0 || data_3d.is_empty() {
                        return false;
                    }
                    // 要求严格的立方体尺寸匹配 size x size x size
                    if data_3d.len() != s { return false; }
                    for plane in data_3d {
                        if plane.len() != s { return false; }
                        for row in plane {
                            if row.len() != s { return false; }
                            for rgb in row {
                                if rgb.len() != 3 { return false; }
                            }
                        }
                    }
                } else {
                    return false;
                }
            },
            _ => return false,
        }
        
        true
    }

    /// 验证文件路径
    pub fn validate_file_path(&self, path: &Path) -> AppResult<()> {
        if !path.exists() {
            return Err(AppError::NotFound(format!("File not found: {}", path.display())));
        }
        
        if !path.is_file() {
            return Err(AppError::Validation(
                "Path is not a file".to_string()
            ));
        }
        
        // 检查文件扩展名
        if let Some(extension) = path.extension() {
            let ext = extension.to_string_lossy().to_lowercase();
            let supported_extensions = ["cube", "3dl", "lut", "csp", "vlt", "mga", "m3d", "look"];
            
            if !supported_extensions.contains(&ext.as_str()) {
                return Err(AppError::Validation(format!(
                    "Unsupported file extension: {}", ext
                )));
            }
        } else {
            return Err(AppError::Validation(
                "File has no extension".to_string()
            ));
        }
        
        Ok(())
    }

    /// 批量验证
    pub fn batch_validate(&self, lut_data_list: &[LutData]) -> Vec<ValidationResult> {
        lut_data_list.iter()
            .map(|lut_data| self.validate(lut_data))
            .collect()
    }

    /// 生成验证报告
    pub fn generate_report(&self, results: &[ValidationResult]) -> ValidationReport {
        let mut report = ValidationReport::new();
        
        for result in results {
            report.total_validated += 1;
            
            if result.is_valid() {
                report.valid_count += 1;
            } else {
                report.invalid_count += 1;
            }
            
            report.total_errors += result.errors.len();
            report.total_warnings += result.warnings.len();
            report.total_info += result.info.len();
        }
        
        report.success_rate = if report.total_validated > 0 {
            report.valid_count as f32 / report.total_validated as f32
        } else {
            0.0
        };
        
        report
    }
}

impl Default for LutValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// 验证配置
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// 严格验证模式
    pub strict_validation: bool,
    /// 检查数值范围
    pub check_value_ranges: bool,
    /// 检查性能影响
    pub check_performance: bool,
    /// 检查兼容性
    pub check_compatibility: bool,
    /// 最大LUT大小
    pub max_lut_size: usize,
    /// 最大内存使用（MB）
    pub max_memory_usage_mb: f32,
    /// 最小范围利用率
    pub min_range_utilization: f32,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            strict_validation: false,
            check_value_ranges: true,
            check_performance: true,
            check_compatibility: true,
            max_lut_size: 128,
            max_memory_usage_mb: 100.0,
            min_range_utilization: 0.1,
        }
    }
}

/// 验证结果
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub info: Vec<ValidationInfo>,
    pub validation_time_ms: u64,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            info: Vec::new(),
            validation_time_ms: 0,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    pub fn add_error(&mut self, error: ValidationError) {
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: ValidationWarning) {
        self.warnings.push(warning);
    }

    pub fn add_info(&mut self, info: ValidationInfo) {
        self.info.push(info);
    }

    pub fn severity_level(&self) -> ValidationSeverity {
        if !self.errors.is_empty() {
            ValidationSeverity::Error
        } else if !self.warnings.is_empty() {
            ValidationSeverity::Warning
        } else {
            ValidationSeverity::Info
        }
    }
}

/// 验证错误
#[derive(Debug, Clone)]
pub enum ValidationError {
    InvalidSize {
        size: usize,
        message: String,
    },
    DataLengthMismatch {
        expected: usize,
        actual: usize,
    },
    InvalidRange {
        range_type: String,
        min: f32,
        max: f32,
    },
    InvalidDomainRange {
        channel: usize,
        min: f32,
        max: f32,
    },
    InvalidValue {
        index: usize,
        channel: usize,
        value: f32,
        reason: String,
    },
    UnsupportedType {
        lut_type: LutType,
    },
    FormatTypeMismatch {
        format: LutFormat,
        expected_type: LutType,
        actual_type: LutType,
    },
    PerformanceIssue {
        size: usize,
        issue: String,
    },
}

/// 验证警告
#[derive(Debug, Clone)]
pub enum ValidationWarning {
    LargeSize {
        size: usize,
        max_recommended: usize,
    },
    ValueOutOfRange {
        index: usize,
        channel: usize,
        value: f32,
        expected_min: f32,
        expected_max: f32,
    },
    UnderutilizedRange {
        channel: usize,
        utilization: f32,
        min_value: f32,
        max_value: f32,
    },
    UncommonSize {
        size: usize,
        common_sizes: Vec<usize>,
    },
    UnsupportedFeature {
        format: LutFormat,
        feature: String,
    },
    PerformanceImpact {
        size: usize,
        impact: PerformanceImpact,
    },
    HighMemoryUsage {
        usage_mb: f32,
        max_recommended_mb: f32,
    },
    CompatibilityIssue {
        software: String,
        issue: String,
        suggestion: String,
    },
}

/// 验证信息
#[derive(Debug, Clone)]
pub enum ValidationInfo {
    StatisticInfo {
        category: String,
        message: String,
    },
    FormatInfo {
        message: String,
    },
    MissingOptionalData {
        field: String,
    },
    PerformanceInfo {
        message: String,
    },
    StandardCompliance {
        standard: String,
        message: String,
    },
}

/// 性能影响级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerformanceImpact {
    Low,
    Medium,
    High,
    VeryHigh,
}

/// 验证严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationSeverity {
    Info,
    Warning,
    Error,
}

/// 兼容性问题
#[derive(Debug, Clone)]
pub struct CompatibilityIssue {
    pub software: String,
    pub description: String,
    pub suggestion: String,
}

/// 验证报告
#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub total_validated: usize,
    pub valid_count: usize,
    pub invalid_count: usize,
    pub total_errors: usize,
    pub total_warnings: usize,
    pub total_info: usize,
    pub success_rate: f32,
    pub generated_at: std::time::SystemTime,
}

impl ValidationReport {
    pub fn new() -> Self {
        Self {
            total_validated: 0,
            valid_count: 0,
            invalid_count: 0,
            total_errors: 0,
            total_warnings: 0,
            total_info: 0,
            success_rate: 0.0,
            generated_at: std::time::SystemTime::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_valid_3d_lut() -> LutData {
        LutData {
            lut_type: LutType::ThreeDimensional,
            format: LutFormat::Cube,
            size: 2,
            title: Some("Test LUT".to_string()),
            description: Some("Test description".to_string()),
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
            data_3d: Some(vec![
                vec![
                    vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
                    vec![[0.0, 1.0, 0.0], [0.0, 1.0, 1.0]]
                ],
                vec![
                    vec![[1.0, 0.0, 0.0], [1.0, 0.0, 1.0]],
                    vec![[1.0, 1.0, 0.0], [1.0, 1.0, 1.0]]
                ]
            ]),
            data_1d: None,
            metadata: HashMap::new(),
        }
    }

    fn create_invalid_lut() -> LutData {
        LutData {
            lut_type: LutType::ThreeDimensional,
            format: LutFormat::Cube,
            size: 2,
            title: None,
            description: None,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
            data_3d: Some(vec![
                vec![
                    vec![[0.0, 0.0, 0.0]] // 数据长度不匹配
                ]
            ]),
            data_1d: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_validator_creation() {
        let validator = LutValidator::new();
        assert!(!validator.config.strict_validation);
        
        let config = ValidationConfig {
            strict_validation: true,
            ..Default::default()
        };
        let strict_validator = LutValidator::with_config(config);
        assert!(strict_validator.config.strict_validation);
    }

    #[test]
    fn test_valid_lut_validation() {
        let validator = LutValidator::new();
        let lut_data = create_valid_3d_lut();
        
        let result = validator.validate(&lut_data);
        
        assert!(result.is_valid());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_invalid_lut_validation() {
        let validator = LutValidator::new();
        let lut_data = create_invalid_lut();
        
        let result = validator.validate(&lut_data);
        
        assert!(!result.is_valid());
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_quick_validation() {
        let validator = LutValidator::new();
        let valid_lut = create_valid_3d_lut();
        let invalid_lut = create_invalid_lut();
        
        assert!(validator.quick_validate(&valid_lut));
        assert!(!validator.quick_validate(&invalid_lut));
    }

    #[test]
    fn test_performance_estimation() {
        let validator = LutValidator::new();
        let mut lut_data = create_valid_3d_lut();
        
        // 小LUT
        lut_data.size = 2;
        let impact = validator.estimate_performance_impact(&lut_data);
        assert_eq!(impact, PerformanceImpact::Low);
        
        // 大LUT
        lut_data.size = 100;
        let impact = validator.estimate_performance_impact(&lut_data);
        assert_eq!(impact, PerformanceImpact::VeryHigh);
    }

    #[test]
    fn test_memory_estimation() {
        let validator = LutValidator::new();
        let lut_data = create_valid_3d_lut();
        
        let memory_usage = validator.estimate_memory_usage(&lut_data);
        assert!(memory_usage > 0.0);
        assert!(memory_usage < 1.0); // 应该小于1MB
    }

    #[test]
    fn test_format_specific_validation() {
        let validator = LutValidator::new();
        let mut lut_data = create_valid_3d_lut();
        
        // 测试格式类型不匹配
        lut_data.lut_type = LutType::OneDimensional;
        let result = validator.validate(&lut_data);
        
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::FormatTypeMismatch { .. })));
    }

    #[test]
    fn test_batch_validation() {
        let validator = LutValidator::new();
        let luts = vec![create_valid_3d_lut(), create_invalid_lut()];
        
        let results = validator.batch_validate(&luts);
        
        assert_eq!(results.len(), 2);
        assert!(results[0].is_valid());
        assert!(!results[1].is_valid());
    }

    #[test]
    fn test_validation_report() {
        let validator = LutValidator::new();
        let luts = vec![create_valid_3d_lut(), create_invalid_lut()];
        let results = validator.batch_validate(&luts);
        
        let report = validator.generate_report(&results);
        
        assert_eq!(report.total_validated, 2);
        assert_eq!(report.valid_count, 1);
        assert_eq!(report.invalid_count, 1);
        assert_eq!(report.success_rate, 0.5);
    }

    #[test]
    fn test_file_path_validation() {
        let validator = LutValidator::new();
        
        // 测试不存在的文件
        let non_existent = Path::new("/non/existent/file.cube");
        assert!(validator.validate_file_path(non_existent).is_err());
    }

    #[test]
    fn test_software_compatibility() {
        let validator = LutValidator::new();
        let mut lut_data = create_valid_3d_lut();
        
        // 测试大尺寸Cube LUT的兼容性问题
        lut_data.size = 100;
        let issues = validator.check_software_compatibility(&lut_data);
        
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.software.contains("DaVinci")));
    }

    #[test]
    fn test_validation_severity() {
        let mut result = ValidationResult::new();
        
        assert_eq!(result.severity_level(), ValidationSeverity::Info);
        
        result.add_warning(ValidationWarning::LargeSize {
            size: 100,
            max_recommended: 64,
        });
        assert_eq!(result.severity_level(), ValidationSeverity::Warning);
        
        result.add_error(ValidationError::InvalidSize {
            size: 0,
            message: "Zero size".to_string(),
        });
        assert_eq!(result.severity_level(), ValidationSeverity::Error);
    }

    #[test]
    fn test_nan_and_infinite_values() {
        let validator = LutValidator::with_config(ValidationConfig {
            strict_validation: true,
            ..Default::default()
        });
        
        let mut lut_data = create_valid_3d_lut();
        if let Some(ref mut data_3d) = lut_data.data_3d {
            data_3d[0][0][0] = [f32::NAN, 0.0, 0.0];
            data_3d[0][0][1] = [f32::INFINITY, 0.0, 0.0];
        }
        
        let result = validator.validate(&lut_data);
        
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::InvalidValue { reason, .. } if reason.contains("NaN"))));
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::InvalidValue { reason, .. } if reason.contains("Infinite"))));
    }
}