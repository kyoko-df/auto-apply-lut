//! Explicit input interpretation. These transforms produce SDR Rec.709 before
//! the creative LUT. Auto preserves the source and never guesses camera Log.
use crate::types::{AppError, AppResult};
use std::{fmt::Write, path::Path};

pub fn is_sony_log(space: &str) -> bool {
    matches!(space, "sony-slog3-cine" | "sony-slog3")
}

pub fn validate_input_space(space: &str) -> AppResult<()> {
    if matches!(space, "auto" | "rec709" | "rec2020-pq" | "rec2020-hlg") || is_sony_log(space) {
        Ok(())
    } else {
        Err(AppError::InvalidInput("不支持的输入色彩空间".into()))
    }
}

/// Sony's native full-range S-Log3 with BT.709 YCbCr matrix. This must run on
/// source pixels, before the RGB working-frame cache or any automatic scaling.
/// Manual selection is required; metadata alone never selects this transform.
pub fn decode_filter(space: &str) -> &'static str {
    if is_sony_log(space) {
        "scale=in_range=full:out_range=full:in_color_matrix=bt709,format=gbrp16le"
    } else {
        "null"
    }
}

// Sony Technical Summary for S-Gamut3.Cine/S-Log3 and S-Gamut3/S-Log3,
// appendix: https://www.sony.jp/ls-camera/knowledge/pdf/TechnicalSummary_for_S-Gamut3Cine_S-Gamut3_S-Log3_V1_00.pdf
fn slog3_linear(value: f64) -> f64 {
    if value >= 171.2102946929 / 1023.0 {
        0.19 * 10_f64.powf((value * 1023.0 - 420.0) / 261.5) - 0.01
    } else {
        (value * 1023.0 - 95.0) * 0.01125 / (171.2102946929 - 95.0)
    }
}

fn rec709_signal(value: f64) -> f64 {
    if value < 0.018 {
        4.5 * value
    } else {
        1.099 * value.powf(0.45) - 0.099
    }
}

async fn curve(path: &Path, function: fn(f64) -> f64) -> AppResult<()> {
    let mut data = String::from("LUT_1D_SIZE 4096\n");
    for index in 0..4096 {
        let value = function(index as f64 / 4095.0);
        writeln!(data, "{value:.12} {value:.12} {value:.12}").unwrap();
    }
    tokio::fs::write(path, data).await.map_err(AppError::from)
}

/// Own the supplied directory through the child process lifetime. Keep float
/// linear highlights until SDR tone mapping, then quantize once before the
/// shared 16-bit creative-LUT graph. This is a technical SDR rendering, not a
/// reproduction of a manufacturer's creative look.
pub async fn prepare_input_filter(space: &str, directory: &Path) -> AppResult<String> {
    validate_input_space(space)?;
    if !is_sony_log(space) {
        return input_filter(space);
    }
    let decode = directory.join("input-slog3.1d.cube");
    let encode = directory.join("output-rec709.1d.cube");
    curve(&decode, slog3_linear).await?;
    curve(&encode, rec709_signal).await?;
    // inverse(Rec.709 RGB→XYZ) × Sony RGB→XYZ, both at D65. Derived from
    // the chromaticities in Sony's appendix; no chromatic adaptation is needed.
    let matrix = if space == "sony-slog3-cine" {
        "rr=1.626947409729:rg=-0.540138538870:rb=-0.086808870859:gr=-0.178515527115:gg=1.417940927464:gb=-0.239425400349:br=-0.044436115009:bg=-0.195919966172:bb=1.240356081181"
    } else {
        "rr=1.877915128405:rg=-0.794168761314:rb=-0.083746367091:gr=-0.176806981309:gg=1.350999620894:gb=-0.174192639585:br=-0.026201126426:bg=-0.148422262344:bb=1.174623388771"
    };
    let escape = super::lut::escape_filter_value;
    Ok(format!("format=gbrpf32le,lut1d=file={}:interp=linear,colorchannelmixer={matrix},setparams=range=full:color_primaries=bt709:color_trc=linear:colorspace=gbr,tonemap=tonemap=mobius:param=0.3:desat=0:peak=38.420934337203,lut1d=file={}:interp=linear,format=gbrp16le,setparams=range=full:color_primaries=bt709:color_trc=bt709:colorspace=gbr",
        escape(&decode.to_string_lossy()), escape(&encode.to_string_lossy())))
}

