use crate::core::ffmpeg::{EncodingSettings, Resolution};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub(crate) const INTERNAL_HARDWARE_KEY: &str = "__hardware__";
pub(crate) const INTERNAL_TWO_PASS_KEY: &str = "__two_pass__";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingOptions {
    #[serde(default)]
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
    pub output_bit_depth: Option<String>,
    #[serde(default)]
    pub input_color_space: Option<String>,
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

    if width < 2
        || height < 2
        || width > 16384
        || height > 16384
        || width % 2 != 0
        || height % 2 != 0
    {
        return Err("Resolution must use even dimensions between 2 and 16384".to_string());
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

    let depth = options.output_bit_depth.as_deref().unwrap_or("8");
    if !matches!(depth, "8" | "10") {
        return Err("输出位深仅支持 8 或 10 bit".into());
    }
    if depth == "10" && !matches!(settings.video_codec.as_str(), "libx265" | "prores_ks") {
        return Err("10-bit 输出请选择 HEVC 或 ProRes".into());
    }
    extra_params.insert("__bit_depth__".into(), depth.into());
    let input_space = options.input_color_space.as_deref().unwrap_or("auto");
    crate::core::ffmpeg::color::input_filter(input_space).map_err(|e| e.to_string())?;
    extra_params.insert("__input_color_space__".into(), input_space.into());
    if input_space != "auto" {
        for key in ["-color_primaries", "-color_trc", "-colorspace"] {
            extra_params.insert(key.into(), "bt709".into());
        }
        extra_params.insert("-color_range".into(), "tv".into());
    }
    settings.resolution = parse_resolution(options.resolution.as_deref())?;
    if options
        .fps
        .is_some_and(|fps| !fps.is_finite() || fps <= 0.0 || fps > 240.0)
    {
        return Err("Frame rate must be between 0 and 240".to_string());
    }
    settings.fps = options.fps;
    settings.bitrate =
        normalize_optional(options.bitrate.as_deref()).filter(|v| !v.eq_ignore_ascii_case("auto"));

    if options.hardware_acceleration {
        extra_params.insert(INTERNAL_HARDWARE_KEY.to_string(), "1".to_string());
    }
    if !options.preserve_metadata {
        extra_params.insert("-map_metadata".to_string(), "-1".to_string());
    }
    if options.two_pass_encoding {
        extra_params.insert(INTERNAL_TWO_PASS_KEY.to_string(), "1".to_string());
    }
    // A color tag is not a color transform. Preserve source tags unless a real,
    // explicit conversion is implemented; never label camera Log / HDR as Rec.709.
    if !matches!(
        settings.video_codec.as_str(),
        "libx264" | "libx265" | "libvpx-vp9" | "prores_ks"
    ) {
        return Err("Choose H.264, HEVC, VP9 or ProRes for LUT processing".to_string());
    }
    if !matches!(
        settings.audio_codec.as_str(),
        "aac" | "copy" | "libopus" | "pcm_s16le" | "pcm_s24le"
    ) {
        return Err("Unsupported audio encoder".to_string());
    }
    let format = options
        .output_format
        .as_deref()
        .unwrap_or("mp4")
        .trim_start_matches('.');
    if !matches!(format, "mp4" | "mov" | "mkv" | "webm") {
        return Err("Output format must be MP4, MOV, MKV or WebM".to_string());
    }
    if format == "webm"
        && (settings.video_codec != "libvpx-vp9"
            || !matches!(settings.audio_codec.as_str(), "libopus" | "copy"))
    {
        return Err("WebM requires VP9 video and Opus audio".to_string());
    }
    if settings.video_codec == "prores_ks" && !matches!(format, "mov" | "mkv") {
        return Err("ProRes requires a MOV or MKV output".to_string());
    }
    if matches!(format, "mp4" | "mov") {
        extra_params.insert("-movflags".to_string(), "+faststart".to_string());
    }
    if let Some(bitrate) = &settings.bitrate {
        let numeric = bitrate.trim_end_matches(['k', 'K', 'm', 'M']);
        if numeric
            .parse::<f64>()
            .map_or(true, |n| !n.is_finite() || n <= 0.0)
        {
            return Err("Bitrate must be positive, for example 12M or 8000k".to_string());
        }
    }
    if options.two_pass_encoding
        && (settings.bitrate.is_none() || settings.video_codec != "libx264")
    {
        return Err("Two-pass encoding requires H.264 and an explicit bitrate".to_string());
    }

    settings.extra_params = extra_params;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(value: serde_json::Value) -> ProcessingOptions {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn acceleration_is_an_encoder_request_and_source_color_tags_are_preserved() {
        let settings =
            build_encoding_settings(&options(serde_json::json!({"hardware_acceleration": true})))
                .unwrap();
        assert_eq!(
            settings
                .extra_params
                .get(INTERNAL_HARDWARE_KEY)
                .map(String::as_str),
            Some("1")
        );
        assert!(!settings.extra_params.contains_key("-hwaccel"));
        assert!(!settings.extra_params.contains_key("-color_trc"));
        assert!(!settings.extra_params.contains_key("-colorspace"));
    }

    #[test]
    fn incompatible_containers_and_invalid_rates_fail_before_task_creation() {
        for invalid in [
            serde_json::json!({"output_format":"webm"}),
            serde_json::json!({"video_codec":"prores_ks", "output_format":"mp4"}),
            serde_json::json!({"fps": -1}),
            serde_json::json!({"bitrate":"-10M"}),
            serde_json::json!({"two_pass_encoding":true}),
        ] {
            assert!(build_encoding_settings(&options(invalid)).is_err());
        }
        assert!(build_encoding_settings(&options(serde_json::json!({"output_format":"webm", "video_codec":"libvpx-vp9", "audio_codec":"libopus"}))).is_ok());
        assert!(build_encoding_settings(&options(
            serde_json::json!({"two_pass_encoding":true,"bitrate":"8M"})
        ))
        .is_ok());
    }
}
