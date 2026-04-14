//! LUT处理器模块
//! 提供LUT应用到图像数据的功能

use crate::core::lut::{LutData, LutType, LutUtils};
use crate::types::{AppError, AppResult};
use rayon::prelude::*;
use std::sync::Arc;
use tokio::task;

/// LUT处理器
pub struct LutProcessor {
    /// 并行处理的线程数
    thread_count: usize,
    /// 是否启用GPU加速（预留）
    gpu_acceleration: bool,
}

impl LutProcessor {
    /// 创建新的LUT处理器
    pub fn new() -> Self {
        Self {
            thread_count: num_cpus::get(),
            gpu_acceleration: false,
        }
    }

    /// 创建带配置的LUT处理器
    pub fn with_config(thread_count: usize, gpu_acceleration: bool) -> Self {
        Self {
            thread_count,
            gpu_acceleration,
        }
    }

    /// 应用LUT到图像数据
    pub async fn apply(
        &self,
        lut_data: &LutData,
        image_data: &[u8],
        width: u32,
        height: u32,
        channels: u32,
    ) -> AppResult<Vec<u8>> {
        if channels != 3 && channels != 4 {
            return Err(AppError::Validation(
                "Only RGB and RGBA images are supported".to_string(),
            ));
        }

        let pixel_count = (width * height) as usize;
        let expected_size = pixel_count * channels as usize;

        if image_data.len() != expected_size {
            return Err(AppError::Validation(format!(
                "Image data size mismatch: expected {}, got {}",
                expected_size,
                image_data.len()
            )));
        }

        match lut_data.lut_type {
            LutType::ThreeDimensional => {
                self.apply_3d_lut(lut_data, image_data, width, height, channels)
                    .await
            }
            LutType::OneDimensional => {
                self.apply_1d_lut(lut_data, image_data, width, height, channels)
                    .await
            }
            _ => Err(AppError::Validation(
                "Unsupported LUT type for processing".to_string(),
            )),
        }
    }

    /// 应用3D LUT
    async fn apply_3d_lut(
        &self,
        lut_data: &LutData,
        image_data: &[u8],
        width: u32,
        height: u32,
        channels: u32,
    ) -> AppResult<Vec<u8>> {
        let lut_data = Arc::new(lut_data.clone());
        let image_data = image_data.to_vec();
        let pixel_count = (width * height) as usize;

        // 使用Rayon进行并行处理
        let result = task::spawn_blocking(move || {
            let mut output = vec![0u8; image_data.len()];

            output
                .par_chunks_mut(channels as usize)
                .zip(image_data.par_chunks(channels as usize))
                .for_each(|(output_pixel, input_pixel)| {
                    // 将8位值转换为0-1范围的浮点数
                    let r = input_pixel[0] as f32 / 255.0;
                    let g = input_pixel[1] as f32 / 255.0;
                    let b = input_pixel[2] as f32 / 255.0;

                    // 应用3D LUT插值
                    if let Ok(result) = LutUtils::interpolate_3d(&lut_data, r, g, b) {
                        // 将结果转换回8位值
                        output_pixel[0] = (result[0].clamp(0.0, 1.0) * 255.0).round() as u8;
                        output_pixel[1] = (result[1].clamp(0.0, 1.0) * 255.0).round() as u8;
                        output_pixel[2] = (result[2].clamp(0.0, 1.0) * 255.0).round() as u8;

                        // 保持Alpha通道不变（如果存在）
                        if channels == 4 {
                            output_pixel[3] = input_pixel[3];
                        }
                    } else {
                        // 如果插值失败，保持原值
                        output_pixel.copy_from_slice(input_pixel);
                    }
                });

            output
        })
        .await
        .map_err(|e| AppError::LutProcessing(format!("Failed to apply 3D LUT: {}", e)))?;

        Ok(result)
    }

