use super::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

fn engine() -> Option<PathBuf> {
    let bundled =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/bin/macos/aarch64/ffmpeg");
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) && bundled.is_file() {
        return Some(bundled);
    }
    match crate::core::ffmpeg::discover_ffmpeg_path() {
        Ok(path) => Some(path),
        Err(error) if std::env::var_os("LUTLAB_REQUIRE_MEDIA_TESTS").is_some() => {
            panic!("real media engine required: {error}")
        }
        Err(_) => None,
    }
}
fn fixture(path: &Path, alpha: bool) -> PhotoFrame {
    let frame = PhotoFrame {
        width: 8,
        height: 4,
        rgb: (0..96).map(|i| (i * 683) as u16).collect(),
        alpha: alpha.then(|| (0..32).map(|i| (i * 2114) as u16).collect()),
    };
    let output = PhotoOutput::Png {
        bit_depth: 16,
        alpha_policy: AlphaPolicy::Preserve,
    };
    codec::encode(
        &frame,
        &output,
        &mut std::fs::File::create(path).unwrap(),
        &CancellationToken::new(),
    )
    .unwrap();
    frame
}
fn job(input: &Path, output: &Path, format: PhotoOutput) -> processor::PhotoJob {
    processor::PhotoJob {
        input: input.into(),
        output: output.into(),
        luts: vec![],
        intensity: 1.0,
        lut_space: None,
        source: SourceInterpretation::Embedded,
        format,
        preserve_metadata: true,
        preserve_gps: false,
        expected_version: None,
    }
}
fn inversion(path: &Path) {
    let mut content = "LUT_3D_SIZE 2\n".to_string();
    for b in 0..=1 {
        for g in 0..=1 {
            for r in 0..=1 {
                content.push_str(&format!("{} {} {}\n", 1 - r, 1 - g, 1 - b));
            }
        }
    }
    std::fs::write(path, content).unwrap();
}

#[test]
fn output_combinations_protect_alpha_and_validate_real_options() {
    assert!(PhotoOutput::Jpeg {
        quality: 95,
        alpha_policy: AlphaPolicy::Preserve
    }
    .validate(true)
    .is_err());
    assert!(PhotoOutput::Tiff {
        bit_depth: 10,
        alpha_policy: AlphaPolicy::Preserve
    }
    .validate(false)
    .is_err());
    assert!(PhotoOutput::Png {
        bit_depth: 16,
        alpha_policy: AlphaPolicy::Preserve
    }
    .validate(true)
    .is_ok());
}

#[tokio::test]
async fn actual_photo_exports_preserve_precision_alpha_icc_and_metadata() {
    let Some(engine) = engine() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("源 照片.png");
    let expected = fixture(&input, true);
    let mut tags = little_exif::metadata::Metadata::new();
    tags.set_tag(little_exif::exif_tag::ExifTag::Make("Camera".into()));
    tags.set_tag(little_exif::exif_tag::ExifTag::DateTimeOriginal(
        "2026:10:06 12:00:00".into(),
    ));
    codec::write_png_exif(&input, &tags).unwrap();
    let before = std::fs::read(&input).unwrap();
    let processor = processor::PhotoProcessor::with_budget(256 * 1024 * 1024);
    for format in [
        PhotoOutput::Png {
            bit_depth: 16,
            alpha_policy: AlphaPolicy::Preserve,
        },
        PhotoOutput::Tiff {
            bit_depth: 16,
            alpha_policy: AlphaPolicy::Flatten { color: [255; 3] },
        },
        PhotoOutput::Jpeg {
            quality: 95,
            alpha_policy: AlphaPolicy::Flatten { color: [255; 3] },
        },
    ] {
        let output = dir.path().join(format!("output.{}", format.extension()));
        processor
            .process(
                &engine,
                job(&input, &output, format.clone()),
                processor::PhotoControl::default(),
                Arc::new(|_| {}),
            )
            .await
            .unwrap();
        let actual = metadata::header(&output).unwrap();
        assert_eq!(
            (actual.info.width, actual.info.height, actual.info.bit_depth),
            (8, 4, format.bit_depth())
        );
        assert_eq!(actual.info.color_status, "embedded");
        assert_eq!(actual.info.orientation, 1);
        let tags = little_exif::metadata::Metadata::new_from_path(&output).unwrap();
        assert!(
            matches!(tags.get_tag_by_hex(0x010f, None).next(), Some(little_exif::exif_tag::ExifTag::Make(v)) if v.trim_end_matches('\0') == "Camera"),
            "{:?}",
            tags.get_tag_by_hex(0x010f, None).next()
        );
        if matches!(format, PhotoOutput::Png { .. }) {
            let frame = codec::decode(
                &output,
                &SourceInterpretation::Embedded,
                &CancellationToken::new(),
            )
            .unwrap();
            assert_eq!(frame.alpha, expected.alpha);
            assert!(frame
                .rgb
                .iter()
                .zip(&expected.rgb)
                .all(|(a, b)| a.abs_diff(*b) <= 2));
            assert!(frame.rgb.iter().any(|v| v % 257 != 0));
        }
    }
    assert_eq!(std::fs::read(&input).unwrap(), before);
}

