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

#[test]
fn native_extensions_cannot_fall_back_to_a_renamed_preview_or_regular_tiff() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preview.ARW");
    fixture(&path, false); // A PNG, deliberately renamed to ARW.
    assert!(metadata::header(&path).is_err());
    let path = dir.path().join("preview.HIF");
    fixture(&path, false);
    assert!(codec::decode(
        &path,
        &SourceInterpretation::Assign {
            space: PhotoSpace::Srgb
        },
        &CancellationToken::new()
    )
    .is_err());
}

#[test]
#[ignore = "requires generated fixtures; see docs/photo-native-inputs.md"]
fn generated_heif_orientations_preserve_full_image_and_precision() {
    let dir = PathBuf::from(
        std::env::var_os("LUTLAB_PHOTO_FIXTURES").expect("set LUTLAB_PHOTO_FIXTURES"),
    );
    let token = CancellationToken::new();
    let untagged = dir.join("generated-orientation-1.heic");
    assert_eq!(
        camera_header(&untagged).unwrap().info.color_status,
        "unknown"
    );
    assert!(camera_decode(&untagged, &SourceInterpretation::Embedded, &token).is_err());
    let base = camera_decode(
        &dir.join("generated-orientation-1.heic"),
        &SourceInterpretation::Assign {
            space: PhotoSpace::Srgb,
        },
        &token,
    )
    .unwrap();
    assert_eq!((base.width, base.height), (64, 48));
    // Quadrants guard against a bitmap vertical flip; the encoded test image has a top-left origin.
    let pixel = |frame: &PhotoFrame, x: usize, y: usize| {
        let i = (y * frame.width as usize + x) * 3;
        [frame.rgb[i], frame.rgb[i + 1], frame.rgb[i + 2]]
    };
    assert!(pixel(&base, 8, 8)[0] > 45000 && pixel(&base, 8, 8)[1] < 20000);
    assert!(pixel(&base, 8, 40)[1] > 45000);
    for exif in 1..=8 {
        let path = dir.join(format!("generated-orientation-{exif}.heic"));
        let info = camera_header(&path).unwrap().info;
        let actual = camera_decode(
            &path,
            &SourceInterpretation::Assign {
                space: PhotoSpace::Srgb,
            },
            &token,
        )
        .unwrap();
        let buffer: image::ImageBuffer<image::Rgb<u16>, Vec<u16>> =
            image::ImageBuffer::from_raw(base.width, base.height, base.rgb.clone()).unwrap();
        let mut expected = image::DynamicImage::ImageRgb16(buffer);
        expected.apply_orientation(image::metadata::Orientation::from_exif(exif).unwrap());
        let expected = expected.into_rgb16();
        assert_eq!(
            (actual.width, actual.height),
            expected.dimensions(),
            "EXIF {exif}"
        );
        assert_eq!((info.width, info.height), expected.dimensions());
        assert!(
            actual
                .rgb
                .iter()
                .zip(expected.as_raw())
                .all(|(a, b)| a.abs_diff(*b) <= 4),
            "EXIF {exif}"
        );
    }
}

