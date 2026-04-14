use crate::core::ffmpeg::{EncodingSettings, Resolution};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub(crate) const INTERNAL_TWO_PASS_KEY: &str = "__two_pass__";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingOptions {
    pub hardware_acceleration: bool,
    #[serde(default)]
    pub output_format: Option<String>,
    #[serde(default)]
    pub video_codec: Option<String>,
    #[serde(default)]
    pub audio_codec: Option<String>,
    #[serde(default)]
    pub quality_preset: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub fps: Option<f64>,
    #[serde(default)]
    pub bitrate: Option<String>,
    #[serde(default)]
    pub color_space: Option<String>,
    #[serde(default)]
    pub two_pass_encoding: bool,
    #[serde(default = "default_preserve_metadata")]
    pub preserve_metadata: bool,
}

fn default_preserve_metadata() -> bool {
    true
}

fn parse_resolution(value: Option<&str>) -> Result<Option<Resolution>, String> {
    let raw = value.unwrap_or("").trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("original") {
        return Ok(None);
    }

    let (w, h) = raw
        .split_once('x')
        .or_else(|| raw.split_once('X'))
        .ok_or_else(|| format!("Invalid resolution format: {}", raw))?;

    let width = w
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("Invalid resolution width: {}", w))?;
    let height = h
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("Invalid resolution height: {}", h))?;

    if width == 0 || height == 0 {
        return Err("Resolution must be positive".to_string());
    }

    Ok(Some(Resolution { width, height }))
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value.and_then(|v| {
        let s = v.trim();
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    })
}

fn apply_quality_preset(
    preset_name: Option<&str>,
    settings: &mut EncodingSettings,
    extra_params: &mut HashMap<String, String>,
    output_format: Option<&str>,
) {
    match preset_name.unwrap_or("").trim() {
        "high_quality" => {
            settings.crf = 18;
            settings.preset = "slow".to_string();
        }
        "fast" => {
            settings.crf = 28;
            settings.preset = "fast".to_string();
        }
        "web_optimized" => {
            settings.crf = 25;
            settings.preset = "medium".to_string();
            if matches!(output_format, Some("mp4") | Some("mov")) {
                extra_params.insert("-movflags".to_string(), "+faststart".to_string());
            }
        }
        _ => {
            settings.crf = 23;
            settings.preset = "medium".to_string();
        }
    }
}

fn apply_color_space(color_space: Option<&str>, extra_params: &mut HashMap<String, String>) {
    match color_space.unwrap_or("").trim() {
        "rec2020" => {
            extra_params.insert("-colorspace".to_string(), "bt2020nc".to_string());
            extra_params.insert("-color_primaries".to_string(), "bt2020".to_string());
            extra_params.insert("-color_trc".to_string(), "smpte2084".to_string());
        }
        "srgb" => {
            extra_params.insert("-colorspace".to_string(), "bt709".to_string());
            extra_params.insert("-color_primaries".to_string(), "bt709".to_string());
            extra_params.insert("-color_trc".to_string(), "iec61966-2-1".to_string());
        }
        "adobe_rgb" => {
            extra_params.insert("-colorspace".to_string(), "bt709".to_string());
            extra_params.insert("-color_primaries".to_string(), "bt470bg".to_string());
            extra_params.insert("-color_trc".to_string(), "gamma22".to_string());
        }
        "dci_p3" => {
            extra_params.insert("-colorspace".to_string(), "bt709".to_string());
            extra_params.insert("-color_primaries".to_string(), "smpte432".to_string());
            extra_params.insert("-color_trc".to_string(), "smpte2084".to_string());
        }
        _ => {
            extra_params.insert("-colorspace".to_string(), "bt709".to_string());
            extra_params.insert("-color_primaries".to_string(), "bt709".to_string());
            extra_params.insert("-color_trc".to_string(), "bt709".to_string());
        }
    }
}

pub(crate) fn build_encoding_settings(
    options: &ProcessingOptions,
) -> Result<EncodingSettings, String> {
    let mut settings = EncodingSettings::default();
    let mut extra_params: HashMap<String, String> = HashMap::new();

    if let Some(video_codec) = normalize_optional(options.video_codec.as_deref()) {
        settings.video_codec = video_codec;
    }
    if let Some(audio_codec) = normalize_optional(options.audio_codec.as_deref()) {
        settings.audio_codec = audio_codec;
    }

    apply_quality_preset(
        options.quality_preset.as_deref(),
        &mut settings,
        &mut extra_params,
        options.output_format.as_deref(),
    );

    settings.resolution = parse_resolution(options.resolution.as_deref())?;
    settings.fps = options.fps.filter(|v| *v > 0.0);
    settings.bitrate =
        normalize_optional(options.bitrate.as_deref()).filter(|v| !v.eq_ignore_ascii_case("auto"));

    if options.hardware_acceleration {
        extra_params.insert("-hwaccel".to_string(), "auto".to_string());
    }
    if !options.preserve_metadata {
        extra_params.insert("-map_metadata".to_string(), "-1".to_string());
    }
    if options.two_pass_encoding {
        extra_params.insert(INTERNAL_TWO_PASS_KEY.to_string(), "1".to_string());
    }
    apply_color_space(options.color_space.as_deref(), &mut extra_params);

    settings.extra_params = extra_params;
    Ok(settings)
}
