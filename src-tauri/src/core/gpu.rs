//! GPU管理模块
//! 提供GPU信息查询和硬件加速检测功能

use crate::types::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    pub memory_total: Option<u64>,
    pub memory_used: Option<u64>,
    pub temperature: Option<f32>,
    pub utilization: Option<f32>,
    pub supports_hardware_acceleration: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardwareAccelerationInfo {
    pub available: bool,
    pub supported_codecs: Vec<String>,
    pub recommended_settings: Vec<String>,
}

pub struct GpuManager;

impl GpuManager {
    pub fn new() -> Self {
        Self
    }

    pub async fn get_gpu_info(&self) -> AppResult<Vec<GpuInfo>> {
        let mut gpus = Vec::new();

        #[cfg(target_os = "macos")]
        {
            if let Ok(output) = Command::new("system_profiler")
                .args(["SPDisplaysDataType", "-json"])
                .output()
            {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    gpus.extend(parse_macos_gpu_info(&output_str)?);
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Ok(output) = Command::new("wmic")
                .args([
                    "path",
                    "win32_VideoController",
                    "get",
                    "name,AdapterRAM,VideoProcessor",
                    "/format:csv",
                ])
                .output()
            {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    gpus.extend(parse_windows_gpu_info(&output_str));
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            if let Ok(output) = Command::new("lspci").args(["-nn"]).output() {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    gpus.extend(parse_linux_gpu_info(&output_str));
                }
            }
        }

        if gpus.is_empty() {
            gpus.push(GpuInfo {
                name: "Unknown GPU".to_string(),
                vendor: "Unknown".to_string(),
                memory_total: None,
                memory_used: None,
                temperature: None,
                utilization: None,
                supports_hardware_acceleration: false,
            });
        }

        Ok(gpus)
    }

    pub async fn get_hardware_acceleration_info(&self) -> AppResult<HardwareAccelerationInfo> {
        let mut supported_codecs = Vec::new();

        if let Ok(output) = Command::new("ffmpeg").args(["-hwaccels"]).output() {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                supported_codecs = parse_ffmpeg_hwaccels(&output_str);
            }
        }

        let recommended_settings = recommended_settings_for_current_platform(&supported_codecs);

        Ok(HardwareAccelerationInfo {
            available: !supported_codecs.is_empty(),
            supported_codecs,
            recommended_settings,
        })
    }

    pub async fn check_hardware_acceleration(&self) -> AppResult<bool> {
        Ok(self.get_hardware_acceleration_info().await?.available)
    }

    pub async fn test_hardware_acceleration(&self, codec: &str) -> AppResult<bool> {
        let status = Command::new("ffmpeg")
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=320x240:rate=1",
                "-c:v",
                codec,
                "-f",
                "null",
                "-",
            ])
            .status()
            .map_err(|e| AppError::Gpu(format!("Failed to test hardware acceleration: {}", e)))?;

        Ok(status.success())
    }
}

impl Default for GpuManager {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_macos_gpu_info(output: &str) -> AppResult<Vec<GpuInfo>> {
    let value: serde_json::Value = serde_json::from_str(output)?;
    let displays = value
        .get("SPDisplaysDataType")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    let mut gpus = Vec::new();
    for display in displays {
        if let Some(name) = display.get("sppci_model").and_then(|value| value.as_str()) {
            let vendor = display
                .get("sppci_vendor")
                .and_then(|value| value.as_str())
                .unwrap_or("Unknown")
                .to_string();
            let memory_total = display
                .get("sppci_vram")
                .and_then(|value| value.as_str())
                .and_then(parse_memory_mb_text)
                .map(|mb| mb * 1024 * 1024);

            gpus.push(build_gpu_info(name, Some(vendor), memory_total));
        }
    }

    Ok(gpus)
}

fn parse_windows_gpu_info(output: &str) -> Vec<GpuInfo> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() < 4 {
                return None;
            }

            let name = fields[2].trim();
            if name.is_empty() {
                return None;
            }

            let memory_total = fields[1].trim().parse::<u64>().ok();
            let vendor_hint = fields[3].trim();
            let vendor = infer_vendor(name)
                .or_else(|| (!vendor_hint.is_empty()).then(|| vendor_hint.to_string()));

            Some(build_gpu_info(name, vendor, memory_total))
        })
        .collect()
}

fn parse_linux_gpu_info(output: &str) -> Vec<GpuInfo> {
    output
        .lines()
        .filter(|line| {
            line.contains("VGA compatible controller")
                || line.contains("3D controller")
                || line.contains("Display controller")
        })
        .filter_map(|line| {
            let name = line.split(": ").last()?.trim();
            if name.is_empty() {
                return None;
            }

            Some(build_gpu_info(name, infer_vendor(name), None))
        })
        .collect()
}

fn parse_ffmpeg_hwaccels(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "Hardware acceleration methods:")
        .map(|line| line.to_string())
        .collect()
}

