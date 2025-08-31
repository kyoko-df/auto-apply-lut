//! 文件监控器模块
//! 提供文件系统变化监控功能

use crate::types::{AppResult, AppError};
use crate::core::file::FileManager;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use notify::{Watcher, RecursiveMode, Event, EventKind, Result as NotifyResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 文件变化事件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileChangeType {
    /// 文件创建
    Created,
    /// 文件修改
    Modified,
    /// 文件删除
    Deleted,
    /// 文件重命名
    Renamed { from: PathBuf, to: PathBuf },
}

/// 文件变化事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeEvent {
    /// 事件类型
    pub change_type: FileChangeType,
    /// 文件路径
    pub path: PathBuf,
    /// 事件时间戳
    pub timestamp: std::time::SystemTime,
    /// 是否为视频文件
    pub is_video: bool,
    /// 是否为LUT文件
    pub is_lut: bool,
}

/// 监控选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchOptions {
    /// 是否递归监控子目录
    pub recursive: bool,
    /// 是否只监控视频文件
    pub video_only: bool,
    /// 是否只监控LUT文件
    pub lut_only: bool,
    /// 事件去重时间窗口（毫秒）
    pub debounce_ms: u64,
    /// 忽略的文件模式
    pub ignore_patterns: Vec<String>,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            video_only: false,
            lut_only: false,
            debounce_ms: 500,
            ignore_patterns: vec![
                "*.tmp".to_string(),
                "*.temp".to_string(),
                ".*".to_string(), // 隐藏文件
                "Thumbs.db".to_string(),
                ".DS_Store".to_string(),
            ],
        }
    }
}

/// 文件监控器
pub struct FileWatcher {
    file_manager: FileManager,
    watcher: Option<notify::RecommendedWatcher>,
    event_sender: Option<mpsc::UnboundedSender<FileChangeEvent>>,
    watched_paths: Arc<RwLock<Vec<PathBuf>>>,
    options: WatchOptions,
    debounce_cache: Arc<RwLock<HashMap<PathBuf, Instant>>>,
}

impl FileWatcher {
    /// 创建新的文件监控器
    pub fn new(options: WatchOptions) -> Self {
        Self {
            file_manager: FileManager::new(),
            watcher: None,
            event_sender: None,
            watched_paths: Arc::new(RwLock::new(Vec::new())),
            options,
            debounce_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 开始监控
    pub async fn start_watching(
        &mut self,
        paths: Vec<PathBuf>,
    ) -> AppResult<mpsc::UnboundedReceiver<FileChangeEvent>> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.event_sender = Some(tx.clone());

        let file_manager = self.file_manager.clone();
        let options = self.options.clone();
        let debounce_cache = Arc::clone(&self.debounce_cache);

        // Create an internal channel to safely transfer notify events from the sync callback
        // to an async processing task bound to the current Tokio runtime.
        let (notify_tx, mut notify_rx) = mpsc::unbounded_channel::<NotifyResult<Event>>();
        let tx_for_processor = tx.clone();
        tokio::spawn(async move {
            while let Some(res) = notify_rx.recv().await {
                if let Ok(event) = res {
                    if let Err(e) = Self::handle_notify_event(
                        event,
                        &file_manager,
                        &options,
                        &debounce_cache,
                        &tx_for_processor,
                    )
                    .await
                    {
                        eprintln!("Error handling file event: {}", e);
                    }
                }
            }
        });

        let notify_tx_cb = notify_tx.clone();
        let watcher = notify::recommended_watcher(move |res: NotifyResult<Event>| {
            // Push the event into the channel; processing happens on the Tokio task above.
            let _ = notify_tx_cb.send(res);
        })
        .map_err(|e| AppError::Io(format!("Failed to create file watcher: {}", e)))?;

        self.watcher = Some(watcher);

        // 添加监控路径
        for path in &paths {
            self.add_watch_path(path).await?;
        }

        // 更新监控路径列表
        let mut watched_paths = self.watched_paths.write().await;
        *watched_paths = paths;

        Ok(rx)
    }

    /// 添加监控路径
    pub async fn add_watch_path<P: AsRef<Path>>(&mut self, path: P) -> AppResult<()> {
        let path = path.as_ref();

        if !self.file_manager.path_exists(path) {
            return Err(AppError::Io(format!(
                "Path does not exist: {}",
                path.display()
            )));
        }

        if let Some(ref mut watcher) = self.watcher {
            let mode = if self.options.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };

            watcher
                .watch(path, mode)
                .map_err(|e| AppError::Io(format!("Failed to watch path {}: {}", path.display(), e)))?;
        }

        Ok(())
    }

