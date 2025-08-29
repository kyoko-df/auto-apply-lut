# API设计文档

## 概述

本文档定义了Video LUT Processor应用中前端(React)与后端(Rust/Tauri)之间的通信接口。所有API都通过Tauri的IPC机制实现，使用JSON格式进行数据交换。

## 数据类型定义

### 基础类型

```typescript
// 通用响应类型
interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
  code?: number;
}

// 进度信息
interface ProgressInfo {
  current: number;     // 当前进度 (0-100)
  total: number;       // 总数
  percentage: number;  // 百分比 (0-100)
  eta?: number;        // 预计剩余时间(秒)
  speed?: number;      // 处理速度
}

// 任务状态枚举
enum TaskStatus {
  Pending = 'pending',
  Processing = 'processing', 
  Completed = 'completed',
  Failed = 'failed',
  Cancelled = 'cancelled'
}
```

### 视频相关类型

```typescript
// 视频文件信息
interface VideoInfo {
  id: string;
  path: string;
  filename: string;
  size: number;           // 文件大小(字节)
  duration: number;       // 时长(秒)
  width: number;          // 宽度
  height: number;         // 高度
  fps: number;           // 帧率
  codec: string;         // 编码格式
  bitrate: number;       // 比特率
  format: string;        // 容器格式
  thumbnail?: string;    // 缩略图base64
  created_at: string;    // 创建时间
  status: FileStatus;    // 文件状态
}

// 文件状态枚举
enum FileStatus {
  Ready = 'ready',
  Processing = 'processing',
  Completed = 'completed',
  Failed = 'failed',
  Skipped = 'skipped'
}

// 支持的视频格式
interface SupportedFormat {
  extension: string;
  name: string;
  description: string;
  supported_codecs: string[];
}

// 视频处理设置
interface VideoProcessingSettings {
  output_format: string;     // 输出格式
  quality: number;           // 质量 (0-100)
  resolution?: {
    width: number;
    height: number;
  };
  fps?: number;              // 目标帧率
  bitrate?: number;          // 目标比特率
  codec?: string;            // 目标编码
  custom_params?: string;    // 自定义FFmpeg参数
  gpu_acceleration: GpuAccelerationSettings;
}

// GPU加速设置
interface GpuAccelerationSettings {
  enabled: boolean;
  device_id?: string;        // GPU设备ID
  encoder?: string;          // 编码器 (nvenc_h264, vaapi, etc.)
  preset?: string;           // 编码预设
  max_gpu_memory?: number;   // 最大GPU内存使用(MB)
}
```

### LUT相关类型

```typescript
// LUT文件信息
interface LutInfo {
  id: string;
  path: string;
  filename: string;
  format: LutFormat;         // LUT格式
  size: number;              // 文件大小
  dimensions: number;        // LUT维度 (通常是33)
  description?: string;      // 描述
  preview?: string;          // 预览图base64
  intensity: number;         // 强度 (0-100)
  created_at: string;
}

// LUT格式枚举
enum LutFormat {
  Cube = 'cube',
  ThreeDL = '3dl',
  Lut = 'lut',
  Mga = 'mga'
}

// LUT应用设置
interface LutSettings {
  intensity: number;         // 应用强度 (0-100)
  interpolation: string;     // 插值方法
  color_space?: string;      // 色彩空间
}
```

### 任务相关类型

```typescript
// 处理任务
interface ProcessingTask {
  id: string;
  video_id: string;
  lut_id: string;
  output_path: string;
  status: TaskStatus;
  progress: ProgressInfo;
  settings: VideoProcessingSettings & LutSettings;
  created_at: string;
  started_at?: string;
  completed_at?: string;
  error_message?: string;
}

// 批量处理请求
interface BatchProcessingRequest {
  video_ids: string[];
  lut_id: string;
  output_directory: string;
  settings: VideoProcessingSettings & LutSettings;
  max_concurrent: number;    // 最大并发数
  priority: TaskPriority;    // 任务优先级
  auto_retry: boolean;       // 自动重试失败的任务
  max_retries: number;       // 最大重试次数
}

// 任务优先级
enum TaskPriority {
  Low = 'low',
  Normal = 'normal',
  High = 'high'
}

// 批量处理响应
interface BatchProcessingResponse {
  batch_id: string;
  task_ids: string[];
  total_tasks: number;
}
```

