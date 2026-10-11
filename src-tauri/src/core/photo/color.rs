use super::{check_cancel, AlphaPolicy, PhotoFrame, PhotoSpace};
use lcms2::{
    CIExyY, CIExyYTRIPLE, ColorSpaceSignature, Flags, Intent, PixelFormat, Profile, ThreadContext,
    ToneCurve, Transform,
};
use tokio_util::sync::CancellationToken;

pub fn profile(space: PhotoSpace) -> Result<Profile<ThreadContext>, String> {
    let context = ThreadContext::new();
    if space == PhotoSpace::Srgb {
        return Ok(Profile::new_srgb_context(context));
    }
    let white = CIExyY {
        x: 0.3127,
        y: 0.3290,
        Y: 1.0,
    };
    let primaries = match space {
        PhotoSpace::AdobeRgb => [(0.64, 0.33), (0.21, 0.71), (0.15, 0.06)],
        _ => [(0.68, 0.32), (0.265, 0.69), (0.15, 0.06)],
    };
    let xy = |(x, y)| CIExyY { x, y, Y: 1.0 };
    let primaries = CIExyYTRIPLE {
        Red: xy(primaries[0]),
        Green: xy(primaries[1]),
        Blue: xy(primaries[2]),
    };
    let curve = if space == PhotoSpace::AdobeRgb {
        ToneCurve::new(563.0 / 256.0)
    } else {
        ToneCurve::new_parametric(4, &[2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045])
            .map_err(|e| e.to_string())?
    };
    Profile::new_rgb_context(context, &white, &primaries, &[&curve, &curve, &curve])
        .map_err(|e| e.to_string())
}

pub fn to_srgb(
    rgb: &mut [u16],
    icc: &[u8],
    gray: bool,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let context = ThreadContext::new();
    let input = Profile::new_icc_context(context.clone(), icc)
        .map_err(|e| format!("无效 ICC 配置：{e}"))?;
    let signature = input.color_space();
    if signature != ColorSpaceSignature::RgbData
        && !(gray && signature == ColorSpaceSignature::GrayData)
    {
        return Err("ICC 配置与照片通道不匹配".into());
    }
    let output = Profile::new_srgb_context(context.clone());
    if signature == ColorSpaceSignature::RgbData && known_srgb(&input, &output) {
        return super::check_cancel(cancel);
    }
    if signature == ColorSpaceSignature::GrayData {
        let transform: Transform<u16, [u16; 3], ThreadContext> = Transform::new_flags_context(
            context.clone(),
            &input,
            PixelFormat::GRAY_16,
            &output,
            PixelFormat::RGB_16,
            Intent::RelativeColorimetric,
            Flags::BLACKPOINT_COMPENSATION,
        )
        .map_err(|e| e.to_string())?;
        for chunk in rgb.chunks_mut(3 * 16384) {
            check_cancel(cancel)?;
            let values: Vec<u16> = chunk.chunks_exact(3).map(|p| p[0]).collect();
            let mut converted = vec![[0u16; 3]; values.len()];
            transform.transform_pixels(&values, &mut converted);
            for (dst, src) in chunk.chunks_exact_mut(3).zip(converted) {
                dst.copy_from_slice(&src);
            }
        }
    } else {
        let transform: Transform<[u16; 3], [u16; 3], ThreadContext> = Transform::new_flags_context(
            context.clone(),
            &input,
            PixelFormat::RGB_16,
            &output,
            PixelFormat::RGB_16,
            Intent::RelativeColorimetric,
            Flags::BLACKPOINT_COMPENSATION,
        )
        .map_err(|e| e.to_string())?;
        for chunk in rgb.chunks_mut(3 * 16384) {
            check_cancel(cancel)?;
            let mut pixels: Vec<[u16; 3]> =
                chunk.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
            transform.transform_in_place(&mut pixels);
            for (dst, src) in chunk.chunks_exact_mut(3).zip(pixels) {
                dst.copy_from_slice(&src);
            }
        }
    }
    Ok(())
}

fn linear(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn encoded(v: f64) -> f64 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}
pub fn apply_alpha(
    frame: &mut PhotoFrame,
    policy: &AlphaPolicy,
    cancel: &CancellationToken,
) -> Result<(), String> {
    if let (Some(alpha), AlphaPolicy::Flatten { color }) = (&frame.alpha, policy) {
        let matte = color.map(|v| linear(v as f64 / 255.0));
        for (i, (pixel, a)) in frame.rgb.chunks_exact_mut(3).zip(alpha).enumerate() {
            if i % 16384 == 0 {
                check_cancel(cancel)?;
            }
            let a = *a as f64 / 65535.0;
            for channel in 0..3 {
                pixel[channel] = (encoded(
                    linear(pixel[channel] as f64 / 65535.0) * a + matte[channel] * (1.0 - a),
                ) * 65535.0)
                    .round()
                    .clamp(0.0, 65535.0) as u16;
            }
        }
        frame.alpha = None;
    }
    Ok(())
}

