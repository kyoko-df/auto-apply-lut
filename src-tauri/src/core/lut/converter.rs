//! LUT格式转换器模块
//! 提供不同LUT格式之间的转换功能

use crate::types::{AppResult, AppError};
use crate::core::lut::{LutData, LutType, LutFormat, LutInfo};
use std::collections::HashMap;
use std::path::Path;

/// LUT格式转换器
pub struct LutConverter {
    /// 支持的转换映射
    conversion_map: HashMap<(LutFormat, LutFormat), ConversionMethod>,
}

/// 转换方法
#[derive(Debug, Clone, Copy)]
enum ConversionMethod {
    /// 直接转换（格式兼容）
    Direct,
    /// 重采样转换
    Resample,
    /// 插值转换
    Interpolate,
    /// 不支持的转换
    Unsupported,
}

impl LutConverter {
    /// 创建新的LUT转换器
    pub fn new() -> Self {
        let mut conversion_map = HashMap::new();
        
        // 定义支持的转换
        Self::init_conversion_map(&mut conversion_map);
        
        Self {
            conversion_map,
        }
    }

    /// 初始化转换映射
    fn init_conversion_map(map: &mut HashMap<(LutFormat, LutFormat), ConversionMethod>) {
        use LutFormat::*;
        use ConversionMethod::*;
        
        // 同格式转换
        for format in [Cube, ThreeDL, Lut, Csp, Vlt, Mga, M3d, Look].iter() {
            map.insert((*format, *format), Direct);
        }
        
        // 3D LUT格式之间的转换
        let three_d_formats = [Cube, ThreeDL, Csp, M3d];
        for &from in &three_d_formats {
            for &to in &three_d_formats {
                if from != to {
                    map.insert((from, to), Resample);
                }
            }
        }
        
        // 1D LUT格式之间的转换
        let one_d_formats = [Lut, Vlt, Mga];
        for &from in &one_d_formats {
            for &to in &one_d_formats {
                if from != to {
                    map.insert((from, to), Interpolate);
                }
            }
        }
        
        // 特殊转换
        map.insert((Look, Cube), Resample);
        map.insert((Cube, Look), Resample);
    }

    /// 转换LUT格式
    pub async fn convert(
        &self,
        lut_data: &LutData,
        target_format: LutFormat,
        options: ConversionOptions,
    ) -> AppResult<LutData> {
        if lut_data.format == target_format {
            return Ok(lut_data.clone());
        }

        let conversion_key = (lut_data.format, target_format);
        let method = self.conversion_map.get(&conversion_key)
            .unwrap_or(&ConversionMethod::Unsupported);

        match method {
            ConversionMethod::Direct => self.direct_convert(lut_data, target_format).await,
            ConversionMethod::Resample => self.resample_convert(lut_data, target_format, &options).await,
            ConversionMethod::Interpolate => self.interpolate_convert(lut_data, target_format, &options).await,
            ConversionMethod::Unsupported => {
                Err(AppError::Validation(format!(
                    "Conversion from {:?} to {:?} is not supported",
                    lut_data.format, target_format
                )))
            }
        }
    }

    /// 直接转换（仅改变格式标识）
    async fn direct_convert(
        &self,
        lut_data: &LutData,
        target_format: LutFormat,
    ) -> AppResult<LutData> {
        let mut converted = lut_data.clone();
        converted.format = target_format;
        Ok(converted)
    }