    /// 应用1D LUT
    async fn apply_1d_lut(
        &self,
        lut_data: &LutData,
        image_data: &[u8],
        width: u32,
        height: u32,
        channels: u32,
    ) -> AppResult<Vec<u8>> {
        let lut_data = Arc::new(lut_data.clone());
        let image_data = image_data.to_vec();

        let result = task::spawn_blocking(move || {
            let mut output = vec![0u8; image_data.len()];

            output
                .par_chunks_mut(channels as usize)
                .zip(image_data.par_chunks(channels as usize))
                .for_each(|(output_pixel, input_pixel)| {
                    // 分别处理每个颜色通道
                    for channel in 0..3 {
                        let value = input_pixel[channel] as f32 / 255.0;

                        if let Ok(result) = LutUtils::interpolate_1d(&lut_data, value, channel) {
                            output_pixel[channel] = (result.clamp(0.0, 1.0) * 255.0).round() as u8;
                        } else {
                            output_pixel[channel] = input_pixel[channel];
                        }
                    }

                    // 保持Alpha通道不变（如果存在）
                    if channels == 4 {
                        output_pixel[3] = input_pixel[3];
                    }
                });

            output
        })
        .await
        .map_err(|e| AppError::LutProcessing(format!("Failed to apply 1D LUT: {}", e)))?;

        Ok(result)
    }

    /// 批量应用LUT到多个图像
    pub async fn batch_apply(
        &self,
        lut_data: &LutData,
        images: Vec<ImageData>,
    ) -> AppResult<Vec<Vec<u8>>> {
        let mut results = Vec::new();

        for image in images {
            let result = self
                .apply(
                    lut_data,
                    &image.data,
                    image.width,
                    image.height,
                    image.channels,
                )
                .await?;
            results.push(result);
        }

        Ok(results)
    }

    /// 应用LUT到图像的指定区域
    pub async fn apply_region(
        &self,
        lut_data: &LutData,
        image_data: &[u8],
        width: u32,
        height: u32,
        channels: u32,
        region: ImageRegion,
    ) -> AppResult<Vec<u8>> {
        // 验证区域参数
        if region.x + region.width > width || region.y + region.height > height {
            return Err(AppError::Validation(
                "Region exceeds image boundaries".to_string(),
            ));
        }

        let mut output = image_data.to_vec();

        // 提取区域数据
        let mut region_data = Vec::new();
        for y in region.y..region.y + region.height {
            for x in region.x..region.x + region.width {
                let pixel_index = ((y * width + x) * channels) as usize;
                for c in 0..channels {
                    region_data.push(image_data[pixel_index + c as usize]);
                }
            }
        }

        // 应用LUT到区域
        let processed_region = self
            .apply(
                lut_data,
                &region_data,
                region.width,
                region.height,
                channels,
            )
            .await?;

        // 将处理后的区域数据写回原图像
        let mut region_index = 0;
        for y in region.y..region.y + region.height {
            for x in region.x..region.x + region.width {
                let pixel_index = ((y * width + x) * channels) as usize;
                for c in 0..channels {
                    output[pixel_index + c as usize] = processed_region[region_index];
                    region_index += 1;
                }
            }
        }

        Ok(output)
    }

    /// 应用LUT并混合原图像（强度控制）
    pub async fn apply_with_intensity(
        &self,
        lut_data: &LutData,
        image_data: &[u8],
        width: u32,
        height: u32,
        channels: u32,
        intensity: f32,
    ) -> AppResult<Vec<u8>> {
        if !(0.0..=1.0).contains(&intensity) {
            return Err(AppError::Validation(
                "Intensity must be between 0.0 and 1.0".to_string(),
            ));
        }

        // 应用LUT
        let processed = self
            .apply(lut_data, image_data, width, height, channels)
            .await?;

        // 混合原图像和处理后的图像
        let mut output = vec![0u8; image_data.len()];

        for i in 0..image_data.len() {
            let original = image_data[i] as f32;
            let processed_val = processed[i] as f32;
            let blended = original * (1.0 - intensity) + processed_val * intensity;
            output[i] = blended.clamp(0.0, 255.0).round() as u8;
        }

        Ok(output)
    }

