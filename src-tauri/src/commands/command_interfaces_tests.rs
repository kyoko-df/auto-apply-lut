use super::{batch_manager, file_manager, gpu_manager, lut_manager, processor, system_manager};
use crate::core::ffmpeg::processor::VideoProcessor;
use crate::core::gpu::GpuManager;
use crate::core::lut::LutManager;
use crate::core::task::{TaskManager, TaskType};
use crate::utils::config::ConfigManager;
use crate::utils::path_utils::{get_app_data_dir, get_cache_dir};
use crate::FfplayState;
use std::env;
use std::ffi::OsString;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;
use uuid::Uuid;

fn run_async<F>(fut: F) -> F::Output
where
    F: Future,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(fut)
}

fn state_ref<T: Send + Sync + 'static>(value: &T) -> tauri::State<'_, T> {
    // SAFETY: tauri::State is a transparent wrapper over &T in tauri 2.x.
    // Tests only need to invoke command functions without a full Tauri runtime.
    unsafe { std::mem::transmute::<&T, tauri::State<'_, T>>(value) }
}

struct EnvVarGuard {
    key: &'static str,
    old: Option<OsString>,
}

impl EnvVarGuard {
    fn set<V: AsRef<std::ffi::OsStr>>(key: &'static str, value: V) -> Self {
        let old = env::var_os(key);
        unsafe { env::set_var(key, value) };
        Self { key, old }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(old) = self.old.take() {
            unsafe { env::set_var(self.key, old) };
        } else {
            unsafe { env::remove_var(self.key) };
        }
    }
}

static HOME_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_temp_home<R>(f: impl FnOnce(&Path) -> R) -> R {
    let lock = HOME_ENV_LOCK.get_or_init(|| Mutex::new(())).lock().expect("lock poisoned");
    let home_dir = tempdir().expect("failed to create temp home");
    let _home_guard = EnvVarGuard::set("HOME", home_dir.path().as_os_str());
    let result = f(home_dir.path());
    drop(lock);
    result
}

#[test]
fn command_interfaces_gpu() {
    let gpu_manager_state = GpuManager::new();

    let gpus = run_async(gpu_manager::get_gpu_info(state_ref(&gpu_manager_state)))
        .expect("get_gpu_info failed");
    assert!(!gpus.is_empty());

    let hw = run_async(gpu_manager::check_hardware_acceleration(state_ref(&gpu_manager_state)))
        .expect("check_hardware_acceleration failed");
    assert_eq!(hw.available, !hw.supported_codecs.is_empty());

    let result = run_async(gpu_manager::test_hardware_acceleration(
        "definitely_invalid_codec".to_string(),
        state_ref(&gpu_manager_state),
    ));
    match result {
        Ok(success) => assert!(!success),
        Err(msg) => assert!(msg.contains("Failed to test hardware acceleration")),
    }
}

#[test]
fn command_interfaces_lut() {
    let lut_manager_state = LutManager::new();

    let formats = run_async(lut_manager::get_supported_lut_formats(state_ref(&lut_manager_state)))
        .expect("get_supported_lut_formats failed");
    assert!(formats.iter().any(|f| f == "cube"));

    let err = run_async(lut_manager::validate_lut_file(
        "not_exists.cube".to_string(),
        state_ref(&lut_manager_state),
    ))
    .expect_err("validate_lut_file should fail for non-existent file");
    assert!(!err.is_empty());

    let err = run_async(lut_manager::get_lut_info(
        "not_exists.cube".to_string(),
        state_ref(&lut_manager_state),
    ))
    .expect_err("get_lut_info should fail for non-existent file");
    assert!(!err.is_empty());
}

#[test]
fn command_interfaces_file() {
    let temp = tempdir().expect("failed to create temp dir");
    let base = temp.path();
    let input_file = base.join("input.txt");
    std::fs::write(&input_file, b"abc").expect("failed to write input file");

    let created_dir = base.join("created_dir");
    let created = run_async(file_manager::create_directory(
        created_dir.to_string_lossy().to_string(),
    ))
    .expect("create_directory failed");
    assert_eq!(created, "Directory created successfully");

    let listing = run_async(file_manager::list_directory(
        base.to_string_lossy().to_string(),
    ))
    .expect("list_directory failed");
    assert!(listing.files.iter().any(|f| f.path == input_file));

    let copied_file = base.join("copied.txt");
    let copied = run_async(file_manager::copy_file(
        input_file.to_string_lossy().to_string(),
        copied_file.to_string_lossy().to_string(),
    ))
    .expect("copy_file failed");
    assert_eq!(copied, "File copied successfully");
    assert!(copied_file.exists());

    let moved_file = base.join("moved.txt");
    let moved = run_async(file_manager::move_file(
        copied_file.to_string_lossy().to_string(),
        moved_file.to_string_lossy().to_string(),
    ))
    .expect("move_file failed");
    assert_eq!(moved, "File moved successfully");
    assert!(moved_file.exists());
    assert!(!copied_file.exists());

    let info = run_async(file_manager::get_file_info(
        moved_file.to_string_lossy().to_string(),
    ))
    .expect("get_file_info failed");
    assert_eq!(info.path, moved_file);

    let missing_path = base.join("missing.file");
    let err = run_async(file_manager::open_file(
        missing_path.to_string_lossy().to_string(),
    ))
    .expect_err("open_file should fail for non-existent file");
    assert_eq!(err, "File does not exist");

    let err = run_async(file_manager::open_folder(
        missing_path.to_string_lossy().to_string(),
    ))
    .expect_err("open_folder should fail for non-existent folder");
    assert_eq!(err, "Folder does not exist");

    let err = run_async(file_manager::open_file_location(
        missing_path.to_string_lossy().to_string(),
    ))
    .expect_err("open_file_location should fail for non-existent file");
    assert_eq!(err, "File does not exist");

    let deleted = run_async(file_manager::delete_path(
        moved_file.to_string_lossy().to_string(),
    ))
    .expect("delete_path failed for file");
    assert_eq!(deleted, "Path deleted successfully");
    assert!(!moved_file.exists());

    let err = run_async(file_manager::delete_path(
        base.join("not-exists").to_string_lossy().to_string(),
    ))
    .expect_err("delete_path should fail for non-existent path");
    assert_eq!(err, "Path does not exist");
}

#[test]
fn command_interfaces_file_ffplay() {
    with_temp_home(|_| {
        let config_manager = Mutex::new(ConfigManager::new().expect("config init failed"));
        let ffplay_state = FfplayState(Mutex::new(None));

        let err = run_async(file_manager::play_with_ffplay(
            "/definitely/missing/video.mp4".to_string(),
            state_ref(&config_manager),
            state_ref(&ffplay_state),
        ))
        .expect_err("play_with_ffplay should fail for non-existent file");
        assert_eq!(err, "File does not exist");

        let stopped = run_async(file_manager::stop_ffplay(state_ref(&ffplay_state)))
            .expect("stop_ffplay failed");
        assert_eq!(stopped, "ffplay not running");
    });
}

#[test]
fn command_interfaces_system() {
    with_temp_home(|_| {
        let config_manager = Mutex::new(ConfigManager::new().expect("config init failed"));

        let sys = run_async(system_manager::get_system_info()).expect("get_system_info failed");
        assert!(sys.cpu_count > 0);

        let settings = run_async(system_manager::get_app_settings(state_ref(&config_manager)))
            .expect("get_app_settings failed");
        assert!(!settings.log_level.is_empty());
        assert_eq!(settings.output_format, "mp4");
        assert_eq!(settings.video_codec, "libx264");
        assert_eq!(settings.audio_codec, "aac");

        let updated_settings = system_manager::AppSettings {
            default_output_dir: "/tmp/output".to_string(),
            ffmpeg_path: "/tmp/ffmpeg".to_string(),
            max_concurrent_tasks: 3,
            cache_size_mb: 2048,
            hardware_acceleration: true,
            log_level: "debug".to_string(),
            ui_theme: "dark".to_string(),
            language: "en-US".to_string(),
            output_format: "mov".to_string(),
            video_codec: "hevc_videotoolbox".to_string(),
            audio_codec: "opus".to_string(),
            quality_preset: "high_quality".to_string(),
            resolution: "4k".to_string(),
            fps: Some(29.97),
            bitrate: "18M".to_string(),
            lut_intensity: 82.5,
            lut_error_strategy: "SkipOnError".to_string(),
            color_space: "rec2020".to_string(),
            two_pass_encoding: true,
            preserve_metadata: false,
        };
        let updated = run_async(system_manager::update_app_settings(
            updated_settings,
            state_ref(&config_manager),
        ))
        .expect("update_app_settings failed");
        assert_eq!(updated, "Settings updated successfully");

        let persisted = run_async(system_manager::get_app_settings(state_ref(&config_manager)))
            .expect("get_app_settings after update failed");
        assert_eq!(persisted.default_output_dir, "/tmp/output");
        assert_eq!(persisted.ffmpeg_path, "/tmp/ffmpeg");
        assert_eq!(persisted.max_concurrent_tasks, 3);
        assert_eq!(persisted.cache_size_mb, 2048);
        assert!(persisted.hardware_acceleration);
        assert_eq!(persisted.log_level, "debug");
        assert_eq!(persisted.ui_theme, "dark");
        assert_eq!(persisted.language, "en-US");
        assert_eq!(persisted.output_format, "mov");
        assert_eq!(persisted.video_codec, "hevc_videotoolbox");
        assert_eq!(persisted.audio_codec, "opus");
        assert_eq!(persisted.quality_preset, "high_quality");
        assert_eq!(persisted.resolution, "4k");
        assert_eq!(persisted.fps, Some(29.97));
        assert_eq!(persisted.bitrate, "18M");
        assert_eq!(persisted.lut_intensity, 82.5);
        assert_eq!(persisted.lut_error_strategy, "SkipOnError");
        assert_eq!(persisted.color_space, "rec2020");
        assert!(persisted.two_pass_encoding);
        assert!(!persisted.preserve_metadata);

        let log_dir = get_app_data_dir().expect("get_app_data_dir failed").join("logs");
        std::fs::create_dir_all(&log_dir).expect("failed to create logs dir");
        let test_log_name = format!("command-test-{}.log", Uuid::new_v4());
        let test_log_path = log_dir.join(&test_log_name);
        std::fs::write(&test_log_path, "hello-log").expect("failed to write log file");

        let logs = run_async(system_manager::get_log_files()).expect("get_log_files failed");
        assert!(logs.iter().any(|f| f == &test_log_name));

        let content = run_async(system_manager::read_log_file(test_log_name.clone()))
            .expect("read_log_file failed");
        assert_eq!(content, "hello-log");
        let _ = std::fs::remove_file(test_log_path);

        let cache_dir = get_cache_dir().expect("get_cache_dir failed");
        let cache_file = cache_dir.join("cache-test.bin");
        std::fs::write(&cache_file, vec![1u8; 1024]).expect("failed to write cache test file");
        let cache_size = run_async(system_manager::get_cache_size()).expect("get_cache_size failed");
        assert!(cache_size >= 1024);

        let clear_msg = run_async(system_manager::clear_cache()).expect("clear_cache failed");
        assert_eq!(clear_msg, "Cache cleared successfully");
        let cache_size_after =
            run_async(system_manager::get_cache_size()).expect("get_cache_size after clear failed");
        assert_eq!(cache_size_after, 0);

        let codecs =
            run_async(system_manager::get_available_codecs()).expect("get_available_codecs failed");
        assert!(!codecs.video_codecs.is_empty());
        assert!(!codecs.audio_codecs.is_empty());

        run_async(system_manager::set_ffmpeg_path_config(
            Some("/definitely/not/found/ffmpeg".to_string()),
            state_ref(&config_manager),
        ))
        .expect("set_ffmpeg_path_config failed");

        let ffmpeg_cfg =
            run_async(system_manager::get_ffmpeg_path_config(state_ref(&config_manager)))
                .expect("get_ffmpeg_path_config failed");
        assert_eq!(
            ffmpeg_cfg.ffmpeg_path.as_deref(),
            Some("/definitely/not/found/ffmpeg")
        );

        let ffmpeg_info = run_async(system_manager::get_ffmpeg_info(state_ref(&config_manager)));
        assert!(ffmpeg_info.is_err());
    });
}

#[test]
fn command_interfaces_batch() {
    let temp = tempdir().expect("failed to create temp dir");
    let input_dir = temp.path().join("input");
    let output_dir = temp.path().join("output");
    std::fs::create_dir_all(&input_dir).expect("failed to create input dir");
    std::fs::create_dir_all(&output_dir).expect("failed to create output dir");

    let video_path = input_dir.join("demo.mp4");
    let lut_path = input_dir.join("look.cube");
    std::fs::write(&video_path, b"video").expect("failed to write video file");
    std::fs::write(&lut_path, b"lut").expect("failed to write lut file");

    let scan = run_async(batch_manager::scan_directory_for_videos(
        input_dir.to_string_lossy().to_string(),
    ))
    .expect("scan_directory_for_videos failed");
    assert!(scan.video_files.iter().any(|p| p.ends_with("demo.mp4")));
    assert!(scan.lut_files.iter().any(|p| p.ends_with("look.cube")));

    let generated = run_async(batch_manager::generate_batch_from_directory(
        input_dir.to_string_lossy().to_string(),
        lut_path.to_string_lossy().to_string(),
        output_dir.to_string_lossy().to_string(),
        0.75,
    ))
    .expect("generate_batch_from_directory failed");
    assert_eq!(generated.len(), 1);
    assert_eq!(generated[0].intensity, 0.75);

    let task_manager = TaskManager::default();
    let video_processor = VideoProcessor::new(PathBuf::from("ffmpeg"));
    let lut_manager = LutManager::new();

    let req = batch_manager::BatchRequest {
        items: vec![batch_manager::BatchItem {
            input_path: "/not-exists-input.mp4".to_string(),
            output_path: output_dir.join("out.mp4").to_string_lossy().to_string(),
            lut_paths: vec![lut_path.to_string_lossy().to_string()],
            lut_path: Some(lut_path.to_string_lossy().to_string()),
            intensity: 1.0,
        }],
        hardware_acceleration: false,
        output_directory: output_dir.to_string_lossy().to_string(),
        preserve_structure: false,
    };
    let start_result = run_async(batch_manager::start_batch_processing(
        req,
        state_ref(&task_manager),
        state_ref(&video_processor),
        state_ref(&lut_manager),
    ));
    assert!(start_result.is_err());

    let progress = run_async(batch_manager::get_batch_progress(
        "batch-x".to_string(),
        state_ref(&task_manager),
    ));
    assert!(progress.is_err());

    let cancel_msg = run_async(batch_manager::cancel_batch(
        "batch-x".to_string(),
        state_ref(&task_manager),
    ));
    assert!(cancel_msg.is_err());
}

#[test]
fn command_interfaces_processor() {
    let task_manager = TaskManager::default();
    let video_processor = VideoProcessor::new(PathBuf::from("ffmpeg"));
    let lut_manager = LutManager::new();

    let start_res = run_async(processor::start_video_processing(
        processor::ProcessRequest {
            input_path: "input.mp4".to_string(),
            output_path: "output.mp4".to_string(),
            output_directory: None,
            output_format: None,
            lut_paths: vec![],
            lut_path: None,
            intensity: 1.0,
            hardware_acceleration: false,
            video_codec: None,
            audio_codec: None,
            quality_preset: None,
            resolution: None,
            fps: None,
            bitrate: None,
            color_space: None,
            two_pass_encoding: false,
            preserve_metadata: true,
            lut_error_strategy: processor::LutErrorStrategy::StopOnError,
        },
        state_ref(&task_manager),
        state_ref(&video_processor),
        state_ref(&lut_manager),
    ));
    assert!(start_res.is_err());

    let missing =
        run_async(processor::get_task_progress("missing-task".to_string(), state_ref(&task_manager)));
    assert!(missing.is_err());

    let task_id = task_manager
        .create_task(TaskType::VideoProcessing, "unit-test-task".to_string())
        .expect("failed to create task");
    let progress = run_async(processor::get_task_progress(
        task_id.clone(),
        state_ref(&task_manager),
    ))
    .expect("get_task_progress failed");
    assert_eq!(progress.progress, 0.0);

    let cancel_msg = run_async(processor::cancel_task(
        task_id.clone(),
        state_ref(&task_manager),
        state_ref(&video_processor),
    ))
    .expect("cancel_task failed");
    assert_eq!(cancel_msg, "Task cancelled successfully");

    let all_tasks =
        run_async(processor::get_all_tasks(state_ref(&task_manager))).expect("get_all_tasks failed");
    assert!(!all_tasks.is_empty());
}

#[test]
fn command_interfaces_processor_video_info() {
    with_temp_home(|_| {
        let config_manager = Mutex::new(ConfigManager::new().expect("config init failed"));
        config_manager
            .lock()
            .expect("config lock poisoned")
            .set_ffmpeg_path(Some("/definitely/not/found/ffmpeg".to_string()))
            .expect("failed to set ffmpeg path");

        let result = run_async(processor::get_video_info(
            "/not/found/video.mp4".to_string(),
            state_ref(&config_manager),
        ));
        assert!(result.is_err());
    });
}
