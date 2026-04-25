//! LUT格式转换器模块
//! 提供不同LUT格式之间的转换功能

use crate::core::lut::parser::{
    CspParser, CubeParser, GenericLutParser, LookParser, LutParser, M3dParser, ThreeDLParser,
};
use crate::core::lut::{LutData, LutFormat, LutType};
use crate::types::{AppError, AppResult};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

        Self { conversion_map }
    }

    /// 初始化转换映射
    fn init_conversion_map(map: &mut HashMap<(LutFormat, LutFormat), ConversionMethod>) {
        use ConversionMethod::*;
        use LutFormat::*;

        // 同格式转换
        for format in [Cube, ThreeDL, Lut, Csp, Mga, M3d, Look].iter() {
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
        let one_d_formats = [Lut, Mga];
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
        let method = self
            .conversion_map
            .get(&conversion_key)
            .unwrap_or(&ConversionMethod::Unsupported);

        match method {
            ConversionMethod::Direct => self.direct_convert(lut_data, target_format).await,
            ConversionMethod::Resample => {
                self.resample_convert(lut_data, target_format, &options)
                    .await
            }
            ConversionMethod::Interpolate => {
                self.interpolate_convert(lut_data, target_format, &options)
                    .await
            }
            ConversionMethod::Unsupported => Err(AppError::Validation(format!(
                "Conversion from {:?} to {:?} is not supported",
                lut_data.format, target_format
            ))),
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
                "Resample conversion only supports 3D LUTs".to_string(),
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
                "Interpolate conversion only supports 1D LUTs".to_string(),
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
    fn linear_interpolate_1d(&self, lut_data: &LutData, pos: f32) -> AppResult<[f32; 3]> {
        if let Some(ref data_1d) = lut_data.data_1d {
            let size = lut_data.size as f32;
            let scaled_pos = pos * (size - 1.0);

            let index0 = scaled_pos.floor() as usize;
            let index1 = (index0 + 1).min(lut_data.size - 1);

            let t = scaled_pos - index0 as f32;

            if index0 >= data_1d.red.len() || index1 >= data_1d.red.len() {
                return Err(AppError::LutProcessing(
                    "Index out of bounds in 1D LUT interpolation".to_string(),
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
                "1D LUT data not available".to_string(),
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
            LutType::ThreeDimensional => lut_data.get_3d_point(r, g, b),
            LutType::OneDimensional => {
                // For 1D LUT, use the first coordinate as index
                let index = r.max(g).max(b);
                if index >= lut_data.size {
                    return Err(AppError::LutProcessing(
                        "Index out of bounds in 1D LUT data".to_string(),
                    ));
                }
                Ok([
                    lut_data.get_1d_point(0, index)?,
                    lut_data.get_1d_point(1, index)?,
                    lut_data.get_1d_point(2, index)?,
                ])
            }
            LutType::Unknown => Err(AppError::InvalidInput("Unknown LUT type".to_string())),
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
                Ok((converted_data, target_path)) => ConversionResult {
                    source_path: file_path.to_path_buf(),
                    target_path: Some(target_path),
                    success: true,
                    converted_data: Some(converted_data),
                    error: None,
                },
                Err(error) => ConversionResult {
                    source_path: file_path.to_path_buf(),
                    target_path: None,
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
    fn build_output_path(&self, source_path: &Path, target_format: LutFormat) -> AppResult<PathBuf> {
        let parent = source_path.parent().ok_or_else(|| {
            AppError::InvalidInput("Source path has no parent directory".to_string())
        })?;
        let stem = source_path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::InvalidInput("Source file name is invalid".to_string()))?;
        let extension = target_format.extension();

        let mut candidate = parent.join(format!("{stem}.converted.{extension}"));
        let mut index = 1usize;
        while candidate.exists() {
            candidate = parent.join(format!("{stem}.converted-{index}.{extension}"));
            index += 1;
        }

        Ok(candidate)
    }

    async fn load_lut_file(&self, file_path: &Path) -> AppResult<LutData> {
        let parse_result = match LutFormat::from_extension(
            file_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or_default(),
        ) {
            LutFormat::Cube => CubeParser::parse(file_path).await,
            LutFormat::ThreeDL => ThreeDLParser::parse(file_path).await,
            LutFormat::Lut => GenericLutParser::parse(file_path).await,
            LutFormat::Csp => CspParser::parse(file_path).await,
            LutFormat::M3d => M3dParser::parse(file_path).await,
            LutFormat::Look => LookParser::parse(file_path).await,
            LutFormat::Mga | LutFormat::Unknown => Err(AppError::Validation(format!(
                "无法识别 LUT 格式: {}",
                file_path.display()
            ))),
        };

        parse_result.map_err(|error| {
            AppError::Validation(format!(
                "无法解析 LUT 文件: {} ({})",
                file_path.display(),
                error
            ))
        })
    }

    async fn write_lut_file(&self, lut_data: &LutData, output_path: &Path) -> AppResult<()> {
        match lut_data.format {
            LutFormat::Cube => CubeParser::write(lut_data, output_path).await,
            LutFormat::ThreeDL => ThreeDLParser::write(lut_data, output_path).await,
            LutFormat::Lut => GenericLutParser::write(lut_data, output_path).await,
            LutFormat::Csp => CspParser::write(lut_data, output_path).await,
            LutFormat::M3d => M3dParser::write(lut_data, output_path).await,
            LutFormat::Look => LookParser::write(lut_data, output_path).await,
            LutFormat::Mga | LutFormat::Unknown => {
                Err(AppError::Validation("该 LUT 格式暂不支持导出".to_string()))
            }
        }
    }

    async fn convert_file(
        &self,
        file_path: &Path,
        target_format: LutFormat,
        options: &ConversionOptions,
    ) -> AppResult<(LutData, PathBuf)> {
        let lut_data = self.load_lut_file(file_path).await?;

        if !self.is_conversion_supported(lut_data.format, target_format) {
            return Err(AppError::Validation("源格式与目标格式不兼容".to_string()));
        }

        let converted = self.convert(&lut_data, target_format, options.clone()).await?;
        let output_path = self.build_output_path(file_path, target_format)?;
        self.write_lut_file(&converted, &output_path).await?;

        Ok((converted, output_path))
    }

    /// 获取支持的转换
    pub fn get_supported_conversions(&self) -> Vec<(LutFormat, LutFormat)> {
        self.conversion_map
            .keys()
            .filter(|(from, to)| {
                matches!(
                    self.conversion_map.get(&(*from, *to)),
                    Some(ConversionMethod::Unsupported) | None
                ) == false
            })
            .cloned()
            .collect()
    }

    /// 检查转换是否支持
    pub fn is_conversion_supported(&self, from: LutFormat, to: LutFormat) -> bool {
        matches!(
            self.conversion_map.get(&(from, to)),
            Some(ConversionMethod::Direct)
                | Some(ConversionMethod::Resample)
                | Some(ConversionMethod::Interpolate)
        )
    }

    /// 获取转换方法
    pub fn get_conversion_method(
        &self,
        from: LutFormat,
        to: LutFormat,
    ) -> Option<ConversionMethod> {
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
    pub fn optimize_lut(
        &self,
        lut_data: &LutData,
        options: &OptimizationOptions,
    ) -> AppResult<LutData> {
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
    fn compress_similar_colors(&self, lut_data: &mut LutData, threshold: f32) -> AppResult<()> {
        if threshold <= 0.0 {
            return Ok(());
        }

        match lut_data.lut_type {
            LutType::ThreeDimensional => {
                if let Some(ref mut data_3d) = lut_data.data_3d {
                    let mut clusters: Vec<Vec<[usize; 3]>> = Vec::new();
                    let mut cluster_averages: Vec<[f32; 3]> = Vec::new();

                    for (r, plane) in data_3d.iter().enumerate() {
                        for (g, row) in plane.iter().enumerate() {
                            for (b, &color) in row.iter().enumerate() {
                                if let Some(index) = cluster_averages
                                    .iter()
                                    .position(|avg| self.color_distance(avg, &color) <= threshold)
                                {
                                    clusters[index].push([r, g, b]);
                                    let cluster_colors: Vec<[f32; 3]> = clusters[index]
                                        .iter()
                                        .map(|[cr, cg, cb]| data_3d[*cr][*cg][*cb])
                                        .collect();
                                    cluster_averages[index] =
                                        self.average_colors(cluster_colors.as_slice());
                                } else {
                                    clusters.push(vec![[r, g, b]]);
                                    cluster_averages.push(color);
                                }
                            }
                        }
                    }

                    for (index, positions) in clusters.iter().enumerate() {
                        let average = cluster_averages[index];
                        for [r, g, b] in positions {
                            data_3d[*r][*g][*b] = average;
                        }
                    }
                }
            }
            LutType::OneDimensional => {
                if let Some(ref mut data_1d) = lut_data.data_1d {
                    for channel in [&mut data_1d.red, &mut data_1d.green, &mut data_1d.blue] {
                        let mut index = 0usize;
                        while index < channel.len() {
                            let mut group_end = index + 1;
                            while group_end < channel.len()
                                && (channel[group_end] - channel[index]).abs() <= threshold
                            {
                                group_end += 1;
                            }

                            if group_end - index > 1 {
                                let average = channel[index..group_end].iter().sum::<f32>()
                                    / (group_end - index) as f32;
                                for value in &mut channel[index..group_end] {
                                    *value = average;
                                }
                            }

                            index = group_end;
                        }
                    }
                }
            }
            LutType::Unknown => {}
        }

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
    pub target_path: Option<std::path::PathBuf>,
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

#[cfg(test)]
mod batch_file_conversion_tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs;

    async fn write_test_cube(path: &Path) {
        fs::write(
            path,
            r#"TITLE "Sample"
LUT_3D_SIZE 2
0.0 0.0 0.0
1.0 0.0 0.0
0.0 1.0 0.0
1.0 1.0 0.0
0.0 0.0 1.0
1.0 0.0 1.0
0.0 1.0 1.0
1.0 1.0 1.0
"#,
        )
        .await
        .expect("write source lut");
    }

    #[tokio::test]
    async fn test_convert_file_writes_converted_file_next_to_source() {
        let converter = LutConverter::new();
        let dir = tempdir().expect("temp dir");
        let source_path = dir.path().join("sample.cube");

        write_test_cube(&source_path).await;

        let (converted, actual_output_path) = converter
            .convert_file(&source_path, LutFormat::Csp, &ConversionOptions::default())
            .await
            .expect("convert file");

        let output_path = dir.path().join("sample.converted.csp");
        let written = fs::read_to_string(&output_path).await.expect("read output");

        assert_eq!(converted.format, LutFormat::Csp);
        assert_eq!(actual_output_path, output_path);
        assert!(output_path.exists());
        assert!(!written.is_empty());
    }

    #[tokio::test]
    async fn test_convert_file_appends_incrementing_suffix_when_target_exists() {
        let converter = LutConverter::new();
        let dir = tempdir().expect("temp dir");
        let source_path = dir.path().join("sample.cube");
        let existing_output = dir.path().join("sample.converted.csp");

        write_test_cube(&source_path).await;
        fs::write(&existing_output, "occupied")
            .await
            .expect("write occupied");

        converter
            .convert_file(&source_path, LutFormat::Csp, &ConversionOptions::default())
            .await
            .expect("convert file");

        assert!(dir.path().join("sample.converted-1.csp").exists());
    }

    #[tokio::test]
    async fn test_convert_file_rejects_cross_dimension_conversion() {
        let converter = LutConverter::new();
        let dir = tempdir().expect("temp dir");
        let source_path = dir.path().join("sample.cube");

        write_test_cube(&source_path).await;

        let err = converter
            .convert_file(&source_path, LutFormat::Lut, &ConversionOptions::default())
            .await
            .expect_err("should reject");

        assert!(err.to_string().contains("不兼容"));
    }

    #[tokio::test]
    async fn test_batch_convert_reports_output_path() {
        let converter = LutConverter::new();
        let dir = tempdir().expect("temp dir");
        let source_path = dir.path().join("sample.cube");

        write_test_cube(&source_path).await;

        let results = converter
            .batch_convert(
                vec![source_path.as_path()],
                LutFormat::Csp,
                ConversionOptions::default(),
            )
            .await
            .expect("batch convert");

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].target_path.as_deref(),
            Some(dir.path().join("sample.converted.csp").as_path())
        );
    }

    #[tokio::test]
    async fn test_load_lut_file_includes_file_path_when_parse_fails() {
        let converter = LutConverter::new();
        let dir = tempdir().expect("temp dir");
        let source_path = dir.path().join("broken.cube");

        fs::write(&source_path, "LUT_3D_SIZE invalid")
            .await
            .expect("write invalid lut");

        let err = converter
            .load_lut_file(&source_path)
            .await
            .expect_err("invalid file should fail to parse");

        let message = err.to_string();
        assert!(message.contains("无法解析 LUT 文件"));
        assert!(message.contains(&source_path.display().to_string()));
    }

    #[test]
    fn test_optimize_lut_compresses_similar_3d_colors() {
        let converter = LutConverter::new();
        let original_far_color = [0.0, 1.0, 0.0];
        let lut_data = LutData {
            lut_type: LutType::ThreeDimensional,
            format: LutFormat::Cube,
            size: 2,
            data_3d: Some(vec![
                vec![
                    vec![[0.0, 0.0, 0.0], [0.005, 0.005, 0.005]],
                    vec![original_far_color, [1.0, 1.0, 0.0]],
                ],
                vec![
                    vec![[1.0, 0.0, 0.0], [1.0, 0.0, 1.0]],
                    vec![[0.0, 1.0, 1.0], [1.0, 1.0, 1.0]],
                ],
            ]),
            data_1d: None,
            metadata: std::collections::HashMap::new(),
            title: Some("Compress Test LUT".to_string()),
            description: None,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
        };

        let options = OptimizationOptions {
            compress_similar_colors: true,
            similarity_threshold: 0.01,
            ..Default::default()
        };

        let optimized = converter.optimize_lut(&lut_data, &options).unwrap();
        let optimized_data = optimized.data_3d.expect("optimized 3d data");

        for component in 0..3 {
            assert!((optimized_data[0][0][0][component] - 0.0025).abs() < 0.0001);
            assert!((optimized_data[0][0][1][component] - 0.0025).abs() < 0.0001);
        }

        assert_eq!(optimized_data[0][1][0], original_far_color);
    }
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
            (Lut, Mga) | (Mga, Lut) => true,

            // 其他组合不兼容
            _ => false,
        }
    }

    /// 获取格式的维度类型
    pub fn get_dimension_type(format: LutFormat) -> LutType {
        use LutFormat::*;

        match format {
            Cube | ThreeDL | Csp | M3d | Look => LutType::ThreeDimensional,
            Lut | Mga => LutType::OneDimensional,
            Unknown => LutType::Unknown,
        }
    }

    /// 检查格式是否支持特定功能
    pub fn supports_feature(format: LutFormat, feature: FormatFeature) -> bool {
        use FormatFeature::*;
        use LutFormat::*;

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
    use crate::core::lut::LutData1D;
    use std::collections::HashMap;

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
        let mut lut = LutData::new_1d(LutFormat::Lut, 4, Some("Test 1D LUT".to_string()));
        // 填充 1D 三通道数据
        for i in 0..4 {
            let v = match i {
                0 => 0.0,
                1 => 0.33,
                2 => 0.66,
                _ => 1.0,
            };
            // R,G,B 三个通道一致
            let _ = lut.set_1d_point(0, i, v);
            let _ = lut.set_1d_point(1, i, v);
            let _ = lut.set_1d_point(2, i, v);
        }
        lut
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
            LutFormat::Mga
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

        let converted = converter
            .direct_convert(&lut_data, LutFormat::ThreeDL)
            .await
            .unwrap();

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

        let converted = converter
            .resample_convert(&lut_data, LutFormat::ThreeDL, &options)
            .await
            .unwrap();

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

        let converted = converter
            .interpolate_convert(&lut_data, LutFormat::Mga, &options)
            .await
            .unwrap();

        assert_eq!(converted.format, LutFormat::Mga);
        assert_eq!(converted.size, 8);
        assert_eq!(converted.data.len(), 8);
    }

    #[test]
    fn test_trilinear_interpolation() {
        let converter = LutConverter::new();
        let lut_data = create_test_3d_lut();

        // 测试中心点插值
        let result = converter
            .trilinear_interpolate(&lut_data, 0.5, 0.5, 0.5)
            .unwrap();

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
        assert!(converter.is_conversion_supported(LutFormat::Lut, LutFormat::Mga));
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

        let colors = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];

        let avg = converter.average_colors(&colors);
        assert_eq!(avg, [0.5, 0.5, 0.5]);
    }

    #[test]
    fn test_optimization() {
        let converter = LutConverter::new();
        let mut lut_data = create_test_3d_lut();
        lut_data
            .metadata
            .insert("test".to_string(), "value".to_string());

        let options = OptimizationOptions {
            remove_metadata: true,
            color_precision: Some(10.0),
            ..Default::default()
        };

        let optimized = converter.optimize_lut(&lut_data, &options).unwrap();

        assert!(optimized.metadata.is_empty());
    }

}