    /// 预览LUT效果（缩略图）
    pub async fn preview(
        &self,
        lut_data: &LutData,
        image_data: &[u8],
        width: u32,
        height: u32,
        channels: u32,
        preview_size: u32,
    ) -> AppResult<Vec<u8>> {
        // 计算缩放比例
        let scale_x = width as f32 / preview_size as f32;
        let scale_y = height as f32 / preview_size as f32;

        // 生成缩略图数据
        let mut preview_data = Vec::new();

        for y in 0..preview_size {
            for x in 0..preview_size {
                let src_x = (x as f32 * scale_x) as u32;
                let src_y = (y as f32 * scale_y) as u32;

                if src_x < width && src_y < height {
                    let pixel_index = ((src_y * width + src_x) * channels) as usize;
                    for c in 0..channels {
                        preview_data.push(image_data[pixel_index + c as usize]);
                    }
                } else {
                    // 填充黑色像素
                    for _ in 0..channels {
                        preview_data.push(0);
                    }
                }
            }
        }

        // 应用LUT到缩略图
        self.apply(
            lut_data,
            &preview_data,
            preview_size,
            preview_size,
            channels,
        )
        .await
    }

    /// 获取处理器配置
    pub fn get_config(&self) -> ProcessorConfig {
        ProcessorConfig {
            thread_count: self.thread_count,
            gpu_acceleration: self.gpu_acceleration,
        }
    }

    /// 设置线程数
    pub fn set_thread_count(&mut self, count: usize) {
        self.thread_count = count.max(1);
    }

    /// 启用/禁用GPU加速
    pub fn set_gpu_acceleration(&mut self, enabled: bool) {
        self.gpu_acceleration = enabled;
    }
}

impl Default for LutProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// 图像数据结构
#[derive(Debug, Clone)]
pub struct ImageData {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub channels: u32,
}

/// 图像区域
#[derive(Debug, Clone, Copy)]
pub struct ImageRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// 处理器配置
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    pub thread_count: usize,
    pub gpu_acceleration: bool,
}

/// LUT处理统计信息
#[derive(Debug, Clone)]
pub struct ProcessingStats {
    pub pixels_processed: usize,
    pub processing_time_ms: u64,
    pub throughput_mpixels_per_sec: f64,
}

/// 性能分析器
pub struct PerformanceProfiler {
    start_time: std::time::Instant,
    pixels_processed: usize,
}

impl PerformanceProfiler {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            pixels_processed: 0,
        }
    }

    pub fn start(&mut self) {
        self.start_time = std::time::Instant::now();
        self.pixels_processed = 0;
    }

    pub fn add_pixels(&mut self, count: usize) {
        self.pixels_processed += count;
    }

    pub fn finish(&self) -> ProcessingStats {
        let elapsed = self.start_time.elapsed();
        let processing_time_ms = elapsed.as_millis() as u64;
        let throughput = if processing_time_ms > 0 {
            (self.pixels_processed as f64) / (processing_time_ms as f64 / 1000.0) / 1_000_000.0
        } else {
            0.0
        };

        ProcessingStats {
            pixels_processed: self.pixels_processed,
            processing_time_ms,
            throughput_mpixels_per_sec: throughput,
        }
    }
}

/// LUT处理工具函数
pub struct ProcessingUtils;

impl ProcessingUtils {
    /// 验证图像数据格式
    pub fn validate_image_data(
        data: &[u8],
        width: u32,
        height: u32,
        channels: u32,
    ) -> AppResult<()> {
        let expected_size = (width * height * channels) as usize;
        if data.len() != expected_size {
            return Err(AppError::Validation(format!(
                "Image data size mismatch: expected {}, got {}",
                expected_size,
                data.len()
            )));
        }

        // 支持 1~4 通道，其中 3、4 通道用于实际 LUT 处理；1 通道用于直方图/统计等工具函数
        if channels == 0 || channels > 4 {
            return Err(AppError::Validation(
                "Channels must be between 1 and 4".to_string(),
            ));
        }

        // 对灰度图（1 通道）增加约束：要求为正方形，避免某些处理中的歧义
        if channels == 1 && width != height {
            return Err(AppError::Validation(
                "Grayscale images (1 channel) must be square".to_string(),
            ));
        }

        Ok(())
    }

