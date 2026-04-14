//! LUT文件解析器模块
//! 支持多种LUT格式的解析和写入

use crate::core::lut::{LutData, LutFormat, LutType};
use crate::types::{AppError, AppResult};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

/// LUT解析器特征
#[async_trait]
pub trait LutParser {
    /// 解析LUT文件
    async fn parse(path: &Path) -> AppResult<LutData>;

    /// 写入LUT文件
    async fn write(lut_data: &LutData, path: &Path) -> AppResult<()>;

    /// 解析文件头信息
    async fn parse_header(path: &Path)
        -> AppResult<(LutType, u32, Option<String>, Option<String>)>;
}

/// CUBE格式解析器
pub struct CubeParser;

#[async_trait]
impl LutParser for CubeParser {
    async fn parse(path: &Path) -> AppResult<LutData> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AppError::Io(format!("Failed to read CUBE file: {}", e)))?;

        let mut lines = content.lines();
        let mut size = 0u32;
        let mut title = None;
        let mut domain_min = [0.0f32; 3];
        let mut domain_max = [1.0f32; 3];
        let mut data = Vec::new();
        let mut metadata = HashMap::new();

        // 解析头部信息
        for line in &mut lines {
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with("TITLE") {
                let mut t = line
                    .split_whitespace()
                    .skip(1)
                    .collect::<Vec<_>>()
                    .join(" ");
                if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
                    t = t[1..t.len() - 1].to_string();
                }
                title = Some(t);
            } else if line.starts_with("LUT_3D_SIZE") {
                size = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| AppError::Validation("Invalid LUT_3D_SIZE".to_string()))?;
            } else if line.starts_with("DOMAIN_MIN") {
                let values: Vec<f32> = line
                    .split_whitespace()
                    .skip(1)
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if values.len() == 3 {
                    domain_min = [values[0], values[1], values[2]];
                }
            } else if line.starts_with("DOMAIN_MAX") {
                let values: Vec<f32> = line
                    .split_whitespace()
                    .skip(1)
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if values.len() == 3 {
                    domain_max = [values[0], values[1], values[2]];
                }
            } else {
                // 尝试解析RGB数据
                let values: Vec<f32> = line
                    .split_whitespace()
                    .filter_map(|s| s.parse().ok())
                    .collect();

                if values.len() == 3 {
                    data.push([values[0], values[1], values[2]]);
                }
            }
        }

        // 验证数据完整性
        let expected_size = (size as usize).pow(3);
        if data.len() != expected_size {
            return Err(AppError::Validation(format!(
                "Expected {} data points, found {}",
                expected_size,
                data.len()
            )));
        }

        let mut data_3d =
            vec![vec![vec![[0.0, 0.0, 0.0]; size as usize]; size as usize]; size as usize];
        for (i, color) in data.iter().enumerate() {
            let r = i / (size as usize * size as usize);
            let g = (i / size as usize) % size as usize;
            let b = i % size as usize;
            if r < size as usize && g < size as usize && b < size as usize {
                data_3d[r][g][b] = *color;
            }
        }

        Ok(LutData {
            lut_type: LutType::ThreeDimensional,
            format: LutFormat::Cube,
            size: size.try_into().unwrap(),
            description: None,
            data_3d: Some(data_3d),
            data_1d: None,
            metadata,
            title,
            domain_min,
            domain_max,
        })
    }

    async fn write(lut_data: &LutData, path: &Path) -> AppResult<()> {
        let mut content = String::new();

        // 写入头部信息
        if let Some(title) = &lut_data.title {
            content.push_str(&format!("TITLE {}\n", title));
        }

        content.push_str(&format!("LUT_3D_SIZE {}\n", lut_data.size));
        content.push_str(&format!(
            "DOMAIN_MIN {} {} {}\n",
            lut_data.domain_min[0], lut_data.domain_min[1], lut_data.domain_min[2]
        ));
        content.push_str(&format!(
            "DOMAIN_MAX {} {} {}\n",
            lut_data.domain_max[0], lut_data.domain_max[1], lut_data.domain_max[2]
        ));

        // 写入数据
        if let Some(ref data_3d) = lut_data.data_3d {
            for r in 0..data_3d.len() {
                for g in 0..data_3d[r].len() {
                    for b in 0..data_3d[r][g].len() {
                        let point = data_3d[r][g][b];
                        content.push_str(&format!("{} {} {}\n", point[0], point[1], point[2]));
                    }
                }
            }
        }

        fs::write(path, content)
            .await
            .map_err(|e| AppError::Io(format!("Failed to write CUBE file: {}", e)))
    }

    async fn parse_header(
        path: &Path,
    ) -> AppResult<(LutType, u32, Option<String>, Option<String>)> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AppError::Io(format!("Failed to read CUBE file: {}", e)))?;

        let mut size = 0u32;
        let mut title = None;

        for line in content.lines() {
            let line = line.trim();

            if line.starts_with("TITLE") {
                let mut t = line
                    .split_whitespace()
                    .skip(1)
                    .collect::<Vec<_>>()
                    .join(" ");
                if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
                    t = t[1..t.len() - 1].to_string();
                }
                title = Some(t);
            } else if line.starts_with("LUT_3D_SIZE") {
                size = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                break;
            }
        }

        Ok((LutType::ThreeDimensional, size, title.clone(), title))
    }
}

