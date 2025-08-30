//! 任务管理模块

use crate::types::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use uuid::Uuid;

/// 任务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    /// 等待中
    Pending,
    /// 运行中
    Running,
    /// 已完成
    Completed,
    /// 已失败
    Failed,
    /// 已取消
    Cancelled,
}

/// 任务类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    /// 视频处理
    VideoProcessing,
    /// LUT应用
    LutApplication,
    /// 批处理
    BatchProcessing,
    /// 文件转换
    FileConversion,
}

/// 任务信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// 任务ID
    pub id: String,
    /// 任务类型
    pub task_type: TaskType,
    /// 任务状态
    pub status: TaskStatus,
    /// 任务名称
    pub name: String,
    /// 任务描述
    pub description: Option<String>,
    /// 进度 (0-100)
    pub progress: f64,
    /// 输入文件路径
    pub input_path: Option<String>,
    /// 输出文件路径
    pub output_path: Option<String>,
    /// 错误信息
    pub error: Option<String>,
    /// 创建时间
    pub created_at: i64,
    /// 开始时间
    pub started_at: Option<i64>,
    /// 完成时间
    pub completed_at: Option<i64>,
}

impl Task {
    /// 创建新任务
    pub fn new(task_type: TaskType, name: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            task_type,
            status: TaskStatus::Pending,
            name,
            description: None,
            progress: 0.0,
            input_path: None,
            output_path: None,
            error: None,
            created_at: chrono::Utc::now().timestamp(),
            started_at: None,
            completed_at: None,
        }
    }
    
    /// 设置描述
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }
    
    /// 设置输入路径
    pub fn with_input_path(mut self, input_path: String) -> Self {
        self.input_path = Some(input_path);
        self
    }
    
    /// 设置输出路径
    pub fn with_output_path(mut self, output_path: String) -> Self {
        self.output_path = Some(output_path);
        self
    }
    
    /// 开始任务
    pub fn start(&mut self) {
        self.status = TaskStatus::Running;
        self.started_at = Some(chrono::Utc::now().timestamp());
    }
    
    /// 更新进度
    pub fn update_progress(&mut self, progress: f64) {
        self.progress = progress.clamp(0.0, 100.0);
    }
    
    /// 完成任务
    pub fn complete(&mut self) {
        self.status = TaskStatus::Completed;
        self.progress = 100.0;
        self.completed_at = Some(chrono::Utc::now().timestamp());
    }
    
    /// 失败任务
    pub fn fail(&mut self, error: String) {
        self.status = TaskStatus::Failed;
        self.error = Some(error);
        self.completed_at = Some(chrono::Utc::now().timestamp());
    }
    
    /// 取消任务
    pub fn cancel(&mut self) {
        self.status = TaskStatus::Cancelled;
        self.completed_at = Some(chrono::Utc::now().timestamp());
    }
}

/// 任务管理器
#[derive(Debug)]
pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<String, Task>>>,
    tx: mpsc::UnboundedSender<TaskEvent>,
}

impl Clone for TaskManager {
    fn clone(&self) -> Self {
        Self {
            tasks: self.tasks.clone(),
            tx: self.tx.clone(),
        }
    }
}

/// 任务事件
#[derive(Debug, Clone)]
pub enum TaskEvent {
    /// 任务创建
    Created(Task),
    /// 任务开始
    Started(String),
    /// 进度更新
    ProgressUpdated(String, f64),
    /// 任务完成
    Completed(String),
    /// 任务失败
    Failed(String, String),
    /// 任务取消
    Cancelled(String),
}

impl TaskManager {
    /// 创建新的任务管理器
    pub fn new() -> (Self, mpsc::UnboundedReceiver<TaskEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let manager = Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            tx,
        };
        (manager, rx)
    }
    
    /// 创建任务
    pub fn create_task(&self, task_type: TaskType, name: String) -> AppResult<String> {
        let task = Task::new(task_type, name);
        let task_id = task.id.clone();
        
        {
            let mut tasks = self.tasks.lock().map_err(|e| {
                AppError::Internal(format!("Failed to lock tasks: {}", e))
            })?;
            tasks.insert(task_id.clone(), task.clone());
        }
        
        let _ = self.tx.send(TaskEvent::Created(task));
        Ok(task_id)
    }
    
    /// 获取任务
    pub fn get_task(&self, task_id: &str) -> AppResult<Option<Task>> {
        let tasks = self.tasks.lock().map_err(|e| {
            AppError::Internal(format!("Failed to lock tasks: {}", e))
        })?;
        Ok(tasks.get(task_id).cloned())
    }
    
    /// 获取所有任务
    pub fn get_all_tasks(&self) -> AppResult<Vec<Task>> {
        let tasks = self.tasks.lock().map_err(|e| {
            AppError::Internal(format!("Failed to lock tasks: {}", e))
        })?;
        Ok(tasks.values().cloned().collect())
    }
    
    /// 开始任务
    pub fn start_task(&self, task_id: &str) -> AppResult<()> {
        let mut tasks = self.tasks.lock().map_err(|e| {
            AppError::Internal(format!("Failed to lock tasks: {}", e))
        })?;
        
        if let Some(task) = tasks.get_mut(task_id) {
            task.start();
            let _ = self.tx.send(TaskEvent::Started(task_id.to_string()));
        }
        
        Ok(())
    }
    
    /// 更新任务进度
    pub fn update_progress(&self, task_id: &str, progress: f64) -> AppResult<()> {
        let mut tasks = self.tasks.lock().map_err(|e| {
            AppError::Internal(format!("Failed to lock tasks: {}", e))
        })?;
        
        if let Some(task) = tasks.get_mut(task_id) {
            task.update_progress(progress);
            let _ = self.tx.send(TaskEvent::ProgressUpdated(task_id.to_string(), progress));
        }
        
        Ok(())
    }
    
    /// 完成任务
    pub fn complete_task(&self, task_id: &str) -> AppResult<()> {
        let mut tasks = self.tasks.lock().map_err(|e| {
            AppError::Internal(format!("Failed to lock tasks: {}", e))
        })?;
        
        if let Some(task) = tasks.get_mut(task_id) {
            task.complete();
            let _ = self.tx.send(TaskEvent::Completed(task_id.to_string()));
        }
        
        Ok(())
    }
    
    /// 失败任务
    pub fn fail_task(&self, task_id: &str, error: String) -> AppResult<()> {
        let mut tasks = self.tasks.lock().map_err(|e| {
            AppError::Internal(format!("Failed to lock tasks: {}", e))
        })?;
        
        if let Some(task) = tasks.get_mut(task_id) {
            task.fail(error.clone());
            let _ = self.tx.send(TaskEvent::Failed(task_id.to_string(), error));
        }
        
        Ok(())
    }
    
    /// 取消任务
    pub fn cancel_task(&self, task_id: &str) -> AppResult<()> {
        let mut tasks = self.tasks.lock().map_err(|e| {
            AppError::Internal(format!("Failed to lock tasks: {}", e))
        })?;
        
        if let Some(task) = tasks.get_mut(task_id) {
            task.cancel();
            let _ = self.tx.send(TaskEvent::Cancelled(task_id.to_string()));
        }
        
        Ok(())
    }
    
    /// 删除任务
    pub fn remove_task(&self, task_id: &str) -> AppResult<()> {
        let mut tasks = self.tasks.lock().map_err(|e| {
            AppError::Internal(format!("Failed to lock tasks: {}", e))
        })?;
        
        tasks.remove(task_id);
        Ok(())
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new().0
    }
}