    /// 转换图像格式（RGB <-> RGBA）
    pub fn convert_format(
        data: &[u8],
        width: u32,
        height: u32,
        from_channels: u32,
        to_channels: u32,
    ) -> AppResult<Vec<u8>> {
        if from_channels == to_channels {
            return Ok(data.to_vec());
        }

        let pixel_count = (width * height) as usize;
        let mut output = Vec::new();

        match (from_channels, to_channels) {
            (3, 4) => {
                // RGB -> RGBA
                for i in 0..pixel_count {
                    let base = i * 3;
                    output.push(data[base]); // R
                    output.push(data[base + 1]); // G
                    output.push(data[base + 2]); // B
                    output.push(255); // A (不透明)
                }
            }
            (4, 3) => {
                // RGBA -> RGB
                for i in 0..pixel_count {
                    let base = i * 4;
                    output.push(data[base]); // R
                    output.push(data[base + 1]); // G
                    output.push(data[base + 2]); // B
                                                 // 忽略Alpha通道
                }
            }
            _ => {
                return Err(AppError::Validation(
                    "Unsupported format conversion".to_string(),
                ));
            }
        }

        Ok(output)
    }

    /// 计算图像直方图
    pub fn calculate_histogram(
        data: &[u8],
        width: u32,
        height: u32,
        channels: u32,
    ) -> AppResult<Vec<Vec<u32>>> {
        Self::validate_image_data(data, width, height, channels)?;

        let mut histograms = vec![vec![0u32; 256]; channels.min(3) as usize];

        for pixel in data.chunks(channels as usize) {
            for (channel, &value) in pixel.iter().take(3).enumerate() {
                histograms[channel][value as usize] += 1;
            }
        }

        Ok(histograms)
    }

    /// 计算图像统计信息
    pub fn calculate_image_stats(
        data: &[u8],
        width: u32,
        height: u32,
        channels: u32,
    ) -> AppResult<ImageStats> {
        Self::validate_image_data(data, width, height, channels)?;

        let pixel_count = (width * height) as usize;
        let mut channel_stats = Vec::new();

        for channel in 0..channels.min(3) {
            let mut values = Vec::new();

            for i in 0..pixel_count {
                let pixel_index = i * channels as usize + channel as usize;
                values.push(data[pixel_index]);
            }

            values.sort_unstable();

            let sum: u64 = values.iter().map(|&v| v as u64).sum();
            let mean = sum as f64 / values.len() as f64;

            let variance: f64 = values
                .iter()
                .map(|&v| (v as f64 - mean).powi(2))
                .sum::<f64>()
                / values.len() as f64;

            channel_stats.push(ChannelStats {
                min: values[0],
                max: values[values.len() - 1],
                mean,
                std_dev: variance.sqrt(),
                median: values[values.len() / 2],
            });
        }

        Ok(ImageStats {
            width,
            height,
            channels,
            pixel_count: pixel_count as u32,
            channel_stats,
        })
    }
}

/// 图像统计信息
#[derive(Debug, Clone)]
pub struct ImageStats {
    pub width: u32,
    pub height: u32,
    pub channels: u32,
    pub pixel_count: u32,
    pub channel_stats: Vec<ChannelStats>,
}

/// 通道统计信息
#[derive(Debug, Clone)]
pub struct ChannelStats {
    pub min: u8,
    pub max: u8,
    pub mean: f64,
    pub std_dev: f64,
    pub median: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::lut::{LutData, LutFormat, LutType};
    use std::collections::HashMap;

