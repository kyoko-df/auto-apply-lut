import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Settings, Film, Layers, Play, RefreshCw, FolderOpen, FileVideo } from 'lucide-react';
import MultiFileSelector from './components/MultiFileSelector';
import LutSelector from './components/LutSelector';
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
  eta?: number;
  speed?: number;
  error?: string;
  inputPath?: string;
  outputPath?: string;
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
  color_space: string;
  hardware_acceleration: boolean;
  two_pass_encoding: boolean;
  preserve_metadata: boolean;
  output_directory: string;
}

function App() {
  const [videoFile, setVideoFile] = useState<string | null>(null);
  const [lutFile, setLutFile] = useState<string | null>(null);
  const [processedVideoPath, setProcessedVideoPath] = useState<string | null>(null);
  const [batchFiles, setBatchFiles] = useState<string[]>([]);
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
    color_space: 'rec709',
    hardware_acceleration: true,
    two_pass_encoding: false,
    preserve_metadata: true,
    output_directory: ''
  });

  const handleClearFiles = useCallback(() => {
    setVideoFile(null);
    setBatchFiles([]);
    setLutFile(null);
    setProcessedVideoPath(null);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      try {
        const off = await listen('batch', (event: any) => {
          const payload = event?.payload || {};
          const batchId: string = payload.batch_id || 'unknown';
          const overallProgress: number = payload.overall_progress ?? 0;
          const message: string = payload.message ?? '';
          const eventType: string = payload.event_type ?? '';
          const currentItem = payload.current_item as { id: string; input_path?: string; output_path?: string; progress?: number; status?: string; error?: string } | null;

          setProcessingTasks(prev => {
            const overallId = `batch_${batchId} `;
            const exists = prev.some(t => t.id === overallId);
            const updatedOverall: ProcessingTask = {
              id: overallId,
              name: `批处理（${batchId}）总体进度`,
              progress: Math.max(0, Math.min(100, Math.round(overallProgress))),
              status: eventType === 'Completed' ? 'completed' : eventType === 'Failed' ? 'failed' : 'processing',
              stage: message || (eventType === 'Completed' ? '批处理完成' : eventType === 'Failed' ? '批处理失败' : '进行中...')
            };
            const next = exists
              ? prev.map(t => t.id === overallId ? updatedOverall : t)
              : [...prev, updatedOverall];

            if (currentItem && currentItem.id) {
              const fileId = currentItem.id;
              const fileExists = next.some(t => t.id === fileId);
              const statusMap: Record<string, ProcessingTask['status']> = {
                Running: 'processing',
                Completed: 'completed',
                Failed: 'failed',
                Pending: 'pending',
                Cancelled: 'cancelled',
                Paused: 'pending',
                Resumed: 'processing',
              };
              const fileStatus: ProcessingTask['status'] = statusMap[currentItem.status || 'Running'] || 'processing';
              const updatedFile: ProcessingTask = {
                id: fileId,
                name: currentItem.input_path ? currentItem.input_path.split(/[/\\]/).pop() || fileId : fileId,
                progress: Math.max(0, Math.min(100, Math.round((currentItem.progress ?? 0)) ?? 0)),
                status: fileStatus,
                stage: message || (fileStatus === 'completed' ? '已完成' : fileStatus === 'failed' ? '失败' : '处理中'),
                error: currentItem.error || undefined,
                inputPath: currentItem.input_path,
                outputPath: currentItem.output_path,
              };
              return fileExists
                ? next.map(t => t.id === fileId ? updatedFile : t)
                : [...next, updatedFile];
            }

            return next;
          });
        });
        unlisten = off;
      } catch (e) {
        console.warn('订阅批处理事件失败:', e);
      }
    })();
    return () => {
      if (unlisten) {
        try { unlisten(); } catch { }
      }
    };
  }, []);

  const handleStartBatch = useCallback(async () => {
    if (batchFiles.length === 0 || !lutFile) {
      console.error('需要选择待处理文件（一个或多个）以及 LUT 文件');
      return;
    }

    try {
      const first = batchFiles[0];
      const outputDirectory = (settings.output_directory && settings.output_directory.trim().length > 0)
        ? settings.output_directory
        : (first.includes('/')
          ? first.substring(0, first.lastIndexOf('/'))
          : first.substring(0, first.lastIndexOf('\\')));

      const items = batchFiles.map((inputPath) => {
        const p = inputPath.split(/[/\\]/);
        const fileName = p[p.length - 1] || 'unknown';
        const stem = fileName.replace(/\.[^/.]+$/, '');
        const outputPath = `${outputDirectory}/${stem}_processed.mp4`;
        return {
          input_path: inputPath,
          output_path: outputPath,
          lut_path: lutFile!,
          intensity: settings.lut_intensity / 100.0,
        };
      });

      const resp = await invoke('start_batch_processing', {
        request: {
          items,
          hardware_acceleration: settings.hardware_acceleration,
          output_directory: outputDirectory,
          preserve_structure: true,
        }
      });

      const { batch_id } = resp as { batch_id: string };
      setProcessingTasks(prev => {
        const overallId = `batch_${batch_id}`;
        if (prev.some(t => t.id === overallId)) return prev;
        const newOverall: ProcessingTask = {
          id: overallId,
          name: `批处理（${batch_id}）总体进度`,
          progress: 0,
          status: 'processing',
          stage: '已启动，等待事件...'
        };
        return [...prev, newOverall];
      });
    } catch (error) {
      console.error('启动批处理失败:', error);
      setProcessingTasks(prev => [...prev, {
        id: `batch_error_${Date.now()}`,
        name: '批处理启动失败',
        progress: 0,
        status: 'failed',
        stage: '批处理失败',
        error: error instanceof Error ? error.message : '未知错误'
      }]);
    }
  }, [batchFiles, lutFile, settings]);

  const handleCancelTask = useCallback((taskId: string) => {
    setProcessingTasks(prev =>
      prev.map(task =>
        task.id === taskId
          ? { ...task, status: 'cancelled' as const, stage: '已取消' }
          : task
      )
    );
  }, []);

  const handleRetryTask = useCallback((taskId: string) => {
    setProcessingTasks(prev =>
      prev.map(task =>
        task.id === taskId
          ? { ...task, status: 'pending' as const, progress: 0, stage: '准备重试...', error: undefined }
          : task
      )
    );
  }, []);

  const handleClearCompleted = useCallback(() => {
    setProcessingTasks(prev =>
      prev.filter(task =>
        task.status !== 'completed' && task.status !== 'failed' && task.status !== 'cancelled'
      )
    );
    const hasCompletedTasks = processingTasks.some(task =>
      task.status === 'completed' || task.status === 'failed'
    );
    if (hasCompletedTasks) {
      setProcessedVideoPath(null);
      handleClearFiles();
    }
  }, [processingTasks]);

  const handleSettingsChange = useCallback((newSettings: ProcessingSettings) => {
    setSettings(newSettings);
  }, []);

  return (
    <div className="flex h-screen w-full bg-[var(--color-background)] text-[var(--color-text-primary)] font-sans overflow-hidden">
      {/* Sidebar */}
      <aside className="w-80 flex-shrink-0 bg-[var(--color-surface-translucent)] backdrop-blur-xl border-r border-[var(--color-border)] flex flex-col z-20">
        <div className="h-14 flex items-center px-5 border-b border-[var(--color-border)] drag-region shrink-0">
          <div className="flex items-center gap-2.5 text-[var(--color-text-primary)] font-semibold select-none">
            <div className="p-1.5 bg-gradient-to-br from-[var(--color-accent)] to-[var(--color-accent-hover)] rounded-lg text-white shadow-sm">
              <Film size={16} strokeWidth={2.5} />
            </div>
            <span className="tracking-tight">Auto Apply LUT</span>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto p-5 space-y-8 custom-scrollbar">
          {/* File Selection Section */}
          <section>
            <h3 className="section-title flex items-center gap-2 mb-3 px-1">
              <FileVideo size={14} />
              视频文件
            </h3>
            <MultiFileSelector
              title=""
              acceptExtensions={["mp4", "mov", "avi", "mkv", "wmv", "flv", "webm", "m4v"]}
              disabled={processingTasks.some(task => task.status === 'processing')}
              onChange={setBatchFiles}
            />
            {batchFiles.length > 0 && (
              <div className="mt-3 px-3 py-2 bg-[var(--color-surface)] rounded-lg text-xs font-medium text-[var(--color-text-secondary)] border border-[var(--color-border)] shadow-sm flex justify-between items-center">
                <span>已选择 {batchFiles.length} 个文件</span>
                <button 
                  onClick={handleClearFiles}
                  className="text-[var(--color-accent)] hover:text-[var(--color-accent-hover)] transition-colors"
                >
                  清除
                </button>
              </div>
            )}
          </section>

          {/* LUT Selection Section */}
          <section>
            <h3 className="section-title flex items-center gap-2 mb-3 px-1">
              <Layers size={14} />
              LUT 预设
            </h3>
            <LutSelector
              disabled={processingTasks.some(task => task.status === 'processing')}
              onSelect={setLutFile}
            />
          </section>
        </div>

        {/* Fixed Actions Footer */}
        <div className="p-5 border-t border-[var(--color-border)] bg-[var(--color-surface)]/50 backdrop-blur-sm space-y-3 shrink-0">
          <button
            className="apple-button w-full justify-center h-10 text-[15px] shadow-sm hover:shadow-md transition-all"
            onClick={handleStartBatch}
            disabled={batchFiles.length === 0 || !lutFile || processingTasks.some(task => task.status === 'processing')}
          >
            <Play size={16} fill="currentColor" className="mr-1" />
            开始处理
          </button>

          <button
            className="apple-button secondary w-full justify-center h-10 text-[14px]"
            onClick={() => setIsSettingsOpen(true)}
            disabled={processingTasks.some(task => task.status === 'processing')}
          >
            <Settings size={16} className="mr-1" />
            处理设置
          </button>
        </div>

        {/* Status Footer in Sidebar */}
        <div className="px-5 py-3 border-t border-[var(--color-border)] bg-[var(--color-surface)] text-xs text-[var(--color-text-secondary)] shrink-0">
          {processingTasks.some(task => task.status === 'processing') ? (
            <div className="flex items-center gap-2 text-[var(--color-accent)]">
              <RefreshCw size={12} className="animate-spin" />
              <span className="font-medium">正在处理任务...</span>
            </div>
          ) : (
            <div className="flex items-center gap-2">
              <div className="w-2 h-2 rounded-full bg-[var(--color-success)] shadow-[0_0_8px_rgba(52,199,89,0.4)]"></div>
              <span className="font-medium">就绪</span>
            </div>
          )}
        </div>
      </aside>

      {/* Main Content */}
      <main className="flex-1 flex flex-col min-w-0 bg-[var(--color-background)]">
        <div className="flex-1 p-6 overflow-y-auto">
          <div className="max-w-4xl mx-auto space-y-6">
            {/* Preview Area */}
            <div className="apple-card p-0 overflow-hidden bg-black aspect-video flex items-center justify-center relative group">
              <VideoPreview
                videoPath={videoFile || undefined}
                lutPath={lutFile || undefined}
                onProcessingStart={() => console.log('Processing started')}
                onProcessingComplete={(outputPath) => setProcessedVideoPath(outputPath)}
                onProcessingError={(error) => console.error('Processing error:', error)}
              />
              {!videoFile && (
                <div className="absolute inset-0 flex flex-col items-center justify-center text-[var(--color-text-tertiary)] pointer-events-none">
                  <Film size={48} strokeWidth={1} />
                  <p className="mt-4 text-sm font-medium">选择视频以预览</p>
                </div>
              )}
            </div>

            {/* Task List / Status */}
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">处理队列</h2>
                {processingTasks.length > 0 && (
                  <button
                    onClick={handleClearCompleted}
                    className="text-xs text-[var(--color-accent)] hover:underline"
                  >
                    清除已完成
                  </button>
                )}
              </div>

              <ProcessingStatus
                tasks={processingTasks}
                onCancelTask={handleCancelTask}
                onRetryTask={handleRetryTask}
                onClearCompleted={handleClearCompleted}
              />
            </div>

            {/* Completed File Actions */}
            {processedVideoPath && (
              <div className="apple-card bg-[var(--color-surface)] border-l-4 border-l-[var(--color-success)]">
                <div className="flex items-start justify-between">
                  <div>
                    <h4 className="font-medium text-[var(--color-text-primary)] flex items-center gap-2">
                      <div className="w-5 h-5 rounded-full bg-[var(--color-success)] flex items-center justify-center text-white">
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round">
                          <polyline points="20 6 9 17 4 12"></polyline>
                        </svg>
                      </div>
                      处理完成
                    </h4>
                    <p className="mt-1 text-sm text-[var(--color-text-secondary)] break-all">
                      {processedVideoPath}
                    </p>
                  </div>
                  <div className="flex gap-2">
                    <button
                      className="apple-button secondary text-xs"
                      onClick={() => invoke('open_file', { path: processedVideoPath })}
                    >
                      <FileVideo size={14} />
                      打开
                    </button>
                    <button
                      className="apple-button secondary text-xs"
                      onClick={() => {
                        let folderPath = processedVideoPath;
                        if (processedVideoPath.includes('/')) {
                          folderPath = processedVideoPath.substring(0, processedVideoPath.lastIndexOf('/'));
                        } else if (processedVideoPath.includes('\\')) {
                          folderPath = processedVideoPath.substring(0, processedVideoPath.lastIndexOf('\\'));
                        }
                        invoke('open_folder', { path: folderPath });
                      }}
                    >
                      <FolderOpen size={14} />
                      所在文件夹
                    </button>
                  </div>
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
        disabled={processingTasks.some(task => task.status === 'processing')}
      />
    </div>
  );
}

export default App;
