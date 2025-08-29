//! LUT缓存模块
//! 提供LUT数据的内存缓存和磁盘缓存功能

use crate::types::{AppResult, AppError};
use crate::core::lut::{LutData, LutInfo};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, Duration, Instant};
use tokio::fs;
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};

/// LUT缓存管理器
pub struct LutCache {
    /// 内存缓存
    memory_cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    /// 缓存配置
    config: CacheConfig,
    /// 缓存统计
    stats: Arc<RwLock<CacheStats>>,
    /// 磁盘缓存目录
    disk_cache_dir: PathBuf,
}

impl LutCache {
    /// 创建新的LUT缓存
    pub async fn new(cache_dir: PathBuf) -> AppResult<Self> {
        let config = CacheConfig::default();
        
        // 确保缓存目录存在
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir).await
                .map_err(AppError::from)?;
        }
        
        let cache = Self {
            memory_cache: Arc::new(RwLock::new(HashMap::new())),
            config,
            stats: Arc::new(RwLock::new(CacheStats::new())),
            disk_cache_dir: cache_dir,
        };
        
        // 启动清理任务
        cache.start_cleanup_task().await;
        
        Ok(cache)
    }

    /// 创建带配置的LUT缓存
    pub async fn with_config(cache_dir: PathBuf, config: CacheConfig) -> AppResult<Self> {
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir).await
                .map_err(AppError::from)?;
        }
        
        let cache = Self {
            memory_cache: Arc::new(RwLock::new(HashMap::new())),
            config,
            stats: Arc::new(RwLock::new(CacheStats::new())),
            disk_cache_dir: cache_dir,
        };
        
        cache.start_cleanup_task().await;
        
        Ok(cache)
    }

    /// 获取LUT数据
    pub async fn get(&self, key: &str) -> Option<Arc<LutData>> {
        // 首先检查内存缓存
        if let Some(entry) = self.get_from_memory(key) {
            self.update_stats(|stats| {
                stats.memory_hits += 1;
                stats.total_hits += 1;
            });
            return Some(entry.data.clone());
        }
        
        // 检查磁盘缓存
        if let Ok(Some(data)) = self.get_from_disk(key).await {
            // 将数据加载到内存缓存
            self.put_in_memory(key, data.clone()).await;
            
            self.update_stats(|stats| {
                stats.disk_hits += 1;
                stats.total_hits += 1;
            });
            return Some(data);
        }
        
        // 缓存未命中
        self.update_stats(|stats| {
            stats.total_misses += 1;
        });
        
        None
    }

    /// 存储LUT数据
    pub async fn put(&self, key: &str, data: Arc<LutData>) -> AppResult<()> {
        // 存储到内存缓存
        self.put_in_memory(key, data.clone()).await;
        
        // 如果启用磁盘缓存，也存储到磁盘
        if self.config.enable_disk_cache {
            self.put_to_disk(key, &data).await?;
        }
        
        self.update_stats(|stats| {
            stats.total_puts += 1;
        });
        
        Ok(())
    }

    /// 从内存缓存获取
    fn get_from_memory(&self, key: &str) -> Option<CacheEntry> {
        let cache = self.memory_cache.read().unwrap();
        
        if let Some(entry) = cache.get(key) {
            // 检查是否过期
            if !entry.is_expired(self.config.memory_ttl) {
                let mut updated_entry = entry.clone();
                updated_entry.last_accessed = Instant::now();
                updated_entry.access_count += 1;
                return Some(updated_entry);
            }
        }
        
        None
    }

    /// 存储到内存缓存
    async fn put_in_memory(&self, key: &str, data: Arc<LutData>) {
        let mut cache = self.memory_cache.write().unwrap();
        
        // 检查缓存大小限制
        if cache.len() >= self.config.max_memory_entries {
            self.evict_lru_memory(&mut cache);
        }
        
        let entry = CacheEntry {
            data,
            created_at: Instant::now(),
            last_accessed: Instant::now(),
            access_count: 1,
            size_bytes: self.estimate_size(key),
        };
        
        cache.insert(key.to_string(), entry);
    }

    /// 从磁盘缓存获取
    async fn get_from_disk(&self, key: &str) -> AppResult<Option<Arc<LutData>>> {
        let cache_file = self.get_cache_file_path(key);
        
        if !cache_file.exists() {
            return Ok(None);
        }
        
        // 检查文件是否过期
        if let Ok(metadata) = fs::metadata(&cache_file).await {
            if let Ok(modified) = metadata.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    if elapsed > self.config.disk_ttl {
                        // 文件过期，删除它
                        let _ = fs::remove_file(&cache_file).await;
                        return Ok(None);
                    }
                }
            }
        }
        
        // 读取缓存文件
        match fs::read(&cache_file).await {
            Ok(data) => {
                match bincode::deserialize::<LutData>(&data) {
                    Ok(lut_data) => Ok(Some(Arc::new(lut_data))),
                    Err(_) => {
                        // 反序列化失败，删除损坏的缓存文件
                        let _ = fs::remove_file(&cache_file).await;
                        Ok(None)
                    }
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// 存储到磁盘缓存
    async fn put_to_disk(&self, key: &str, data: &LutData) -> AppResult<()> {
        let cache_file = self.get_cache_file_path(key);
        
        // 确保父目录存在
        if let Some(parent) = cache_file.parent() {
            fs::create_dir_all(parent).await
                .map_err(AppError::from)?;
        }
        
        // 序列化数据
        let serialized = bincode::serialize(data)
            .map_err(|e| AppError::Serialization(e.to_string()))?;
        
        // 写入文件
        fs::write(&cache_file, serialized).await
            .map_err(AppError::from)?;
        
        Ok(())
    }

    /// 生成缓存键
    pub fn generate_key(&self, file_path: &Path, modification_time: Option<SystemTime>) -> String {
        let mut hasher = Sha256::new();
        
        // 添加文件路径
        hasher.update(file_path.to_string_lossy().as_bytes());
        
        // 添加修改时间（如果有）
        if let Some(mtime) = modification_time {
            if let Ok(duration) = mtime.duration_since(SystemTime::UNIX_EPOCH) {
                hasher.update(duration.as_secs().to_le_bytes());
            }
        }
        
        format!("{:x}", hasher.finalize())
    }

    /// 生成基于内容的缓存键
    pub fn generate_content_key(&self, content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        format!("{:x}", hasher.finalize())
    }

    /// 预加载LUT
    pub async fn preload(&self, file_paths: Vec<PathBuf>) -> AppResult<()> {
        for path in file_paths {
            let key = self.generate_key(&path, None);
            
            // 如果缓存中不存在，则加载并缓存
            if self.get(&key).await.is_none() {
                // 这里需要实际的LUT加载逻辑
                // 暂时跳过实际加载
                continue;
            }
        }
        
        Ok(())
    }

    /// 清除过期缓存
    pub async fn cleanup_expired(&self) -> AppResult<()> {
        // 清理内存缓存
        self.cleanup_memory_expired();
        
        // 清理磁盘缓存
        self.cleanup_disk_expired().await?;
        
        Ok(())
    }

    /// 清理内存中的过期缓存
    fn cleanup_memory_expired(&self) {
        let mut cache = self.memory_cache.write().unwrap();
        let ttl = self.config.memory_ttl;
        
        cache.retain(|_, entry| !entry.is_expired(ttl));
    }

    /// 清理磁盘中的过期缓存
    async fn cleanup_disk_expired(&self) -> AppResult<()> {
        let mut entries = fs::read_dir(&self.disk_cache_dir).await
            .map_err(AppError::from)?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(AppError::from)? {
            
            let path = entry.path();
            if path.is_file() {
                if let Ok(metadata) = fs::metadata(&path).await {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            if elapsed > self.config.disk_ttl {
                                let _ = fs::remove_file(&path).await;
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }

    /// LRU淘汰（内存缓存）
    fn evict_lru_memory(&self, cache: &mut HashMap<String, CacheEntry>) {
        if cache.is_empty() {
            return;
        }
        
        // 找到最久未访问的条目
        let lru_key = cache.iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(key, _)| key.clone());
        
        if let Some(key) = lru_key {
            cache.remove(&key);
            
            self.update_stats(|stats| {
                stats.memory_evictions += 1;
            });
        }
    }

    /// 获取缓存文件路径
    fn get_cache_file_path(&self, key: &str) -> PathBuf {
        self.disk_cache_dir.join(format!("{}.cache", key))
    }

    /// 估算条目大小
    fn estimate_size(&self, key: &str) -> usize {
        // 简单估算：键长度 + 基本开销
        key.len() + 1024 // 1KB基本开销
    }

    /// 更新统计信息
    fn update_stats<F>(&self, updater: F)
    where
        F: FnOnce(&mut CacheStats),
    {
        if let Ok(mut stats) = self.stats.write() {
            updater(&mut stats);
        }
    }

    /// 获取缓存统计信息
    pub fn get_stats(&self) -> CacheStats {
        self.stats.read().unwrap().clone()
    }

    /// 清空所有缓存
    pub async fn clear_all(&self) -> AppResult<()> {
        // 清空内存缓存
        self.memory_cache.write().unwrap().clear();
        
        // 清空磁盘缓存
        let mut entries = fs::read_dir(&self.disk_cache_dir).await
            .map_err(AppError::from)?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(AppError::from)? {
            
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "cache") {
                let _ = fs::remove_file(&path).await;
            }
        }
        
        // 重置统计信息
        *self.stats.write().unwrap() = CacheStats::new();
        
        Ok(())
    }

    /// 获取缓存大小信息
    pub async fn get_size_info(&self) -> AppResult<CacheSizeInfo> {
        let memory_entries = self.memory_cache.read().unwrap().len();
        
        let mut disk_entries = 0;
        let mut disk_size_bytes = 0;
        
        let mut entries = fs::read_dir(&self.disk_cache_dir).await
            .map_err(AppError::from)?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(AppError::from)? {
            
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "cache") {
                disk_entries += 1;
                
                if let Ok(metadata) = fs::metadata(&path).await {
                    disk_size_bytes += metadata.len();
                }
            }
        }
        
        Ok(CacheSizeInfo {
            memory_entries,
            disk_entries,
            disk_size_bytes,
        })
    }

    /// 启动清理任务
    async fn start_cleanup_task(&self) {
        let cache = Arc::new(self.memory_cache.clone());
        let config = self.config.clone();
        let disk_cache_dir = self.disk_cache_dir.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.cleanup_interval);
            
            loop {
                interval.tick().await;
                
                // 清理内存缓存
                {
                    let mut memory_cache = cache.write().unwrap();
                    memory_cache.retain(|_, entry| !entry.is_expired(config.memory_ttl));
                }
                
                // 清理磁盘缓存
                if let Ok(mut entries) = fs::read_dir(&disk_cache_dir).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let path = entry.path();
                        if path.is_file() {
                            if let Ok(metadata) = fs::metadata(&path).await {
                                if let Ok(modified) = metadata.modified() {
                                    if let Ok(elapsed) = modified.elapsed() {
                                        if elapsed > config.disk_ttl {
                                            let _ = fs::remove_file(&path).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    /// 设置缓存配置
    pub fn set_config(&mut self, config: CacheConfig) {
        self.config = config;
    }

    /// 获取缓存配置
    pub fn get_config(&self) -> &CacheConfig {
        &self.config
    }

    /// 检查键是否存在
    pub async fn contains_key(&self, key: &str) -> bool {
        // 检查内存缓存
        if self.get_from_memory(key).is_some() {
            return true;
        }
        
        // 检查磁盘缓存
        let cache_file = self.get_cache_file_path(key);
        cache_file.exists()
    }

    /// 移除指定键的缓存
    pub async fn remove(&self, key: &str) -> AppResult<bool> {
        let mut removed = false;
        
        // 从内存缓存移除
        {
            let mut cache = self.memory_cache.write().unwrap();
            if cache.remove(key).is_some() {
                removed = true;
            }
        }
        
        // 从磁盘缓存移除
        let cache_file = self.get_cache_file_path(key);
        if cache_file.exists() {
            fs::remove_file(&cache_file).await
                .map_err(AppError::from)?;
            removed = true;
        }
        
        Ok(removed)
    }

    /// 获取所有缓存键
    pub async fn get_all_keys(&self) -> AppResult<Vec<String>> {
        let mut keys = Vec::new();
        
        // 从内存缓存获取键
        {
            let cache = self.memory_cache.read().unwrap();
            keys.extend(cache.keys().cloned());
        }
        
        // 从磁盘缓存获取键
        let mut entries = fs::read_dir(&self.disk_cache_dir).await
            .map_err(AppError::from)?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(AppError::from)? {
            
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "cache") {
                if let Some(stem) = path.file_stem() {
                    let key = stem.to_string_lossy().to_string();
                    if !keys.contains(&key) {
                        keys.push(key);
                    }
                }
            }
        }
        
        Ok(keys)
    }
}

/// 缓存条目
#[derive(Debug, Clone)]
struct CacheEntry {
    data: Arc<LutData>,
    created_at: Instant,
    last_accessed: Instant,
    access_count: u64,
    size_bytes: usize,
}

impl CacheEntry {
    /// 检查是否过期
    fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }
}

/// 缓存配置
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// 最大内存缓存条目数
    pub max_memory_entries: usize,
    /// 内存缓存TTL
    pub memory_ttl: Duration,
    /// 是否启用磁盘缓存
    pub enable_disk_cache: bool,
    /// 磁盘缓存TTL
    pub disk_ttl: Duration,
    /// 清理间隔
    pub cleanup_interval: Duration,
    /// 最大磁盘缓存大小（字节）
    pub max_disk_size_bytes: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_memory_entries: 100,
            memory_ttl: Duration::from_secs(3600), // 1小时
            enable_disk_cache: true,
            disk_ttl: Duration::from_secs(86400), // 24小时
            cleanup_interval: Duration::from_secs(300), // 5分钟
            max_disk_size_bytes: 1024 * 1024 * 1024, // 1GB
        }
    }
}

/// 缓存统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub memory_hits: u64,
    pub disk_hits: u64,
    pub total_hits: u64,
    pub total_misses: u64,
    pub total_puts: u64,
    pub memory_evictions: u64,
    pub created_at: SystemTime,
}

impl CacheStats {
    pub fn new() -> Self {
        Self {
            memory_hits: 0,
            disk_hits: 0,
            total_hits: 0,
            total_misses: 0,
            total_puts: 0,
            memory_evictions: 0,
            created_at: SystemTime::now(),
        }
    }

    /// 计算命中率
    pub fn hit_rate(&self) -> f64 {
        let total_requests = self.total_hits + self.total_misses;
        if total_requests == 0 {
            0.0
        } else {
            self.total_hits as f64 / total_requests as f64
        }
    }

    /// 计算内存命中率
    pub fn memory_hit_rate(&self) -> f64 {
        if self.total_hits == 0 {
            0.0
        } else {
            self.memory_hits as f64 / self.total_hits as f64
        }
    }
}

/// 缓存大小信息
#[derive(Debug, Clone)]
pub struct CacheSizeInfo {
    pub memory_entries: usize,
    pub disk_entries: usize,
    pub disk_size_bytes: u64,
}

/// 缓存策略
#[derive(Debug, Clone, Copy)]
pub enum CacheStrategy {
    /// 仅内存缓存
    MemoryOnly,
    /// 仅磁盘缓存
    DiskOnly,
    /// 内存+磁盘缓存
    Hybrid,
    /// 禁用缓存
    Disabled,
}

/// 缓存预热器
pub struct CacheWarmer {
    cache: Arc<LutCache>,
}

impl CacheWarmer {
    pub fn new(cache: Arc<LutCache>) -> Self {
        Self { cache }
    }

    /// 预热常用LUT
    pub async fn warm_popular_luts(&self, lut_paths: Vec<PathBuf>) -> AppResult<()> {
        for path in lut_paths {
            // 生成缓存键
            let key = self.cache.generate_key(&path, None);
            
            // 如果缓存中不存在，则预加载
            if !self.cache.contains_key(&key).await {
                // 这里需要实际的LUT加载逻辑
                // 暂时跳过
                continue;
            }
        }
        
        Ok(())
    }

    /// 基于使用频率预热
    pub async fn warm_by_frequency(&self, usage_stats: HashMap<String, u64>) -> AppResult<()> {
        // 按使用频率排序
        let mut sorted_luts: Vec<_> = usage_stats.into_iter().collect();
        sorted_luts.sort_by(|a, b| b.1.cmp(&a.1));
        
        // 预热前N个最常用的LUT
        let top_n = 20;
        for (path_str, _count) in sorted_luts.into_iter().take(top_n) {
            let path = PathBuf::from(path_str);
            let key = self.cache.generate_key(&path, None);
            
            if !self.cache.contains_key(&key).await {
                // 预加载LUT
                continue;
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::collections::HashMap;

    fn create_test_lut() -> LutData {
        LutData {
            lut_type: crate::core::lut::LutType::ThreeDimensional,
            format: crate::core::lut::LutFormat::Cube,
            size: 2,
            input_range: (0.0, 1.0),
            output_range: (0.0, 1.0),
            data: vec![
                [0.0, 0.0, 0.0], [0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0], [0.0, 1.0, 1.0],
                [1.0, 0.0, 0.0], [1.0, 0.0, 1.0],
                [1.0, 1.0, 0.0], [1.0, 1.0, 1.0],
            ],
            metadata: HashMap::new(),
            title: Some("Test LUT".to_string()),
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
        }
    }

    #[tokio::test]
    async fn test_cache_creation() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LutCache::new(temp_dir.path().to_path_buf()).await.unwrap();
        
        assert_eq!(cache.get_stats().total_hits, 0);
        assert_eq!(cache.get_stats().total_misses, 0);
    }

    #[tokio::test]
    async fn test_memory_cache() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LutCache::new(temp_dir.path().to_path_buf()).await.unwrap();
        
        let lut_data = Arc::new(create_test_lut());
        let key = "test_key";
        
        // 存储数据
        cache.put(key, lut_data.clone()).await.unwrap();
        
        // 获取数据
        let retrieved = cache.get(key).await;
        assert!(retrieved.is_some());
        
        let stats = cache.get_stats();
        assert_eq!(stats.total_puts, 1);
        assert_eq!(stats.memory_hits, 1);
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LutCache::new(temp_dir.path().to_path_buf()).await.unwrap();
        
        let result = cache.get("non_existent_key").await;
        assert!(result.is_none());
        
        let stats = cache.get_stats();
        assert_eq!(stats.total_misses, 1);
    }

    #[tokio::test]
    async fn test_key_generation() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LutCache::new(temp_dir.path().to_path_buf()).await.unwrap();
        
        let path = Path::new("/test/path.cube");
        let key1 = cache.generate_key(path, None);
        let key2 = cache.generate_key(path, None);
        
        assert_eq!(key1, key2);
        assert!(!key1.is_empty());
    }

    #[tokio::test]
    async fn test_content_key_generation() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LutCache::new(temp_dir.path().to_path_buf()).await.unwrap();
        
        let content = b"test content";
        let key1 = cache.generate_content_key(content);
        let key2 = cache.generate_content_key(content);
        
        assert_eq!(key1, key2);
        assert!(!key1.is_empty());
    }

    #[tokio::test]
    async fn test_cache_removal() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LutCache::new(temp_dir.path().to_path_buf()).await.unwrap();
        
        let lut_data = Arc::new(create_test_lut());
        let key = "test_key";
        
        // 存储数据
        cache.put(key, lut_data).await.unwrap();
        assert!(cache.contains_key(key).await);
        
        // 移除数据
        let removed = cache.remove(key).await.unwrap();
        assert!(removed);
        assert!(!cache.contains_key(key).await);
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LutCache::new(temp_dir.path().to_path_buf()).await.unwrap();
        
        let lut_data = Arc::new(create_test_lut());
        
        // 存储多个数据
        cache.put("key1", lut_data.clone()).await.unwrap();
        cache.put("key2", lut_data.clone()).await.unwrap();
        
        // 清空缓存
        cache.clear_all().await.unwrap();
        
        // 验证缓存已清空
        assert!(cache.get("key1").await.is_none());
        assert!(cache.get("key2").await.is_none());
        
        let stats = cache.get_stats();
        assert_eq!(stats.total_hits, 0);
        assert_eq!(stats.total_misses, 2);
    }

    #[tokio::test]
    async fn test_cache_size_info() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LutCache::new(temp_dir.path().to_path_buf()).await.unwrap();
        
        let lut_data = Arc::new(create_test_lut());
        cache.put("test_key", lut_data).await.unwrap();
        
        let size_info = cache.get_size_info().await.unwrap();
        assert_eq!(size_info.memory_entries, 1);
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let temp_dir = TempDir::new().unwrap();
        
        let config = CacheConfig {
            memory_ttl: Duration::from_millis(100),
            ..Default::default()
        };
        
        let cache = LutCache::with_config(temp_dir.path().to_path_buf(), config).await.unwrap();
        
        let lut_data = Arc::new(create_test_lut());
        cache.put("test_key", lut_data).await.unwrap();
        
        // 等待过期
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // 应该无法获取过期的数据
        let result = cache.get("test_key").await;
        assert!(result.is_none());
    }

    #[test]
    fn test_cache_stats() {
        let stats = CacheStats::new();
        assert_eq!(stats.hit_rate(), 0.0);
        assert_eq!(stats.memory_hit_rate(), 0.0);
        
        let mut stats = CacheStats {
            memory_hits: 8,
            disk_hits: 2,
            total_hits: 10,
            total_misses: 5,
            ..CacheStats::new()
        };
        
        assert_eq!(stats.hit_rate(), 10.0 / 15.0);
        assert_eq!(stats.memory_hit_rate(), 8.0 / 10.0);
    }

    #[test]
    fn test_cache_entry_expiration() {
        let entry = CacheEntry {
            data: Arc::new(create_test_lut()),
            created_at: Instant::now() - Duration::from_secs(10),
            last_accessed: Instant::now(),
            access_count: 1,
            size_bytes: 1024,
        };
        
        assert!(entry.is_expired(Duration::from_secs(5)));
        assert!(!entry.is_expired(Duration::from_secs(15)));
    }

    #[tokio::test]
    async fn test_cache_warmer() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(LutCache::new(temp_dir.path().to_path_buf()).await.unwrap());
        let warmer = CacheWarmer::new(cache.clone());
        
        let paths = vec![PathBuf::from("/test/lut1.cube"), PathBuf::from("/test/lut2.cube")];
        
        // 预热应该成功（即使LUT不存在）
        let result = warmer.warm_popular_luts(paths).await;
        assert!(result.is_ok());
    }
}