/// 3DL格式解析器
pub struct ThreeDLParser;

#[async_trait]
impl LutParser for ThreeDLParser {
    async fn parse(path: &Path) -> AppResult<LutData> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AppError::Io(format!("Failed to read 3DL file: {}", e)))?;

        let lines: Vec<&str> = content.lines().collect();

        // 3DL文件通常有固定的大小（32x32x32）
        let size = 32u32;
        let expected_lines = (size as usize).pow(3);

        if lines.len() < expected_lines {
            return Err(AppError::Validation(format!(
                "3DL file too short: expected {} lines, found {}",
                expected_lines,
                lines.len()
            )));
        }

        let mut data = Vec::new();

        for line in lines.iter().take(expected_lines) {
            let values: Vec<f32> = line
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();

            if values.len() >= 3 {
                // 3DL格式通常使用0-4095范围，需要归一化到0-1
                data.push([values[0] / 4095.0, values[1] / 4095.0, values[2] / 4095.0]);
            } else {
                return Err(AppError::Validation(format!(
                    "Invalid 3DL line format: {}",
                    line
                )));
            }
        }

        let mut data_3d =
            vec![vec![vec![[0.0, 0.0, 0.0]; size as usize]; size as usize]; size as usize];
        for (i, color) in data.iter().enumerate() {
            let r = i / (size as usize * size as usize);
            let g = (i / size as usize) % size as usize;
            let b = i % size as usize;
            if r < size as usize && g < size as usize && b < size as usize {
                data_3d[r][g][b] = *color;
            }
        }

        Ok(LutData {
            lut_type: LutType::ThreeDimensional,
            format: LutFormat::ThreeDL,
            size: size.try_into().unwrap(),
            description: None,
            data_3d: Some(data_3d),
            data_1d: None,
            metadata: HashMap::new(),
            title: None,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
        })
    }

    async fn write(lut_data: &LutData, path: &Path) -> AppResult<()> {
        let mut content = String::new();

        // 写入数据（转换回0-4095范围）
        if let Some(ref data_3d) = lut_data.data_3d {
            for r in 0..data_3d.len() {
                for g in 0..data_3d[r].len() {
                    for b in 0..data_3d[r][g].len() {
                        let point = data_3d[r][g][b];
                        content.push_str(&format!(
                            "{} {} {}\n",
                            (point[0] * 4095.0).round() as u16,
                            (point[1] * 4095.0).round() as u16,
                            (point[2] * 4095.0).round() as u16
                        ));
                    }
                }
            }
        }

        fs::write(path, content)
            .await
            .map_err(|e| AppError::Io(format!("Failed to write 3DL file: {}", e)))
    }

    async fn parse_header(
        path: &Path,
    ) -> AppResult<(LutType, u32, Option<String>, Option<String>)> {
        // 3DL文件没有明确的头部，默认为32x32x32
        Ok((LutType::ThreeDimensional, 32, None, None))
    }
}