    fn create_test_lut() -> LutData {
        LutData {
            lut_type: LutType::ThreeDimensional,
            format: LutFormat::Cube,
            size: 2,
            title: None,
            description: None,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
            data_3d: Some(vec![
                vec![
                    vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
                    vec![[0.0, 1.0, 0.0], [0.0, 1.0, 1.0]],
                ],
                vec![
                    vec![[1.0, 0.0, 0.0], [1.0, 0.0, 1.0]],
                    vec![[1.0, 1.0, 0.0], [1.0, 1.0, 1.0]],
                ],
            ]),
            data_1d: None,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_processor_creation() {
        let processor = LutProcessor::new();
        let config = processor.get_config();

        assert!(config.thread_count > 0);
        assert!(!config.gpu_acceleration);
    }

    #[tokio::test]
    async fn test_3d_lut_application() {
        let processor = LutProcessor::new();
        let lut_data = create_test_lut();

        // 创建测试图像数据（2x2 RGB）
        let image_data = vec![
            0, 0, 0, // 黑色
            255, 0, 0, // 红色
            0, 255, 0, // 绿色
            255, 255, 255, // 白色
        ];

        let result = processor
            .apply(&lut_data, &image_data, 2, 2, 3)
            .await
            .unwrap();

        assert_eq!(result.len(), image_data.len());
        // 验证黑色像素保持黑色
        assert_eq!(result[0], 0);
        assert_eq!(result[1], 0);
        assert_eq!(result[2], 0);
    }

    #[tokio::test]
    async fn test_intensity_blending() {
        let processor = LutProcessor::new();
        let lut_data = create_test_lut();

        let image_data = vec![128, 128, 128]; // 灰色像素

        // 强度为0应该返回原图像
        let result_0 = processor
            .apply_with_intensity(&lut_data, &image_data, 1, 1, 3, 0.0)
            .await
            .unwrap();
        assert_eq!(result_0, image_data);

        // 强度为1应该完全应用LUT
        let result_1 = processor
            .apply_with_intensity(&lut_data, &image_data, 1, 1, 3, 1.0)
            .await
            .unwrap();
        let full_lut = processor
            .apply(&lut_data, &image_data, 1, 1, 3)
            .await
            .unwrap();
        assert_eq!(result_1, full_lut);
    }

    #[tokio::test]
    async fn test_region_processing() {
        let processor = LutProcessor::new();
        let lut_data = create_test_lut();

        // 创建4x4图像
        let image_data = vec![128u8; 4 * 4 * 3];

        let region = ImageRegion {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        };

        let result = processor
            .apply_region(&lut_data, &image_data, 4, 4, 3, region)
            .await
            .unwrap();

        assert_eq!(result.len(), image_data.len());
        // 区域外的像素应该保持不变
        assert_eq!(result[0], 128); // 第一个像素
    }

    #[test]
    fn test_format_conversion() {
        let rgb_data = vec![255, 0, 0, 0, 255, 0]; // 2像素RGB

        // RGB -> RGBA
        let rgba_data = ProcessingUtils::convert_format(&rgb_data, 2, 1, 3, 4).unwrap();
        assert_eq!(rgba_data.len(), 8);
        assert_eq!(rgba_data[3], 255); // Alpha通道
        assert_eq!(rgba_data[7], 255); // Alpha通道

        // RGBA -> RGB
        let back_to_rgb = ProcessingUtils::convert_format(&rgba_data, 2, 1, 4, 3).unwrap();
        assert_eq!(back_to_rgb, rgb_data);
    }

    #[test]
    fn test_image_validation() {
        let data = vec![255u8; 100];

        // 有效数据
        assert!(ProcessingUtils::validate_image_data(&data, 10, 10, 1).is_ok());

        // 无效大小
        assert!(ProcessingUtils::validate_image_data(&data, 10, 10, 2).is_err());

        // 无效通道数
        assert!(ProcessingUtils::validate_image_data(&data, 20, 5, 1).is_err());
    }

    #[test]
    fn test_histogram_calculation() {
        let data = vec![
            0, 0, 0, // 黑色
            255, 255, 255, // 白色
            128, 128, 128, // 灰色
        ];

        let histograms = ProcessingUtils::calculate_histogram(&data, 3, 1, 3).unwrap();

        assert_eq!(histograms.len(), 3); // RGB三个通道
        assert_eq!(histograms[0][0], 1); // R通道的0值有1个
        assert_eq!(histograms[0][128], 1); // R通道的128值有1个
        assert_eq!(histograms[0][255], 1); // R通道的255值有1个
    }

    #[test]
    fn test_performance_profiler() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start();
        profiler.add_pixels(1000);

        std::thread::sleep(std::time::Duration::from_millis(10));

        let stats = profiler.finish();
        assert_eq!(stats.pixels_processed, 1000);
        assert!(stats.processing_time_ms >= 10);
    }
}