## API接口定义

### 1. 文件管理API

#### 1.1 选择单个视频文件
```rust
#[tauri::command]
async fn select_single_video_file() -> Result<Option<String>, String>
```

**功能**: 打开文件选择对话框，选择单个视频文件

**参数**: 无

**返回**: 
```typescript
ApiResponse<string | null> // 选中的文件路径，null表示用户取消
```

**前端调用**:
```typescript
import { invoke } from '@tauri-apps/api/tauri';

const selectSingleFile = async (): Promise<string | null> => {
  return await invoke('select_single_video_file');
};
```

#### 1.2 选择多个视频文件
```rust
#[tauri::command]
async fn select_multiple_video_files() -> Result<Vec<String>, String>
```

**功能**: 打开文件选择对话框，选择多个视频文件

**参数**: 无

**返回**: 
```typescript
ApiResponse<string[]> // 选中的文件路径列表
```

**前端调用**:
```typescript
const selectMultipleFiles = async (): Promise<string[]> => {
  return await invoke('select_multiple_video_files');
};
```

#### 1.3 选择视频文件夹
```rust
#[tauri::command]
async fn select_video_folder() -> Result<Option<String>, String>
```

**功能**: 打开文件夹选择对话框，选择包含视频文件的文件夹

**参数**: 无

**返回**: 
```typescript
ApiResponse<string | null> // 选中的文件夹路径
```

#### 1.4 扫描文件夹中的视频文件
```rust
#[tauri::command]
async fn scan_folder_for_videos(
    folder_path: String,
    recursive: bool
) -> Result<Vec<String>, String>
```

**功能**: 扫描指定文件夹中的视频文件

**参数**:
- `folder_path`: 文件夹路径
- `recursive`: 是否递归扫描子文件夹

**返回**:
```typescript
ApiResponse<string[]> // 找到的视频文件路径列表
```

#### 1.5 获取视频文件信息
```rust
#[tauri::command]
async fn get_video_info(path: String) -> Result<VideoInfo, String>
```

**功能**: 获取指定视频文件的详细信息

**参数**:
- `path`: 视频文件路径

**返回**:
```typescript
ApiResponse<VideoInfo>
```

**前端调用**:
```typescript
const getVideoInfo = async (path: string): Promise<VideoInfo> => {
  return await invoke('get_video_info', { path });
};
```

#### 1.6 批量获取视频信息
```rust
#[tauri::command]
async fn get_videos_info(paths: Vec<String>) -> Result<Vec<VideoInfo>, String>
```

**功能**: 批量获取多个视频文件信息

**参数**:
- `paths`: 视频文件路径列表

**返回**:
```typescript
ApiResponse<VideoInfo[]>
```

#### 1.7 验证视频文件
```rust
#[tauri::command]
async fn validate_video_file(path: String) -> Result<bool, String>
```

**功能**: 验证文件是否为有效的视频文件

**参数**:
- `path`: 文件路径

**返回**:
```typescript
ApiResponse<boolean>
```

#### 1.8 获取支持的视频格式
```rust
#[tauri::command]
async fn get_supported_video_formats() -> Result<Vec<SupportedFormat>, String>
```

**功能**: 获取支持的视频格式列表

**返回**:
```typescript
ApiResponse<SupportedFormat[]>
```

### 2. LUT管理API

#### 2.1 选择LUT文件
```rust
#[tauri::command]
async fn select_lut_file() -> Result<Option<String>, String>
```

**功能**: 打开文件选择对话框，选择LUT文件

**返回**:
```typescript
ApiResponse<string | null> // LUT文件路径，null表示用户取消
```

**前端调用**:
```typescript
const selectLutFile = async (): Promise<string | null> => {
  return await invoke('select_lut_file');
};
```

#### 2.2 加载LUT文件
```rust
#[tauri::command]
async fn load_lut_file(path: String) -> Result<LutInfo, String>
```

**功能**: 加载并解析LUT文件

**参数**:
- `path`: LUT文件路径

**返回**:
```typescript
ApiResponse<LutInfo>
```

#### 2.3 验证LUT文件
```rust
#[tauri::command]
async fn validate_lut_file(path: String) -> Result<bool, String>
```

