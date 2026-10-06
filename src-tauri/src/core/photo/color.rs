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