pub fn linear_srgb() -> Result<Profile<ThreadContext>, String> {
    rgb_profile(
        [(0.64, 0.33), (0.30, 0.60), (0.15, 0.06)],
        &ToneCurve::new(1.0),
    )
}
fn rgb_profile(
    points: [(f64, f64); 3],
    curve: &ToneCurve,
) -> Result<Profile<ThreadContext>, String> {
    let xy = |(x, y)| CIExyY { x, y, Y: 1. };
    Profile::new_rgb_context(
        ThreadContext::new(),
        &CIExyY {
            x: 0.3127,
            y: 0.3290,
            Y: 1.,
        },
        &CIExyYTRIPLE {
            Red: xy(points[0]),
            Green: xy(points[1]),
            Blue: xy(points[2]),
        },
        &[curve, curve, curve],
    )
    .map_err(|e| e.to_string())
}
pub fn nclx_profile(primaries: u16, transfer: u16) -> Result<Option<Vec<u8>>, String> {
    let points = match primaries {
        1 => [(0.64, 0.33), (0.30, 0.60), (0.15, 0.06)],
        12 => [(0.68, 0.32), (0.265, 0.69), (0.15, 0.06)],
        9 => [(0.708, 0.292), (0.170, 0.797), (0.131, 0.046)],
        _ => return Ok(None),
    };
    let curve = match transfer {
        13 => ToneCurve::new_parametric(4, &[2.4, 1. / 1.055, 0.055 / 1.055, 1. / 12.92, 0.04045]),
        1 => ToneCurve::new_parametric(4, &[1. / 0.45, 1. / 1.099, 0.099 / 1.099, 1. / 4.5, 0.081]),
        14 | 15 => {
            let alpha = 1.09929682680944;
            ToneCurve::new_parametric(
                4,
                &[
                    1. / 0.45,
                    1. / alpha,
                    (alpha - 1.) / alpha,
                    1. / 4.5,
                    0.0812428582986315,
                ],
            )
        }
        _ => return Ok(None),
    }
    .map_err(|e| e.to_string())?;
    Ok(Some(
        rgb_profile(points, &curve)?
            .icc()
            .map_err(|e| e.to_string())?,
    ))
}
// Skip only a matrix/shaper profile with the actual standard sRGB colorants,
// white and curves. Names/EXIF labels alone are never evidence for an ICC.
fn known_srgb(input: &Profile<ThreadContext>, standard: &Profile<ThreadContext>) -> bool {
    use lcms2::{Tag, TagSignature::*};
    if !input.is_matrix_shaper()
        || [
            AToB0Tag, AToB1Tag, AToB2Tag, DToB0Tag, DToB1Tag, DToB2Tag, DToB3Tag,
        ]
        .iter()
        .any(|tag| input.has_tag(*tag))
    {
        return false;
    }
    for tag in [
        RedColorantTag,
        GreenColorantTag,
        BlueColorantTag,
        MediaWhitePointTag,
    ] {
        let (Tag::CIEXYZ(a), Tag::CIEXYZ(b)) = (input.read_tag(tag), standard.read_tag(tag)) else {
            return false;
        };
        if (a.X - b.X).abs() > 2. / 65536.
            || (a.Y - b.Y).abs() > 2. / 65536.
            || (a.Z - b.Z).abs() > 2. / 65536.
        {
            return false;
        }
    }
    for tag in [RedTRCTag, GreenTRCTag, BlueTRCTag] {
        let (Tag::ToneCurve(a), Tag::ToneCurve(b)) = (input.read_tag(tag), standard.read_tag(tag))
        else {
            return false;
        };
        for n in 0..=4096 {
            let v = n as f32 / 4096.;
            if (a.eval(v) - b.eval(v)).abs() > 0.00001 {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod portable_tests {
    use super::*;
    #[test]
    fn srgb_identity_preserves_every_value() {
        let mut rgb: Vec<u16> = (0..=65535).flat_map(|v| [v, v, v]).collect();
        let expected = rgb.clone();
        to_srgb(
            &mut rgb,
            &profile(PhotoSpace::Srgb).unwrap().icc().unwrap(),
            false,
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(rgb, expected);
    }
    #[test]
    fn linear_srgb_is_encoded_once() {
        let mut rgb = vec![0, 0, 0, 16384, 16384, 16384, 65535, 65535, 65535];
        to_srgb(
            &mut rgb,
            &linear_srgb().unwrap().icc().unwrap(),
            false,
            &CancellationToken::new(),
        )
        .unwrap();
        assert!(rgb[3].abs_diff((encoded(16384. / 65535.) * 65535.).round() as u16) <= 3);
        assert_eq!(&rgb[6..], &[65535; 3]);
    }
    #[test]
    fn non_srgb_profiles_are_not_skipped() {
        let s = profile(PhotoSpace::Srgb).unwrap();
        assert!(!known_srgb(&profile(PhotoSpace::AdobeRgb).unwrap(), &s));
        assert!(!known_srgb(&linear_srgb().unwrap(), &s));
        assert!(nclx_profile(2, 2).unwrap().is_none());
        assert!(nclx_profile(9, 18).unwrap().is_none());
    }
}