/// 通用LUT格式解析器
pub struct GenericLutParser;

#[async_trait]
impl LutParser for GenericLutParser {
    async fn parse(path: &Path) -> AppResult<LutData> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AppError::Io(format!("Failed to read LUT file: {}", e)))?;

        let lines: Vec<&str> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();

        // 检测是1D还是3D LUT
        let mut is_1d = true;
        let mut size = 0u32;

        // 尝试检测格式
        for line in &lines {
            let values: Vec<f32> = line
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();

            if values.len() == 3 {
                size += 1;
            } else if values.len() == 6 {
                // 可能是1D LUT格式（输入值 + RGB输出值）
                is_1d = true;
                size += 1;
            }
        }

        // 判断是否为3D LUT
        let cube_root = (size as f64).cbrt();
        if (cube_root.round() - cube_root).abs() < 0.001 {
            is_1d = false;
            size = cube_root.round() as u32;
        }

        let mut data = Vec::new();

        for line in &lines {
            let values: Vec<f32> = line
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();

            if values.len() >= 3 {
                if is_1d && values.len() >= 6 {
                    // 1D LUT格式：输入值 R G B
                    data.push([values[3], values[4], values[5]]);
                } else {
                    // 3D LUT格式：R G B
                    data.push([values[0], values[1], values[2]]);
                }
            }
        }

        let lut_type = if is_1d {
            LutType::OneDimensional
        } else {
            LutType::ThreeDimensional
        };

        Ok(LutData {
            lut_type,
            format: LutFormat::Lut,
            size: size.try_into().unwrap(),
            description: None,
            data_3d: if lut_type == LutType::ThreeDimensional {
                let mut data_3d =
                    vec![vec![vec![[0.0, 0.0, 0.0]; size as usize]; size as usize]; size as usize];
                for (i, color) in data.iter().enumerate() {
                    let r = i / (size as usize * size as usize);
                    let g = (i / size as usize) % size as usize;
                    let b = i % size as usize;
                    if r < size as usize && g < size as usize && b < size as usize {
                        data_3d[r][g][b] = *color;
                    }
                }
                Some(data_3d)
            } else {
                None
            },
            data_1d: if lut_type == LutType::OneDimensional {
                Some(crate::core::lut::LutData1D {
                    red: data.iter().map(|c| c[0]).collect(),
                    green: data.iter().map(|c| c[1]).collect(),
                    blue: data.iter().map(|c| c[2]).collect(),
                    input_range: (0.0, 1.0),
                    output_range: (0.0, 1.0),
                })
            } else {
                None
            },
            metadata: HashMap::new(),
            title: None,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
        })
    }

    async fn write(lut_data: &LutData, path: &Path) -> AppResult<()> {
        let mut content = String::new();

        match lut_data.lut_type {
            LutType::OneDimensional => {
                // 1D LUT格式
                if let Some(ref data_1d) = lut_data.data_1d {
                    let size = data_1d.red.len();
                    for i in 0..size {
                        let input_value = i as f32 / (size - 1) as f32;
                        content.push_str(&format!(
                            "{} {} {} {}\n",
                            input_value, data_1d.red[i], data_1d.green[i], data_1d.blue[i]
                        ));
                    }
                }
            }
            LutType::ThreeDimensional => {
                // 3D LUT格式
                if let Some(ref data_3d) = lut_data.data_3d {
                    for r in 0..data_3d.len() {
                        for g in 0..data_3d[r].len() {
                            for b in 0..data_3d[r][g].len() {
                                let point = data_3d[r][g][b];
                                content
                                    .push_str(&format!("{} {} {}\n", point[0], point[1], point[2]));
                            }
                        }
                    }
                }
            }
            _ => {
                return Err(AppError::Validation(
                    "Unsupported LUT type for .lut format".to_string(),
                ));
            }
        }

        fs::write(path, content)
            .await
            .map_err(|e| AppError::Io(format!("Failed to write LUT file: {}", e)))
    }

    async fn parse_header(
        path: &Path,
    ) -> AppResult<(LutType, u32, Option<String>, Option<String>)> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AppError::Io(format!("Failed to read LUT file: {}", e)))?;

        let lines: Vec<&str> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let mut size = lines.len() as u32;
        let mut is_1d = true;

        // 检测格式
        if let Some(first_line) = lines.first() {
            let values: Vec<f32> = first_line
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();

            if values.len() == 3 {
                // 可能是3D LUT
                let cube_root = (size as f64).cbrt();
                if (cube_root.round() - cube_root).abs() < 0.001 {
                    is_1d = false;
                    size = cube_root.round() as u32;
                }
            }
        }

        let lut_type = if is_1d {
            LutType::OneDimensional
        } else {
            LutType::ThreeDimensional
        };
        Ok((lut_type, size, None, None))
    }
}

