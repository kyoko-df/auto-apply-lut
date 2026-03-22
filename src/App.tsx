import { useEffect, useRef, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import FileUpload from './components/FileUpload';
import VideoPreview from './components/VideoPreview';
import SettingsModal from './components/SettingsModal';
import ProcessingStatus from './components/ProcessingStatus';
import './App.css';

interface ProcessingTask {
  id: string;
  name: string;
  progress: number;
  status: 'pending' | 'processing' | 'completed' | 'failed' | 'cancelled';
  stage: string;
  sourcePath: string;
  backendTaskId?: string;
  batchId?: string;
  outputPath?: string;
  eta?: number;
  speed?: number;
  error?: string;
}

interface ProcessingSettings {
  output_format: string;
  video_codec: string;
  audio_codec: string;
  quality_preset: string;
  resolution: string;
  fps: number | null;
  bitrate: string;
  lut_intensity: number;
  lut_error_strategy: 'StopOnError' | 'SkipOnError';
  color_space: string;
  hardware_acceleration: boolean;
  two_pass_encoding: boolean;
  preserve_metadata: boolean;
  output_directory: string;
}

interface StartProcessResponse {
  task_id: string;
  output_path?: string;
}

interface TaskProgressResponse {
  progress: number;
  status_message?: string;
  status?: string;
  error?: string;
  output_path?: string;
}

interface BatchItemRequest {
  input_path: string;
  output_path: string;
  lut_paths: string[];
  lut_path: string | null;
  intensity: number;
}

interface BatchStartResponse {
  batch_id: string;
  total_items: number;
  status: string;
  message: string;
}

interface BatchProgressResponse {
  batch_id: string;
  total_items: number;
  completed_items: number;
  failed_items: number;
  cancelled_items?: number;
  current_item?: string;
  overall_progress: number;
  status: string;
  errors: string[];
  items?: BatchItemProgressResponse[];
}

interface BatchItemProgressResponse {
  input_path: string;
  output_path: string;
  status: string;
  progress: number;
  error?: string;
}

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

const getFileName = (path: string): string => {
  const parts = path.split(/[\/\\]/);
  return parts[parts.length - 1] || 'unknown';
};

const buildExpectedOutputPath = (
  inputPath: string,
  outputDirectory: string,
  outputFormat: string
): string => {
  const slashIndex = Math.max(inputPath.lastIndexOf('/'), inputPath.lastIndexOf('\\'));
  const sep = inputPath.lastIndexOf('\\') > inputPath.lastIndexOf('/') ? '\\' : '/';
  const parent = slashIndex >= 0 ? inputPath.slice(0, slashIndex) : '';
  const fileName = slashIndex >= 0 ? inputPath.slice(slashIndex + 1) : inputPath;
  const dotIndex = fileName.lastIndexOf('.');
  const fileStem = dotIndex > 0 ? fileName.slice(0, dotIndex) : fileName;
  const inputExt = dotIndex > 0 ? fileName.slice(dotIndex + 1) : 'mp4';
  const cleanedOutputFormat = outputFormat.trim().replace(/^\./, '');
  const ext = cleanedOutputFormat || inputExt || 'mp4';
  const dir = outputDirectory.trim() || parent;

  if (!dir) {
    return `${fileStem}_lut_applied.${ext}`;
  }

  const needsSep = !dir.endsWith('/') && !dir.endsWith('\\');
  return `${dir}${needsSep ? sep : ''}${fileStem}_lut_applied.${ext}`;
};

function App() {
  const [videoFiles, setVideoFiles] = useState<string[]>([]);
  const [activeVideoFile, setActiveVideoFile] = useState<string | null>(null);
  const [lutFiles, setLutFiles] = useState<string[]>([]);
  const [processedVideoPath, setProcessedVideoPath] = useState<string | null>(null);
  const [processingTasks, setProcessingTasks] = useState<ProcessingTask[]>([]);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [settings, setSettings] = useState<ProcessingSettings>({
    output_format: 'mp4',
    video_codec: 'libx264',
    audio_codec: 'aac',
    quality_preset: 'balanced',
    resolution: 'original',
    fps: null,
    bitrate: 'auto',
    lut_intensity: 100,
    lut_error_strategy: 'StopOnError',
    color_space: 'rec709',
    hardware_acceleration: false,
    two_pass_encoding: false,
    preserve_metadata: true,
    output_directory: ''
  });

  const processingTasksRef = useRef<ProcessingTask[]>([]);
  const cancelledTaskIdsRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    processingTasksRef.current = processingTasks;
  }, [processingTasks]);

  useEffect(() => {
    try {
      const raw = window.localStorage.getItem('auto-apply-lut:lastLuts:v1');
      if (!raw) return;
      const parsed = JSON.parse(raw) as unknown;
      if (!Array.isArray(parsed)) return;
      const list = parsed.filter((x): x is string => typeof x === 'string' && x.trim().length > 0);
      if (list.length > 0) setLutFiles(list);
    } catch {
      return;
    }
  }, []);

  useEffect(() => {
    try {
      window.localStorage.setItem('auto-apply-lut:lastLuts:v1', JSON.stringify(lutFiles));
    } catch {
      return;
    }
  }, [lutFiles]);

  const handleVideoSelect = useCallback((filePaths: string[]) => {
    setProcessedVideoPath(null);
    setVideoFiles(filePaths);
    setActiveVideoFile(prev => {
      if (filePaths.length === 0) return null;
      if (prev && filePaths.includes(prev)) return prev;
      return filePaths[0];
    });
  }, []);

  const handleActiveVideoChange = useCallback((filePath: string | null) => {
    setActiveVideoFile(filePath);
    setProcessedVideoPath(null);
  }, []);

  const handleLutSelect = useCallback((filePaths: string[]) => {
    setLutFiles(filePaths);
  }, []);

  const runSingleTask = useCallback(async (task: ProcessingTask) => {
    const uiTaskId = task.id;
    const sourcePath = task.sourcePath;

    if (cancelledTaskIdsRef.current.has(uiTaskId)) {
      setProcessingTasks(prev =>
        prev.map(t =>
          t.id === uiTaskId
            ? { ...t, status: 'cancelled', stage: '已取消' }
            : t
        )
      );
      return;
    }

    setProcessingTasks(prev =>
      prev.map(t =>
        t.id === uiTaskId
          ? { ...t, status: 'processing', stage: '应用LUT...' }
          : t
      )
    );

    try {
      const result = await invoke<StartProcessResponse>('start_video_processing', {
        request: {
          input_path: sourcePath,
          output_path: '',
          output_directory: settings.output_directory || null,
          output_format: settings.output_format,
          lut_paths: lutFiles,
          intensity: settings.lut_intensity / 100.0,
          hardware_acceleration: settings.hardware_acceleration,
          video_codec: settings.video_codec,
          audio_codec: settings.audio_codec,
          quality_preset: settings.quality_preset,
          resolution: settings.resolution,
          fps: settings.fps,
          bitrate: settings.bitrate,
          color_space: settings.color_space,
          two_pass_encoding: settings.two_pass_encoding,
          preserve_metadata: settings.preserve_metadata,
          lut_error_strategy: settings.lut_error_strategy,
        }
      });

      const backendTaskId = result.task_id;
      const expectedOutputPath = result.output_path || task.outputPath || `${sourcePath}_processed.mp4`;

      setProcessingTasks(prev =>
        prev.map(t =>
          t.id === uiTaskId
            ? { ...t, backendTaskId, outputPath: expectedOutputPath }
            : t
        )
      );

      let consecutivePollErrors = 0;
      for (;;) {
        if (cancelledTaskIdsRef.current.has(uiTaskId)) {
          await invoke('cancel_task', { taskId: backendTaskId }).catch(() => undefined);
          setProcessingTasks(prev =>
            prev.map(t =>
              t.id === uiTaskId
                ? { ...t, status: 'cancelled', stage: '已取消' }
                : t
            )
          );
          break;
        }

        let progressInfo: TaskProgressResponse;
        try {
          progressInfo = await invoke<TaskProgressResponse>('get_task_progress', { taskId: backendTaskId });
          consecutivePollErrors = 0;
        } catch (error) {
          consecutivePollErrors += 1;
          if (consecutivePollErrors >= 3) {
            const message = error instanceof Error ? error.message : '获取进度失败';
            setProcessingTasks(prev =>
              prev.map(t =>
                t.id === uiTaskId
                  ? { ...t, status: 'failed', stage: '处理失败', error: message }
                  : t
              )
            );
            break;
          }
          await sleep(1000);
          continue;
        }

        const progress = Math.max(0, Math.min(100, Math.round(progressInfo.progress || 0)));
        const status = (progressInfo.status || '').toLowerCase();
        const outputPath = progressInfo.output_path || expectedOutputPath;

        setProcessingTasks(prev =>
          prev.map(t =>
            t.id === uiTaskId
              ? {
                  ...t,
                  progress,
                  stage: progressInfo.status_message || '处理中...',
                  outputPath,
                }
              : t
          )
        );

        if (status === 'failed') {
          setProcessingTasks(prev =>
            prev.map(t =>
              t.id === uiTaskId
                ? {
                    ...t,
                    status: 'failed',
                    stage: '处理失败',
                    error: progressInfo.error || '未知错误',
                  }
                : t
            )
          );
          break;
        }

        if (status === 'cancelled') {
          setProcessingTasks(prev =>
            prev.map(t =>
              t.id === uiTaskId
                ? { ...t, status: 'cancelled', stage: '已取消' }
                : t
            )
          );
          break;
        }

        if (status === 'completed' || progress >= 100) {
          try {
            await invoke('get_file_info', { path: outputPath });
            setProcessingTasks(prev =>
              prev.map(t =>
                t.id === uiTaskId
                  ? {
                      ...t,
                      status: 'completed',
                      progress: 100,
                      stage: '已完成',
                      outputPath,
                    }
                  : t
              )
            );
            setProcessedVideoPath(outputPath);
          } catch {
            setProcessingTasks(prev =>
              prev.map(t =>
                t.id === uiTaskId
                  ? { ...t, status: 'failed', stage: '处理失败：未找到输出文件' }
                  : t
              )
            );
          }
          break;
        }

        await sleep(1000);
      }
    } catch (error) {
      setProcessingTasks(prev =>
        prev.map(t =>
          t.id === uiTaskId
            ? {
                ...t,
                status: 'failed',
                stage: '处理失败',
                error: error instanceof Error ? error.message : '未知错误',
              }
            : t
        )
      );
    }
  }, [lutFiles, settings]);

  const runBatchTasks = useCallback(async (queuedTasks: ProcessingTask[]) => {
    const queuedIdSet = new Set(queuedTasks.map(task => task.id));

    try {
      const items: BatchItemRequest[] = queuedTasks.map(task => ({
        input_path: task.sourcePath,
        output_path: task.outputPath || '',
        lut_paths: lutFiles,
        lut_path: lutFiles[0] || null,
        intensity: settings.lut_intensity / 100.0,
      }));

      const startRes = await invoke<BatchStartResponse>('start_batch_processing', {
        request: {
          items,
          hardware_acceleration: settings.hardware_acceleration,
          output_directory: settings.output_directory || '',
          preserve_structure: false,
        }
      });

      const batchId = startRes.batch_id;
      setProcessingTasks(prev =>
        prev.map(task =>
          queuedIdSet.has(task.id)
            ? { ...task, batchId, stage: '等待批处理调度...', status: 'pending' }
            : task
        )
      );

      let consecutivePollErrors = 0;

      for (;;) {
        let progress: BatchProgressResponse;
        try {
          progress = await invoke<BatchProgressResponse>('get_batch_progress', { batchId });
          consecutivePollErrors = 0;
        } catch (error) {
          consecutivePollErrors += 1;
          if (consecutivePollErrors >= 3) {
            const message = error instanceof Error ? error.message : '获取批处理进度失败';
            setProcessingTasks(prev =>
              prev.map(task =>
                queuedIdSet.has(task.id) && (task.status === 'pending' || task.status === 'processing')
                  ? { ...task, status: 'failed', stage: '处理失败', error: message, batchId }
                  : task
              )
            );
            break;
          }
          await sleep(1000);
          continue;
        }

        const total = Math.max(queuedTasks.length, 1);
        const completedCount = Math.max(0, Math.min(progress.completed_items || 0, queuedTasks.length));
        const failedCount = Math.max(0, progress.failed_items || 0);
        const cancelledCount = Math.max(0, progress.cancelled_items || 0);
        const currentItem = progress.current_item || '';
        const overall = Math.max(0, Math.min(100, progress.overall_progress || 0));
        const status = (progress.status || '').toLowerCase();
        const errors = Array.isArray(progress.errors) ? progress.errors : [];
        const backendItems = Array.isArray(progress.items) ? progress.items : [];
        const backendItemMap = new Map(backendItems.map(item => [item.input_path, item] as const));

        const failedByPath = new Map<string, string>();
        for (const err of errors) {
          const found = queuedTasks.find(task => err.includes(task.sourcePath));
          if (found) {
            failedByPath.set(found.sourcePath, err);
          }
        }

        let fallbackFailed = Math.max(0, failedCount - failedByPath.size);
        const currentIndex = currentItem
          ? queuedTasks.findIndex(task => task.sourcePath === currentItem)
          : -1;
        const doneUnits = completedCount + failedCount + cancelledCount;
        const currentFraction = Math.max(0, Math.min(1, (overall / 100) * total - doneUnits));

        setProcessingTasks(prev =>
          prev.map(task => {
            if (!queuedIdSet.has(task.id)) return task;

            const backendItem = backendItemMap.get(task.sourcePath);
            if (backendItem) {
              const backendStatus = (backendItem.status || '').toLowerCase();
              const base = {
                ...task,
                batchId,
                outputPath: backendItem.output_path || task.outputPath,
                progress: Math.max(0, Math.min(100, Math.round(backendItem.progress || 0))),
              };

              if (backendStatus === 'completed') {
                return { ...base, status: 'completed', stage: '已完成', progress: 100, error: undefined };
              }
              if (backendStatus === 'failed') {
                return {
                  ...base,
                  status: 'failed',
                  stage: '处理失败',
                  progress: 100,
                  error: backendItem.error || failedByPath.get(task.sourcePath) || '批处理任务失败',
                };
              }
              if (backendStatus === 'cancelled') {
                return { ...base, status: 'cancelled', stage: '已取消', progress: 100 };
              }
              if (backendStatus === 'running') {
                return { ...base, status: 'processing', stage: '批处理中...' };
              }
              return { ...base, status: 'pending', stage: '等待批处理...' };
            }

            const queuedIndex = queuedTasks.findIndex(t => t.id === task.id);
            if (queuedIndex < 0) return task;

            const explicitError = failedByPath.get(task.sourcePath);
            if (explicitError) {
              return {
                ...task,
                batchId,
                status: 'failed',
                progress: 100,
                stage: '处理失败',
                error: explicitError,
              };
            }

            if (queuedIndex < completedCount) {
              return {
                ...task,
                batchId,
                status: 'completed',
                progress: 100,
                stage: '已完成',
              };
            }

            const couldBeFallbackFailed =
              queuedIndex >= completedCount &&
              (currentIndex < 0 || queuedIndex < currentIndex);
            if (fallbackFailed > 0 && couldBeFallbackFailed) {
              fallbackFailed -= 1;
              return {
                ...task,
                batchId,
                status: 'failed',
                progress: 100,
                stage: '处理失败',
                error: task.error || '批处理任务失败',
              };
            }

            if (status === 'cancelled') {
              if (task.status === 'completed' || task.status === 'failed') {
                return { ...task, batchId };
              }
              return {
                ...task,
                batchId,
                status: 'cancelled',
                stage: '已取消',
              };
            }

            if (currentItem && task.sourcePath === currentItem) {
              return {
                ...task,
                batchId,
                status: 'processing',
                progress: Math.max(task.progress, Math.round(currentFraction * 100)),
                stage: '批处理中...',
              };
            }

            return {
              ...task,
              batchId,
              status: 'pending',
              stage: '等待批处理...',
            };
          })
        );

        if (backendItems.length > 0) {
          const latestCompleted = backendItems
            .filter(item => (item.status || '').toLowerCase() === 'completed')
            .slice(-1)[0];
          if (latestCompleted?.output_path) {
            setProcessedVideoPath(latestCompleted.output_path);
          }
        } else if (completedCount > 0) {
          const lastCompletedTask = queuedTasks[Math.min(completedCount - 1, queuedTasks.length - 1)];
          if (lastCompletedTask?.outputPath) {
            setProcessedVideoPath(lastCompletedTask.outputPath);
          }
        }

        if (status === 'completed' || status === 'failed' || status === 'cancelled') {
          break;
        }

        await sleep(1000);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : '启动批处理失败';
      setProcessingTasks(prev =>
        prev.map(task =>
          queuedIdSet.has(task.id)
            ? {
                ...task,
                status: 'failed',
                stage: '处理失败',
                error: message,
              }
            : task
        )
      );
    }
  }, [lutFiles, settings]);

  const handleProcessVideo = useCallback(async () => {
    const targets = videoFiles.length > 0
      ? videoFiles
      : activeVideoFile
        ? [activeVideoFile]
        : [];

    if (targets.length === 0 || lutFiles.length === 0) {
      console.error('需要选择视频文件和LUT文件');
      return;
    }

    const runId = Date.now();
    const queuedTasks: ProcessingTask[] = targets.map((videoPath, index) => {
      const outputPath = buildExpectedOutputPath(videoPath, settings.output_directory, settings.output_format);
      const id = `task_${runId}_${index}`;
      cancelledTaskIdsRef.current.delete(id);

      return {
        id,
        name: `处理 ${getFileName(videoPath)}`,
        progress: 0,
        status: 'pending',
        stage: '准备中...',
        sourcePath: videoPath,
        outputPath,
      };
    });

    setProcessedVideoPath(null);
    setProcessingTasks(prev => [...prev, ...queuedTasks]);

    if (queuedTasks.length > 1) {
      await runBatchTasks(queuedTasks);
      return;
    }

    await runSingleTask(queuedTasks[0]);
  }, [activeVideoFile, lutFiles, settings, videoFiles, runSingleTask, runBatchTasks]);

  const handleCancelTask = useCallback((taskId: string) => {
    cancelledTaskIdsRef.current.add(taskId);

    const existingTask = processingTasksRef.current.find(task => task.id === taskId);
    if (!existingTask) return;

    if (existingTask.batchId) {
      invoke('cancel_batch', { batchId: existingTask.batchId }).catch(() => undefined);
      setProcessingTasks(prev =>
        prev.map(task =>
          task.batchId === existingTask.batchId && task.status !== 'completed' && task.status !== 'failed'
            ? { ...task, status: 'cancelled', stage: '已取消' }
            : task
        )
      );
      return;
    }

    if (existingTask.backendTaskId) {
      invoke('cancel_task', { taskId: existingTask.backendTaskId }).catch(() => undefined);
    }

    setProcessingTasks(prev =>
      prev.map(task =>
        task.id === taskId
          ? { ...task, status: 'cancelled', stage: '已取消' }
          : task
      )
    );
  }, []);

  const handleRetryTask = useCallback((taskId: string) => {
    cancelledTaskIdsRef.current.delete(taskId);

    const existingTask = processingTasksRef.current.find(task => task.id === taskId);
    if (!existingTask) return;

    // Create a fresh task with a new ID to avoid stale state
    const newId = `task_${Date.now()}_retry`;
    const retryTask: ProcessingTask = {
      ...existingTask,
      id: newId,
      status: 'pending',
      progress: 0,
      stage: '准备重试...',
      error: undefined,
      backendTaskId: undefined,
      batchId: undefined,
    };

    // Replace the old task with the new retry task
    setProcessingTasks(prev =>
      prev.map(task => (task.id === taskId ? retryTask : task))
    );

    // Actually re-run the task
    runSingleTask(retryTask);
  }, [runSingleTask]);

  const handleClearCompleted = useCallback(() => {
    setProcessingTasks(prev =>
      prev.filter(task =>
        task.status !== 'completed' && task.status !== 'failed' && task.status !== 'cancelled'
      )
    );
    setProcessedVideoPath(null);
  }, []);

  const handleSettingsChange = useCallback((newSettings: ProcessingSettings) => {
    setSettings(newSettings);
  }, []);

  const handleOpenProcessedFile = useCallback(async () => {
    if (!processedVideoPath) return;
    try {
      await invoke('open_file', { path: processedVideoPath });
    } catch (error) {
      console.error('打开视频文件失败:', error);
      alert('打开视频文件失败: ' + (error instanceof Error ? error.message : '未知错误'));
    }
  }, [processedVideoPath]);

  const handleOpenProcessedFolder = useCallback(async () => {
    if (!processedVideoPath) return;
    try {
      let folderPath = '';
      if (processedVideoPath.includes('/')) {
        folderPath = processedVideoPath.substring(0, processedVideoPath.lastIndexOf('/'));
      } else if (processedVideoPath.includes('\\')) {
        folderPath = processedVideoPath.substring(0, processedVideoPath.lastIndexOf('\\'));
      }

      if (!folderPath) {
        throw new Error('无法提取文件夹路径');
      }

      await invoke('open_folder', { path: folderPath });
    } catch (error) {
      console.error('打开文件夹失败:', error);
      alert('打开文件夹失败: ' + (error instanceof Error ? error.message : '未知错误'));
    }
  }, [processedVideoPath]);

  const hasProcessingTask = processingTasks.some(task => task.status === 'processing');
  const selectedVideoCount = videoFiles.length > 0 ? videoFiles.length : activeVideoFile ? 1 : 0;
  const canProcess = selectedVideoCount > 0 && lutFiles.length > 0 && !hasProcessingTask;

  return (
    <div className="app">
      <header className="app-header">
        <div className="header-brand">
          <h1>Auto Apply LUT</h1>
          <p>自动化视频 LUT 批处理工作台</p>
        </div>
        <div className="header-metrics">
          <div className="metric-chip">
            <span className="metric-label">视频</span>
            <span className="metric-value">{selectedVideoCount}</span>
          </div>
          <div className="metric-chip">
            <span className="metric-label">LUT</span>
            <span className="metric-value">{lutFiles.length}</span>
          </div>
          <div className={`metric-chip ${hasProcessingTask ? 'is-busy' : ''}`}>
            <span className="metric-label">状态</span>
            <span className="metric-value">{hasProcessingTask ? '处理中' : '待命'}</span>
          </div>
        </div>
      </header>

      <main className="app-main">
        <div className="app-grid">
          <FileUpload
            onVideoSelect={handleVideoSelect}
            onActiveVideoChange={handleActiveVideoChange}
            onLutSelect={handleLutSelect}
            lutPaths={lutFiles}
            disabled={hasProcessingTask}
          />

          <div className="preview-section card">
            <VideoPreview
              videoPath={activeVideoFile || undefined}
              processedVideoPath={processedVideoPath || undefined}
              lutPaths={lutFiles}
            />
          </div>

          <div className="settings-section card">
            <div className="settings-summary">
              <h3>处理控制</h3>
              <p>{canProcess ? '已就绪，可开始处理' : '请先选择视频文件和 LUT 文件'}</p>
            </div>
            <div className="settings-actions">
              <button
                className="btn-settings"
                onClick={() => setIsSettingsOpen(true)}
                disabled={hasProcessingTask}
              >
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none">
                  <path d="M12 15a3 3 0 100-6 3 3 0 000 6z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                  <path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-2 2 2 2 0 01-2-2v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83 0 2 2 0 010-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 01-2-2 2 2 0 012-2h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 010-2.83 2 2 0 012.83 0l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 012-2 2 2 0 012 2v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 0 2 2 0 010 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9a1.65 1.65 0 001.51 1H21a2 2 0 012 2 2 2 0 01-2 2h-.09a1.65 1.65 0 00-1.51 1z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                </svg>
                处理设置
              </button>

              <button
                className="btn-primary"
                onClick={handleProcessVideo}
                disabled={!canProcess}
              >
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none">
                  <path d="M9 11H5v2h4v-2zm0-4H5v2h4V7zm0 8H5v2h4v-2zm12-8h-4v2h4V7zm0 4h-4v2h4v-2zm0 4h-4v2h4v-2zM14 4H10v16h4V4z" fill="currentColor"/>
                </svg>
                {selectedVideoCount > 1 ? `批量应用 LUT (${selectedVideoCount})` : '应用 LUT'}
              </button>
            </div>
          </div>

          <div className="status-section card">
            <ProcessingStatus
              tasks={processingTasks}
              onCancelTask={handleCancelTask}
              onRetryTask={handleRetryTask}
              onClearCompleted={handleClearCompleted}
            />

            {processedVideoPath && (
              <div className="processed-video-info">
                <h4>✅ 处理完成</h4>
                <p>
                  输出文件: {processedVideoPath}
                </p>
                <div className="processed-video-actions">
                  <button
                    className="output-action output-action-primary"
                    onClick={handleOpenProcessedFile}
                  >
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
                      <path d="M8 3v3a2 2 0 002 2h6a2 2 0 002-2V3m-1 8a3 3 0 100 6 3 3 0 000-6z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                      <rect x="3" y="6" width="18" height="15" rx="2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                    </svg>
                    打开视频文件
                  </button>
                  <button
                    className="output-action output-action-success"
                    onClick={handleOpenProcessedFolder}
                  >
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
                      <path d="M3 9l9-7 9 7v11a2 2 0 01-2 2H5a2 2 0 01-2-2z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                      <polyline points="9,22 9,12 15,12 15,22" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                    </svg>
                    打开文件夹
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      </main>

      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        onSettingsChange={handleSettingsChange}
        disabled={hasProcessingTask}
      />
    </div>
  );
}

export default App;
