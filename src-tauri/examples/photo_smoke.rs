//! Generated media and a repeatable native photo smoke test, outside the source tree.
use auto_apply_lut_lib::core::{
    ffmpeg,
    photo::{
        self,
        processor::{PhotoControl, PhotoJob, PhotoProcessor},
        AlphaPolicy, PhotoFrame, PhotoOutput, PhotoSpace, SourceInterpretation,
    },
};
use std::{path::PathBuf, sync::Arc};
use tokio_util::sync::CancellationToken;

fn pattern(width: u32, height: u32, alpha: bool) -> PhotoFrame {
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    let mut mask = alpha.then(|| Vec::with_capacity(width as usize * height as usize));
    for y in 0..height {
        for x in 0..width {
            let fx = x as f64 / (width - 1) as f64;
            let fy = y as f64 / (height - 1) as f64;
            rgb.extend([
                ((0.1 + 0.8 * fx) * 65535.0) as u16,
                ((0.12 + 0.78 * fy) * 65535.0) as u16,
                ((0.3 + 0.3 * (fx * 12.0).sin() + 0.3 * fy).clamp(0.0, 1.0) * 65535.0) as u16,
            ]);
            if let Some(mask) = &mut mask {
                mask.push((((fx - 0.5).hypot(fy - 0.5) * 2.0).clamp(0.0, 1.0) * 65535.0) as u16);
            }
        }
    }
    PhotoFrame {
        width,
        height,
        rgb,
        alpha: mask,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: photo_smoke <--generate|--process> <fresh-fixture-directory>".into());
    }
    let root = PathBuf::from(&args[1]);
    let engine = ffmpeg::discover_ffmpeg_path()?;
    let photos = root.join("photos");
    let outputs = root.join("outputs");
    let videos = root.join("videos");
    if args[0] == "--generate" {
        if root.exists() && std::fs::read_dir(&root)?.next().is_some() {
            return Err("fixture directory must be empty; existing files are protected".into());
        }
        for directory in [&photos, &outputs, &videos] {
            std::fs::create_dir_all(directory)?;
        }
        for (name, width, height, alpha, format) in [
            (
                "测试照片 JPEG.jpg",
                1200,
                800,
                false,
                PhotoOutput::Jpeg {
                    quality: 95,
                    alpha_policy: AlphaPolicy::Preserve,
                },
            ),
            (
                "透明照片 PNG.png",
                800,
                1200,
                true,
                PhotoOutput::Png {
                    bit_depth: 16,
                    alpha_policy: AlphaPolicy::Preserve,
                },
            ),
            (
                "大尺寸照片 TIFF.tif",
                4000,
                3000,
                false,
                PhotoOutput::Tiff {
                    bit_depth: 16,
                    alpha_policy: AlphaPolicy::Preserve,
                },
            ),
        ] {
            let frame = pattern(width, height, alpha);
            let path = photos.join(name);
            photo::codec::encode(
                &frame,
                &format,
                &mut std::fs::File::create(&path)?,
                &CancellationToken::new(),
            )?;
            let mut metadata = little_exif::metadata::Metadata::new();
            metadata.set_tag(little_exif::exif_tag::ExifTag::Make(
                "LUTlab generated test fixture".into(),
            ));
            photo::codec::write_metadata(&path, metadata, width, height)?;
        }
        let mut lut = "TITLE \"Generated RGB channel look\"\nLUT_3D_SIZE 2\n".to_string();
        for b in 0..=1 {
            for g in 0..=1 {
                for r in 0..=1 {
                    lut.push_str(&format!(
                        "{} {} {}\n",
                        r as f64 * 0.85 + 0.1,
                        g as f64 * 0.9,
                        b as f64 * 0.75
                    ));
                }
            }
        }
        std::fs::write(photos.join("测试风格.cube"), lut)?;
        for (index, size) in [(1, "320x240"), (2, "640x360")] {
            let status = tokio::process::Command::new(&engine)
                .args([
                    "-v",
                    "error",
                    "-nostdin",
                    "-n",
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("testsrc2=size={size}:rate=24:duration=1"),
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=1",
                    "-c:v",
                    "libx264",
                    "-threads",
                    "1",
                    "-c:a",
                    "aac",
                    "-shortest",
                ])
                .arg(videos.join(format!("测试视频 {index}.mp4")))
                .status()
                .await?;
            if !status.success() {
                return Err("video fixture generation failed".into());
            }
        }
        println!("Generated actual media in {}", root.display());
        return Ok(());
    }
    if args[0] != "--process" {
        return Err("unsupported smoke mode".into());
    }
    std::fs::create_dir_all(&outputs)?;
    let processor = PhotoProcessor::default();
    let mut reports = Vec::new();
    for source in std::fs::read_dir(&photos)?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|s| s != "cube"))
    {
        let input = source.path();
        let info = photo::metadata::header(&input)?.info;
        let output = outputs.join(format!(
            "{}-native.png",
            input.file_stem().unwrap().to_string_lossy()
        ));
        let started = std::time::Instant::now();
        let actual = processor
            .process(
                &engine,
                PhotoJob {
                    input: input.clone(),
                    output,
                    luts: vec![photos.join("测试风格.cube")],
                    intensity: 0.5,
                    lut_space: Some(PhotoSpace::Srgb),
                    source: SourceInterpretation::Embedded,
                    format: PhotoOutput::Png {
                        bit_depth: 16,
                        alpha_policy: AlphaPolicy::Preserve,
                    },
                    preserve_metadata: true,
                    preserve_gps: false,
                    expected_version: Some(info.source_version),
                },
                PhotoControl::default(),
                Arc::new(|_| {}),
            )
            .await?;
        let verified = photo::metadata::header(&actual)?.info;
        if (
            verified.width,
            verified.height,
            verified.bit_depth,
            verified.has_alpha,
        ) != (info.width, info.height, 16, info.has_alpha)
        {
            return Err("native smoke validation failed".into());
        }
        reports.push(serde_json::json!({"source":input,"output":actual,"width":verified.width,"height":verified.height,"bit_depth":verified.bit_depth,"alpha":verified.has_alpha,"color_status":verified.color_status,"elapsed_seconds":started.elapsed().as_secs_f64()}));
    }
    std::fs::write(
        root.join("photo-smoke-report.json"),
        serde_json::to_vec_pretty(&reports)?,
    )?;
    println!("Verified {} native photo exports", reports.len());
    Ok(())
}