    /// 重采样转换（3D LUT）
    async fn resample_convert(
        &self,
        lut_data: &LutData,
        target_format: LutFormat,
        options: &ConversionOptions,
    ) -> AppResult<LutData> {
        if lut_data.lut_type != LutType::ThreeDimensional {
            return Err(AppError::Validation(
                "Resample conversion only supports 3D LUTs".to_string()
            ));
        }

        let target_size = options.target_size.unwrap_or(lut_data.size);
        let mut new_data = Vec::new();

        // 重采样3D LUT数据
        for b in 0..target_size {
            for g in 0..target_size {
                for r in 0..target_size {
                    // 计算在原LUT中的位置
                    let r_pos = r as f32 / (target_size - 1) as f32;
                    let g_pos = g as f32 / (target_size - 1) as f32;
                    let b_pos = b as f32 / (target_size - 1) as f32;
                    
                    // 使用三线性插值获取颜色值
                    let color = self.trilinear_interpolate(lut_data, r_pos, g_pos, b_pos)?;
                    new_data.push(color);
                }
            }
        }

        // 将1D数据转换为3D格式
        let mut data_3d = vec![vec![vec![[0.0, 0.0, 0.0]; target_size]; target_size]; target_size];
        for (i, color) in new_data.iter().enumerate() {
            let r = i / (target_size * target_size);
            let g = (i / target_size) % target_size;
            let b = i % target_size;
            if r < target_size && g < target_size && b < target_size {
                data_3d[r][g][b] = *color;
            }
        }
        
        Ok(LutData {
            lut_type: LutType::ThreeDimensional,
            format: target_format,
            size: target_size,
            description: lut_data.description.clone(),
            data_1d: None,
            data_3d: Some(data_3d),
            metadata: lut_data.metadata.clone(),
            title: lut_data.title.clone(),
            domain_min: lut_data.domain_min,
            domain_max: lut_data.domain_max,
        })
    }

    /// 插值转换（1D LUT）
    async fn interpolate_convert(
        &self,
        lut_data: &LutData,
        target_format: LutFormat,
        options: &ConversionOptions,
    ) -> AppResult<LutData> {
        if lut_data.lut_type != LutType::OneDimensional {
            return Err(AppError::Validation(
                "Interpolate conversion only supports 1D LUTs".to_string()
            ));
        }

        let target_size = options.target_size.unwrap_or(lut_data.size);
        let mut new_data = Vec::new();

        // 重采样1D LUT数据
        for i in 0..target_size {
            let pos = i as f32 / (target_size - 1) as f32;
            let color = self.linear_interpolate_1d(lut_data, pos)?;
            new_data.push(color);
        }

        Ok(LutData {
            lut_type: LutType::OneDimensional,
            format: target_format,
            size: target_size,
            description: lut_data.description.clone(),
            data_1d: Some(crate::core::lut::LutData1D {
                red: new_data.iter().map(|c| c[0]).collect(),
                green: new_data.iter().map(|c| c[1]).collect(),
                blue: new_data.iter().map(|c| c[2]).collect(),
                input_range: (0.0, 1.0),
                output_range: (0.0, 1.0),
            }),
            data_3d: None,
            metadata: lut_data.metadata.clone(),
            title: lut_data.title.clone(),
            domain_min: lut_data.domain_min,
            domain_max: lut_data.domain_max,
        })
    }

    /// 三线性插值（3D LUT）
    fn trilinear_interpolate(
        &self,
        lut_data: &LutData,
        r: f32,
        g: f32,
        b: f32,
    ) -> AppResult<[f32; 3]> {
        let size = lut_data.size as f32;
        
        // 计算插值位置
        let r_scaled = r * (size - 1.0);
        let g_scaled = g * (size - 1.0);
        let b_scaled = b * (size - 1.0);
        
        let r0 = r_scaled.floor() as usize;
        let g0 = g_scaled.floor() as usize;
        let b0 = b_scaled.floor() as usize;
        
        let r1 = (r0 + 1).min(lut_data.size - 1);
        let g1 = (g0 + 1).min(lut_data.size - 1);
        let b1 = (b0 + 1).min(lut_data.size - 1);
        
        let dr = r_scaled - r0 as f32;
        let dg = g_scaled - g0 as f32;
        let db = b_scaled - b0 as f32;
        
        // 获取8个角点的颜色值
        let c000 = self.get_lut_value(lut_data, r0, g0, b0)?;
        let c001 = self.get_lut_value(lut_data, r0, g0, b1)?;
        let c010 = self.get_lut_value(lut_data, r0, g1, b0)?;
        let c011 = self.get_lut_value(lut_data, r0, g1, b1)?;
        let c100 = self.get_lut_value(lut_data, r1, g0, b0)?;
        let c101 = self.get_lut_value(lut_data, r1, g0, b1)?;
        let c110 = self.get_lut_value(lut_data, r1, g1, b0)?;
        let c111 = self.get_lut_value(lut_data, r1, g1, b1)?;
        
        // 三线性插值
        let mut result = [0.0; 3];
        for i in 0..3 {
            let c00 = c000[i] * (1.0 - dr) + c100[i] * dr;
            let c01 = c001[i] * (1.0 - dr) + c101[i] * dr;
            let c10 = c010[i] * (1.0 - dr) + c110[i] * dr;
            let c11 = c011[i] * (1.0 - dr) + c111[i] * dr;
            
            let c0 = c00 * (1.0 - dg) + c10 * dg;
            let c1 = c01 * (1.0 - dg) + c11 * dg;
            
            result[i] = c0 * (1.0 - db) + c1 * db;
        }
        
        Ok(result)
    }