#[tokio::test]
#[ignore = "requires public Sony samples outside the repository; see docs/photo-native-inputs.md"]
async fn actual_sony_heif_arw_and_hdr_boundary() {
    let dir = PathBuf::from(
        std::env::var_os("LUTLAB_PHOTO_FIXTURES").expect("set LUTLAB_PHOTO_FIXTURES"),
    );
    let token = CancellationToken::new();
    let hlg = dir.join("a7c2-hlg.heif");
    let error = camera_header(&hlg).err().unwrap();
    assert!(error.contains("HLG/PQ"), "{error}");
    assert!(camera_decode(
        &hlg,
        &SourceInterpretation::Assign {
            space: PhotoSpace::Srgb
        },
        &token
    )
    .err()
    .unwrap()
    .contains("HLG/PQ"));
    let Some(engine) = engine() else {
        panic!("real FFmpeg is required for camera fixture tests")
    };
    let out = tempfile::tempdir().unwrap();
    let lut = out.path().join("inversion.cube");
    inversion(&lut);
    for (name, dimensions, bits) in [
        ("a7c2.arw", (7008, 4672), 16),
        ("20241120-214155-cam0-000-000004-0.HIF", (9504, 6336), 10),
    ] {
        let input = dir.join(name);
        let before = metadata::version(&input).unwrap();
        let header = camera_header(&input).unwrap();
        assert_eq!((header.info.width, header.info.height), dimensions);
        assert_eq!(header.info.bit_depth, bits);
        let original = camera_decode(&input, &SourceInterpretation::Embedded, &token).unwrap();
        assert_eq!((original.width, original.height), dimensions);
        assert!(
            original.rgb.iter().any(|v| v % 257 != 0),
            "16-bit precision lost"
        );
        assert!(
            original.rgb.iter().max().unwrap() - original.rgb.iter().min().unwrap() > 10000,
            "empty/failed rendering"
        );
        if name.ends_with("arw") {
            assert!(camera_decode(
                &input,
                &SourceInterpretation::Assign {
                    space: PhotoSpace::Srgb
                },
                &token
            )
            .is_err());
        }
        // Keep a few pixel references instead of holding another full camera image during processing.
        let expected: Vec<_> = original.rgb.iter().step_by(12347).copied().collect();
        drop(original);
        let output = out.path().join(format!("{name}.png"));
        let format = PhotoOutput::Png {
            bit_depth: 16,
            alpha_policy: AlphaPolicy::Preserve,
        };
        let mut request = job(&input, &output, format.clone());
        request.luts = vec![lut.clone()];
        request.intensity = 0.25;
        request.lut_space = Some(PhotoSpace::Srgb);
        let processor = processor::PhotoProcessor::with_budget(4 * 1024 * 1024 * 1024);
        processor
            .process(
                &engine,
                request,
                processor::PhotoControl::default(),
                Arc::new(|_| {}),
            )
            .await
            .unwrap();
        let actual_info = camera_header(&output).unwrap().info;
        assert_eq!(
            (
                actual_info.width,
                actual_info.height,
                actual_info.bit_depth,
                actual_info.orientation
            ),
            (dimensions.0, dimensions.1, 16, 1)
        );
        assert_eq!(actual_info.color_status, "embedded");
        let actual = camera_decode(&output, &SourceInterpretation::Embedded, &token).unwrap();
        for (value, original) in actual.rgb.iter().step_by(12347).zip(expected) {
            let target = (original as f64 * 0.75 + (65535 - original) as f64 * 0.25).round() as u16;
            assert!(
                value.abs_diff(target) <= 5,
                "real LUT pixel {value} != {target}"
            );
        }
        drop(actual);
        assert!(processor
            .process(
                &engine,
                job(&input, &output, format),
                processor::PhotoControl::default(),
                Arc::new(|_| {})
            )
            .await
            .is_err());
        assert_eq!(metadata::version(&input).unwrap(), before);
        let tags = little_exif::metadata::Metadata::new_from_path(&output).unwrap();
        assert!(
            matches!(tags.get_tag_by_hex(0x010f, None).next(), Some(little_exif::exif_tag::ExifTag::Make(v)) if v.trim_end_matches('\0') == "SONY")
        );
        assert!(tags.get_tag_by_hex(0x9003, None).next().is_some());
        println!(
            "verified {name}: {}x{}, full decode/development, RGB16 LUT 25%, embedded sRGB output",
            dimensions.0, dimensions.1
        );
    }
}

#[test]
#[ignore = "requires public Sony samples; see docs/photo-native-inputs.md"]
fn actual_sony_primary_color_metadata() {
    let dir = PathBuf::from(std::env::var_os("LUTLAB_PHOTO_FIXTURES").expect("set fixtures"));
    let raw = camera_header(&dir.join("a7c2.arw")).unwrap().info;
    assert_eq!((raw.width, raw.height, raw.bit_depth), (7008, 4672, 16));
    assert_eq!(raw.color_status, "srgb");
    let tags = codec::capture_metadata(&dir.join("a7c2.arw"), true, false).unwrap();
    assert_eq!(
        tags.get_tag(&little_exif::exif_tag::ExifTag::DateTimeOriginal(
            String::new()
        ))
        .next()
        .unwrap(),
        &little_exif::exif_tag::ExifTag::DateTimeOriginal("2023:08:09 13:52:26".into())
    );
    let sdr = camera_header(&dir.join("20241120-214155-cam0-000-000004-0.HIF"))
        .unwrap()
        .info;
    assert_eq!((sdr.width, sdr.height, sdr.bit_depth), (9504, 6336, 10));
    assert_eq!(sdr.color_status, "srgb");
    let hlg = dir.join("a7c2-hlg.heif");
    assert!(camera_header(&hlg).err().unwrap().contains("HLG/PQ"));
    assert!(camera_decode(
        &hlg,
        &SourceInterpretation::Assign {
            space: PhotoSpace::Srgb
        },
        &CancellationToken::new()
    )
    .err()
    .unwrap()
    .contains("HLG/PQ"));
}

fn camera_header(path: &Path) -> Result<metadata::PhotoHeader, String> {
    let engine = engine().expect("camera test needs real configured FFmpeg");
    metadata::header_with_engine(path, Some(&engine), &CancellationToken::new())
}
fn camera_decode(
    path: &Path,
    source: &SourceInterpretation,
    token: &CancellationToken,
) -> Result<PhotoFrame, String> {
    let engine = engine().expect("camera test needs real configured FFmpeg");
    codec::decode_with_engine(path, source, token, Some(&engine))
}

