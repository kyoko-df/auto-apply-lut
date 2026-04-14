//! LUT数据结构模块

use crate::types::error::{AppError, AppResult};
use crate::types::{LutFormat, LutType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// LUT数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LutData {
    /// LUT类型
    pub lut_type: LutType,
    /// LUT格式
    pub format: LutFormat,
    /// 标题
    pub title: Option<String>,
    /// 描述
    pub description: Option<String>,
    /// 域范围
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    /// LUT大小
    pub size: usize,
    /// 3D LUT数据 (R, G, B)
    pub data_3d: Option<Vec<Vec<Vec<[f32; 3]>>>>,
    /// 1D LUT数据
    pub data_1d: Option<LutData1D>,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

/// 1D LUT数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LutData1D {
    /// 红色通道数据
    pub red: Vec<f32>,
    /// 绿色通道数据
    pub green: Vec<f32>,
    /// 蓝色通道数据
    pub blue: Vec<f32>,
    /// 输入范围
    pub input_range: (f32, f32),
    /// 输出范围
    pub output_range: (f32, f32),
}

impl LutData {
    /// 创建新的3D LUT数据
    pub fn new_3d(format: LutFormat, size: usize, title: Option<String>) -> Self {
        Self {
            lut_type: LutType::ThreeDimensional,
            format,
            title,
            description: None,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
            size,
            data_3d: Some(vec![vec![vec![[0.0, 0.0, 0.0]; size]; size]; size]),
            data_1d: None,
            metadata: HashMap::new(),
        }
    }

    /// 创建新的1D LUT数据
    pub fn new_1d(format: LutFormat, size: usize, title: Option<String>) -> Self {
        Self {
            lut_type: LutType::OneDimensional,
            format,
            title,
            description: None,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
            size,
            data_3d: None,
            data_1d: Some(LutData1D {
                red: vec![0.0; size],
                green: vec![0.0; size],
                blue: vec![0.0; size],
                input_range: (0.0, 1.0),
                output_range: (0.0, 1.0),
            }),
            metadata: HashMap::new(),
        }
    }

    /// 设置3D LUT数据点
    pub fn set_3d_point(&mut self, r: usize, g: usize, b: usize, value: [f32; 3]) -> AppResult<()> {
        if let Some(ref mut data) = self.data_3d {
            if r < self.size && g < self.size && b < self.size {
                data[r][g][b] = value;
                Ok(())
            } else {
                Err(AppError::InvalidInput("LUT索引超出范围".to_string()))
            }
        } else {
            Err(AppError::InvalidInput("不是3D LUT数据".to_string()))
        }
    }

    /// 获取3D LUT数据点
    pub fn get_3d_point(&self, r: usize, g: usize, b: usize) -> AppResult<[f32; 3]> {
        if let Some(ref data) = self.data_3d {
            if r < self.size && g < self.size && b < self.size {
                Ok(data[r][g][b])
            } else {
                Err(AppError::InvalidInput("LUT索引超出范围".to_string()))
            }
        } else {
            Err(AppError::InvalidInput("不是3D LUT数据".to_string()))
        }
    }

    /// 设置1D LUT数据点
    pub fn set_1d_point(&mut self, channel: usize, index: usize, value: f32) -> AppResult<()> {
        if let Some(ref mut data) = self.data_1d {
            if index < self.size {
                match channel {
                    0 => data.red[index] = value,
                    1 => data.green[index] = value,
                    2 => data.blue[index] = value,
                    _ => return Err(AppError::InvalidInput("无效的通道索引".to_string())),
                }
                Ok(())
            } else {
                Err(AppError::InvalidInput("LUT索引超出范围".to_string()))
            }
        } else {
            Err(AppError::InvalidInput("不是1D LUT数据".to_string()))
        }
    }

    /// 获取1D LUT数据点
    pub fn get_1d_point(&self, channel: usize, index: usize) -> AppResult<f32> {
        if let Some(ref data) = self.data_1d {
            if index < self.size {
                match channel {
                    0 => Ok(data.red[index]),
                    1 => Ok(data.green[index]),
                    2 => Ok(data.blue[index]),
                    _ => Err(AppError::InvalidInput("无效的通道索引".to_string())),
                }
            } else {
                Err(AppError::InvalidInput("LUT索引超出范围".to_string()))
            }
        } else {
            Err(AppError::InvalidInput("不是1D LUT数据".to_string()))
        }
    }

    /// 应用3D LUT变换
    pub fn apply_3d_transform(&self, input: [f32; 3]) -> AppResult<[f32; 3]> {
        if let Some(ref data) = self.data_3d {
            let [r, g, b] = input;

            // 将输入值映射到LUT索引空间
            let r_scaled = (r - self.domain_min[0]) / (self.domain_max[0] - self.domain_min[0]);
            let g_scaled = (g - self.domain_min[1]) / (self.domain_max[1] - self.domain_min[1]);
            let b_scaled = (b - self.domain_min[2]) / (self.domain_max[2] - self.domain_min[2]);

            let r_index = (r_scaled * (self.size - 1) as f32).clamp(0.0, (self.size - 1) as f32);
            let g_index = (g_scaled * (self.size - 1) as f32).clamp(0.0, (self.size - 1) as f32);
            let b_index = (b_scaled * (self.size - 1) as f32).clamp(0.0, (self.size - 1) as f32);

            // 三线性插值
            let r0 = r_index.floor() as usize;
            let g0 = g_index.floor() as usize;
            let b0 = b_index.floor() as usize;

            let r1 = (r0 + 1).min(self.size - 1);
            let g1 = (g0 + 1).min(self.size - 1);
            let b1 = (b0 + 1).min(self.size - 1);

            let dr = r_index - r0 as f32;
            let dg = g_index - g0 as f32;
            let db = b_index - b0 as f32;

            // 获取8个顶点的值
            let c000 = data[r0][g0][b0];
            let c001 = data[r0][g0][b1];
            let c010 = data[r0][g1][b0];
            let c011 = data[r0][g1][b1];
            let c100 = data[r1][g0][b0];
            let c101 = data[r1][g0][b1];
            let c110 = data[r1][g1][b0];
            let c111 = data[r1][g1][b1];

            // 三线性插值计算
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
        } else {
            Err(AppError::InvalidInput("不是3D LUT数据".to_string()))
        }
    }