fn recommended_settings_for_current_platform(hwaccels: &[String]) -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        recommended_settings_for(hwaccels, "macos")
    }

    #[cfg(target_os = "windows")]
    {
        recommended_settings_for(hwaccels, "windows")
    }

    #[cfg(target_os = "linux")]
    {
        recommended_settings_for(hwaccels, "linux")
    }
}

fn recommended_settings_for(hwaccels: &[String], platform: &str) -> Vec<String> {
    let mut settings = Vec::new();
    match platform {
        "macos" => {
            if hwaccels.iter().any(|value| value == "videotoolbox") {
                settings.push("-c:v h264_videotoolbox".to_string());
                settings.push("-c:v hevc_videotoolbox".to_string());
            }
        }
        "windows" => {
            if hwaccels.iter().any(|value| value == "d3d11va" || value == "nvdec") {
                settings.push("-c:v h264_nvenc".to_string());
                settings.push("-c:v hevc_nvenc".to_string());
            }
            if hwaccels.iter().any(|value| value == "dxva2") {
                settings.push("-hwaccel dxva2".to_string());
            }
        }
        "linux" => {
            if hwaccels.iter().any(|value| value == "vaapi") {
                settings.push("-hwaccel vaapi".to_string());
                settings.push("-c:v h264_vaapi".to_string());
            }
            if hwaccels.iter().any(|value| value == "nvdec" || value == "cuda") {
                settings.push("-c:v h264_nvenc".to_string());
                settings.push("-c:v hevc_nvenc".to_string());
            }
        }
        _ => {}
    }
    settings
}

fn build_gpu_info(name: &str, vendor: Option<String>, memory_total: Option<u64>) -> GpuInfo {
    GpuInfo {
        name: name.to_string(),
        vendor: vendor.unwrap_or_else(|| infer_vendor(name).unwrap_or_else(|| "Unknown".to_string())),
        memory_total,
        memory_used: None,
        temperature: None,
        utilization: None,
        supports_hardware_acceleration: supports_hardware_acceleration(name),
    }
}

fn infer_vendor(name: &str) -> Option<String> {
    let name_lower = name.to_lowercase();
    if name_lower.contains("nvidia") || name_lower.contains("geforce") || name_lower.contains("quadro") {
        Some("NVIDIA".to_string())
    } else if name_lower.contains("amd") || name_lower.contains("radeon") || name_lower.contains("firepro") {
        Some("AMD".to_string())
    } else if name_lower.contains("intel") || name_lower.contains("iris") || name_lower.contains("uhd") {
        Some("Intel".to_string())
    } else if name_lower.contains("apple") || name_lower.contains("m1") || name_lower.contains("m2") || name_lower.contains("m3") || name_lower.contains("m4") {
        Some("Apple".to_string())
    } else {
        None
    }
}

fn supports_hardware_acceleration(gpu_name: &str) -> bool {
    infer_vendor(gpu_name).is_some()
}

fn parse_memory_mb_text(value: &str) -> Option<u64> {
    value
        .split_whitespace()
        .find_map(|part| part.parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows_gpu_csv() {
        let output = "Node,AdapterRAM,Name,VideoProcessor\nPC,4293918720,NVIDIA GeForce RTX 3070,GeForce RTX 3070\n";
        let gpus = parse_windows_gpu_info(output);
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].vendor, "NVIDIA");
        assert_eq!(gpus[0].memory_total, Some(4293918720));
        assert!(gpus[0].supports_hardware_acceleration);
    }

    #[test]
    fn parses_linux_gpu_output() {
        let output = "01:00.0 VGA compatible controller: NVIDIA Corporation GA104 [GeForce RTX 3070] (rev a1)\n";
        let gpus = parse_linux_gpu_info(output);
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].vendor, "NVIDIA");
        assert!(gpus[0].name.contains("GeForce RTX 3070"));
    }

    #[test]
    fn parses_ffmpeg_hwaccels_and_recommendations() {
        let hwaccels = parse_ffmpeg_hwaccels("Hardware acceleration methods:\nvaapi\ncuda\nvideotoolbox\n");
        assert_eq!(hwaccels, vec!["vaapi", "cuda", "videotoolbox"]);

        let linux = recommended_settings_for(&hwaccels, "linux");
        assert!(linux.iter().any(|item| item == "-hwaccel vaapi"));
        assert!(linux.iter().any(|item| item == "-c:v h264_nvenc"));

        let macos = recommended_settings_for(&hwaccels, "macos");
        assert!(macos.iter().any(|item| item == "-c:v h264_videotoolbox"));
    }

    #[test]
    fn parses_macos_gpu_json() {
        let output = r#"{
          "SPDisplaysDataType": [
            {
              "sppci_model": "Apple M3",
              "sppci_vendor": "Apple",
              "sppci_vram": "16384 MB"
            }
          ]
        }"#;

        let gpus = parse_macos_gpu_info(output).expect("parse macos");
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].vendor, "Apple");
        assert_eq!(gpus[0].memory_total, Some(16384 * 1024 * 1024));
        assert!(gpus[0].supports_hardware_acceleration);
    }
}