/// CSP格式解析器
pub struct CspParser;

#[async_trait]
impl LutParser for CspParser {
    async fn parse(path: &Path) -> AppResult<LutData> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AppError::Io(format!("Failed to read CSP file: {}", e)))?;

        let mut lines = content.lines();
        let mut size = 0u32;
        let mut data = Vec::new();
        let mut metadata = HashMap::new();

        // 解析CSP头部
        for line in &mut lines {
            let line = line.trim();

            if line.starts_with("CSPLUTV100") {
                metadata.insert("version".to_string(), "1.00".to_string());
            } else if line.starts_with("3D") {
                // 解析3D LUT大小
                if let Some(size_str) = line.split_whitespace().nth(1) {
                    size = size_str.parse().unwrap_or(32);
                }
                break;
            }
        }

        // 解析数据
        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let values: Vec<f32> = line
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();

            if values.len() >= 3 {
                data.push([values[0], values[1], values[2]]);
            }
        }

        // 将1D数据转换为3D格式
        let size_usize = size as usize;
        let mut data_3d = vec![vec![vec![[0.0, 0.0, 0.0]; size_usize]; size_usize]; size_usize];
        for (i, color) in data.iter().enumerate() {
            let r = i / (size_usize * size_usize);
            let g = (i / size_usize) % size_usize;
            let b = i % size_usize;
            if r < size_usize && g < size_usize && b < size_usize {
                data_3d[r][g][b] = *color;
            }
        }

        Ok(LutData {
            lut_type: LutType::ThreeDimensional,
            format: LutFormat::Csp,
            size: size.try_into().unwrap(),
            description: None,
            data_3d: Some(data_3d),
            data_1d: None,
            metadata,
            title: None,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
        })
    }

    async fn write(lut_data: &LutData, path: &Path) -> AppResult<()> {
        let mut content = String::new();

        // 写入CSP头部
        content.push_str("CSPLUTV100\n");
        content.push_str(&format!("3D {}\n", lut_data.size));
        content.push_str("\n");

        // 写入数据
        if let Some(ref data_3d) = lut_data.data_3d {
            for r in 0..lut_data.size {
                for g in 0..lut_data.size {
                    for b in 0..lut_data.size {
                        let point = data_3d[r][g][b];
                        content.push_str(&format!(
                            "{:.6} {:.6} {:.6}\n",
                            point[0], point[1], point[2]
                        ));
                    }
                }
            }
        }

        fs::write(path, content)
            .await
            .map_err(|e| AppError::Io(format!("Failed to write CSP file: {}", e)))
    }

    async fn parse_header(
        path: &Path,
    ) -> AppResult<(LutType, u32, Option<String>, Option<String>)> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AppError::Io(format!("Failed to read CSP file: {}", e)))?;

        let mut size = 0u32;

        for line in content.lines() {
            let line = line.trim();

            if line.starts_with("3D") {
                if let Some(size_str) = line.split_whitespace().nth(1) {
                    size = size_str.parse().unwrap_or(32);
                }
                break;
            }
        }

        Ok((LutType::ThreeDimensional, size, None, None))
    }
}

/// VLT格式解析器（占位符）
pub struct VltParser;

