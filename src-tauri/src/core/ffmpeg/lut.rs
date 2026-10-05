//! Shared, deterministic LUT preparation for previews and exports.
//!
//! Keep the temporary directory alive until FFmpeg exits. Native LUT files are
//! copied without resampling; legacy textual formats are serialized as CUBE.

use crate::core::lut::parser::{
    CubeParser, GenericLutParser, LookParser, LutParser, M3dParser, MgaParser,
};
use crate::core::lut::LutData;
use crate::types::{AppError, AppResult, LutFormat, LutType};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use tokio::fs;

const MAX_LUT_BYTES: u64 = 64 * 1024 * 1024;

pub async fn prepare_luts(paths: &[PathBuf], directory: &Path) -> AppResult<Vec<PathBuf>> {
    let mut prepared = Vec::with_capacity(paths.len());
    for (index, source) in paths.iter().enumerate() {
        let metadata = fs::metadata(source).await.map_err(|error| {
            AppError::InvalidInput(format!("无法读取 LUT {}：{error}", source.display()))
        })?;
        if !metadata.is_file() || metadata.len() > MAX_LUT_BYTES {
            return Err(AppError::InvalidInput("LUT 必须是小于 64 MB 的文件".into()));
        }
        let format = LutFormat::from_extension(
            source
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
        );
        if matches!(
            format,
            LutFormat::Cube | LutFormat::ThreeDL | LutFormat::Csp
        ) {
            // Names encode 1D CUBE explicitly so the filter builder needs no I/O.
            let is_1d = if format == LutFormat::Cube {
                let content = fs::read_to_string(source)
                    .await
                    .map_err(|error| AppError::InvalidInput(format!("无法读取 CUBE：{error}")))?;
                let has_1d = content
                    .lines()
                    .any(|line| line.trim_start().starts_with("LUT_1D_SIZE"));
                let has_3d = content
                    .lines()
                    .any(|line| line.trim_start().starts_with("LUT_3D_SIZE"));
                if has_1d && has_3d {
                    return Err(AppError::InvalidInput(
                        "暂不支持同时包含 1D 和 3D 表的 CUBE，请先拆分 LUT".into(),
                    ));
                }
                has_1d
            } else {
                false
            };
            let suffix = if is_1d { "1d.cube" } else { format.extension() };
            let target = directory.join(format!("grade-{index}.{suffix}"));
            fs::copy(source, &target)
                .await
                .map_err(|error| AppError::Io(error.to_string()))?;
            prepared.push(target);
            continue;
        }
        let data = match format {
            LutFormat::Lut => GenericLutParser::parse(source).await?,
            LutFormat::Look => LookParser::parse(source).await?,
            LutFormat::M3d => M3dParser::parse(source).await?,
            LutFormat::Mga => MgaParser::parse(source).await?,
            _ => return Err(AppError::InvalidInput("不支持的 LUT 格式".into())),
        };
        let target = match data.lut_type {
            LutType::OneDimensional => {
                let target = directory.join(format!("grade-{index}.1d.cube"));
                fs::write(&target, one_dimensional_cube(&data)?)
                    .await
                    .map_err(|error| AppError::Io(error.to_string()))?;
                target
            }
            LutType::ThreeDimensional => {
                let target = directory.join(format!("grade-{index}.cube"));
                CubeParser::write(&data, &target).await?;
                target
            }
            LutType::Unknown => return Err(AppError::InvalidInput("无法识别 LUT 维度".into())),
        };
        prepared.push(target);
    }
    Ok(prepared)
}

fn one_dimensional_cube(data: &LutData) -> AppResult<String> {
    let channels = data
        .data_1d
        .as_ref()
        .ok_or_else(|| AppError::InvalidInput("1D LUT 缺少通道数据".into()))?;
    let size = channels.red.len();
    if size < 2 || size != data.size || channels.green.len() != size || channels.blue.len() != size
    {
        return Err(AppError::InvalidInput(
            "1D LUT 通道长度不一致或数据点不足".into(),
        ));
    }
    if channels
        .red
        .iter()
        .chain(&channels.green)
        .chain(&channels.blue)
        .any(|value| !value.is_finite())
        || (0..3).any(|index| {
            !data.domain_min[index].is_finite()
                || !data.domain_max[index].is_finite()
                || data.domain_min[index] >= data.domain_max[index]
        })
    {
        return Err(AppError::InvalidInput(
            "1D LUT 包含无效数值或输入范围".into(),
        ));
    }
    let mut content = format!(
        "LUT_1D_SIZE {size}\nDOMAIN_MIN {} {} {}\nDOMAIN_MAX {} {} {}\n",
        data.domain_min[0],
        data.domain_min[1],
        data.domain_min[2],
        data.domain_max[0],
        data.domain_max[1],
        data.domain_max[2],
    );
    for index in 0..size {
        let _ = writeln!(
            content,
            "{} {} {}",
            channels.red[index], channels.green[index], channels.blue[index]
        );
    }
    Ok(content)
}