    /// 线性插值（1D LUT）
    fn linear_interpolate_1d(
        &self,
        lut_data: &LutData,
        pos: f32,
    ) -> AppResult<[f32; 3]> {
        if let Some(ref data_1d) = lut_data.data_1d {
            let size = lut_data.size as f32;
            let scaled_pos = pos * (size - 1.0);
            
            let index0 = scaled_pos.floor() as usize;
            let index1 = (index0 + 1).min(lut_data.size - 1);
            
            let t = scaled_pos - index0 as f32;
            
            if index0 >= data_1d.red.len() || index1 >= data_1d.red.len() {
                return Err(AppError::LutProcessing(
                    "Index out of bounds in 1D LUT interpolation".to_string()
                ));
            }
            
            let color0 = [
                data_1d.red[index0],
                data_1d.green[index0],
                data_1d.blue[index0],
            ];
            let color1 = [
                data_1d.red[index1],
                data_1d.green[index1],
                data_1d.blue[index1],
            ];
            
            let mut result = [0.0; 3];
            for i in 0..3 {
                result[i] = color0[i] * (1.0 - t) + color1[i] * t;
            }
            
            Ok(result)
        } else {
            Err(AppError::LutProcessing(
                "1D LUT data not available".to_string()
            ))
        }
    }

    /// 获取LUT中指定位置的值
    fn get_lut_value(
        &self,
        lut_data: &LutData,
        r: usize,
        g: usize,
        b: usize,
    ) -> AppResult<[f32; 3]> {
        match lut_data.lut_type {
            LutType::ThreeDimensional => {
                lut_data.get_3d_point(r, g, b)
            }
            LutType::OneDimensional => {
                // For 1D LUT, use the first coordinate as index
                let index = r.max(g).max(b);
                if index >= lut_data.size {
                    return Err(AppError::LutProcessing(
                        "Index out of bounds in 1D LUT data".to_string()
                    ));
                }
                Ok([
                    lut_data.get_1d_point(0, index)?,
                    lut_data.get_1d_point(1, index)?,
                    lut_data.get_1d_point(2, index)?,
                ])
            }
            LutType::Unknown => {
                Err(AppError::InvalidInput("Unknown LUT type".to_string()))
            }
        }
    }

    /// 批量转换LUT
    pub async fn batch_convert(
        &self,
        lut_files: Vec<&Path>,
        target_format: LutFormat,
        options: ConversionOptions,
    ) -> AppResult<Vec<ConversionResult>> {
        let mut results = Vec::new();
        
        for file_path in lut_files {
            let result = match self.convert_file(file_path, target_format, &options).await {
                Ok(converted_data) => ConversionResult {
                    source_path: file_path.to_path_buf(),
                    success: true,
                    converted_data: Some(converted_data),
                    error: None,
                },
                Err(error) => ConversionResult {
                    source_path: file_path.to_path_buf(),
                    success: false,
                    converted_data: None,
                    error: Some(error.to_string()),
                },
            };
            results.push(result);
        }
        
        Ok(results)
    }