**功能**: 验证LUT文件格式是否正确

**参数**:
- `path`: LUT文件路径

**返回**:
```typescript
ApiResponse<boolean>
```

#### 2.4 获取LUT库
```rust
#[tauri::command]
async fn get_lut_library() -> Result<Vec<LutInfo>, String>
```

**功能**: 获取已加载的LUT库列表

**返回**:
```typescript
ApiResponse<LutInfo[]>
```

#### 2.6 添加LUT到收藏
```rust
#[tauri::command]
async fn add_lut_to_favorites(lut_id: String) -> Result<(), String>
```

**功能**: 将LUT添加到收藏夹

**参数**:
- `lut_id`: LUT ID

#### 2.7 从收藏中移除LUT
```rust
#[tauri::command]
async fn remove_lut_from_favorites(lut_id: String) -> Result<(), String>
```

**功能**: 从收藏夹中移除LUT

**参数**:
- `lut_id`: LUT ID

#### 2.8 获取收藏的LUT列表
```rust
#[tauri::command]
async fn get_favorite_luts() -> Result<Vec<LutInfo>, String>
```

**功能**: 获取收藏的LUT列表

**返回**:
```typescript
ApiResponse<LutInfo[]>
```

#### 2.9 搜索LUT
```rust
#[tauri::command]
async fn search_luts(query: String) -> Result<Vec<LutInfo>, String>
```

**功能**: 根据关键词搜索LUT

**参数**:
- `query`: 搜索关键词

**返回**:
```typescript
ApiResponse<LutInfo[]>
```

#### 2.5 生成LUT预览
```rust
#[tauri::command]
async fn generate_lut_preview(
    lut_id: String,
    sample_image: String
) -> Result<String, String>
```

**功能**: 为LUT生成预览图

**参数**:
- `lut_id`: LUT ID
- `sample_image`: 示例图片路径

**返回**:
```typescript
ApiResponse<string> // 预览图base64
```

### 3. 处理引擎API

#### 3.1 开始批量处理
```rust
#[tauri::command]
async fn start_batch_processing(
    request: BatchProcessingRequest
) -> Result<BatchProcessingResponse, String>
```

**功能**: 开始批量视频处理

**参数**:
- `request`: 批量处理请求

**返回**:
```typescript
ApiResponse<BatchProcessingResponse>
```

**前端调用**:
```typescript
const startBatchProcessing = async (
  request: BatchProcessingRequest
): Promise<BatchProcessingResponse> => {
  return await invoke('start_batch_processing', { request });
};
```

#### 3.2 暂停处理
```rust
#[tauri::command]
async fn pause_processing(batch_id: String) -> Result<(), String>
```

**功能**: 暂停批量处理

**参数**:
- `batch_id`: 批次ID

**前端调用**:
```typescript
const pauseProcessing = async (batchId: string): Promise<void> => {
  return await invoke('pause_processing', { batch_id: batchId });
};
```

#### 3.3 恢复处理
```rust
#[tauri::command]
async fn resume_processing(batch_id: String) -> Result<(), String>
```

**功能**: 恢复批量处理

**参数**:
- `batch_id`: 批次ID

**前端调用**:
```typescript
const resumeProcessing = async (batchId: string): Promise<void> => {
  return await invoke('resume_processing', { batch_id: batchId });
};
```

#### 3.4 取消处理
```rust
#[tauri::command]
async fn cancel_processing(batch_id: String) -> Result<(), String>
```

**功能**: 取消批量处理

**参数**:
- `batch_id`: 批次ID

**前端调用**:
```typescript
const cancelProcessing = async (batchId: string): Promise<void> => {
  return await invoke('cancel_processing', { batch_id: batchId });
};
```

#### 3.5 暂停单个任务
```rust
#[tauri::command]
async fn pause_task(task_id: String) -> Result<(), String>
```

**功能**: 暂停单个处理任务

**参数**:
- `task_id`: 任务ID

#### 3.6 恢复单个任务
```rust
#[tauri::command]
async fn resume_task(task_id: String) -> Result<(), String>
```

**功能**: 恢复单个处理任务

**参数**:
- `task_id`: 任务ID

#### 3.7 取消单个任务
```rust
#[tauri::command]
async fn cancel_task(task_id: String) -> Result<(), String>
```