pub fn input_filter(space: &str) -> AppResult<String> {
    match space {
        "auto" => Ok("null".into()),
        "rec709" => Ok(
            "format=gbrp16le,zscale=pin=bt709:tin=bt709:min=gbr:p=bt709:t=bt709:m=gbr:r=full"
                .into(),
        ),
        "rec2020-pq" | "rec2020-hlg" => {
            let transfer = if space == "rec2020-pq" {
                "smpte2084"
            } else {
                "arib-std-b67"
            };
            Ok(format!("format=gbrp16le,zscale=pin=bt2020:tin={transfer}:min=gbr:rin=full:p=bt2020:t=linear:m=gbr:r=full:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=mobius:desat=2,zscale=t=bt709:m=gbr:r=full,format=gbrp16le"))
        }
        _ => Err(AppError::InvalidInput("不支持的输入色彩空间".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn sony_log_pixels_preserve_middle_gray_highlights_and_apply_the_gamut_matrix() {
        let Ok(engine) = crate::core::ffmpeg::discover_ffmpeg_path() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let odd = dir.path().join("Sony 'curve: [test],;");
        tokio::fs::create_dir(&odd).await.unwrap();
        // Equal-channel code values are independent reference points published
        // by Sony: black=95, 18% gray=420, ~90% white=598 (10-bit full range).
        let source = dir.path().join("chart.png");
        let encode = |linear: f64| (420.0 + ((linear + 0.01) / 0.19).log10() * 261.5) / 1023.0;
        let colors = [
            [95.0 / 1023.0; 3],
            [420.0 / 1023.0; 3],
            [598.0 / 1023.0; 3],
            [900.0 / 1023.0; 3],
            [encode(0.05), encode(0.1), encode(0.2)],
        ];
        let image = image::ImageBuffer::<image::Rgb<u16>, Vec<u16>>::from_fn(80, 16, |x, _| {
            image::Rgb(colors[x as usize / 16].map(|v| (v * 65535.0).round() as u16))
        });
        image.save(&source).unwrap();
        for space in ["sony-slog3-cine", "sony-slog3"] {
            let filter = prepare_input_filter(space, &odd).await.unwrap();
            let output = tokio::process::Command::new(&engine)
                .args(["-v", "error", "-i"])
                .arg(&source)
                .args([
                    "-vf",
                    &filter,
                    "-frames:v",
                    "1",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgb48le",
                    "pipe:1",
                ])
                .kill_on_drop(true)
                .output()
                .await
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let pixels: Vec<u16> = output
                .stdout
                .chunks_exact(2)
                .map(|v| u16::from_le_bytes([v[0], v[1]]))
                .collect();
            let sample = |patch: usize, channel: usize| {
                f64::from(pixels[(8 * 80 + patch * 16 + 8) * 3 + channel]) / 65535.0
            };
            assert!(sample(0, 0) < 0.001);
            assert!((sample(1, 0) - 0.4090077).abs() < 0.001);
            assert!((sample(1, 0) - sample(1, 2)).abs() < 0.0001);
            assert!(sample(2, 0) > 0.6 && sample(2, 0) < 0.95);
            assert!(sample(3, 0) > sample(2, 0) + 0.1 && sample(3, 0) <= 1.0);
            // Independently computed from Sony's D65 chromaticities; all three
            // channels are below the Mobius knee and require no tone mapping.
            let linear = if space.ends_with("cine") {
                [0.0099717424, 0.0849832363, 0.2262574139]
            } else {
                [-0.0022703931, 0.0914210851, 0.2187723952]
            };
            for (channel, value) in linear.into_iter().enumerate() {
                let v = f64::max(value, 0.0);
                let expected = if v < 0.018 {
                    4.5 * v
                } else {
                    1.099 * v.powf(0.45) - 0.099
                };
                assert!(
                    (sample(4, channel) - expected).abs() < 0.002,
                    "{space} channel {channel}: {} != {expected}",
                    sample(4, channel)
                );
            }
        }
    }
    #[test]
    fn transforms_are_explicit_and_do_not_accept_filter_injection() {
        assert_eq!(input_filter("auto").unwrap(), "null");
        assert!(input_filter("rec2020-pq").unwrap().contains("tonemap"));
        assert!(input_filter("auto,scale=1:1").is_err());
    }
}
