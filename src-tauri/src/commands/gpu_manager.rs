use crate::core::gpu::{GpuInfo, GpuManager, HardwareAccelerationInfo};
use crate::utils::logger;
use tauri::State;

#[tauri::command]
pub async fn get_gpu_info(gpu_manager: State<'_, GpuManager>) -> Result<Vec<GpuInfo>, String> {
    gpu_manager.get_gpu_info().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_hardware_acceleration(
    gpu_manager: State<'_, GpuManager>,
) -> Result<HardwareAccelerationInfo, String> {
    gpu_manager
        .get_hardware_acceleration_info()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_hardware_acceleration(
    codec: String,
    gpu_manager: State<'_, GpuManager>,
) -> Result<bool, String> {
    match gpu_manager.test_hardware_acceleration(&codec).await {
        Ok(success) => {
            if success {
                logger::log_info(&format!(
                    "Hardware acceleration test passed for codec: {}",
                    codec
                ));
            } else {
                logger::log_warn(&format!(
                    "Hardware acceleration test failed for codec: {}",
                    codec
                ));
            }
            Ok(success)
        }
        Err(e) => {
            logger::log_error(&format!("Failed to test hardware acceleration: {}", e));
            Err(e.to_string())
        }
    }
}
