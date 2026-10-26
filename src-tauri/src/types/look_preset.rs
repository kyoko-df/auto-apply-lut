use crate::core::photo::SourceInterpretation;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetMediaMode {
    Video,
    Photo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookPreset {
    pub id: String,
    pub name: String,
    pub media_mode: PresetMediaMode,
    pub lut_path: Option<String>,
    pub lut_fingerprint: Option<String>,
    pub intensity: f32,
    pub raw: Option<SourceInterpretation>,
}

pub fn validate_presets(presets: &[LookPreset]) -> Result<(), String> {
    if presets.len() > 100 {
        return Err("最多保存 100 个调色预设".into());
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for preset in presets {
        if preset.id.is_empty()
            || preset.id.len() > 128
            || !preset
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            || !ids.insert(&preset.id)
        {
            return Err("调色预设 ID 无效或重复".into());
        }
        if preset.name.trim().is_empty()
            || preset.name.trim() != preset.name
            || preset.name.chars().count() > 80
            || preset.name.chars().any(char::is_control)
        {
            return Err("预设名称必须为 1–80 个字符，不能包含控制字符".into());
        }
        if !names.insert((preset.media_mode, preset.name.to_lowercase())) {
            return Err("同类素材的预设名称不能重复".into());
        }
        if !preset.intensity.is_finite() || !(0.0..=100.0).contains(&preset.intensity) {
            return Err("预设 LUT 强度必须在 0–100 之间".into());
        }
        match (&preset.lut_path, &preset.lut_fingerprint) {
            (None, None) => {}
            (Some(path), Some(hash))
                if !path.trim().is_empty()
                    && path.chars().count() <= 4096
                    && !path.contains('\0')
                    && hash.len() == 64
                    && hash
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) => {}
            _ => return Err("预设 LUT 路径或内容指纹无效".into()),
        }
        if let Some(raw) = &preset.raw {
            if preset.media_mode != PresetMediaMode::Photo
                || !matches!(raw, SourceInterpretation::Raw { .. })
            {
                return Err("RAW 显影参数仅适用于照片预设".into());
            }
            raw.validate(std::path::Path::new("preset.arw"))?;
        }
    }
    Ok(())
}

pub fn deserialize_presets<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<LookPreset>, D::Error> {
    let presets = Vec::<LookPreset>::deserialize(deserializer)?;
    validate_presets(&presets).map_err(serde::de::Error::custom)?;
    Ok(presets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn value() -> Value {
        json!({ "id": "preset-1", "name": "暖色", "media_mode": "photo",
            "lut_path": "/LUT's/风格.cube", "lut_fingerprint": "a".repeat(64),
            "intensity": 45.0, "raw": { "mode": "raw", "exposure_ev": -1.0, "white_balance": "daylight" } })
    }

    #[test]
    fn preset_round_trip_keeps_raw_and_content_identity() {
        let preset: LookPreset = serde_json::from_value(value()).unwrap();
        validate_presets(std::slice::from_ref(&preset)).unwrap();
        assert_eq!(serde_json::to_value(preset).unwrap(), value());
    }

    #[test]
    fn rejects_invalid_names_strengths_lut_identity_and_raw_modes() {
        for (field, invalid) in [
            ("name", json!("  ")),
            ("name", json!("a".repeat(81))),
            ("name", json!("a\nb")),
            ("intensity", json!(-1)),
            ("intensity", json!(101)),
            ("lut_fingerprint", json!("bad")),
            ("lut_path", Value::Null),
            ("id", json!("../preset")),
            ("raw", json!({ "mode": "embedded" })),
            (
                "raw",
                json!({ "mode": "raw", "exposure_ev": 4, "white_balance": "camera" }),
            ),
            ("media_mode", json!("video")),
        ] {
            let mut input = value();
            input[field] = invalid;
            let preset: LookPreset = serde_json::from_value(input).unwrap();
            assert!(validate_presets(&[preset]).is_err(), "accepted {field}");
        }
        let mut nonfinite: LookPreset = serde_json::from_value(value()).unwrap();
        nonfinite.intensity = f32::NAN;
        assert!(validate_presets(&[nonfinite]).is_err());
        let mut extra = value();
        extra["input_color_space"] = json!("sony-slog3-cine");
        assert!(serde_json::from_value::<LookPreset>(extra).is_err());
    }

    #[test]
    fn prevents_duplicate_ids_names_and_unbounded_libraries() {
        let first: LookPreset = serde_json::from_value(value()).unwrap();
        let mut next = first.clone();
        assert!(validate_presets(&[first.clone(), next.clone()]).is_err());
        next.id = "preset-2".into();
        assert!(validate_presets(&[first.clone(), next.clone()]).is_err());
        next.media_mode = PresetMediaMode::Video;
        next.raw = None;
        validate_presets(&[first.clone(), next]).unwrap();
        let many: Vec<_> = (0..101)
            .map(|i| LookPreset {
                id: format!("p-{i}"),
                name: format!("Look {i}"),
                ..first.clone()
            })
            .collect();
        assert!(validate_presets(&many).is_err());
    }
}