**功能**: 取消单个处理任务

**参数**:
- `task_id`: 任务ID

#### 3.8 重试失败任务
```rust
#[tauri::command]
async fn retry_failed_task(task_id: String) -> Result<(), String>
```

**功能**: 重试失败的任务

**参数**:
- `task_id`: 任务ID

#### 3.9 获取处理状态
```rust
#[tauri::command]
async fn get_processing_status(batch_id: String) -> Result<Vec<ProcessingTask>, String>
```

**功能**: 获取批量处理的当前状态

**参数**:
- `batch_id`: 批次ID

**返回**:
```typescript
ApiResponse<ProcessingTask[]>
```

#### 3.10 获取任务详情
```rust
#[tauri::command]
async fn get_task_details(task_id: String) -> Result<ProcessingTask, String>
```

**功能**: 获取单个任务的详细信息

**参数**:
- `task_id`: 任务ID

**返回**:
```typescript
ApiResponse<ProcessingTask>
```

#### 3.11 获取系统资源监控
```rust
#[tauri::command]
async fn get_system_resources() -> Result<SystemResources, String>
```

**功能**: 获取当前系统资源使用情况

**返回**:
```typescript
interface SystemResources {
  cpu_usage: number;         // CPU使用率 (0-100)
  memory_usage: number;      // 内存使用率 (0-100)
  memory_used: number;       // 已使用内存(MB)
  memory_total: number;      // 总内存(MB)
  disk_usage: number;        // 磁盘使用率 (0-100)
  gpu_usage?: number;        // GPU使用率 (0-100)
  gpu_memory_usage?: number; // GPU内存使用率 (0-100)
  active_tasks: number;      // 活跃任务数
  queue_length: number;      // 队列长度
}

ApiResponse<SystemResources>
```

#### 3.12 获取处理统计
```rust
#[tauri::command]
async fn get_processing_statistics(
    batch_id: String
) -> Result<ProcessingStatistics, String>
```

**功能**: 获取批量处理的统计信息

**参数**:
- `batch_id`: 批次ID

**返回**:
```typescript
interface ProcessingStatistics {
  total_tasks: number;
  completed_tasks: number;
  failed_tasks: number;
  pending_tasks: number;
  processing_tasks: number;
  total_size_processed: number;  // 已处理文件总大小(字节)
  total_time_elapsed: number;    // 总耗时(秒)
  average_speed: number;         // 平均处理速度(MB/s)
  estimated_time_remaining: number; // 预计剩余时间(秒)
  success_rate: number;          // 成功率 (0-100)
}

ApiResponse<ProcessingStatistics>
```

#### 3.13 清理完成的任务
```rust
#[tauri::command]
async fn cleanup_completed_tasks(batch_id: String) -> Result<u32, String>
```

**功能**: 清理已完成的任务记录

**参数**:
- `batch_id`: 批次ID

**返回**:
```typescript
ApiResponse<number> // 清理的任务数量
```

### 4. 系统配置API

#### 4.1 获取系统信息
```rust
#[tauri::command]
async fn get_system_info() -> Result<SystemInfo, String>
```

**功能**: 获取系统信息

**返回**:
```typescript
interface SystemInfo {
  os: string;
  arch: string;
  cpu_cores: number;
  cpu_threads: number;
  memory_total: number;
  memory_available: number;
  disk_space_available: number;
  ffmpeg_version?: string;
  ffmpeg_path?: string;
  gpu_devices: GpuDevice[];
}

interface GpuDevice {
  id: string;
  name: string;
  vendor: GpuVendor;
  memory_total: number;
  memory_available: number;
  compute_capability?: string;
  driver_version?: string;
  supported_encoders: string[];
}

enum GpuVendor {
  Nvidia = 'nvidia',
  Amd = 'amd',
  Intel = 'intel',
  Apple = 'apple',
  Unknown = 'unknown'
}

ApiResponse<SystemInfo>
```

#### 4.2 获取推荐处理设置
```rust
#[tauri::command]
async fn get_recommended_settings(
    video_count: u32,
    total_size: u64
) -> Result<RecommendedSettings, String>
```

**功能**: 根据系统配置和视频数量获取推荐的处理设置

