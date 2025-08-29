use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use chrono::{DateTime, Utc};
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
    /// 失败
    Failed,
    /// 已取消
    Cancelled,
    /// 暂停
    Paused,
}

/// 任务类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskType {
    /// 单个视频处理
    SingleVideo,
    /// 批量视频处理
    BatchVideo,
    /// LUT验证
    LutValidation,
    /// 系统检查
    SystemCheck,
}

/// 任务优先级
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum TaskPriority {
    /// 低优先级
    Low = 1,
    /// 普通优先级
    Normal = 2,
    /// 高优先级
    High = 3,
    /// 紧急优先级
    Urgent = 4,
}

/// 任务信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    /// 任务ID
    pub id: Uuid,
    /// 任务名称
    pub name: String,
    /// 任务描述
    pub description: Option<String>,
    /// 任务类型
    pub task_type: TaskType,
    /// 任务状态
    pub status: TaskStatus,
    /// 任务优先级
    pub priority: TaskPriority,
    /// 进度（0-100）
    pub progress: f32,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 开始时间
    pub started_at: Option<DateTime<Utc>>,
    /// 完成时间
    pub completed_at: Option<DateTime<Utc>>,
    /// 错误信息
    pub error_message: Option<String>,
    /// 任务配置
    pub config: TaskConfig,
    /// 结果信息
    pub result: Option<TaskResult>,
}

/// 任务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    /// 输入文件路径
    pub input_paths: Vec<PathBuf>,
    /// 输出目录
    pub output_dir: PathBuf,
    /// LUT文件路径
    pub lut_path: Option<PathBuf>,
    /// 视频处理选项
    pub video_options: Option<super::video::VideoProcessOptions>,
    /// LUT应用选项
    pub lut_options: Option<super::lut::LutApplyOptions>,
    /// 是否使用GPU加速
    pub use_gpu: bool,
    /// 并发数
    pub concurrency: Option<usize>,
    /// 是否覆盖现有文件
    pub overwrite: bool,
}

/// 任务结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// 处理成功的文件数
    pub success_count: usize,
    /// 处理失败的文件数
    pub failed_count: usize,
    /// 跳过的文件数
    pub skipped_count: usize,
    /// 总处理时间（秒）
    pub total_duration: f64,
    /// 平均处理时间（秒）
    pub average_duration: f64,
    /// 输出文件列表
    pub output_files: Vec<PathBuf>,
    /// 失败的文件列表
    pub failed_files: Vec<FailedFile>,
    /// 性能统计
    pub performance_stats: Option<PerformanceStats>,
}

/// 失败的文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedFile {
    /// 文件路径
    pub path: PathBuf,
    /// 错误信息
    pub error: String,
    /// 错误代码
    pub error_code: Option<i32>,
}

/// 性能统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceStats {
    /// CPU使用率（平均）
    pub avg_cpu_usage: f32,
    /// 内存使用量（峰值，MB）
    pub peak_memory_usage: u64,
    /// GPU使用率（平均，如果使用GPU）
    pub avg_gpu_usage: Option<f32>,
    /// GPU内存使用量（峰值，MB）
    pub peak_gpu_memory: Option<u64>,
    /// 磁盘IO统计
    pub disk_io: DiskIoStats,
}

/// 磁盘IO统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskIoStats {
    /// 读取字节数
    pub bytes_read: u64,
    /// 写入字节数
    pub bytes_written: u64,
    /// 读取操作数
    pub read_operations: u64,
    /// 写入操作数
    pub write_operations: u64,
}

/// 任务进度信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    /// 任务ID
    pub task_id: Uuid,
    /// 当前进度（0-100）
    pub progress: f32,
    /// 当前处理的文件
    pub current_file: Option<PathBuf>,
    /// 已处理文件数
    pub processed_count: usize,
    /// 总文件数
    pub total_count: usize,
    /// 剩余时间估计（秒）
    pub estimated_remaining: Option<f64>,
    /// 处理速度（文件/秒）
    pub processing_speed: Option<f32>,
    /// 状态消息
    pub status_message: Option<String>,
}

impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::Normal
    }
}

impl TaskInfo {
    /// 创建新任务
    pub fn new(
        name: String,
        task_type: TaskType,
        config: TaskConfig,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            task_type,
            status: TaskStatus::Pending,
            priority: TaskPriority::Normal,
            progress: 0.0,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
            config,
            result: None,
        }
    }

    /// 开始任务
    pub fn start(&mut self) {
        self.status = TaskStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// 完成任务
    pub fn complete(&mut self, result: TaskResult) {
        self.status = TaskStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.progress = 100.0;
        self.result = Some(result);
    }

    /// 任务失败
    pub fn fail(&mut self, error: String) {
        self.status = TaskStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error_message = Some(error);
    }

    /// 取消任务
    pub fn cancel(&mut self) {
        self.status = TaskStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }

    /// 更新进度
    pub fn update_progress(&mut self, progress: f32) {
        self.progress = progress.clamp(0.0, 100.0);
    }

    /// 获取运行时间
    pub fn get_duration(&self) -> Option<chrono::Duration> {
        if let Some(started) = self.started_at {
            let end_time = self.completed_at.unwrap_or_else(Utc::now);
            Some(end_time - started)
        } else {
            None
        }
    }

    /// 检查任务是否完成
    pub fn is_finished(&self) -> bool {
        matches!(
            self.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }

    /// 检查任务是否可以取消
    pub fn can_cancel(&self) -> bool {
        matches!(self.status, TaskStatus::Pending | TaskStatus::Running | TaskStatus::Paused)
    }
}