use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use super::task::{TaskInfo, TaskStatus, TaskResult};
use super::video::VideoProcessOptions;
use super::lut::LutApplyOptions;

/// 批处理任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTask {
    /// 批处理ID
    pub id: Uuid,
    /// 批处理名称
    pub name: String,
    /// 描述
    pub description: Option<String>,
    /// 批处理状态
    pub status: BatchStatus,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 开始时间
    pub started_at: Option<DateTime<Utc>>,
    /// 完成时间
    pub completed_at: Option<DateTime<Utc>>,
    /// 批处理配置
    pub config: BatchConfig,
    /// 子任务列表
    pub tasks: Vec<TaskInfo>,
    /// 总进度（0-100）
    pub progress: f32,
    /// 批处理结果
    pub result: Option<BatchResult>,
    /// 错误信息
    pub error_message: Option<String>,
}

/// 批处理状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BatchStatus {
    /// 准备中
    Preparing,
    /// 等待中
    Pending,
    /// 运行中
    Running,
    /// 暂停
    Paused,
    /// 已完成
    Completed,
    /// 失败
    Failed,
    /// 已取消
    Cancelled,
}

/// 批处理配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    /// 输入目录或文件列表
    pub input_sources: Vec<InputSource>,
    /// 输出目录
    pub output_dir: PathBuf,
    /// LUT文件路径
    pub lut_path: Option<PathBuf>,
    /// 视频处理选项
    pub video_options: Option<VideoProcessOptions>,
    /// LUT应用选项
    pub lut_options: Option<LutApplyOptions>,
    /// 并发设置
    pub concurrency_config: ConcurrencyConfig,
    /// 文件过滤器
    pub file_filter: FileFilter,
    /// 错误处理策略
    pub error_handling: ErrorHandlingStrategy,
    /// 是否覆盖现有文件
    pub overwrite: bool,
    /// 是否创建备份
    pub create_backup: bool,
    /// 输出文件命名规则
    pub naming_rule: NamingRule,
}

/// 输入源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputSource {
    /// 单个文件
    File(PathBuf),
    /// 目录（递归）
    Directory {
        path: PathBuf,
        recursive: bool,
        max_depth: Option<usize>,
    },
    /// 文件模式匹配
    Pattern {
        pattern: String,
        base_dir: PathBuf,
    },
}

/// 并发配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyConfig {
    /// 最大并发任务数
    pub max_concurrent_tasks: usize,
    /// 是否启用GPU加速
    pub use_gpu: bool,
    /// GPU并发限制
    pub gpu_concurrent_limit: Option<usize>,
    /// CPU核心使用限制
    pub cpu_core_limit: Option<usize>,
    /// 内存使用限制（MB）
    pub memory_limit: Option<u64>,
}

/// 文件过滤器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFilter {
    /// 包含的文件扩展名
    pub include_extensions: Vec<String>,
    /// 排除的文件扩展名
    pub exclude_extensions: Vec<String>,
    /// 最小文件大小（字节）
    pub min_file_size: Option<u64>,
    /// 最大文件大小（字节）
    pub max_file_size: Option<u64>,
    /// 文件名模式（正则表达式）
    pub name_pattern: Option<String>,
    /// 排除的文件名模式
    pub exclude_pattern: Option<String>,
    /// 修改时间过滤
    pub modified_after: Option<DateTime<Utc>>,
    pub modified_before: Option<DateTime<Utc>>,
}

/// 错误处理策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ErrorHandlingStrategy {
    /// 遇到错误时停止
    StopOnError,
    /// 跳过错误继续处理
    SkipOnError,
    /// 重试指定次数
    RetryOnError { max_retries: usize },
    /// 询问用户
    AskUser,
}

/// 文件命名规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamingRule {
    /// 文件名模板
    pub template: String,
    /// 是否保留原始文件名
    pub keep_original_name: bool,
    /// 前缀
    pub prefix: Option<String>,
    /// 后缀
    pub suffix: Option<String>,
    /// 是否添加时间戳
    pub add_timestamp: bool,
    /// 时间戳格式
    pub timestamp_format: Option<String>,
    /// 计数器设置
    pub counter_config: Option<CounterConfig>,
}

/// 计数器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterConfig {
    /// 起始数字
    pub start: usize,
    /// 步长
    pub step: usize,
    /// 填充位数
    pub padding: usize,
}

/// 批处理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    /// 总任务数
    pub total_tasks: usize,
    /// 成功任务数
    pub successful_tasks: usize,
    /// 失败任务数
    pub failed_tasks: usize,
    /// 跳过任务数
    pub skipped_tasks: usize,
    /// 总处理时间（秒）
    pub total_duration: f64,
    /// 平均任务处理时间（秒）
    pub average_task_duration: f64,
    /// 总处理的文件大小（字节）
    pub total_file_size: u64,
    /// 处理速度（MB/s）
    pub processing_speed: f64,
    /// 输出文件列表
    pub output_files: Vec<PathBuf>,
    /// 失败的任务
    pub failed_tasks_details: Vec<FailedTaskDetail>,
    /// 性能统计
    pub performance_summary: PerformanceSummary,
}