**参数**:
- `video_count`: 视频文件数量
- `total_size`: 视频文件总大小(字节)

**返回**:
```typescript
interface RecommendedSettings {
  max_concurrent_tasks: number;
  recommended_concurrent_tasks: number;
  memory_limit_per_task: number;
  use_gpu_acceleration: boolean;
  recommended_gpu_device?: string;
  estimated_processing_time: number; // 秒
  disk_space_required: number; // 字节
  warnings: string[];
}

ApiResponse<RecommendedSettings>
```

#### 4.3 检测GPU加速能力
```rust
#[tauri::command]
async fn detect_gpu_capabilities() -> Result<GpuCapabilities, String>
```

**功能**: 检测GPU加速能力

**返回**:
```typescript
interface GpuCapabilities {
  available: boolean;
  devices: GpuDevice[];
  supported_formats: string[];
  recommended_device?: string;
}

ApiResponse<GpuCapabilities>
```

#### 4.2 检查FFmpeg
```rust
#[tauri::command]
async fn check_ffmpeg() -> Result<FFmpegInfo, String>
```

**功能**: 检查FFmpeg是否可用

**返回**:
```typescript
interface FFmpegInfo {
  available: boolean;
  version?: string;
  path?: string;
  supported_formats: string[];
}

ApiResponse<FFmpegInfo>
```

#### 4.3 设置输出目录
```rust
#[tauri::command]
async fn select_output_directory() -> Result<String, String>
```

**功能**: 选择输出目录

**返回**:
```typescript
ApiResponse<string> // 选中的目录路径
```

#### 4.4 保存用户设置
```rust
#[tauri::command]
async fn save_user_settings(settings: UserSettings) -> Result<(), String>
```

**功能**: 保存用户设置

**参数**:
```typescript
interface UserSettings {
  default_output_directory?: string;
  max_concurrent_tasks: number;
  default_video_settings: VideoProcessingSettings;
  default_lut_settings: LutSettings;
  gpu_settings: GpuSettings;
  ui_theme: 'light' | 'dark';
  language: string;
  auto_save_interval: number;        // 自动保存间隔(秒)
  enable_notifications: boolean;     // 启用通知
  enable_auto_retry: boolean;        // 启用自动重试
  max_retry_attempts: number;        // 最大重试次数
  temp_directory?: string;           // 临时文件目录
  log_level: LogLevel;               // 日志级别
}

interface GpuSettings {
  auto_detect: boolean;              // 自动检测GPU
  preferred_device?: string;         // 首选GPU设备ID
  enable_gpu_acceleration: boolean;  // 启用GPU加速
  max_gpu_memory_usage: number;      // 最大GPU内存使用率 (0-100)
  fallback_to_cpu: boolean;          // GPU失败时回退到CPU
}

enum LogLevel {
  Error = 'error',
  Warn = 'warn',
  Info = 'info',
  Debug = 'debug'
}
```

#### 4.5 加载用户设置
```rust
#[tauri::command]
async fn load_user_settings() -> Result<UserSettings, String>
```

**功能**: 加载用户设置

**返回**:
```typescript
ApiResponse<UserSettings>
```

## 事件系统

### 进度事件

```typescript
// 监听处理进度
import { listen } from '@tauri-apps/api/event';

interface ProgressEvent {
  batch_id: string;
  task_id: string;
  progress: ProgressInfo;
  status: TaskStatus;
}

// 监听进度更新
const unlistenProgress = await listen<ProgressEvent>('processing-progress', (event) => {
  console.log('Progress update:', event.payload);
});

// 监听任务完成
const unlistenComplete = await listen<ProcessingTask>('task-completed', (event) => {
  console.log('Task completed:', event.payload);
});

// 监听任务失败
const unlistenError = await listen<{task_id: string, error: string}>('task-failed', (event) => {
  console.log('Task failed:', event.payload);
});
```

### 系统事件