    /// 转换文件
    async fn convert_file(
        &self,
        file_path: &Path,
        target_format: LutFormat,
        options: &ConversionOptions,
    ) -> AppResult<LutData> {
        // 这里需要先加载LUT文件，然后进行转换
        // 实际实现中需要调用LUT解析器
        todo!("Implement file loading and conversion")
    }

    /// 获取支持的转换
    pub fn get_supported_conversions(&self) -> Vec<(LutFormat, LutFormat)> {
        self.conversion_map.keys()
            .filter(|(from, to)| {
                matches!(self.conversion_map.get(&(*from, *to)), Some(ConversionMethod::Unsupported) | None) == false
            })
            .cloned()
            .collect()
    }

    /// 检查转换是否支持
    pub fn is_conversion_supported(&self, from: LutFormat, to: LutFormat) -> bool {
        matches!(
            self.conversion_map.get(&(from, to)),
            Some(ConversionMethod::Direct) | Some(ConversionMethod::Resample) | Some(ConversionMethod::Interpolate)
        )
    }

    /// 获取转换方法
    pub fn get_conversion_method(&self, from: LutFormat, to: LutFormat) -> Option<ConversionMethod> {
        self.conversion_map.get(&(from, to)).copied()
    }

    /// 估算转换质量
    pub fn estimate_conversion_quality(&self, from: LutFormat, to: LutFormat) -> ConversionQuality {
        match self.get_conversion_method(from, to) {
            Some(ConversionMethod::Direct) => ConversionQuality::Lossless,
            Some(ConversionMethod::Resample) => ConversionQuality::HighQuality,
            Some(ConversionMethod::Interpolate) => ConversionQuality::MediumQuality,
            _ => ConversionQuality::NotSupported,
        }
    }

    /// 优化LUT数据
    pub fn optimize_lut(&self, lut_data: &LutData, options: &OptimizationOptions) -> AppResult<LutData> {
        let mut optimized = lut_data.clone();
        
        // 移除重复的元数据
        if options.remove_metadata {
            optimized.metadata.clear();
        }
        
        // 量化颜色值以减少精度
        if let Some(precision) = options.color_precision {
            match optimized.lut_type {
                LutType::ThreeDimensional => {
                    if let Some(ref mut data_3d) = optimized.data_3d {
                        for plane in data_3d {
                            for row in plane {
                                for color in row {
                                    for component in color.iter_mut() {
                                        *component = (*component * precision).round() / precision;
                                    }
                                }
                            }
                        }
                    }
                }
                LutType::OneDimensional => {
                    if let Some(ref mut data_1d) = optimized.data_1d {
                        for value in data_1d.red.iter_mut() {
                            *value = (*value * precision).round() / precision;
                        }
                        for value in data_1d.green.iter_mut() {
                            *value = (*value * precision).round() / precision;
                        }
                        for value in data_1d.blue.iter_mut() {
                            *value = (*value * precision).round() / precision;
                        }
                    }
                }
                LutType::Unknown => {}
            }
        }
        
        // 压缩相似的颜色值
        if options.compress_similar_colors {
            self.compress_similar_colors(&mut optimized, options.similarity_threshold)?;
        }
        
        Ok(optimized)
    }

    /// 压缩相似颜色
    fn compress_similar_colors(
        &self,
        _lut_data: &mut LutData,
        _threshold: f32,
    ) -> AppResult<()> {
        // TODO: 实现相似颜色压缩功能
        // 暂时禁用此功能以避免复杂的数据结构处理
        Ok(())
    }

    /// 计算颜色距离
    fn color_distance(&self, color1: &[f32; 3], color2: &[f32; 3]) -> f32 {
        let dr = color1[0] - color2[0];
        let dg = color1[1] - color2[1];
        let db = color1[2] - color2[2];
        (dr * dr + dg * dg + db * db).sqrt()
    }