#[tokio::test]
async fn actual_shared_lut_matches_analytic_fractional_rgb16_results() {
    let Some(engine) = engine() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.png");
    let expected = fixture(&input, false);
    let lut = dir.path().join("look ' 中文.cube");
    inversion(&lut);
    let processor = processor::PhotoProcessor::with_budget(256 * 1024 * 1024);
    for strength in [0.0, 0.5, 1.0] {
        let output = dir.path().join(format!("grade-{strength}.png"));
        let mut request = job(
            &input,
            &output,
            PhotoOutput::Png {
                bit_depth: 16,
                alpha_policy: AlphaPolicy::Preserve,
            },
        );
        request.luts = vec![lut.clone()];
        request.intensity = strength;
        request.lut_space = Some(PhotoSpace::Srgb);
        processor
            .process(
                &engine,
                request,
                processor::PhotoControl::default(),
                Arc::new(|_| {}),
            )
            .await
            .unwrap();
        let actual = codec::decode(
            &output,
            &SourceInterpretation::Embedded,
            &CancellationToken::new(),
        )
        .unwrap();
        for (value, original) in actual.rgb.iter().zip(&expected.rgb) {
            let target = ((1.0 - strength as f64) * *original as f64
                + strength as f64 * (65535 - *original) as f64)
                .round() as u16;
            assert!(
                value.abs_diff(target) <= 2,
                "{strength}: {value} != {target}"
            );
        }
    }
}

#[tokio::test]
async fn concurrent_exports_never_clobber_and_failed_or_cancelled_work_leaves_no_partial_file() {
    let Some(engine) = engine() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.png");
    fixture(&input, false);
    let processor = processor::PhotoProcessor::with_budget(512 * 1024 * 1024);
    let output = dir.path().join("same.png");
    let format = PhotoOutput::Png {
        bit_depth: 16,
        alpha_policy: AlphaPolicy::Preserve,
    };
    let (a, b) = tokio::join!(
        processor.process(
            &engine,
            job(&input, &output, format.clone()),
            processor::PhotoControl::default(),
            Arc::new(|_| {})
        ),
        processor.process(
            &engine,
            job(&input, &output, format.clone()),
            processor::PhotoControl::default(),
            Arc::new(|_| {})
        ),
    );
    let first = a.unwrap();
    let second = b.unwrap();
    assert_ne!(first, second);
    assert!(first.is_file() && second.is_file());
    let completed = std::fs::read(&output).unwrap();
    assert!(processor
        .process(
            &engine,
            job(&input, &output, format.clone()),
            processor::PhotoControl::default(),
            Arc::new(|_| {})
        )
        .await
        .is_err());
    assert_eq!(std::fs::read(&output).unwrap(), completed);
    let control = processor::PhotoControl::default();
    control.cancel();
    let cancelled = dir.path().join("cancelled.png");
    assert!(processor
        .process(
            &engine,
            job(&input, &cancelled, format.clone()),
            control,
            Arc::new(|_| {})
        )
        .await
        .is_err());
    assert!(!cancelled.exists());
    let corrupt = dir.path().join("broken.png");
    std::fs::write(&corrupt, b"not a photo").unwrap();
    let failed = dir.path().join("failed.png");
    assert!(processor
        .process(
            &engine,
            job(&corrupt, &failed, format),
            processor::PhotoControl::default(),
            Arc::new(|_| {})
        )
        .await
        .is_err());
    assert!(!failed.exists());
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".lutlab")));
}