#[async_trait]
impl LutParser for VltParser {
    async fn parse(_path: &Path) -> AppResult<LutData> {
        Err(AppError::Validation(
            "VLT format parsing not implemented yet".to_string(),
        ))
    }

    async fn write(_lut_data: &LutData, _path: &Path) -> AppResult<()> {
        Err(AppError::Validation(
            "VLT format writing not implemented yet".to_string(),
        ))
    }

    async fn parse_header(
        _path: &Path,
    ) -> AppResult<(LutType, u32, Option<String>, Option<String>)> {
        Err(AppError::Validation(
            "VLT format header parsing not implemented yet".to_string(),
        ))
    }
}

/// MGA格式解析器（占位符）
pub struct MgaParser;

#[async_trait]
impl LutParser for MgaParser {
    async fn parse(path: &Path) -> AppResult<LutData> {
        parse_textual_3d_lut(path, LutFormat::Mga).await
    }

    async fn write(lut_data: &LutData, path: &Path) -> AppResult<()> {
        write_textual_3d_lut(lut_data, path, "MGA 1.0", "GRID_SIZE").await
    }

    async fn parse_header(
        path: &Path,
    ) -> AppResult<(LutType, u32, Option<String>, Option<String>)> {
        parse_textual_3d_header(path, LutFormat::Mga).await
    }
}

/// M3D格式解析器（占位符）
pub struct M3dParser;

#[async_trait]
impl LutParser for M3dParser {
    async fn parse(path: &Path) -> AppResult<LutData> {
        parse_textual_3d_lut(path, LutFormat::M3d).await
    }

    async fn write(lut_data: &LutData, path: &Path) -> AppResult<()> {
        write_textual_3d_lut(lut_data, path, "M3D", "LUT_3D_SIZE").await
    }

    async fn parse_header(
        path: &Path,
    ) -> AppResult<(LutType, u32, Option<String>, Option<String>)> {
        parse_textual_3d_header(path, LutFormat::M3d).await
    }
}

/// LOOK格式解析器（占位符）
pub struct LookParser;

#[async_trait]
impl LutParser for LookParser {
    async fn parse(path: &Path) -> AppResult<LutData> {
        parse_textual_3d_lut(path, LutFormat::Look).await
    }

    async fn write(lut_data: &LutData, path: &Path) -> AppResult<()> {
        write_textual_3d_lut(lut_data, path, "LOOK", "SIZE").await
    }

    async fn parse_header(
        path: &Path,
    ) -> AppResult<(LutType, u32, Option<String>, Option<String>)> {
        parse_textual_3d_header(path, LutFormat::Look).await
    }
}

async fn parse_textual_3d_lut(path: &Path, format: LutFormat) -> AppResult<LutData> {
    let content = fs::read_to_string(path)
        .await
        .map_err(|e| AppError::Io(format!("Failed to read {:?} file: {}", format, e)))?;
    parse_textual_3d_content(&content, format)
}

async fn parse_textual_3d_header(
    path: &Path,
    format: LutFormat,
) -> AppResult<(LutType, u32, Option<String>, Option<String>)> {
    let lut_data = parse_textual_3d_lut(path, format).await?;
    Ok((
        lut_data.lut_type,
        lut_data.size as u32,
        lut_data.title.clone(),
        lut_data.description.clone(),
    ))
}

async fn write_textual_3d_lut(
    lut_data: &LutData,
    path: &Path,
    header: &str,
    size_label: &str,
) -> AppResult<()> {
    let mut content = String::new();
    content.push_str(header);
    content.push('\n');
    if let Some(title) = &lut_data.title {
        content.push_str(&format!("TITLE \"{}\"\n", title));
    }
    if let Some(description) = &lut_data.description {
        content.push_str(&format!("DESCRIPTION \"{}\"\n", description));
    }
    content.push_str(&format!("{} {}\n", size_label, lut_data.size));
    content.push_str(&format!(
        "DOMAIN_MIN {} {} {}\n",
        lut_data.domain_min[0], lut_data.domain_min[1], lut_data.domain_min[2]
    ));
    content.push_str(&format!(
        "DOMAIN_MAX {} {} {}\n\n",
        lut_data.domain_max[0], lut_data.domain_max[1], lut_data.domain_max[2]
    ));

    if let Some(ref data_3d) = lut_data.data_3d {
        for r in 0..lut_data.size {
            for g in 0..lut_data.size {
                for b in 0..lut_data.size {
                    let point = data_3d[r][g][b];
                    content.push_str(&format!(
                        "{:.6} {:.6} {:.6}\n",
                        point[0], point[1], point[2]
                    ));
                }
            }
        }
    }

    fs::write(path, content)
        .await
        .map_err(|e| AppError::Io(format!("Failed to write {:?} file: {}", lut_data.format, e)))
}