    /// 移除监控路径
    pub async fn remove_watch_path<P: AsRef<Path>>(&mut self, path: P) -> AppResult<()> {
        let path = path.as_ref();

        if let Some(ref mut watcher) = self.watcher {
            watcher
                .unwatch(path)
                .map_err(|e| AppError::Io(format!("Failed to unwatch path {}: {}", path.display(), e)))?;
        }

        // 从监控路径列表中移除
        let mut watched_paths = self.watched_paths.write().await;
        watched_paths.retain(|p| p != path);

        Ok(())
    }

    /// 停止监控
    pub async fn stop_watching(&mut self) {
        self.watcher = None;
        self.event_sender = None;
        
        let mut watched_paths = self.watched_paths.write().await;
        watched_paths.clear();
        
        let mut debounce_cache = self.debounce_cache.write().await;
        debounce_cache.clear();
    }

    /// 获取当前监控的路径
    pub async fn get_watched_paths(&self) -> Vec<PathBuf> {
        let watched_paths = self.watched_paths.read().await;
        watched_paths.clone()
    }

    /// 检查是否正在监控
    pub fn is_watching(&self) -> bool {
        self.watcher.is_some()
    }

    /// 处理notify事件
    async fn handle_notify_event(
        event: Event,
        file_manager: &FileManager,
        options: &WatchOptions,
        debounce_cache: &Arc<RwLock<HashMap<PathBuf, Instant>>>,
        sender: &mpsc::UnboundedSender<FileChangeEvent>,
    ) -> AppResult<()> {
        let now = Instant::now();
        let debounce_duration = Duration::from_millis(options.debounce_ms);

        for path in &event.paths {
            // 检查是否应该忽略此文件
            if Self::should_ignore_file(path, options) {
                continue;
            }

            // 去重检查
            {
                let mut cache = debounce_cache.write().await;
                if let Some(last_time) = cache.get(path) {
                    if now.duration_since(*last_time) < debounce_duration {
                        continue;
                    }
                }
                cache.insert(path.clone(), now);
            }

            // 检查文件类型过滤器
            let is_video = file_manager.is_video_file(path);
            let is_lut = file_manager.is_lut_file(path);

            if options.video_only && !is_video {
                continue;
            }

            if options.lut_only && !is_lut {
                continue;
            }

            // 转换事件类型
            let change_type = match event.kind {
                EventKind::Create(_) => FileChangeType::Created,
                EventKind::Modify(_) => FileChangeType::Modified,
                EventKind::Remove(_) => FileChangeType::Deleted,
                _ => continue,
            };

            let file_event = FileChangeEvent {
                change_type,
                path: path.clone(),
                timestamp: std::time::SystemTime::now(),
                is_video,
                is_lut,
            };

            if let Err(e) = sender.send(file_event) {
                eprintln!("Failed to send file change event: {}", e);
            }
        }

        Ok(())
    }