#[test]
fn all_exif_orientations_and_unknown_input_interpretation_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    for orientation in 1..=8 {
        let input = dir.path().join(format!("orientation-{orientation}.png"));
        fixture(&input, false);
        let mut tags = little_exif::metadata::Metadata::new();
        tags.set_tag(little_exif::exif_tag::ExifTag::Orientation(vec![
            orientation,
        ]));
        codec::write_png_exif(&input, &tags).unwrap();
        let frame = codec::decode(
            &input,
            &SourceInterpretation::Embedded,
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(
            (frame.width, frame.height),
            if orientation >= 5 { (4, 8) } else { (8, 4) }
        );
        let image = image::ImageBuffer::<image::Rgb<u16>, _>::from_raw(
            8,
            4,
            (0..96).map(|i| (i * 683) as u16).collect::<Vec<_>>(),
        )
        .unwrap();
        let mut expected = image::DynamicImage::ImageRgb16(image);
        expected
            .apply_orientation(image::metadata::Orientation::from_exif(orientation as u8).unwrap());
        for (a, b) in frame.rgb.iter().zip(expected.into_rgb16().into_raw()) {
            assert!(
                a.abs_diff(b) <= 2,
                "orientation {orientation}, actual {a}, expected {b}"
            );
        }
    }
    let unknown = dir.path().join("unknown.png");
    image::RgbImage::new(2, 2).save(&unknown).unwrap();
    assert_eq!(
        metadata::header(&unknown).unwrap().info.color_status,
        "unknown"
    );
    assert!(codec::decode(
        &unknown,
        &SourceInterpretation::Embedded,
        &CancellationToken::new()
    )
    .is_err());
    assert!(codec::decode(
        &unknown,
        &SourceInterpretation::Assign {
            space: PhotoSpace::Srgb
        },
        &CancellationToken::new()
    )
    .is_ok());
}

#[test]
fn adobe_rgb_neutral_conversion_matches_transfer_function_and_invalid_icc_is_rejected() {
    let token = CancellationToken::new();
    let icc = color::profile(PhotoSpace::AdobeRgb).unwrap().icc().unwrap();
    let mut rgb = vec![32768; 3];
    color::to_srgb(&mut rgb, &icc, false, &token).unwrap();
    let linear = (32768.0_f64 / 65535.0).powf(563.0 / 256.0);
    let expected = ((1.055 * linear.powf(1.0 / 2.4) - 0.055) * 65535.0).round() as u16;
    assert!(
        rgb.iter().all(|v| v.abs_diff(expected) <= 12),
        "{rgb:?}, expected {expected}"
    );
    assert!(rgb.iter().all(|v| *v != 32768));
    assert!(color::to_srgb(&mut rgb, b"invalid profile", false, &token).is_err());
    token.cancel();
    assert!(color::to_srgb(&mut rgb, &icc, false, &token).is_err());
}

#[tokio::test]
async fn cancellation_during_native_stage_waits_for_cleanup_and_publish_cannot_follow_cancel() {
    let Some(engine) = engine() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.png");
    fixture(&input, false);
    let output = dir.path().join("cancelled.png");
    let control = processor::PhotoControl::default();
    let cancel = control.clone();
    let processor = processor::PhotoProcessor::with_budget(256 * 1024 * 1024);
    let result = processor
        .process(
            &engine,
            job(
                &input,
                &output,
                PhotoOutput::Png {
                    bit_depth: 16,
                    alpha_policy: AlphaPolicy::Preserve,
                },
            ),
            control,
            Arc::new(move |stage| {
                if stage == PhotoStage::Write.as_str() {
                    cancel.cancel();
                }
            }),
        )
        .await;
    assert!(result.is_err());
    assert!(!output.exists());
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".lutlab")));
    // A released memory reservation must admit a subsequent real export.
    processor
        .process(
            &engine,
            job(
                &input,
                &output,
                PhotoOutput::Png {
                    bit_depth: 16,
                    alpha_policy: AlphaPolicy::Preserve,
                },
            ),
            processor::PhotoControl::default(),
            Arc::new(|_| {}),
        )
        .await
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_frame_bridge_kills_and_reaps_the_running_child() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("pid");
    let engine = dir.path().join("engine");
    std::fs::write(
        &engine,
        format!(
            "#!/bin/sh\necho $$ > '{}'\nexec /bin/sleep 30\n",
            pid_file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o700)).unwrap();
    let lut = dir.path().join("invert.cube");
    inversion(&lut);
    let token = CancellationToken::new();
    let worker_token = token.clone();
    let handle = tokio::spawn(async move {
        crate::core::ffmpeg::photo_bridge::grade_rgb16(
            &engine,
            2,
            2,
            vec![1000; 12],
            &[lut],
            0.5,
            &worker_token,
        )
        .await
    });
    for _ in 0..500 {
        if pid_file.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let pid = std::fs::read_to_string(&pid_file).unwrap();
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), handle)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_err());
    let status = std::process::Command::new("/bin/kill")
        .args(["-0", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success(), "cancelled child still exists");
}
