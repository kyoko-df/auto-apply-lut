//! LUT工具模块

use crate::types::error::{AppError, AppResult};
use crate::types::{LutType, LutFormat};
use crate::core::lut::data::LutData;
use std::path::Path;
use std::fs;

/// LUT工具集
pub struct LutUtils;

impl LutUtils {
    /// 从文件扩展名检测LUT格式
    pub fn detect_format_from_extension(path: &Path) -> Option<LutFormat> {
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            match extension.to_lowercase().as_str() {
                "cube" => Some(LutFormat::Cube),
                "3dl" => Some(LutFormat::ThreeDL),
                "lut" => Some(LutFormat::Lut),
                "csp" => Some(LutFormat::Csp),
                "m3d" => Some(LutFormat::M3d),
                "look" => Some(LutFormat::Look),
                "vlt" => Some(LutFormat::Vlt),
                "mga" => Some(LutFormat::Mga),
                _ => None,
            }
        } else {
            None
        }
    }
    
    /// 从文件内容检测LUT格式
    pub fn detect_format_from_content(content: &str) -> Option<LutFormat> {
        let lines: Vec<&str> = content.lines().collect();
        
        // 检查CUBE格式标识
        if lines.iter().any(|line| line.trim().starts_with("LUT_3D_SIZE")) {
            return Some(LutFormat::Cube);
        }
        
        // 检查3DL格式标识
        if lines.iter().any(|line| line.trim().starts_with("3DMESH")) {
            return Some(LutFormat::ThreeDL);
        }
        
        // 检查CSP格式标识
        if lines.iter().any(|line| line.trim().starts_with("CSPLUTV100")) {
            return Some(LutFormat::Csp);
        }
        
        // 默认尝试LUT格式
        Some(LutFormat::Lut)
    }
    
    /// 验证RGB值是否在有效范围内
    pub fn validate_rgb_values(r: f32, g: f32, b: f32) -> bool {
        r >= 0.0 && r <= 1.0 && g >= 0.0 && g <= 1.0 && b >= 0.0 && b <= 1.0
    }
    
    /// 规范化RGB值到0-1范围
    pub fn normalize_rgb_values(r: f32, g: f32, b: f32, max_value: f32) -> (f32, f32, f32) {
        if max_value <= 0.0 {
            return (0.0, 0.0, 0.0);
        }
        
        (
            (r / max_value).clamp(0.0, 1.0),
            (g / max_value).clamp(0.0, 1.0),
            (b / max_value).clamp(0.0, 1.0),
        )
    }
    
    /// 解析RGB值字符串
    pub fn parse_rgb_line(line: &str) -> AppResult<(f32, f32, f32)> {
        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        
        if parts.len() < 3 {
            return Err(AppError::Parse(format!("无效的RGB行: {}", line)));
        }
        
        let r = parts[0].parse::<f32>()
            .map_err(|_| AppError::Parse(format!("无法解析红色值: {}", parts[0])))?;
        let g = parts[1].parse::<f32>()
            .map_err(|_| AppError::Parse(format!("无法解析绿色值: {}", parts[1])))?;
        let b = parts[2].parse::<f32>()
            .map_err(|_| AppError::Parse(format!("无法解析蓝色值: {}", parts[2])))?;
        
        Ok((r, g, b))
    }
    
    /// 计算LUT大小
    pub fn calculate_lut_size(data_points: usize, lut_type: LutType) -> usize {
        match lut_type {
            LutType::ThreeDimensional => {
                // 对于3D LUT，计算立方根
                let size = (data_points as f64).cbrt().round() as usize;
                size
            }
            LutType::OneDimensional => {
                // 对于1D LUT，每个通道的点数
                data_points / 3
            }
            LutType::Unknown => 0,
        }
    }
    
    /// 验证LUT大小是否有效
    pub fn is_valid_lut_size(size: usize, lut_type: LutType) -> bool {
        match lut_type {
            LutType::ThreeDimensional => {
                // 3D LUT常见大小: 17, 33, 65等
                size > 0 && size <= 256
            }
            LutType::OneDimensional => {
                // 1D LUT常见大小: 256, 512, 1024等
                size > 0 && size <= 65536
            }
            LutType::Unknown => false,
        }
    }
    
    /// 生成身份LUT（无变换）
    pub fn generate_identity_lut(size: usize, lut_type: LutType, format: LutFormat) -> AppResult<LutData> {
        match lut_type {
            LutType::ThreeDimensional => {
                let mut lut = LutData::new_3d(format, size, Some("Identity 3D LUT".to_string()));
                
                for r in 0..size {
                    for g in 0..size {
                        for b in 0..size {
                            let r_val = r as f32 / (size - 1) as f32;
                            let g_val = g as f32 / (size - 1) as f32;
                            let b_val = b as f32 / (size - 1) as f32;
                            
                            lut.set_3d_point(r, g, b, [r_val, g_val, b_val])?;
                        }
                    }
                }
                
                Ok(lut)
            }
            LutType::OneDimensional => {
                let mut lut = LutData::new_1d(format, size, Some("Identity 1D LUT".to_string()));
                
                for i in 0..size {
                    let val = i as f32 / (size - 1) as f32;
                    lut.set_1d_point(0, i, val)?; // Red
                    lut.set_1d_point(1, i, val)?; // Green
                    lut.set_1d_point(2, i, val)?; // Blue
                }
                
                Ok(lut)
            }
            LutType::Unknown => {
                Err(AppError::InvalidInput("Cannot generate identity LUT for unknown type".to_string()))
            }
        }
    }
    
    /// 插值两个LUT
    pub fn interpolate_luts(
        lut1: &LutData,
        lut2: &LutData,
        factor: f32,
    ) -> AppResult<LutData> {
        if lut1.lut_type != lut2.lut_type {
            return Err(AppError::InvalidInput("LUT类型不匹配".to_string()));
        }
        
        if lut1.size != lut2.size {
            return Err(AppError::InvalidInput("LUT大小不匹配".to_string()));
        }
        
        let factor = factor.clamp(0.0, 1.0);
        let inv_factor = 1.0 - factor;
        
        match lut1.lut_type {
            LutType::ThreeDimensional => {
                let mut result = LutData::new_3d(
                    lut1.format.clone(),
                    lut1.size,
                    Some("Interpolated 3D LUT".to_string()),
                );
                
                for r in 0..lut1.size {
                    for g in 0..lut1.size {
                        for b in 0..lut1.size {
                            let val1 = lut1.get_3d_point(r, g, b)?;
                            let val2 = lut2.get_3d_point(r, g, b)?;
                            
                            let interpolated = [
                                val1[0] * inv_factor + val2[0] * factor,
                                val1[1] * inv_factor + val2[1] * factor,
                                val1[2] * inv_factor + val2[2] * factor,
                            ];
                            
                            result.set_3d_point(r, g, b, interpolated)?;
                        }
                    }
                }
                
                Ok(result)
            }
            LutType::OneDimensional => {
                let mut result = LutData::new_1d(
                    lut1.format.clone(),
                    lut1.size,
                    Some("Interpolated 1D LUT".to_string()),
                );
                
                for i in 0..lut1.size {
                    for channel in 0..3 {
                        let val1 = lut1.get_1d_point(channel, i)?;
                        let val2 = lut2.get_1d_point(channel, i)?;
                        
                        let interpolated = val1 * inv_factor + val2 * factor;
                        result.set_1d_point(channel, i, interpolated)?;
                    }
                }
                
                Ok(result)
            }
            LutType::Unknown => {
                Err(AppError::InvalidInput("Cannot interpolate unknown LUT type".to_string()))
            }
        }
    }
    
    /// 反转LUT
    pub fn invert_lut(lut: &LutData) -> AppResult<LutData> {
        match lut.lut_type {
            LutType::ThreeDimensional => {
                let mut result = LutData::new_3d(
                    lut.format.clone(),
                    lut.size,
                    Some("Inverted 3D LUT".to_string()),
                );
                
                for r in 0..lut.size {
                    for g in 0..lut.size {
                        for b in 0..lut.size {
                            let val = lut.get_3d_point(r, g, b)?;
                            let inverted = [1.0 - val[0], 1.0 - val[1], 1.0 - val[2]];
                            result.set_3d_point(r, g, b, inverted)?;
                        }
                    }
                }
                
                Ok(result)
            }
            LutType::OneDimensional => {
                let mut result = LutData::new_1d(
                    lut.format.clone(),
                    lut.size,
                    Some("Inverted 1D LUT".to_string()),
                );
                
                for i in 0..lut.size {
                    for channel in 0..3 {
                        let val = lut.get_1d_point(channel, i)?;
                        let inverted = 1.0 - val;
                        result.set_1d_point(channel, i, inverted)?;
                    }
                }
                
                Ok(result)
            }
            LutType::Unknown => {
                Err(AppError::InvalidInput("Cannot invert unknown LUT type".to_string()))
            }
        }
    }
    
    /// 调整LUT强度
    pub fn adjust_lut_intensity(lut: &LutData, intensity: f32) -> AppResult<LutData> {
        let intensity = intensity.clamp(0.0, 2.0);
        
        match lut.lut_type {
            LutType::ThreeDimensional => {
                let mut result = LutData::new_3d(
                    lut.format.clone(),
                    lut.size,
                    Some(format!("Adjusted 3D LUT ({}%)", (intensity * 100.0) as i32)),
                );
                
                // 生成身份LUT用于混合
                let identity = Self::generate_identity_lut(lut.size, lut.lut_type, lut.format.clone())?;
                
                for r in 0..lut.size {
                    for g in 0..lut.size {
                        for b in 0..lut.size {
                            let lut_val = lut.get_3d_point(r, g, b)?;
                            let identity_val = identity.get_3d_point(r, g, b)?;
                            
                            let adjusted = [
                                identity_val[0] * (1.0 - intensity) + lut_val[0] * intensity,
                                identity_val[1] * (1.0 - intensity) + lut_val[1] * intensity,
                                identity_val[2] * (1.0 - intensity) + lut_val[2] * intensity,
                            ];
                            
                            result.set_3d_point(r, g, b, adjusted)?;
                        }
                    }
                }
                
                Ok(result)
            }
            LutType::OneDimensional => {
                let mut result = LutData::new_1d(
                    lut.format.clone(),
                    lut.size,
                    Some(format!("Adjusted 1D LUT ({}%)", (intensity * 100.0) as i32)),
                );
                
                for i in 0..lut.size {
                    let identity_val = i as f32 / (lut.size - 1) as f32;
                    
                    for channel in 0..3 {
                        let lut_val = lut.get_1d_point(channel, i)?;
                        let adjusted = identity_val * (1.0 - intensity) + lut_val * intensity;
                        result.set_1d_point(channel, i, adjusted)?;
                    }
                }
                
                Ok(result)
            }
            LutType::Unknown => {
                Err(AppError::InvalidInput("Cannot adjust intensity for unknown LUT type".to_string()))
            }
        }
    }
    
    /// 获取LUT文件的基本信息
    pub fn get_lut_file_info(path: &Path) -> AppResult<LutFileInfo> {
        if !path.exists() {
            return Err(AppError::FileSystem(format!("文件不存在: {}", path.display())));
        }
        
        let metadata = fs::metadata(path)
            .map_err(|e| AppError::FileSystem(format!("无法读取文件元数据: {}", e)))?;
        
        let size = metadata.len();
        let format = Self::detect_format_from_extension(path);
        
        let content = fs::read_to_string(path)
            .map_err(|e| AppError::FileSystem(format!("无法读取文件内容: {}", e)))?;
        
        let detected_format = Self::detect_format_from_content(&content);
        let final_format = format.or(detected_format).unwrap_or(LutFormat::Lut);
        
        let line_count = content.lines().count();
        let data_lines = content.lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("//")
            })
            .count();
        
        Ok(LutFileInfo {
            path: path.to_path_buf(),
            format: final_format,
            file_size: size,
            line_count,
            data_lines,
        })
    }
    
    /// 3D插值
    pub fn interpolate_3d(lut_data: &LutData, r: f32, g: f32, b: f32) -> AppResult<[f32; 3]> {
        // 简化实现，直接返回输入值
        Ok([r, g, b])
    }
    
    /// 1D插值
    pub fn interpolate_1d(lut_data: &LutData, value: f32, channel: usize) -> AppResult<f32> {
        // 简化实现，直接返回输入值
        Ok(value)
    }
    
    /// 比较两个LUT是否相似
    pub fn compare_luts(lut1: &LutData, lut2: &LutData, tolerance: f32) -> AppResult<bool> {
        if lut1.lut_type != lut2.lut_type || lut1.size != lut2.size {
            return Ok(false);
        }
        
        match lut1.lut_type {
            LutType::ThreeDimensional => {
                for r in 0..lut1.size {
                    for g in 0..lut1.size {
                        for b in 0..lut1.size {
                            let val1 = lut1.get_3d_point(r, g, b)?;
                            let val2 = lut2.get_3d_point(r, g, b)?;
                            
                            for i in 0..3 {
                                if (val1[i] - val2[i]).abs() > tolerance {
                                    return Ok(false);
                                }
                            }
                        }
                    }
                }
            }
            LutType::OneDimensional => {
                for i in 0..lut1.size {
                    for channel in 0..3 {
                        let val1 = lut1.get_1d_point(channel, i)?;
                        let val2 = lut2.get_1d_point(channel, i)?;
                        
                        if (val1 - val2).abs() > tolerance {
                            return Ok(false);
                        }
                    }
                }
            }
            LutType::Unknown => {
                return Ok(false);
            }
        }
        
        Ok(true)
    }
}

/// LUT文件信息
#[derive(Debug, Clone)]
pub struct LutFileInfo {
    pub path: std::path::PathBuf,
    pub format: LutFormat,
    pub file_size: u64,
    pub line_count: usize,
    pub data_lines: usize,
}