/// 失败任务详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedTaskDetail {
    /// 任务ID
    pub task_id: Uuid,
    /// 输入文件
    pub input_file: PathBuf,
    /// 错误信息
    pub error: String,
    /// 重试次数
    pub retry_count: usize,
    /// 失败时间
    pub failed_at: DateTime<Utc>,
}

/// 性能摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSummary {
    /// 平均CPU使用率
    pub avg_cpu_usage: f32,
    /// 峰值内存使用（MB）
    pub peak_memory_usage: u64,
    /// 平均GPU使用率
    pub avg_gpu_usage: Option<f32>,
    /// 峰值GPU内存使用（MB）
    pub peak_gpu_memory: Option<u64>,
    /// 磁盘IO统计
    pub disk_io_total: super::task::DiskIoStats,
    /// 网络IO统计（如果有）
    pub network_io_total: Option<NetworkIoStats>,
}

/// 网络IO统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkIoStats {
    /// 下载字节数
    pub bytes_downloaded: u64,
    /// 上传字节数
    pub bytes_uploaded: u64,
    /// 下载操作数
    pub download_operations: u64,
    /// 上传操作数
    pub upload_operations: u64,
}

/// 批处理进度信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProgress {
    /// 批处理ID
    pub batch_id: Uuid,
    /// 总进度（0-100）
    pub overall_progress: f32,
    /// 当前任务进度
    pub current_task_progress: Option<super::task::TaskProgress>,
    /// 已完成任务数
    pub completed_tasks: usize,
    /// 总任务数
    pub total_tasks: usize,
    /// 剩余时间估计（秒）
    pub estimated_remaining: Option<f64>,
    /// 处理速度（任务/秒）
    pub processing_speed: Option<f32>,
    /// 状态消息
    pub status_message: Option<String>,
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: num_cpus::get(),
            use_gpu: false,
            gpu_concurrent_limit: Some(1),
            cpu_core_limit: None,
            memory_limit: None,
        }
    }
}

impl Default for FileFilter {
    fn default() -> Self {
        Self {
            include_extensions: vec![
                "mp4".to_string(),
                "mov".to_string(),
                "avi".to_string(),
                "mkv".to_string(),
                "wmv".to_string(),
                "flv".to_string(),
                "webm".to_string(),
            ],
            exclude_extensions: vec![],
            min_file_size: None,
            max_file_size: None,
            name_pattern: None,
            exclude_pattern: None,
            modified_after: None,
            modified_before: None,
        }
    }
}

impl Default for NamingRule {
    fn default() -> Self {
        Self {
            template: "{original_name}_processed.{extension}".to_string(),
            keep_original_name: true,
            prefix: None,
            suffix: Some("_processed".to_string()),
            add_timestamp: false,
            timestamp_format: Some("%Y%m%d_%H%M%S".to_string()),
            counter_config: None,
        }
    }
}

impl BatchTask {
    /// 创建新的批处理任务
    pub fn new(name: String, config: BatchConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            status: BatchStatus::Preparing,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            config,
            tasks: vec![],
            progress: 0.0,
            result: None,
            error_message: None,
        }
    }

    /// 开始批处理
    pub fn start(&mut self) {
        self.status = BatchStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// 暂停批处理
    pub fn pause(&mut self) {
        if self.status == BatchStatus::Running {
            self.status = BatchStatus::Paused;
        }
    }

    /// 恢复批处理
    pub fn resume(&mut self) {
        if self.status == BatchStatus::Paused {
            self.status = BatchStatus::Running;
        }
    }

    /// 完成批处理
    pub fn complete(&mut self, result: BatchResult) {
        self.status = BatchStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.progress = 100.0;
        self.result = Some(result);
    }

    /// 批处理失败
    pub fn fail(&mut self, error: String) {
        self.status = BatchStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error_message = Some(error);
    }

    /// 取消批处理
    pub fn cancel(&mut self) {
        self.status = BatchStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }

    /// 更新进度
    pub fn update_progress(&mut self) {
        if self.tasks.is_empty() {
            self.progress = 0.0;
            return;
        }

        let total_progress: f32 = self.tasks.iter().map(|task| task.progress).sum();
        self.progress = total_progress / self.tasks.len() as f32;
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

    /// 检查是否完成
    pub fn is_finished(&self) -> bool {
        matches!(
            self.status,
            BatchStatus::Completed | BatchStatus::Failed | BatchStatus::Cancelled
        )
    }

    /// 获取活跃任务数
    pub fn get_active_task_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Running)
            .count()
    }

    /// 获取完成任务数
    pub fn get_completed_task_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Completed)
            .count()
    }

    /// 获取失败任务数
    pub fn get_failed_task_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Failed)
            .count()
    }
}