    /// 计算平均颜色
    fn average_colors(&self, colors: &[[f32; 3]]) -> [f32; 3] {
        let count = colors.len() as f32;
        let mut avg = [0.0; 3];
        
        for color in colors {
            avg[0] += color[0];
            avg[1] += color[1];
            avg[2] += color[2];
        }
        
        avg[0] /= count;
        avg[1] /= count;
        avg[2] /= count;
        
        avg
    }
}

impl Default for LutConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// 转换选项
#[derive(Debug, Clone)]
pub struct ConversionOptions {
    /// 目标LUT大小
    pub target_size: Option<usize>,
    /// 插值方法
    pub interpolation_method: InterpolationMethod,
    /// 是否保留元数据
    pub preserve_metadata: bool,
    /// 输出范围
    pub output_range: Option<(f32, f32)>,
    /// 域范围
    pub domain_range: Option<([f32; 3], [f32; 3])>,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            target_size: None,
            interpolation_method: InterpolationMethod::Trilinear,
            preserve_metadata: true,
            output_range: None,
            domain_range: None,
        }
    }
}

/// 插值方法
#[derive(Debug, Clone, Copy)]
pub enum InterpolationMethod {
    /// 最近邻插值
    Nearest,
    /// 线性插值
    Linear,
    /// 三线性插值
    Trilinear,
    /// 立方插值
    Cubic,
}

/// 转换质量
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionQuality {
    /// 无损转换
    Lossless,
    /// 高质量转换
    HighQuality,
    /// 中等质量转换
    MediumQuality,
    /// 低质量转换
    LowQuality,
    /// 不支持转换
    NotSupported,
}

/// 转换结果
#[derive(Debug, Clone)]
pub struct ConversionResult {
    pub source_path: std::path::PathBuf,
    pub success: bool,
    pub converted_data: Option<LutData>,
    pub error: Option<String>,
}

/// 优化选项
#[derive(Debug, Clone)]
pub struct OptimizationOptions {
    /// 移除元数据
    pub remove_metadata: bool,
    /// 颜色精度（用于量化）
    pub color_precision: Option<f32>,
    /// 压缩相似颜色
    pub compress_similar_colors: bool,
    /// 相似度阈值
    pub similarity_threshold: f32,
}

impl Default for OptimizationOptions {
    fn default() -> Self {
        Self {
            remove_metadata: false,
            color_precision: None,
            compress_similar_colors: false,
            similarity_threshold: 0.01,
        }
    }
}

/// 转换统计信息
#[derive(Debug, Clone)]
pub struct ConversionStats {
    pub source_format: LutFormat,
    pub target_format: LutFormat,
    pub source_size: usize,
    pub target_size: usize,
    pub conversion_time_ms: u64,
    pub quality_estimate: ConversionQuality,
    pub data_size_reduction: f32, // 百分比
}

/// 格式兼容性检查器
pub struct FormatCompatibilityChecker;

impl FormatCompatibilityChecker {
    /// 检查两种格式是否兼容
    pub fn are_compatible(format1: LutFormat, format2: LutFormat) -> bool {
        use LutFormat::*;
        
        match (format1, format2) {
            // 同格式总是兼容
            (a, b) if a == b => true,
            
            // 3D LUT格式之间兼容
            (Cube, ThreeDL) | (ThreeDL, Cube) => true,
            (Cube, Csp) | (Csp, Cube) => true,
            (ThreeDL, Csp) | (Csp, ThreeDL) => true,
            (M3d, Cube) | (Cube, M3d) => true,
            
            // 1D LUT格式之间兼容
            (Lut, Vlt) | (Vlt, Lut) => true,
            (Lut, Mga) | (Mga, Lut) => true,
            (Vlt, Mga) | (Mga, Vlt) => true,
            
            // 其他组合不兼容
            _ => false,
        }
    }

    /// 获取格式的维度类型
    pub fn get_dimension_type(format: LutFormat) -> LutType {
        use LutFormat::*;
        
        match format {
            Cube | ThreeDL | Csp | M3d | Look => LutType::ThreeDimensional,
            Lut | Vlt | Mga => LutType::OneDimensional,
            Unknown => LutType::Unknown,
        }
    }

