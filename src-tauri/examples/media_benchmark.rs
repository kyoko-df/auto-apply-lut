//! Developer benchmark harness. Calls the production export engine so its LUT
//! math, fallback policy, threading and audio handling cannot drift from the app.
use auto_apply_lut_lib::core::ffmpeg::{processor::VideoProcessor, EncodingSettings};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("usage: media_benchmark <ffmpeg> <fixture-directory> <report-json>".into());
    }
    let engine = PathBuf::from(&args[0]);
    let directory = PathBuf::from(&args[1]);
    let mut results = Vec::new();
    for (name, intensity, hardware) in [
        ("original-software", 0.0_f32, false),
        ("lut-software", 0.5, false),
        ("lut-auto-hardware", 0.5, true),
    ] {
        let mut processor = VideoProcessor::new(engine.clone());
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        processor.set_progress_sender(sender);
        let progress = tokio::spawn(async move {
            let mut encoder = None;
            let mut attempts = Vec::new();
            let mut fallback = Vec::new();
            while let Some(progress) = receiver.recv().await {
                if let Some(value) = progress.encoder {
                    if attempts.last() != Some(&value) {
                        attempts.push(value.clone());
                    }
                    encoder = Some(value);
                }
                if progress.message.contains("回退") || progress.message.contains("fallback") {
                    fallback.push(progress.message);
                }
            }
            (encoder, attempts, fallback)
        });
        let mut settings = EncodingSettings::default();
        settings
            .extra_params
            .insert("__hardware__".into(), hardware.to_string());
        let output_path = directory.join(format!("{name}.mp4"));
        let result = tokio::time::timeout(
            Duration::from_secs(180),
            processor.apply_luts_with_task_id(
                &directory.join("source-4k.mp4"),
                &output_path,
                &[directory.join("warm.cube")],
                &settings,
                name.to_string(),
                intensity,
            ),
        )
        .await;
        drop(processor);
        let (encoder, attempts, fallback) = progress.await?;
        let record = match result {
            Ok(Ok(result)) => json!({
                "name": name, "success": result.success, "error": result.error,
                "elapsed_seconds": result.elapsed.as_secs_f64(), "bytes": result.file_size,
                "requested_hardware": hardware, "actual_encoder": encoder,
                "encoder_attempts": attempts, "fallback_messages": fallback,
                "lut_intensity": intensity, "output_path": output_path,
                "preset": settings.preset, "crf": settings.crf,
            }),
            Ok(Err(error)) => json!({"name": name, "success": false, "error": error.to_string()}),
            Err(_) => {
                json!({"name": name, "success": false, "error": "export timed out after 180 seconds"})
            }
        };
        results.push(record);
    }
    std::fs::write(&args[2], serde_json::to_vec_pretty(&results)?)?;
    Ok(())
}
