use crate::utils::logger;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    pub memory_total: Option<u64>,
    pub memory_used: Option<u64>,
    pub temperature: Option<f32>,
    pub utilization: Option<f32>,
    pub supports_hardware_acceleration: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HardwareAcceleration {
    pub available: bool,
    pub supported_codecs: Vec<String>,
    pub recommended_settings: Vec<String>,
}

#[tauri::command]
pub async fn get_gpu_info() -> Result<Vec<GpuInfo>, String> {
    let mut gpus = Vec::new();
    
    // Try to get GPU info using different methods based on platform
    #[cfg(target_os = "macos")]
    {
        // macOS: Use system_profiler
        if let Ok(output) = Command::new("system_profiler")
            .args(["SPDisplaysDataType", "-json"])
            .output()
        {
            if output.status.success() {
                if let Ok(json_str) = String::from_utf8(output.stdout) {
                    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&json_str) {
                        if let Some(displays) = json_value["SPDisplaysDataType"].as_array() {
                            for display in displays {
                                if let Some(name) = display["sppci_model"].as_str() {
                                    let vendor = display["sppci_vendor"].as_str().unwrap_or("Unknown").to_string();
                                    let memory = display["sppci_vram"]
                                        .as_str()
                                        .and_then(|s| s.split_whitespace().next())
                                        .and_then(|s| s.parse::<u64>().ok())
                                        .map(|mb| mb * 1024 * 1024); // Convert MB to bytes
                                    
                                    gpus.push(GpuInfo {
                                        name: name.to_string(),
                                        vendor,
                                        memory_total: memory,
                                        memory_used: None,
                                        temperature: None,
                                        utilization: None,
                                        supports_hardware_acceleration: check_hardware_acceleration_support(name),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        // Windows: Use wmic or PowerShell
        if let Ok(output) = Command::new("wmic")
            .args(["path", "win32_VideoController", "get", "name,AdapterRAM,VideoProcessor", "/format:csv"])
            .output()
        {
            if output.status.success() {
                if let Ok(csv_str) = String::from_utf8(output.stdout) {
                    for line in csv_str.lines().skip(1) { // Skip header
                        let fields: Vec<&str> = line.split(',').collect();
                        if fields.len() >= 3 {
                            let name = fields[2].trim();
                            if !name.is_empty() {
                                let memory = fields[1].trim().parse::<u64>().ok();
                                gpus.push(GpuInfo {
                                    name: name.to_string(),
                                    vendor: "Unknown".to_string(),
                                    memory_total: memory,
                                    memory_used: None,
                                    temperature: None,
                                    utilization: None,
                                    supports_hardware_acceleration: check_hardware_acceleration_support(name),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        // Linux: Use lspci
        if let Ok(output) = Command::new("lspci")
            .args(["-v", "-s", "$(lspci | grep VGA | cut -d' ' -f1)"])
            .output()
        {
            if output.status.success() {
                if let Ok(output_str) = String::from_utf8(output.stdout) {
                    for line in output_str.lines() {
                        if line.contains("VGA compatible controller") {
                            let name = line.split(':').last().unwrap_or("Unknown GPU").trim();
                            gpus.push(GpuInfo {
                                name: name.to_string(),
                                vendor: "Unknown".to_string(),
                                memory_total: None,
                                memory_used: None,
                                temperature: None,
                                utilization: None,
                                supports_hardware_acceleration: check_hardware_acceleration_support(name),
                            });
                        }
                    }
                }
            }
        }
    }
    
    // If no GPUs found, add a generic entry
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

#[tauri::command]
pub async fn check_hardware_acceleration() -> Result<HardwareAcceleration, String> {
    let mut supported_codecs = Vec::new();
    let mut recommended_settings = Vec::new();
    let mut available = false;
    
    // Check if FFmpeg supports hardware acceleration
    if let Ok(output) = Command::new("ffmpeg")
        .args(["-hwaccels"])
        .output()
    {
        if output.status.success() {
            if let Ok(output_str) = String::from_utf8(output.stdout) {
                for line in output_str.lines() {
                    let line = line.trim();
                    if !line.is_empty() && line != "Hardware acceleration methods:" {
                        supported_codecs.push(line.to_string());
                        available = true;
                    }
                }
            }
        }
    }
    
    // Add platform-specific recommendations
    #[cfg(target_os = "macos")]
    {
        if supported_codecs.contains(&"videotoolbox".to_string()) {
            recommended_settings.push("-c:v h264_videotoolbox".to_string());
            recommended_settings.push("-c:v hevc_videotoolbox".to_string());
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        if supported_codecs.contains(&"d3d11va".to_string()) {
            recommended_settings.push("-c:v h264_nvenc".to_string());
            recommended_settings.push("-c:v hevc_nvenc".to_string());
        }
        if supported_codecs.contains(&"dxva2".to_string()) {
            recommended_settings.push("-hwaccel dxva2".to_string());
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        if supported_codecs.contains(&"vaapi".to_string()) {
            recommended_settings.push("-hwaccel vaapi".to_string());
            recommended_settings.push("-c:v h264_vaapi".to_string());
        }
        if supported_codecs.contains(&"nvdec".to_string()) {
            recommended_settings.push("-c:v h264_nvenc".to_string());
            recommended_settings.push("-c:v hevc_nvenc".to_string());
        }
    }
    
    Ok(HardwareAcceleration {
        available,
        supported_codecs,
        recommended_settings,
    })
}

#[tauri::command]
pub async fn test_hardware_acceleration(codec: String) -> Result<bool, String> {
    // Test hardware acceleration with a small sample
    let test_args = vec![
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        "testsrc=duration=1:size=320x240:rate=1".to_string(),
        "-c:v".to_string(),
        codec,
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ];
    
    match Command::new("ffmpeg")
        .args(&test_args)
        .output()
    {
        Ok(output) => {
            let success = output.status.success();
            if success {
                logger::log_info(&format!("Hardware acceleration test passed for codec: {}", test_args[5]));
            } else {
                logger::log_warn(&format!("Hardware acceleration test failed for codec: {}", test_args[5]));
            }
            Ok(success)
        }
        Err(e) => {
            logger::log_error(&format!("Failed to test hardware acceleration: {}", e));
            Err(format!("Failed to test hardware acceleration: {}", e))
        }
    }
}

fn check_hardware_acceleration_support(gpu_name: &str) -> bool {
    let gpu_name_lower = gpu_name.to_lowercase();
    
    // Check for known GPU vendors/models that support hardware acceleration
    gpu_name_lower.contains("nvidia") ||
    gpu_name_lower.contains("amd") ||
    gpu_name_lower.contains("radeon") ||
    gpu_name_lower.contains("intel") ||
    gpu_name_lower.contains("apple") ||
    gpu_name_lower.contains("m1") ||
    gpu_name_lower.contains("m2") ||
    gpu_name_lower.contains("m3")
}