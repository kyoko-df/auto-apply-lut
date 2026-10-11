//! FFmpeg 9+ HEIF primary decoder. Primary IDs and color come from the container;
//! neither stream 0 nor a successfully decoded thumbnail counts as a full image.
use super::{check_cancel, color, heif, metadata, native, PhotoFrame, SourceInterpretation};
use serde_json::Value;
use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn run(
    executable: &Path,
    args: &[std::ffi::OsString],
    limit: u64,
    token: &CancellationToken,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    check_cancel(token)?;
    let mut output = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut errors = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(|e| e.to_string())?)
        .stderr(errors.try_clone().map_err(|e| e.to_string())?);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = Child(
        command
            .spawn()
            .map_err(|e| format!("无法启动 HEIF 解码器 {}：{e}", executable.display()))?,
    );
    let start = Instant::now();
    let status = loop {
        check_cancel(token)?;
        if start.elapsed() > timeout {
            return Err("HEIF 完整解码/探测超时".into());
        }
        if output.metadata().map_err(|e| e.to_string())?.len() > limit
            || errors.metadata().map_err(|e| e.to_string())?.len() > 1024 * 1024
        {
            return Err("HEIF 解码器输出超过预期范围".into());
        }
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(15));
    };
    if !status.success() {
        errors.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut message = String::new();
        errors
            .take(16384)
            .read_to_string(&mut message)
            .map_err(|e| e.to_string())?;
        return Err(format!(
            "HEIF 完整主图像解码失败（需要支持 HEIF 网格的 FFmpeg/ffprobe 9+）：{message}"
        ));
    }
    if output.metadata().map_err(|e| e.to_string())?.len() > limit {
        return Err("HEIF 解码输出超过预期范围".into());
    }
    output.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    output
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}
fn args(values: &[&str]) -> Vec<std::ffi::OsString> {
    values.iter().map(std::ffi::OsString::from).collect()
}
fn resolve_engine(value: Option<&Path>) -> Result<PathBuf, String> {
    value
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(|| crate::core::ffmpeg::discover_ffmpeg_path().map_err(|e| e.to_string()))
}
fn number(value: &Value, key: &str) -> Result<u32, String> {
    value[key]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| format!("HEIF 主图像缺少有效 {key}"))
}
fn id(value: &Value) -> Option<u32> {
    u32::from_str_radix(value["id"].as_str()?.trim_start_matches("0x"), 16).ok()
}
fn orientation(value: &Value) -> Result<u8, String> {
    let Some(matrix) = value["side_data_list"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|s| s["side_data_type"] == "Display Matrix")
    else {
        return Ok(1);
    };
    let rows: Vec<Vec<i32>> = matrix["displaymatrix"]
        .as_str()
        .ok_or("HEIF 方向矩阵缺失")?
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(_, row)| {
            row.split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect()
        })
        .collect();
    if rows.len() != 3
        || rows.iter().any(|r| r.len() != 3)
        || rows[0][2] != 0
        || rows[1][2] != 0
        || rows[2][2] != 1073741824
    {
        return Err("HEIF 不支持此方向矩阵".into());
    }
    let matrix = [rows[0][0], rows[0][1], rows[1][0], rows[1][1]];
    [
        [65536, 0, 0, 65536],
        [-65536, 0, 0, 65536],
        [-65536, 0, 0, -65536],
        [65536, 0, 0, -65536],
        [0, 65536, 65536, 0],
        [0, 65536, -65536, 0],
        [0, -65536, -65536, 0],
        [0, -65536, 65536, 0],
    ]
    .iter()
    .position(|v| *v == matrix)
    .map(|i| i as u8 + 1)
    .ok_or("HEIF 不支持此方向矩阵".into())
}
struct Primary {
    width: u32,
    height: u32,
    orientation: u8,
    bits: u8,
    graph: String,
}
fn primary(probe: &Value, primary_id: u32) -> Result<Primary, String> {
    let streams = probe["streams"].as_array().ok_or("HEIF 没有图像流")?;
    let group = probe["stream_groups"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|g| id(g) == Some(primary_id));
    let (width, height, orientation, indices, graph) = if let Some(group) = group {
        if group["type"] != "Tile Grid" {
            return Err("HEIF 主图像不是支持的瓦片网格".into());
        }
        let c = &group["components"][0];
        let w = number(c, "width")?;
        let h = number(c, "height")?;
        let tiles = c["subcomponents"].as_array().ok_or("HEIF 主图像缺少瓦片")?;
        if tiles.is_empty() || tiles.len() > 256 || number(c, "nb_tiles")? as usize != tiles.len() {
            return Err("HEIF 主图像瓦片数量无效".into());
        }
        let mut indices = Vec::new();
        let mut layout = Vec::new();
        let mut pads = String::new();
        let mut area = 0u64;
        let mut rectangles: Vec<(u32, u32, u32, u32)> = Vec::new();
        let cw = number(c, "coded_width")?;
        let ch = number(c, "coded_height")?;
        if cw > 32768
            || ch > 32768
            || number(c, "horizontal_offset")?
                .checked_add(w)
                .is_none_or(|v| v > cw)
            || number(c, "vertical_offset")?
                .checked_add(h)
                .is_none_or(|v| v > ch)
        {
            return Err("HEIF 主图像裁切越界".into());
        }
        for tile in tiles {
            let i = number(tile, "stream_index")?;
            let stream = streams
                .iter()
                .find(|s| s["index"] == i)
                .ok_or("HEIF 网格瓦片流缺失")?;
            if indices.contains(&i) {
                return Err("HEIF 重复瓦片流".into());
            }
            let x = number(tile, "tile_horizontal_offset")?;
            let y = number(tile, "tile_vertical_offset")?;
            let tw = number(stream, "width")?;
            let th = number(stream, "height")?;
            if x.checked_add(tw)
                .is_none_or(|v| v > number(c, "coded_width").unwrap_or(0))
                || y.checked_add(th)
                    .is_none_or(|v| v > number(c, "coded_height").unwrap_or(0))
            {
                return Err("HEIF 瓦片越界".into());
            }
            if rectangles
                .iter()
                .any(|&(rx, ry, rw, rh)| x < rx + rw && rx < x + tw && y < ry + rh && ry < y + th)
            {
                return Err("HEIF 网格瓦片重叠".into());
            }
            rectangles.push((x, y, tw, th));
            area += u64::from(tw) * u64::from(th);
            indices.push(i);
            layout.push(format!("{x}_{y}"));
            pads.push_str(&format!("[0:{i}]"));
        }
        if area != u64::from(cw) * u64::from(ch) {
            return Err("HEIF 网格未覆盖完整主图像".into());
        }
        if area > metadata::MAX_PIXELS * 2 {
            return Err("HEIF 瓦片总像素超过限制".into());
        }
        let graph = format!(
            "{pads}xstack=inputs={}:layout={},crop={w}:{h}:{}:{},format=rgb48le[out]",
            tiles.len(),
            layout.join("|"),
            number(c, "horizontal_offset")?,
            number(c, "vertical_offset")?
        );
        (w, h, orientation(c)?, indices, graph)
    } else {
        let s = streams
            .iter()
            .find(|s| id(s) == Some(primary_id))
            .ok_or("FFmpeg 未识别 HEIF 完整主图像；禁止回退缩略图或第一块瓦片")?;
        let i = number(s, "index")?;
        (
            number(s, "width")?,
            number(s, "height")?,
            orientation(s)?,
            vec![i],
            format!("[0:{i}]format=rgb48le[out]"),
        )
    };
    if width == 0
        || height == 0
        || width > 32768
        || height > 32768
        || u64::from(width) * u64::from(height) > metadata::MAX_PIXELS
    {
        return Err("HEIF 尺寸超过 64 MP / 32768 限制".into());
    }
    let mut bits = 0;
    let mut properties = None;
    for i in indices {
        let stream = streams.iter().find(|s| s["index"] == i).unwrap();
        if stream["codec_name"] != "hevc" {
            return Err("此 HEIF 主图像不是支持的 HEVC 图像".into());
        }
        let pixel = stream["pix_fmt"].as_str().unwrap_or("");
        if pixel.starts_with("yuv")
            && (!matches!(
                stream["color_space"].as_str(),
                Some("bt709" | "bt470bg" | "smpte170m" | "bt2020nc" | "smpte240m" | "fcc")
            ) || !matches!(stream["color_range"].as_str(), Some("pc" | "tv")))
        {
            return Err("HEIF 未声明支持的 YCbCr 矩阵/范围，无法正确解码；请先完整转换为 16 位 sRGB TIFF/PNG".into());
        }
        let depth = if pixel.contains("10") {
            10
        } else if pixel.contains("12") {
            12
        } else if pixel.starts_with("yuv") {
            8
        } else {
            return Err("HEIF 像素格式不支持".into());
        };
        if matches!(
            stream["color_transfer"].as_str(),
            Some("arib-std-b67" | "smpte2084")
        ) {
            return Err("HLG/PQ HDR 照片尚不支持；请先明确转换为 SDR sRGB".into());
        }
        let next = (
            stream["color_space"].clone(),
            stream["color_range"].clone(),
            stream["color_primaries"].clone(),
            stream["color_transfer"].clone(),
            depth,
        );
        if properties.as_ref().is_some_and(|p| p != &next) {
            return Err("HEIF 主图像网格瓦片色彩/位深不一致".into());
        }
        properties = Some(next);
        bits = depth;
    }
    Ok(Primary {
        width,
        height,
        orientation,
        bits,
        graph,
    })
}
fn inspect(
    path: &Path,
    engine: Option<&Path>,
    token: &CancellationToken,
) -> Result<(metadata::PhotoHeader, Primary, PathBuf), String> {
    let version = metadata::version(path)?;
    let source = heif::primary_color(path)?;
    if matches!(
        source,
        heif::Color::Nclx {
            transfer: 16 | 18,
            ..
        }
    ) {
        return Err(
            "HLG/PQ HDR 照片尚不支持；请先明确转换为 SDR sRGB，不能通过指定 sRGB 跳过 HDR 转换"
                .into(),
        );
    }
    let ffmpeg = resolve_engine(engine)?;
    let ffprobe = ffmpeg
        .parent()
        .ok_or("FFmpeg 路径无效")?
        .join(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        });
    if !ffprobe.is_file() {
        return Err("HEIF 需要与当前 FFmpeg 配套的 ffprobe 9+；请配置完整引擎对".into());
    }
    for executable in [&ffmpeg, &ffprobe] {
        let output = run(
            executable,
            &args(&["-version"]),
            65536,
            token,
            Duration::from_secs(10),
        )?;
        let text = String::from_utf8_lossy(&output);
        let major = text
            .split_whitespace()
            .nth(2)
            .unwrap_or("")
            .split('.')
            .next()
            .and_then(|s| s.parse::<u32>().ok());
        if major.is_none_or(|v| v < 9) {
            return Err(
                "完整 HEIF 主图像需要经验证的 FFmpeg/ffprobe 9+；请更新当前配置的引擎".into(),
            );
        }
    }
    let mut arguments = args(&[
        "-v",
        "error",
        "-protocol_whitelist",
        "file,pipe",
        "-show_streams",
        "-show_stream_groups",
        "-of",
        "json",
    ]);
    arguments.push(path.as_os_str().into());
    let output = run(
        &ffprobe,
        &arguments,
        metadata::MAX_METADATA_BYTES as u64,
        token,
        Duration::from_secs(15),
    )?;
    let probe: Value =
        serde_json::from_slice(&output).map_err(|e| format!("HEIF 探测结果无效：{e}"))?;
    let p = primary(&probe, heif::primary_id(path)?)?;
    let (icc, srgb, name, status) = match source {
        heif::Color::Profile(icc) => (Some(icc), false, "HEIF embedded ICC".into(), "embedded"),
        heif::Color::Invalid => (None, false, "无效的 HEIF ICC".into(), "invalid"),
        heif::Color::Nclx {
            primaries: 1,
            transfer: 13,
        } => (None, true, "sRGB (HEIF nclx)".into(), "srgb"),
        heif::Color::Nclx {
            primaries,
            transfer,
        } => match color::nclx_profile(primaries, transfer)? {
            Some(icc) => (
                Some(icc),
                false,
                format!("HEIF nclx {primaries}/{transfer}"),
                "embedded",
            ),
            None => (None, false, "未支持的 HEIF nclx 色彩空间".into(), "unknown"),
        },
        heif::Color::Unknown => (None, false, "未标记色彩空间".into(), "unknown"),
    };
    let mut header = native::make_header(
        path,
        p.width,
        p.height,
        p.orientation,
        p.bits,
        name,
        status,
        icc,
        srgb,
    )?;
    if metadata::version(path)? != version {
        return Err("源照片在探测期间变化".into());
    }
    header.info.source_version = version;
    Ok((header, p, ffmpeg))
}
pub fn header(
    path: &Path,
    engine: Option<&Path>,
    token: &CancellationToken,
) -> Result<metadata::PhotoHeader, String> {
    inspect(path, engine, token).map(|v| v.0)
}
pub fn decode(
    path: &Path,
    source: &SourceInterpretation,
    token: &CancellationToken,
    engine: Option<&Path>,
) -> Result<PhotoFrame, String> {
    let (header, p, ffmpeg) = inspect(path, engine, token)?;
    let (icc, srgb) = match source {
        SourceInterpretation::Assign { space } => (
            Some(metadata::assigned_icc(*space)?),
            *space == super::PhotoSpace::Srgb,
        ),
        SourceInterpretation::Embedded if header.declared_srgb => (None, true),
        SourceInterpretation::Embedded if header.info.color_status == "embedded" => {
            (header.icc, false)
        }
        _ => {
            return Err(crate::types::ui_err(
                "photo.input_profile_unknown",
                "照片色彩空间未知或无效，请明确指定输入解释",
            ))
        }
    };
    let bytes = u64::from(p.width) * u64::from(p.height) * 6;
    let mut arguments = args(&[
        "-hide_banner",
        "-v",
        "error",
        "-nostdin",
        "-threads",
        "2",
        "-protocol_whitelist",
        "file,pipe",
        "-noautorotate",
        "-i",
    ]);
    arguments.push(path.as_os_str().into());
    arguments.extend(args(&[
        "-filter_complex_threads",
        "1",
        "-filter_complex",
        &p.graph,
        "-map",
        "[out]",
        "-frames:v",
        "1",
        "-an",
        "-sn",
        "-dn",
        "-c:v",
        "rawvideo",
        "-threads:v",
        "1",
        "-pix_fmt",
        "rgb48le",
        "-f",
        "rawvideo",
        "pipe:1",
    ]));
    let data = run(&ffmpeg, &arguments, bytes, token, Duration::from_secs(120))?;
    if data.len() as u64 != bytes {
        return Err("HEIF 未返回完整主图像 RGB16；禁止回退瓦片/缩略图".into());
    }
    let mut rgb = Vec::with_capacity(data.len() / 2);
    for chunk in data.chunks(65536) {
        check_cancel(token)?;
        rgb.extend(
            chunk
                .chunks_exact(2)
                .map(|v| u16::from_le_bytes([v[0], v[1]])),
        );
    }
    drop(data);
    if !srgb {
        color::to_srgb(&mut rgb, &icc.ok_or("HEIF ICC 缺失")?, false, token)?;
    }
    if metadata::version(path)? != header.info.source_version {
        return Err("源照片在解码期间变化".into());
    }
    native::oriented(p.width, p.height, rgb, p.orientation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn grid() -> Value {
        let mut stream = json!({"codec_name":"hevc", "width":64, "height":48,
            "pix_fmt":"yuv422p10le", "color_space":"bt709", "color_range":"tv",
            "color_primaries":"bt709", "color_transfer":"iec61966-2-1"});
        stream["index"] = json!(0);
        stream["id"] = json!("0x1");
        let mut second = stream.clone();
        second["index"] = json!(1);
        second["id"] = json!("0x2");
        json!({"streams":[stream,second],"stream_groups":[{"id":"0x3","type":"Tile Grid","components":[{
            "width":128,"height":48,"coded_width":128,"coded_height":48,
            "horizontal_offset":0,"vertical_offset":0,"nb_tiles":2,
            "subcomponents":[{"stream_index":0,"tile_horizontal_offset":0,"tile_vertical_offset":0},
                {"stream_index":1,"tile_horizontal_offset":64,"tile_vertical_offset":0}]}]}]})
    }
    #[test]
    fn full_primary_requires_every_nonoverlapping_tile_and_consistent_color() {
        let probe = grid();
        let p = primary(&probe, 3).unwrap();
        assert_eq!((p.width, p.height, p.bits), (128, 48, 10));
        assert!(p.graph.contains("[0:0][0:1]xstack"));
        assert!(primary(&probe, 9).err().unwrap().contains("完整主图像"));
        for (key, value) in [
            ("tile_horizontal_offset", json!(63)),
            ("tile_horizontal_offset", json!(65)),
            ("stream_index", json!(0)),
        ] {
            let mut bad = grid();
            bad["stream_groups"][0]["components"][0]["subcomponents"][1][key] = value;
            assert!(primary(&bad, 3).is_err());
        }
        let mut hole = grid();
        hole["stream_groups"][0]["components"][0]["coded_width"] = json!(129);
        assert!(primary(&hole, 3).is_err());
        let mut hdr = grid();
        hdr["streams"][1]["color_transfer"] = json!("arib-std-b67");
        assert!(primary(&hdr, 3).err().unwrap().contains("HLG/PQ"));
        let mut matrix = grid();
        matrix["streams"][1]["color_space"] = json!("bt2020nc");
        assert!(primary(&matrix, 3).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn decoder_process_cancellation_and_timeout_kill_and_reap() {
        // This deliberately simulates a slow child; real FFmpeg decoding is
        // covered by owned_heif and the external camera fixtures.
        for cancel in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let marker = dir.path().join("pid");
            let token = CancellationToken::new();
            let child_token = token.clone();
            let worker = cancel.then(|| {
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(80));
                    child_token.cancel();
                })
            });
            let mut arguments = args(&["-c", "echo $$ > \"$1\"; exec sleep 30", "lifecycle-test"]);
            arguments.push(marker.as_os_str().into());
            let result = run(
                Path::new("/bin/sh"),
                &arguments,
                1024,
                &token,
                Duration::from_millis(160),
            );
            assert!(result.is_err());
            if let Some(worker) = worker {
                worker.join().unwrap();
            }
            let pid = std::fs::read_to_string(marker).unwrap();
            let running = Command::new("kill")
                .args(["-0", pid.trim()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success();
            if running {
                let _ = Command::new("kill").args(["-9", pid.trim()]).status();
            }
            assert!(!running, "decoder child was left running/zombie");
        }
    }
}