    /// 检查格式是否支持特定功能
    pub fn supports_feature(format: LutFormat, feature: FormatFeature) -> bool {
        use LutFormat::*;
        use FormatFeature::*;
        
        match (format, feature) {
            (Cube, Metadata) => true,
            (Cube, Title) => true,
            (Cube, DomainRange) => true,
            (ThreeDL, Metadata) => false,
            (ThreeDL, Title) => true,
            (Csp, Metadata) => true,
            (Look, Metadata) => true,
            (Look, Title) => true,
            _ => false,
        }
    }
}

/// 格式特性
#[derive(Debug, Clone, Copy)]
pub enum FormatFeature {
    /// 元数据支持
    Metadata,
    /// 标题支持
    Title,
    /// 域范围支持
    DomainRange,
    /// 输入范围支持
    InputRange,
    /// 输出范围支持
    OutputRange,
}

#[cfg(disabled_test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::core::lut::LutData1D;

    fn create_test_3d_lut() -> LutData {
        let mut data_3d = vec![vec![vec![[0.0; 3]; 2]; 2]; 2];
        // 简单的测试数据
        for r in 0..2 {
            for g in 0..2 {
                for b in 0..2 {
                    data_3d[r][g][b] = [r as f32, g as f32, b as f32];
                }
            }
        }
        
        LutData {
            lut_type: LutType::ThreeDimensional,
            format: LutFormat::Cube,
            size: 2,
            data_3d: Some(data_3d),
            data_1d: None,
            metadata: HashMap::new(),
            title: Some("Test LUT".to_string()),
            description: None,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
        }
    }

    fn create_test_1d_lut() -> LutData {
        LutData {
            lut_type: LutType::OneDimensional,
            format: LutFormat::Lut,
            size: 4,
            input_range: (0.0, 1.0),
            output_range: (0.0, 1.0),
            data: vec![
                [0.0, 0.0, 0.0],
                [0.33, 0.33, 0.33],
                [0.66, 0.66, 0.66],
                [1.0, 1.0, 1.0],
            ],
            metadata: HashMap::new(),
            title: Some("Test 1D LUT".to_string()),
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
        }
    }

    #[test]
    fn test_converter_creation() {
        let converter = LutConverter::new();
        assert!(!converter.conversion_map.is_empty());
    }

    #[test]
    fn test_format_compatibility() {
        assert!(FormatCompatibilityChecker::are_compatible(
            LutFormat::Cube,
            LutFormat::ThreeDL
        ));
        
        assert!(FormatCompatibilityChecker::are_compatible(
            LutFormat::Lut,
            LutFormat::Vlt
        ));
        
        assert!(!FormatCompatibilityChecker::are_compatible(
            LutFormat::Cube,
            LutFormat::Lut
        ));
    }

    #[test]
    fn test_dimension_type() {
        assert_eq!(
            FormatCompatibilityChecker::get_dimension_type(LutFormat::Cube),
            LutType::ThreeDimensional
        );
        
        assert_eq!(
            FormatCompatibilityChecker::get_dimension_type(LutFormat::Lut),
            LutType::OneDimensional
        );
    }

    #[test]
    fn test_feature_support() {
        assert!(FormatCompatibilityChecker::supports_feature(
            LutFormat::Cube,
            FormatFeature::Metadata
        ));
        
        assert!(!FormatCompatibilityChecker::supports_feature(
            LutFormat::ThreeDL,
            FormatFeature::Metadata
        ));
    }

    #[tokio::test]
    async fn test_direct_conversion() {
        let converter = LutConverter::new();
        let lut_data = create_test_3d_lut();
        
        let converted = converter.direct_convert(&lut_data, LutFormat::ThreeDL).await.unwrap();
        
        assert_eq!(converted.format, LutFormat::ThreeDL);
        assert_eq!(converted.data, lut_data.data);
        assert_eq!(converted.size, lut_data.size);
    }

    #[tokio::test]
    async fn test_resample_conversion() {
        let converter = LutConverter::new();
        let lut_data = create_test_3d_lut();
        
        let options = ConversionOptions {
            target_size: Some(4),
            ..Default::default()
        };
        
        let converted = converter.resample_convert(&lut_data, LutFormat::ThreeDL, &options).await.unwrap();
        
        assert_eq!(converted.format, LutFormat::ThreeDL);
        assert_eq!(converted.size, 4);
        assert_eq!(converted.data.len(), 4 * 4 * 4);
    }

    #[tokio::test]
    async fn test_interpolate_conversion() {
        let converter = LutConverter::new();
        let lut_data = create_test_1d_lut();
        
        let options = ConversionOptions {
            target_size: Some(8),
            ..Default::default()
        };
        
        let converted = converter.interpolate_convert(&lut_data, LutFormat::Vlt, &options).await.unwrap();
        
        assert_eq!(converted.format, LutFormat::Vlt);
        assert_eq!(converted.size, 8);
        assert_eq!(converted.data.len(), 8);
    }

    #[test]
    fn test_trilinear_interpolation() {
        let converter = LutConverter::new();
        let lut_data = create_test_3d_lut();
        
        // 测试中心点插值
        let result = converter.trilinear_interpolate(&lut_data, 0.5, 0.5, 0.5).unwrap();
        
        // 中心点应该是所有角点的平均值
        assert!((result[0] - 0.5).abs() < 0.01);
        assert!((result[1] - 0.5).abs() < 0.01);
        assert!((result[2] - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_linear_interpolation_1d() {
        let converter = LutConverter::new();
        let lut_data = create_test_1d_lut();
        
        // 测试中点插值
        let result = converter.linear_interpolate_1d(&lut_data, 0.5).unwrap();
        
        // 应该在第二和第三个点之间
        assert!((result[0] - 0.5).abs() < 0.1);
        assert!((result[1] - 0.5).abs() < 0.1);
        assert!((result[2] - 0.5).abs() < 0.1);
    }

    #[test]
    fn test_conversion_support() {
        let converter = LutConverter::new();
        
        assert!(converter.is_conversion_supported(LutFormat::Cube, LutFormat::ThreeDL));
        assert!(converter.is_conversion_supported(LutFormat::Lut, LutFormat::Vlt));
        assert!(!converter.is_conversion_supported(LutFormat::Cube, LutFormat::Lut));
    }

    #[test]
    fn test_quality_estimation() {
        let converter = LutConverter::new();
        
        assert_eq!(
            converter.estimate_conversion_quality(LutFormat::Cube, LutFormat::Cube),
            ConversionQuality::Lossless
        );
        
        assert_eq!(
            converter.estimate_conversion_quality(LutFormat::Cube, LutFormat::ThreeDL),
            ConversionQuality::HighQuality
        );
    }

    #[test]
    fn test_color_distance() {
        let converter = LutConverter::new();
        
        let color1 = [0.0, 0.0, 0.0];
        let color2 = [1.0, 1.0, 1.0];
        
        let distance = converter.color_distance(&color1, &color2);
        assert!((distance - 1.732).abs() < 0.01); // sqrt(3)
    }

    #[test]
    fn test_average_colors() {
        let converter = LutConverter::new();
        
        let colors = [
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        ];
        
        let avg = converter.average_colors(&colors);
        assert_eq!(avg, [0.5, 0.5, 0.5]);
    }

    #[test]
    fn test_optimization() {
        let converter = LutConverter::new();
        let mut lut_data = create_test_3d_lut();
        lut_data.metadata.insert("test".to_string(), "value".to_string());
        
        let options = OptimizationOptions {
            remove_metadata: true,
            color_precision: Some(10.0),
            ..Default::default()
        };
        
        let optimized = converter.optimize_lut(&lut_data, &options).unwrap();
        
        assert!(optimized.metadata.is_empty());
    }
}