fn parse_textual_3d_content(content: &str, format: LutFormat) -> AppResult<LutData> {
    let mut size = 0usize;
    let mut title = None;
    let mut description = None;
    let mut domain_min = [0.0f32; 3];
    let mut domain_max = [1.0f32; 3];
    let mut data = Vec::new();
    let mut metadata = HashMap::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("//")
            || line.starts_with(';')
        {
            continue;
        }

        if let Some((key, value)) = parse_textual_header_line(line) {
            match key.as_str() {
                "TITLE" | "NAME" => {
                    title = Some(unquote_text(value).to_string());
                    continue;
                }
                "DESCRIPTION" | "DESC" => {
                    description = Some(unquote_text(value).to_string());
                    continue;
                }
                "LUT_3D_SIZE" | "SIZE" | "GRID_SIZE" => {
                    size = value.parse::<usize>().map_err(|_| {
                        AppError::Validation(format!("Invalid LUT size in {:?} file", format))
                    })?;
                    continue;
                }
                "DOMAIN_MIN" => {
                    domain_min = parse_rgb_triplet(value).ok_or_else(|| {
                        AppError::Validation(format!("Invalid DOMAIN_MIN in {:?} file", format))
                    })?;
                    continue;
                }
                "DOMAIN_MAX" => {
                    domain_max = parse_rgb_triplet(value).ok_or_else(|| {
                        AppError::Validation(format!("Invalid DOMAIN_MAX in {:?} file", format))
                    })?;
                    continue;
                }
                "M3D" | "LOOK" | "MGA" => {
                    if !value.is_empty() {
                        metadata.insert(key.to_lowercase(), value.to_string());
                    }
                    continue;
                }
                _ => {
                    metadata.insert(key.to_lowercase(), unquote_text(value).to_string());
                    continue;
                }
            }
        }

        let color = parse_rgb_triplet(line).ok_or_else(|| {
            AppError::Validation(format!("Invalid color row in {:?} file", format))
        })?;
        data.push(color);
    }

    if size == 0 {
        return Err(AppError::Validation(format!(
            "{:?} file missing LUT size",
            format
        )));
    }

    let expected_size = size.pow(3);
    if data.len() != expected_size {
        return Err(AppError::Validation(format!(
            "Expected {} data points, found {}",
            expected_size,
            data.len()
        )));
    }

    let mut data_3d = vec![vec![vec![[0.0, 0.0, 0.0]; size]; size]; size];
    for (index, color) in data.iter().enumerate() {
        let r = index / (size * size);
        let g = (index / size) % size;
        let b = index % size;
        data_3d[r][g][b] = *color;
    }

    Ok(LutData {
        lut_type: LutType::ThreeDimensional,
        format,
        title,
        description,
        domain_min,
        domain_max,
        size,
        data_3d: Some(data_3d),
        data_1d: None,
        metadata,
    })
}

fn parse_textual_header_line(line: &str) -> Option<(String, &str)> {
    if let Some((key, value)) = line.split_once('=') {
        let key = key.trim();
        if is_textual_header_key(key) {
            return Some((key.to_uppercase(), value.trim()));
        }
    }

    let mut parts = line.splitn(2, char::is_whitespace);
    let key = parts.next()?.trim();
    let value = parts.next().unwrap_or("").trim();
    if is_textual_header_key(key) {
        Some((key.to_uppercase(), value))
    } else {
        None
    }
}