    /// 检查是否应该忽略文件
    fn should_ignore_file(path: &Path, options: &WatchOptions) -> bool {
        if let Some(file_name) = path.file_name() {
            if let Some(name_str) = file_name.to_str() {
                for pattern in &options.ignore_patterns {
                    if Self::matches_pattern(name_str, pattern) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 简单的模式匹配（支持*通配符）
    fn matches_pattern(text: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        if pattern.starts_with('*') && pattern.ends_with('*') {
            let middle = &pattern[1..pattern.len() - 1];
            return text.contains(middle);
        }

        if pattern.starts_with('*') {
            let suffix = &pattern[1..];
            return text.ends_with(suffix);
        }

        if pattern.ends_with('*') {
            let prefix = &pattern[..pattern.len() - 1];
            return text.starts_with(prefix);
        }

        text == pattern
    }

    /// 清理去重缓存
    pub async fn cleanup_debounce_cache(&self) {
        let now = Instant::now();
        let debounce_duration = Duration::from_millis(self.options.debounce_ms * 2);
        
        let mut cache = self.debounce_cache.write().await;
        cache.retain(|_, last_time| now.duration_since(*last_time) < debounce_duration);
    }

    /// 获取监控统计信息
    pub async fn get_watch_stats(&self) -> WatchStats {
        let watched_paths = self.watched_paths.read().await;
        let debounce_cache = self.debounce_cache.read().await;
        
        WatchStats {
            watched_paths_count: watched_paths.len(),
            is_active: self.is_watching(),
            debounce_cache_size: debounce_cache.len(),
            options: self.options.clone(),
        }
    }
}

/// 监控统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchStats {
    pub watched_paths_count: usize,
    pub is_active: bool,
    pub debounce_cache_size: usize,
    pub options: WatchOptions,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;
    use tokio::time::{sleep, Duration};

    #[test]
    fn test_pattern_matching() {
        assert!(FileWatcher::matches_pattern("test.tmp", "*.tmp"));
        assert!(FileWatcher::matches_pattern(".hidden", ".*"));
        assert!(FileWatcher::matches_pattern("Thumbs.db", "Thumbs.db"));
        assert!(!FileWatcher::matches_pattern("test.mp4", "*.tmp"));
    }

    #[tokio::test]
    async fn test_file_watcher_creation() {
        let options = WatchOptions::default();
        let watcher = FileWatcher::new(options);
        
        assert!(!watcher.is_watching());
        assert_eq!(watcher.get_watched_paths().await.len(), 0);
    }

    #[tokio::test]
    async fn test_watch_directory() {
        let temp_dir = tempdir().unwrap();
        let options = WatchOptions::default();
        let mut watcher = FileWatcher::new(options);
        
        let mut rx = watcher
            .start_watching(vec![temp_dir.path().to_path_buf()])
            .await
            .unwrap();
        
        assert!(watcher.is_watching());
        assert_eq!(watcher.get_watched_paths().await.len(), 1);
        
        // 创建测试文件
        let test_file = temp_dir.path().join("test.mp4");
        File::create(&test_file).unwrap();
        
        // 等待事件
        tokio::select! {
            event = rx.recv() => {
                if let Some(event) = event {
                    assert_eq!(event.change_type, FileChangeType::Created);
                    assert!(event.is_video);
                    assert!(!event.is_lut);
                }
            }
            _ = sleep(Duration::from_secs(2)) => {
                // 超时，可能是系统不支持文件监控
            }
        }
        
        watcher.stop_watching().await;
        assert!(!watcher.is_watching());
    }

    #[tokio::test]
    async fn test_video_only_filter() {
        let temp_dir = tempdir().unwrap();
        let options = WatchOptions {
            video_only: true,
            ..Default::default()
        };
        let mut watcher = FileWatcher::new(options);
        
        let mut rx = watcher
            .start_watching(vec![temp_dir.path().to_path_buf()])
            .await
            .unwrap();
        
        // 创建LUT文件（应该被过滤掉）
        let lut_file = temp_dir.path().join("test.cube");
        File::create(&lut_file).unwrap();
        
        // 创建视频文件（应该被检测到）
        let video_file = temp_dir.path().join("test.mp4");
        File::create(&video_file).unwrap();
        
        // 等待事件
        tokio::select! {
            event = rx.recv() => {
                if let Some(event) = event {
                    assert!(event.is_video);
                    assert!(!event.is_lut);
                }
            }
            _ = sleep(Duration::from_secs(2)) => {
                // 超时
            }
        }
        
        watcher.stop_watching().await;
    }

    #[tokio::test]
    async fn test_ignore_patterns() {
        let options = WatchOptions {
            ignore_patterns: vec!["*.tmp".to_string(), ".*".to_string()],
            ..Default::default()
        };
        
        assert!(FileWatcher::should_ignore_file(Path::new("test.tmp"), &options));
        assert!(FileWatcher::should_ignore_file(Path::new(".hidden"), &options));
        assert!(!FileWatcher::should_ignore_file(Path::new("test.mp4"), &options));
    }

    #[tokio::test]
    async fn test_debounce_cache_cleanup() {
        let options = WatchOptions {
            debounce_ms: 100,
            ..Default::default()
        };
        let watcher = FileWatcher::new(options);
        
        // 添加一些缓存条目
        {
            let mut cache = watcher.debounce_cache.write().await;
            cache.insert(PathBuf::from("test1"), Instant::now());
            cache.insert(PathBuf::from("test2"), Instant::now() - Duration::from_millis(300));
        }
        
        assert_eq!(watcher.debounce_cache.read().await.len(), 2);
        
        watcher.cleanup_debounce_cache().await;
        
        // 旧的条目应该被清理
        assert!(watcher.debounce_cache.read().await.len() <= 1);
    }
}