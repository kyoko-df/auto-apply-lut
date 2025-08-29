//! 并发工具模块

use crate::types::error::{AppError, AppResult};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::sync::{Semaphore, RwLock};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// 任务池管理器
pub struct TaskPool {
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
}

impl TaskPool {
    /// 创建新的任务池
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
        }
    }
    
    /// 执行任务
    pub async fn execute<F, T>(&self, task: F) -> AppResult<T>
    where
        F: Future<Output = AppResult<T>> + Send + 'static,
        T: Send + 'static,
    {
        let _permit = self.semaphore.acquire().await
            .map_err(|_| AppError::Internal("获取任务许可失败".to_string()))?;
        
        task.await
    }
    
    /// 获取当前可用许可数
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
    
    /// 获取最大并发数
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }
}

/// 进度跟踪器
#[derive(Debug, Clone)]
pub struct ProgressTracker {
    current: Arc<Mutex<u64>>,
    total: u64,
    name: String,
}

impl ProgressTracker {
    /// 创建新的进度跟踪器
    pub fn new(name: String, total: u64) -> Self {
        Self {
            current: Arc::new(Mutex::new(0)),
            total,
            name,
        }
    }
    
    /// 增加进度
    pub fn increment(&self, amount: u64) -> AppResult<()> {
        let mut current = self.current.lock()
            .map_err(|_| AppError::Internal("获取进度锁失败".to_string()))?;
        
        *current = (*current + amount).min(self.total);
        Ok(())
    }
    
    /// 设置当前进度
    pub fn set_current(&self, current: u64) -> AppResult<()> {
        let mut current_guard = self.current.lock()
            .map_err(|_| AppError::Internal("获取进度锁失败".to_string()))?;
        
        *current_guard = current.min(self.total);
        Ok(())
    }
    
    /// 获取当前进度
    pub fn get_current(&self) -> AppResult<u64> {
        let current = self.current.lock()
            .map_err(|_| AppError::Internal("获取进度锁失败".to_string()))?;
        
        Ok(*current)
    }
    
    /// 获取总进度
    pub fn get_total(&self) -> u64 {
        self.total
    }
    
    /// 获取进度百分比
    pub fn get_percentage(&self) -> AppResult<f64> {
        let current = self.get_current()?;
        if self.total == 0 {
            Ok(100.0)
        } else {
            Ok((current as f64 / self.total as f64) * 100.0)
        }
    }
    
    /// 检查是否完成
    pub fn is_complete(&self) -> AppResult<bool> {
        Ok(self.get_current()? >= self.total)
    }
    
    /// 获取名称
    pub fn get_name(&self) -> &str {
        &self.name
    }
}

/// 缓存管理器
pub struct CacheManager<K, V> 
where
    K: Clone + Eq + std::hash::Hash,
    V: Clone,
{
    cache: Arc<RwLock<HashMap<K, V>>>,
    max_size: usize,
}

impl<K, V> CacheManager<K, V>
where
    K: Clone + Eq + std::hash::Hash,
    V: Clone,
{
    /// 创建新的缓存管理器
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size,
        }
    }
    
    /// 获取缓存值
    pub async fn get(&self, key: &K) -> Option<V> {
        let cache = self.cache.read().await;
        cache.get(key).cloned()
    }
    
    /// 设置缓存值
    pub async fn set(&self, key: K, value: V) {
        let mut cache = self.cache.write().await;
        
        // 如果缓存已满，移除一个旧项
        if cache.len() >= self.max_size {
            if let Some(old_key) = cache.keys().next().cloned() {
                cache.remove(&old_key);
            }
        }
        
        cache.insert(key, value);
    }
    
    /// 移除缓存值
    pub async fn remove(&self, key: &K) -> Option<V> {
        let mut cache = self.cache.write().await;
        cache.remove(key)
    }
    
    /// 清空缓存
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
    
    /// 获取缓存大小
    pub async fn size(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }
    
    /// 检查是否包含键
    pub async fn contains_key(&self, key: &K) -> bool {
        let cache = self.cache.read().await;
        cache.contains_key(key)
    }
}

/// 工作队列
pub struct WorkQueue<T> {
    queue: Arc<Mutex<Vec<T>>>,
    workers: usize,
}

impl<T> WorkQueue<T>
where
    T: Send + 'static,
{
    /// 创建新的工作队列
    pub fn new(workers: usize) -> Self {
        Self {
            queue: Arc::new(Mutex::new(Vec::new())),
            workers,
        }
    }
    
    /// 添加工作项
    pub fn push(&self, item: T) -> AppResult<()> {
        let mut queue = self.queue.lock()
            .map_err(|_| AppError::Internal("获取队列锁失败".to_string()))?;
        
        queue.push(item);
        Ok(())
    }
    
    /// 弹出工作项
    pub fn pop(&self) -> AppResult<Option<T>> {
        let mut queue = self.queue.lock()
            .map_err(|_| AppError::Internal("获取队列锁失败".to_string()))?;
        
        Ok(queue.pop())
    }
    
    /// 获取队列长度
    pub fn len(&self) -> AppResult<usize> {
        let queue = self.queue.lock()
            .map_err(|_| AppError::Internal("获取队列锁失败".to_string()))?;
        
        Ok(queue.len())
    }
    
    /// 检查队列是否为空
    pub fn is_empty(&self) -> AppResult<bool> {
        Ok(self.len()? == 0)
    }
    
    /// 清空队列
    pub fn clear(&self) -> AppResult<()> {
        let mut queue = self.queue.lock()
            .map_err(|_| AppError::Internal("获取队列锁失败".to_string()))?;
        
        queue.clear();
        Ok(())
    }
}

/// 批处理器
pub struct BatchProcessor<T, R> {
    batch_size: usize,
    processor: Box<dyn Fn(Vec<T>) -> Pin<Box<dyn Future<Output = AppResult<Vec<R>>> + Send>> + Send + Sync>,
}

impl<T, R> BatchProcessor<T, R>
where
    T: Send + Clone + 'static,
    R: Send + 'static,
{
    /// 创建新的批处理器
    pub fn new<F, Fut>(batch_size: usize, processor: F) -> Self
    where
        F: Fn(Vec<T>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<Vec<R>>> + Send + 'static,
    {
        Self {
            batch_size,
            processor: Box::new(move |items| Box::pin(processor(items))),
        }
    }
    
    /// 处理批次
    pub async fn process_batch(&self, items: Vec<T>) -> AppResult<Vec<R>> {
        let mut results = Vec::new();
        
        for chunk in items.chunks(self.batch_size) {
            let batch_results = (self.processor)(chunk.to_vec()).await?;
            results.extend(batch_results);
        }
        
        Ok(results)
    }
    
    /// 获取批次大小
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }
}

/// 超时包装器
pub async fn with_timeout<F, T>(duration: std::time::Duration, future: F) -> AppResult<T>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(duration, future)
        .await
        .map_err(|_| AppError::Timeout("操作超时".to_string()))
}

/// 重试包装器
pub async fn with_retry<F, Fut, T>(
    max_attempts: usize,
    delay: std::time::Duration,
    operation: F,
) -> AppResult<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = AppResult<T>>,
{
    let mut last_error = None;
    
    for attempt in 1..=max_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(error) => {
                last_error = Some(error);
                if attempt < max_attempts {
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
    
    Err(last_error.unwrap_or_else(|| AppError::Internal("重试失败".to_string())))
}