    /// 应用1D LUT变换
    pub fn apply_1d_transform(&self, input: [f32; 3]) -> AppResult<[f32; 3]> {
        if let Some(ref data) = self.data_1d {
            let mut result = [0.0; 3];
            let channels = [&data.red, &data.green, &data.blue];

            for (i, &value) in input.iter().enumerate() {
                let scaled =
                    (value - data.input_range.0) / (data.input_range.1 - data.input_range.0);
                let index = (scaled * (self.size - 1) as f32).clamp(0.0, (self.size - 1) as f32);

                let i0 = index.floor() as usize;
                let i1 = (i0 + 1).min(self.size - 1);
                let t = index - i0 as f32;

                let v0 = channels[i][i0];
                let v1 = channels[i][i1];

                result[i] = v0 * (1.0 - t) + v1 * t;
            }

            Ok(result)
        } else {
            Err(AppError::InvalidInput("不是1D LUT数据".to_string()))
        }
    }

    /// 设置域范围
    pub fn set_domain(&mut self, min: [f32; 3], max: [f32; 3]) {
        self.domain_min = min;
        self.domain_max = max;
    }

    /// 添加元数据
    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }

    /// 获取元数据
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }

    /// 验证LUT数据完整性
    pub fn validate(&self) -> AppResult<()> {
        match self.lut_type {
            LutType::ThreeDimensional => {
                if self.data_3d.is_none() {
                    return Err(AppError::InvalidInput("3D LUT缺少数据".to_string()));
                }
                if let Some(ref data) = self.data_3d {
                    if data.len() != self.size
                        || data.iter().any(|plane| plane.len() != self.size)
                        || data
                            .iter()
                            .any(|plane| plane.iter().any(|row| row.len() != self.size))
                    {
                        return Err(AppError::InvalidInput("3D LUT数据尺寸不匹配".to_string()));
                    }
                }
            }
            LutType::OneDimensional => {
                if self.data_1d.is_none() {
                    return Err(AppError::InvalidInput("1D LUT缺少数据".to_string()));
                }
                if let Some(ref data) = self.data_1d {
                    if data.red.len() != self.size
                        || data.green.len() != self.size
                        || data.blue.len() != self.size
                    {
                        return Err(AppError::InvalidInput("1D LUT数据尺寸不匹配".to_string()));
                    }
                }
            }
            LutType::Unknown => {
                return Err(AppError::InvalidInput("未知LUT类型无法验证".to_string()));
            }
        }
        Ok(())
    }

    /// 获取LUT数据的统计信息
    pub fn get_statistics(&self) -> AppResult<LutStatistics> {
        match self.lut_type {
            LutType::ThreeDimensional => {
                if let Some(ref data) = self.data_3d {
                    let mut min_values = [f32::INFINITY; 3];
                    let mut max_values = [f32::NEG_INFINITY; 3];
                    let mut sum_values = [0.0; 3];
                    let total_points = self.size * self.size * self.size;

                    for plane in data {
                        for row in plane {
                            for &point in row {
                                for i in 0..3 {
                                    min_values[i] = min_values[i].min(point[i]);
                                    max_values[i] = max_values[i].max(point[i]);
                                    sum_values[i] += point[i];
                                }
                            }
                        }
                    }

                    let avg_values = [
                        sum_values[0] / total_points as f32,
                        sum_values[1] / total_points as f32,
                        sum_values[2] / total_points as f32,
                    ];

                    Ok(LutStatistics {
                        min_values,
                        max_values,
                        avg_values,
                        total_points,
                    })
                } else {
                    Err(AppError::InvalidInput("3D LUT数据不存在".to_string()))
                }
            }
            LutType::OneDimensional => {
                if let Some(ref data) = self.data_1d {
                    let channels = [&data.red, &data.green, &data.blue];
                    let mut min_values = [f32::INFINITY; 3];
                    let mut max_values = [f32::NEG_INFINITY; 3];
                    let mut sum_values = [0.0; 3];

                    for (i, channel) in channels.iter().enumerate() {
                        for &value in channel.iter() {
                            min_values[i] = min_values[i].min(value);
                            max_values[i] = max_values[i].max(value);
                            sum_values[i] += value;
                        }
                    }

                    let avg_values = [
                        sum_values[0] / self.size as f32,
                        sum_values[1] / self.size as f32,
                        sum_values[2] / self.size as f32,
                    ];

                    Ok(LutStatistics {
                        min_values,
                        max_values,
                        avg_values,
                        total_points: self.size * 3,
                    })
                } else {
                    Err(AppError::InvalidInput("1D LUT数据不存在".to_string()))
                }
            }
            LutType::Unknown => Err(AppError::InvalidInput(
                "未知LUT类型无法获取统计信息".to_string(),
            )),
        }
    }
}

/// LUT统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LutStatistics {
    pub min_values: [f32; 3],
    pub max_values: [f32; 3],
    pub avg_values: [f32; 3],
    pub total_points: usize,
}