```typescript
// 监听系统状态变化
const unlistenSystem = await listen<{type: string, message: string}>('system-status', (event) => {
  console.log('System status:', event.payload);
});

// 监听错误事件
const unlistenError = await listen<{error: string, details?: any}>('app-error', (event) => {
  console.error('Application error:', event.payload);
});

// 监听系统资源变化
const unlistenResources = await listen<SystemResources>('system-resources', (event) => {
  console.log('System resources:', event.payload);
});

// 监听GPU状态变化
const unlistenGpu = await listen<{device_id: string, status: string, message?: string}>('gpu-status', (event) => {
  console.log('GPU status:', event.payload);
});

// 监听批次状态变化
const unlistenBatch = await listen<{batch_id: string, status: string, statistics: ProcessingStatistics}>('batch-status', (event) => {
  console.log('Batch status:', event.payload);
});

// 监听文件状态变化
const unlistenFile = await listen<{file_id: string, status: FileStatus, message?: string}>('file-status', (event) => {
  console.log('File status:', event.payload);
});
```

## 错误处理

### 错误代码定义

```typescript
enum ErrorCode {
  // 文件相关错误 (1000-1999)
  FILE_NOT_FOUND = 1001,
  FILE_ACCESS_DENIED = 1002,
  INVALID_VIDEO_FORMAT = 1003,
  VIDEO_CORRUPTED = 1004,
  
  // LUT相关错误 (2000-2999)
  INVALID_LUT_FORMAT = 2001,
  LUT_PARSE_ERROR = 2002,
  LUT_NOT_FOUND = 2003,
  
  // 处理相关错误 (3000-3999)
  FFMPEG_NOT_FOUND = 3001,
  FFMPEG_EXECUTION_FAILED = 3002,
  PROCESSING_CANCELLED = 3003,
  INSUFFICIENT_DISK_SPACE = 3004,
  
  // 系统相关错误 (4000-4999)
  INSUFFICIENT_MEMORY = 4001,
  PERMISSION_DENIED = 4002,
  SYSTEM_ERROR = 4003,
  
  // 未知错误 (9000-9999)
  UNKNOWN_ERROR = 9001
}
```

### 错误响应格式

```typescript
interface ErrorResponse {
  success: false;
  error: string;        // 错误消息
  code: ErrorCode;      // 错误代码
  details?: any;        // 错误详情
  timestamp: string;    // 错误时间
}
```

## 使用示例

### 完整的处理流程