fn is_textual_header_key(key: &str) -> bool {
    let key = key.trim();
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn parse_rgb_triplet(value: &str) -> Option<[f32; 3]> {
    let values: Vec<f32> = value
        .split_whitespace()
        .filter_map(|part| part.parse::<f32>().ok())
        .collect();
    if values.len() >= 3 {
        Some([values[0], values[1], values[2]])
    } else {
        None
    }
}

fn unquote_text(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_cube_parser() {
        let temp_dir = tempdir().unwrap();
        let cube_file = temp_dir.path().join("test.cube");

        let cube_content = r#"TITLE "Test LUT"
LUT_3D_SIZE 2
DOMAIN_MIN 0.0 0.0 0.0
DOMAIN_MAX 1.0 1.0 1.0

0.0 0.0 0.0
0.0 0.0 1.0
0.0 1.0 0.0
0.0 1.0 1.0
1.0 0.0 0.0
1.0 0.0 1.0
1.0 1.0 0.0
1.0 1.0 1.0
"#;

        let mut file = File::create(&cube_file).unwrap();
        file.write_all(cube_content.as_bytes()).unwrap();

        let lut_data = CubeParser::parse(&cube_file).await.unwrap();

        assert_eq!(lut_data.lut_type, LutType::ThreeDimensional);
        assert_eq!(lut_data.format, LutFormat::Cube);
        assert_eq!(lut_data.size, 2);
        if let Some(ref data_3d) = lut_data.data_3d {
            assert_eq!(data_3d.len() * data_3d[0].len() * data_3d[0][0].len(), 8);
        }
        assert_eq!(lut_data.title, Some("Test LUT".to_string()));
    }

    #[tokio::test]
    async fn test_3dl_parser() {
        let temp_dir = tempdir().unwrap();
        let tdl_file = temp_dir.path().join("test.3dl");

        // 创建一个简化的3DL文件（只有前8行用于测试）
        let mut content = String::new();
        for i in 0..8 {
            content.push_str(&format!("{} {} {}\n", i * 512, i * 512, i * 512));
        }

        // 填充到32^3行
        for _ in 8..32768 {
            content.push_str("0 0 0\n");
        }

        let mut file = File::create(&tdl_file).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let lut_data = ThreeDLParser::parse(&tdl_file).await.unwrap();

        assert_eq!(lut_data.lut_type, LutType::ThreeDimensional);
        assert_eq!(lut_data.format, LutFormat::ThreeDL);
        assert_eq!(lut_data.size, 32);
        if let Some(ref data_3d) = lut_data.data_3d {
            assert_eq!(
                data_3d.len() * data_3d[0].len() * data_3d[0][0].len(),
                32768
            );
        }
    }

    #[tokio::test]
    async fn test_csp_parser() {
        let temp_dir = tempdir().unwrap();
        let csp_file = temp_dir.path().join("test.csp");

        let csp_content = r#"CSPLUTV100
3D 2

0.0 0.0 0.0
0.0 0.0 1.0
0.0 1.0 0.0
0.0 1.0 1.0
1.0 0.0 0.0
1.0 0.0 1.0
1.0 1.0 0.0
1.0 1.0 1.0
"#;

        let mut file = File::create(&csp_file).unwrap();
        file.write_all(csp_content.as_bytes()).unwrap();

        let lut_data = CspParser::parse(&csp_file).await.unwrap();

        assert_eq!(lut_data.lut_type, LutType::ThreeDimensional);
        assert_eq!(lut_data.format, LutFormat::Csp);
        assert_eq!(lut_data.size, 2);
        if let Some(ref data_3d) = lut_data.data_3d {
            assert_eq!(data_3d.len() * data_3d[0].len() * data_3d[0][0].len(), 8);
        } else {
            panic!("Expected 3D data for CSP parser test");
        }
    }

    #[tokio::test]
    async fn test_cube_write() {
        let temp_dir = tempdir().unwrap();
        let cube_file = temp_dir.path().join("output.cube");

        let mut data_3d = vec![vec![vec![[0.0, 0.0, 0.0]; 2]; 2]; 2];
        data_3d[0][0][0] = [0.0, 0.0, 0.0];
        data_3d[0][0][1] = [0.0, 0.0, 1.0];
        data_3d[0][1][0] = [0.0, 1.0, 0.0];
        data_3d[0][1][1] = [0.0, 1.0, 1.0];
        data_3d[1][0][0] = [1.0, 0.0, 0.0];
        data_3d[1][0][1] = [1.0, 0.0, 1.0];
        data_3d[1][1][0] = [1.0, 1.0, 0.0];
        data_3d[1][1][1] = [1.0, 1.0, 1.0];

        let lut_data = LutData {
            lut_type: LutType::ThreeDimensional,
            format: LutFormat::Cube,
            size: 2,
            description: None,
            data_3d: Some(data_3d),
            data_1d: None,
            metadata: HashMap::new(),
            title: Some("Test Output".to_string()),
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
        };

        CubeParser::write(&lut_data, &cube_file).await.unwrap();

        assert!(cube_file.exists());

        // 验证写入的内容
        let content = fs::read_to_string(&cube_file).await.unwrap();
        assert!(content.contains("TITLE Test Output"));
        assert!(content.contains("LUT_3D_SIZE 2"));
    }

    #[tokio::test]
    async fn test_mga_parser() {
        let temp_dir = tempdir().unwrap();
        let mga_file = temp_dir.path().join("test.mga");

        let content = r#"MGA 1.0
TITLE "Test MGA"
GRID_SIZE 2
DESCRIPTION "MGA sample"

0.0 0.0 0.0
0.0 0.0 1.0
0.0 1.0 0.0
0.0 1.0 1.0
1.0 0.0 0.0
1.0 0.0 1.0
1.0 1.0 0.0
1.0 1.0 1.0
"#;

        let mut file = File::create(&mga_file).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let lut_data = MgaParser::parse(&mga_file).await.unwrap();
        assert_eq!(lut_data.format, LutFormat::Mga);
        assert_eq!(lut_data.size, 2);
        assert_eq!(lut_data.title, Some("Test MGA".to_string()));
        assert_eq!(lut_data.description, Some("MGA sample".to_string()));
    }

    #[tokio::test]
    async fn test_m3d_parser() {
        let temp_dir = tempdir().unwrap();
        let m3d_file = temp_dir.path().join("test.m3d");

        let content = r#"M3D
TITLE "Test M3D"
LUT_3D_SIZE 2
DOMAIN_MIN 0.0 0.0 0.0
DOMAIN_MAX 1.0 1.0 1.0

0.0 0.0 0.0
0.0 0.0 1.0
0.0 1.0 0.0
0.0 1.0 1.0
1.0 0.0 0.0
1.0 0.0 1.0
1.0 1.0 0.0
1.0 1.0 1.0
"#;

        let mut file = File::create(&m3d_file).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let lut_data = M3dParser::parse(&m3d_file).await.unwrap();
        assert_eq!(lut_data.format, LutFormat::M3d);
        assert_eq!(lut_data.size, 2);
        assert_eq!(lut_data.title, Some("Test M3D".to_string()));
    }

    #[tokio::test]
    async fn test_look_parser() {
        let temp_dir = tempdir().unwrap();
        let look_file = temp_dir.path().join("test.look");

        let content = r#"LOOK
NAME "Test LOOK"
DESCRIPTION "LOOK sample"
SIZE 2

0.0 0.0 0.0
0.0 0.0 1.0
0.0 1.0 0.0
0.0 1.0 1.0
1.0 0.0 0.0
1.0 0.0 1.0
1.0 1.0 0.0
1.0 1.0 1.0
"#;

        let mut file = File::create(&look_file).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let lut_data = LookParser::parse(&look_file).await.unwrap();
        assert_eq!(lut_data.format, LutFormat::Look);
        assert_eq!(lut_data.size, 2);
        assert_eq!(lut_data.title, Some("Test LOOK".to_string()));
        assert_eq!(lut_data.description, Some("LOOK sample".to_string()));
    }
}
