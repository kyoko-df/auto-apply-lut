//! Native photo processing; never treats a photo as a zero-duration video.
pub mod codec;
pub mod color;
mod heif;
mod heif_decode;
pub mod metadata;
mod native;
pub mod processor;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub enum PhotoStage {
    Read,
    Normalize,
    Lut,
    Write,
}

impl PhotoStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "photo.read",
            Self::Normalize => "photo.normalize",
            Self::Lut => "photo.lut",
            Self::Write => "photo.write",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PhotoSpace {
    #[default]
    Srgb,
    AdobeRgb,
    DisplayP3,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SourceInterpretation {
    #[default]
    Embedded,
    Assign {
        space: PhotoSpace,
    },
    Raw {
        #[serde(default)]
        exposure_ev: f32,
        #[serde(default)]
        white_balance: RawWhiteBalance,
    },
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RawWhiteBalance {
    #[default]
    Camera,
    Daylight,
    Auto,
}

impl SourceInterpretation {
    pub fn validate(&self, path: &std::path::Path) -> Result<(), String> {
        let raw = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("arw"));
        match self {
            Self::Raw { exposure_ev, .. } => {
                if !raw || !exposure_ev.is_finite() || !(-2.0..=3.0).contains(exposure_ev) {
                    return Err(crate::types::ui_err(
                        "photo.bad_raw_settings",
                        "RAW 显影参数仅适用于 ARW；曝光必须在 −2 至 +3 EV 之间",
                    ));
                }
            }
            Self::Assign { .. } if raw => {
                return Err(crate::types::ui_err(
                    "photo.raw_profile",
                    "ARW 使用传感器 RAW 显影，不能指定 RGB 输入色彩空间",
                ))
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AlphaPolicy {
    #[default]
    Preserve,
    Flatten {
        color: [u8; 3],
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "lowercase", deny_unknown_fields)]
pub enum PhotoOutput {
    Jpeg {
        quality: u8,
        #[serde(default)]
        alpha_policy: AlphaPolicy,
    },
    Png {
        bit_depth: u8,
        #[serde(default)]
        alpha_policy: AlphaPolicy,
    },
    Tiff {
        bit_depth: u8,
        #[serde(default)]
        alpha_policy: AlphaPolicy,
    },
}
impl PhotoOutput {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Jpeg { .. } => "jpg",
            Self::Png { .. } => "png",
            Self::Tiff { .. } => "tif",
        }
    }
    pub fn bit_depth(&self) -> u8 {
        match self {
            Self::Jpeg { .. } => 8,
            Self::Png { bit_depth, .. } | Self::Tiff { bit_depth, .. } => *bit_depth,
        }
    }
    pub fn alpha_policy(&self) -> &AlphaPolicy {
        match self {
            Self::Jpeg { alpha_policy, .. }
            | Self::Png { alpha_policy, .. }
            | Self::Tiff { alpha_policy, .. } => alpha_policy,
        }
    }
    pub fn validate(&self, has_alpha: bool) -> Result<(), String> {
        match self {
            Self::Jpeg { quality, .. } if !(1..=100).contains(quality) => {
                return Err("JPEG 质量必须在 1–100 之间".into())
            }
            Self::Png { bit_depth, .. } | Self::Tiff { bit_depth, .. }
                if ![8, 16].contains(bit_depth) =>
            {
                return Err("照片位深必须为 8 或 16".into())
            }
            _ => {}
        }
        if has_alpha
            && !matches!(self, Self::Png { .. })
            && matches!(self.alpha_policy(), AlphaPolicy::Preserve)
        {
            return Err("透明照片导出为 JPEG 或 TIFF 时，请明确选择合成背景".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PhotoSettings {
    pub output: PhotoOutput,
    pub preserve_metadata: bool,
    pub preserve_gps: bool,
}
impl Default for PhotoSettings {
    fn default() -> Self {
        Self {
            output: PhotoOutput::Png {
                bit_depth: 16,
                alpha_policy: AlphaPolicy::Preserve,
            },
            preserve_metadata: true,
            preserve_gps: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhotoItemOptions {
    #[serde(default)]
    pub source_interpretation: SourceInterpretation,
    pub lut_space: Option<PhotoSpace>,
    pub lut_fingerprint: Option<String>,
    pub source_version: Option<String>,
}

#[derive(Clone)]
pub struct PhotoFrame {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u16>,
    pub alpha: Option<Vec<u16>>,
}

pub fn check_cancel(token: &tokio_util::sync::CancellationToken) -> Result<(), String> {
    if token.is_cancelled() {
        Err("照片处理已取消".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;

pub fn is_heif(path: &std::path::Path) -> bool {
    native::format(path) == Some("heif")
}