```typescript
import { invoke } from '@tauri-apps/api/tauri';
import { listen } from '@tauri-apps/api/event';

class VideoProcessor {
  private currentBatchId?: string;
  private unlisteners: (() => void)[] = [];

  async processVideos() {
    try {
      // 1. 获取系统信息和推荐设置
      const systemInfo = await invoke<SystemInfo>('get_system_info');
      console.log('System info:', systemInfo);
      
      // 2. 选择视频文件（支持单个或多个）
      const videoPaths = await invoke<string[]>('select_multiple_video_files');
      if (videoPaths.length === 0) {
        console.log('No files selected');
        return;
      }
      
      // 3. 获取视频信息
      const videoInfos = await invoke<VideoInfo[]>('get_videos_info', {
        paths: videoPaths
      });
      
      // 4. 选择LUT文件
      const lutPath = await invoke<string | null>('select_lut_file');
      if (!lutPath) {
        console.log('No LUT selected');
        return;
      }
      
      // 5. 加载LUT信息
      const lutInfo = await invoke<LutInfo>('load_lut_file', {
        path: lutPath
      });
      
      // 6. 选择输出目录
      const outputDir = await invoke<string>('select_output_directory');
      
      // 7. 获取推荐处理设置
      const totalSize = videoInfos.reduce((sum, video) => sum + video.size, 0);
      const recommendedSettings = await invoke<RecommendedSettings>('get_recommended_settings', {
        video_count: videoInfos.length,
        total_size: totalSize
      });
      
      console.log('Recommended settings:', recommendedSettings);
      
      // 8. 检测GPU能力
      const gpuCapabilities = await invoke<GpuCapabilities>('detect_gpu_capabilities');
      console.log('GPU capabilities:', gpuCapabilities);
      
      // 9. 设置事件监听器
      this.setupEventListeners();
      
      // 10. 开始批量处理
      const request: BatchProcessingRequest = {
        video_ids: videoInfos.map(v => v.id),
        lut_id: lutInfo.id,
        output_directory: outputDir,
        settings: {
          output_format: 'mp4',
          quality: 85,
          intensity: 80,
          interpolation: 'linear',
          gpu_acceleration: {
            enabled: gpuCapabilities.available,
            device_id: gpuCapabilities.recommended_device,
            encoder: 'auto',
            preset: 'medium'
          }
        },
        max_concurrent: recommendedSettings.recommended_concurrent_tasks,
        priority: 'normal',
        auto_retry: true,
        max_retries: 3
      };
      
      const response = await invoke<BatchProcessingResponse>(
        'start_batch_processing', 
        { request }
      );
      
      this.currentBatchId = response.batch_id;
      console.log('Processing started:', response);
      
      // 11. 监控处理进度
      this.monitorProgress(response.batch_id);
      
    } catch (error) {
      console.error('Processing failed:', error);
    }
  }
  
  private setupEventListeners() {
    // 监听处理进度
    const unlistenProgress = listen<ProgressEvent>('processing-progress', 
      this.handleProgress.bind(this)
    );
    
    // 监听任务完成
    const unlistenComplete = listen<ProcessingTask>('task-completed', 
      this.handleTaskComplete.bind(this)
    );
    
    // 监听任务失败
    const unlistenError = listen<{task_id: string, error: string}>('task-failed', 
      this.handleTaskError.bind(this)
    );
    
    // 监听系统资源
    const unlistenResources = listen<SystemResources>('system-resources', 
      this.handleResourceUpdate.bind(this)
    );
    
    // 监听GPU状态
    const unlistenGpu = listen<{device_id: string, status: string}>('gpu-status', 
      this.handleGpuStatus.bind(this)
    );
    
    // 保存取消监听函数
    Promise.all([
      unlistenProgress, unlistenComplete, unlistenError, 
      unlistenResources, unlistenGpu
    ]).then(unlisteners => {
      this.unlisteners = unlisteners;
    });
  }
  
  private async monitorProgress(batchId: string) {
    const interval = setInterval(async () => {
      try {
        const statistics = await invoke<ProcessingStatistics>('get_processing_statistics', {
          batch_id: batchId
        });
        
        console.log('Processing statistics:', statistics);
        
        // 如果所有任务完成，停止监控
        if (statistics.completed_tasks + statistics.failed_tasks === statistics.total_tasks) {
          clearInterval(interval);
          console.log('All tasks completed');
        }
      } catch (error) {
        console.error('Failed to get statistics:', error);
      }
    }, 5000); // 每5秒更新一次
  }
  
  // 暂停处理
  async pauseProcessing() {
    if (this.currentBatchId) {
      await invoke('pause_processing', { batch_id: this.currentBatchId });
      console.log('Processing paused');
    }
  }
  
  // 恢复处理
  async resumeProcessing() {
    if (this.currentBatchId) {
      await invoke('resume_processing', { batch_id: this.currentBatchId });
      console.log('Processing resumed');
    }
  }
  
  // 取消处理
  async cancelProcessing() {
    if (this.currentBatchId) {
      await invoke('cancel_processing', { batch_id: this.currentBatchId });
      console.log('Processing cancelled');
    }
  }
  
  private handleProgress(event: Event<ProgressEvent>) {
    const { batch_id, task_id, progress, status } = event.payload;
    console.log(`Task ${task_id}: ${progress.percentage}% (${status})`);
  }
  
  private handleTaskComplete(event: Event<ProcessingTask>) {
    const task = event.payload;
    console.log(`Task completed: ${task.id}`);
  }
  
  private handleTaskError(event: Event<{task_id: string, error: string}>) {
    const { task_id, error } = event.payload;
    console.error(`Task failed: ${task_id}, Error: ${error}`);
  }
  
  private handleResourceUpdate(event: Event<SystemResources>) {
    const resources = event.payload;
    console.log(`CPU: ${resources.cpu_usage}%, Memory: ${resources.memory_usage}%, GPU: ${resources.gpu_usage}%`);
  }
  
  private handleGpuStatus(event: Event<{device_id: string, status: string}>) {
    const { device_id, status } = event.payload;
    console.log(`GPU ${device_id}: ${status}`);
  }
  
  // 清理资源
  cleanup() {
    this.unlisteners.forEach(unlisten => unlisten());
    this.unlisteners = [];
  }
}
```

这个API设计确保了前后端之间清晰、类型安全的通信，支持完整的视频LUT处理工作流程，并提供了良好的错误处理和进度监控机制。