// FFmpeg parses values twice: the graph grammar, then the filter-option grammar.
// Do not shell-quote: Command passes the graph as one argument without a shell.
fn escape_filter_value(value: &str) -> String {
    let mut option = String::new();
    for ch in value.chars() {
        if matches!(ch, '\\' | '\'' | ':') {
            option.push('\\');
        }
        option.push(ch);
    }
    let mut graph = String::new();
    for ch in option.chars() {
        if matches!(ch, '\\' | '\'' | '[' | ']' | ',' | ';') {
            graph.push('\\');
        }
        graph.push(ch);
    }
    graph
}

/// One-input/one-output filter graph, accepting paths returned by prepare_luts.
/// LUT strength blends RGB sample values with the ungraded input at 16-bit precision.
pub fn build_lut_filter(paths: &[PathBuf], intensity: f32) -> AppResult<String> {
    if !intensity.is_finite() || !(0.0..=1.0).contains(&intensity) {
        return Err(AppError::InvalidInput("LUT 强度必须在 0 到 1 之间".into()));
    }
    if paths.is_empty() || intensity == 0.0 {
        return Ok("null".into());
    }
    let filters: Vec<String> = paths
        .iter()
        .map(|path| {
            let escaped = escape_filter_value(&path.to_string_lossy());
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .ends_with(".1d.cube")
            {
                format!("lut1d=file={escaped}:interp=linear")
            } else {
                format!("lut3d=file={escaped}:interp=tetrahedral")
            }
        })
        .collect();
    let chain = filters.join(",");
    if intensity == 1.0 {
        Ok(format!("format=gbrp16le,{chain}"))
    } else {
        Ok(format!(
            "format=gbrp16le,split[lut_original][lut_input];[lut_input]{chain}[lut_graded];[lut_original][lut_graded]blend=all_mode=normal:all_opacity={:.8}",
            1.0 - intensity
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intensity_is_validated_even_without_lut() {
        for intensity in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            assert!(build_lut_filter(&[], intensity).is_err());
        }
        assert_eq!(build_lut_filter(&[], 1.0).unwrap(), "null");
    }

    #[test]
    fn zero_intensity_does_not_read_or_apply_a_lut() {
        assert_eq!(
            build_lut_filter(&[PathBuf::from("missing.cube")], 0.0).unwrap(),
            "null"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ffmpeg_accepts_escaped_paths_and_one_dimensional_cube() {
        let Ok(ffmpeg) = crate::core::ffmpeg::discover_ffmpeg_path() else {
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let unusual = directory
            .path()
            .join("quote' colon: comma, semi; [brackets] \\slash");
        fs::create_dir(&unusual).await.unwrap();
        let source = directory.path().join("one.cube");
        fs::write(&source, "LUT_1D_SIZE 2\n1 1 1\n0 0 0\n")
            .await
            .unwrap();
        let prepared = prepare_luts(&[source], &unusual).await.unwrap();
        for intensity in [0.5, 1.0] {
            let filter = build_lut_filter(&prepared, intensity).unwrap();
            let output = tokio::process::Command::new(&ffmpeg)
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=red:s=16x16:d=1",
                    "-vf",
                ])
                .arg(filter)
                .args(["-frames:v", "1", "-f", "null", "-"])
                .output()
                .await
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn one_dimensional_conversion_rejects_malformed_channels() {
        let mut data = LutData::new_1d(LutFormat::Lut, 2, None);
        data.data_1d.as_mut().unwrap().green.pop();
        assert!(one_dimensional_cube(&data).is_err());
        let mut data = LutData::new_1d(LutFormat::Lut, 2, None);
        data.data_1d.as_mut().unwrap().red[0] = f32::NAN;
        assert!(one_dimensional_cube(&data).is_err());
    }

    #[tokio::test]
    async fn imported_one_dimensional_lut_exports_the_expected_pixels() {
        let Ok(ffmpeg) = crate::core::ffmpeg::discover_ffmpeg_path() else {
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let lut_path = directory.path().join("invert.lut");
        let mut table = String::new();
        for index in 0..256 {
            let value = 1.0 - index as f32 / 255.0;
            writeln!(table, "{value} {value} {value}").unwrap();
        }
        fs::write(&lut_path, table).await.unwrap();
        let validation = crate::core::lut::LutManager::new()
            .validate_lut(&lut_path)
            .await
            .unwrap();
        assert!(validation.is_valid);
        assert_eq!(validation.lut_type, LutType::OneDimensional);

        let source = directory.path().join("source.ppm");
        let mut image = b"P6\n32 32\n255\n".to_vec();
        for _ in 0..32 * 32 {
            image.extend_from_slice(&[30, 90, 180]);
        }
        fs::write(&source, image).await.unwrap();
        let processor = crate::core::ffmpeg::processor::VideoProcessor::new(ffmpeg.clone());
        for (index, intensity) in [0.5, 1.0].into_iter().enumerate() {
            let output_path = directory.path().join(format!("export-{index}.mkv"));
            let result = processor
                .apply_luts_with_task_id(
                    &source,
                    &output_path,
                    std::slice::from_ref(&lut_path),
                    &crate::core::ffmpeg::EncodingSettings::default(),
                    format!("one-dimensional-{index}"),
                    intensity,
                )
                .await
                .unwrap();
            assert!(result.success, "{:?}", result.error);
            let pixels = tokio::process::Command::new(&ffmpeg)
                .args(["-v", "error", "-i"])
                .arg(&output_path)
                .args([
                    "-frames:v",
                    "1",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgb24",
                    "pipe:1",
                ])
                .output()
                .await
                .unwrap();
            assert!(
                pixels.status.success(),
                "{}",
                String::from_utf8_lossy(&pixels.stderr)
            );
            assert_eq!(pixels.stdout.len(), 32 * 32 * 3);
            for channel in 0..3 {
                let mean = pixels
                    .stdout
                    .chunks_exact(3)
                    .map(|pixel| pixel[channel] as f64)
                    .sum::<f64>()
                    / (32.0 * 32.0);
                let original = [30.0, 90.0, 180.0][channel];
                let expected =
                    original * (1.0 - intensity as f64) + (255.0 - original) * intensity as f64;
                assert!(
                    (mean - expected).abs() < 5.0,
                    "strength={intensity}, channel={channel}, actual={mean}, expected={expected}"
                );
            }
        }
    }
}

#[cfg(test)]
mod blend_regression_tests {
    use super::*;

    #[tokio::test]
    async fn native_blend_matches_reference_rgb16_at_fractional_strengths() {
        let Ok(ffmpeg) = crate::core::ffmpeg::discover_ffmpeg_path() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let lut = temp.path().join("invert.cube");
        let mut cube = String::from("LUT_3D_SIZE 2\n");
        for b in 0..=1 {
            for g in 0..=1 {
                for r in 0..=1 {
                    cube.push_str(&format!("{} {} {}\n", 1 - r, 1 - g, 1 - b));
                }
            }
        }
        fs::write(&lut, cube).await.unwrap();
        for intensity in [0.01_f32, 0.1, 0.3, 0.5, 0.7, 0.99] {
            let optimized = build_lut_filter(&[lut.clone()], intensity).unwrap();
            let reference = optimized.replace(
                &format!("blend=all_mode=normal:all_opacity={:.8}", 1.0 - intensity),
                &format!(
                    "blend=all_expr='A*{:.8}+B*{:.8}'",
                    1.0 - intensity,
                    intensity
                ),
            );
            let mut frames = Vec::new();
            for graph in [optimized, reference] {
                let result = tokio::process::Command::new(&ffmpeg)
                    .args([
                        "-v",
                        "error",
                        "-f",
                        "lavfi",
                        "-i",
                        "testsrc2=s=128x72:d=0.1",
                        "-vf",
                        &graph,
                        "-frames:v",
                        "1",
                        "-pix_fmt",
                        "gbrp16le",
                        "-f",
                        "rawvideo",
                        "-",
                    ])
                    .output()
                    .await
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                frames.push(result.stdout);
            }
            assert_eq!(frames[0].len(), 128 * 72 * 3 * 2);
            assert_eq!(frames[0].len(), frames[1].len());
            for (a, b) in frames[0].chunks_exact(2).zip(frames[1].chunks_exact(2)) {
                let a = u16::from_le_bytes([a[0], a[1]]);
                let b = u16::from_le_bytes([b[0], b[1]]);
                assert!(
                    a.abs_diff(b) <= 1,
                    "strength {intensity}: native/reference {a}/{b}"
                );
            }
        }
    }
}
