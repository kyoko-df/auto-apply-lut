//! Single-frame RGB16 bridge. LUT math and escaping remain in the shared graph.
use super::lut::build_lut_filter;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};
use tokio_util::sync::CancellationToken;

pub async fn grade_rgb16(
    engine: &Path,
    width: u32,
    height: u32,
    rgb: Vec<u16>,
    luts: &[PathBuf],
    intensity: f32,
    cancel: &CancellationToken,
) -> Result<Vec<u16>, String> {
    grade_rgb16_with_timeout(
        engine,
        width,
        height,
        rgb,
        luts,
        intensity,
        cancel,
        Duration::from_secs(120),
    )
    .await
}

pub async fn grade_rgb16_with_timeout(
    engine: &Path,
    width: u32,
    height: u32,
    rgb: Vec<u16>,
    luts: &[PathBuf],
    intensity: f32,
    cancel: &CancellationToken,
    timeout: Duration,
) -> Result<Vec<u16>, String> {
    let filter = build_lut_filter(luts, intensity).map_err(|e| e.to_string())?;
    let samples = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(3))
        .ok_or("照片尺寸溢出")?;
    if width == 0 || height == 0 || rgb.len() != samples {
        return Err("照片像素长度不正确".into());
    }
    if cancel.is_cancelled() {
        return Err("照片处理已取消".into());
    }
    if luts.is_empty() || intensity == 0.0 {
        return Ok(rgb);
    }
    let bytes = samples.checked_mul(2).ok_or("照片尺寸溢出")?;
    let mut child = Command::new(engine)
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-threads",
            "1",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgb48le",
            "-video_size",
            &format!("{width}x{height}"),
            "-i",
            "pipe:0",
            "-filter_complex_threads",
            "1",
            "-filter_complex",
            &filter,
            "-frames:v",
            "1",
            "-threads:v",
            "1",
            "-pix_fmt",
            "rgb48le",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("无法启动照片 LUT 处理：{e}"))?;
    let mut stdin = child.stdin.take().ok_or("无法打开像素输入")?;
    let stdout = child.stdout.take().ok_or("无法打开像素输出")?;
    let mut stderr = child.stderr.take().ok_or("无法读取处理错误")?;
    let transfer = async {
        let write = async move {
            for chunk in rgb.chunks(32768) {
                let data: Vec<u8> = chunk.iter().flat_map(|v| v.to_le_bytes()).collect();
                stdin.write_all(&data).await?;
            }
            stdin.shutdown().await?;
            drop(stdin); // Pipe EOF is required; shutdown alone does not close ChildStdin.
            Ok::<_, std::io::Error>(())
        };
        let read = async {
            let mut data = Vec::new();
            stdout.take(bytes as u64 + 1).read_to_end(&mut data).await?;
            Ok::<_, std::io::Error>(data)
        };
        let errors = async {
            let mut tail = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = stderr.read(&mut chunk).await?;
                if n == 0 {
                    break;
                }
                tail.extend_from_slice(&chunk[..n]);
                if tail.len() > 16384 {
                    tail.drain(..tail.len() - 16384);
                }
            }
            Ok::<_, std::io::Error>(tail)
        };
        let (written, output, errors) = tokio::join!(write, read, errors);
        let status = child.wait().await.map_err(|e| e.to_string())?;
        let errors = errors.map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!(
                "照片 LUT 处理失败：{}",
                String::from_utf8_lossy(&errors)
            ));
        }
        written.map_err(|e| format!("照片像素传输失败：{e}"))?;
        let output = output.map_err(|e| e.to_string())?;
        if output.len() != bytes {
            return Err("照片 LUT 输出长度不正确".into());
        }
        Ok(output)
    };
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => Err("照片处理已取消".into()),
        _ = tokio::time::sleep(timeout) => Err("照片 LUT 处理超时".into()),
        result = transfer => result,
    };
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    let output = result?;
    let mut rgb = Vec::with_capacity(samples);
    for chunk in output.chunks(65536) {
        if cancel.is_cancelled() {
            return Err("照片处理已取消".into());
        }
        rgb.extend(
            chunk
                .chunks_exact(2)
                .map(|v| u16::from_le_bytes([v[0], v[1]])),
        );
        tokio::task::yield_now().await;
    }
    Ok(rgb)
}