#[test]
fn owned_heif_grid_decodes_full_primary_in_rgb16() {
    let Some(engine) = engine() else {
        return;
    };
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/fixtures/owned-srgb-grid.heic");
    let token = CancellationToken::new();
    let info = metadata::header_with_engine(&path, Some(&engine), &token).unwrap();
    assert_eq!(
        (info.info.width, info.info.height, info.info.bit_depth),
        (128, 48, 10)
    );
    assert!(info.declared_srgb);
    let frame = codec::decode_with_engine(
        &path,
        &SourceInterpretation::Embedded,
        &token,
        Some(&engine),
    )
    .unwrap();
    assert_eq!(
        (frame.width, frame.height, frame.rgb.len()),
        (128, 48, 128 * 48 * 3)
    );
    for y in 0..48usize {
        assert_eq!(
            &frame.rgb[y * 128 * 3..(y * 128 + 64) * 3],
            &frame.rgb[(y * 128 + 64) * 3..(y * 128 + 128) * 3]
        );
    }
    assert!(frame.rgb.iter().any(|v| v % 257 != 0));
}

#[test]
fn owned_heif_icc_is_transformed_once_and_invalid_profile_needs_assignment() {
    let Some(engine) = engine() else {
        return;
    };
    let original = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/fixtures/owned-srgb-grid.heic"),
    )
    .unwrap();
    fn boxed(kind: &[u8], data: &[u8]) -> Vec<u8> {
        [
            ((data.len() + 8) as u32).to_be_bytes().as_slice(),
            kind,
            data,
        ]
        .concat()
    }
    fn replace_boxes(data: &[u8], icc: &[u8], delta: i32) -> Vec<u8> {
        let mut result = Vec::new();
        let mut cursor = 0;
        while cursor < data.len() {
            let n = u32::from_be_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            let kind = &data[cursor + 4..cursor + 8];
            let mut value = data[cursor + 8..cursor + n].to_vec();
            match kind {
                b"meta" => {
                    value = [value[..4].to_vec(), replace_boxes(&value[4..], icc, delta)].concat();
                }
                b"iprp" | b"ipco" => value = replace_boxes(&value, icc, delta),
                b"colr" => value = [b"prof".as_slice(), icc].concat(),
                b"iloc" => {
                    for offset in [14usize, 28, 42] {
                        let old = u32::from_be_bytes(value[offset..offset + 4].try_into().unwrap());
                        value[offset..offset + 4]
                            .copy_from_slice(&old.checked_add_signed(delta).unwrap().to_be_bytes());
                    }
                }
                _ => {}
            }
            result.extend(boxed(kind, &value));
            cursor += n;
        }
        result
    }
    let dir = tempfile::tempdir().unwrap();
    let token = CancellationToken::new();
    let base = dir.path().join("base.heic");
    std::fs::write(&base, &original).unwrap();
    let base = codec::decode_with_engine(
        &base,
        &SourceInterpretation::Embedded,
        &token,
        Some(&engine),
    )
    .unwrap();
    for space in [PhotoSpace::AdobeRgb, PhotoSpace::DisplayP3] {
        let icc = color::profile(space).unwrap().icc().unwrap();
        let delta = icc.len() as i32 - 7;
        let path = dir.path().join(format!("{space:?}.heic"));
        std::fs::write(&path, replace_boxes(&original, &icc, delta)).unwrap();
        let header = metadata::header_with_engine(&path, Some(&engine), &token).unwrap();
        assert_eq!(header.icc.as_deref(), Some(icc.as_slice()));
        let frame = codec::decode_with_engine(
            &path,
            &SourceInterpretation::Embedded,
            &token,
            Some(&engine),
        )
        .unwrap();
        let mut expected = base.rgb.clone();
        color::to_srgb(&mut expected, &icc, false, &token).unwrap();
        assert!(frame
            .rgb
            .iter()
            .zip(&expected)
            .all(|(a, b)| a.abs_diff(*b) <= 2));
        assert_ne!(frame.rgb, base.rgb);
    }
    let path = dir.path().join("invalid.heic");
    std::fs::write(&path, replace_boxes(&original, b"invalid", 0)).unwrap();
    assert!(codec::decode_with_engine(
        &path,
        &SourceInterpretation::Embedded,
        &token,
        Some(&engine)
    )
    .is_err());
    assert!(codec::decode_with_engine(
        &path,
        &SourceInterpretation::Assign {
            space: PhotoSpace::Srgb
        },
        &token,
        Some(&engine)
    )
    .is_ok());
}
