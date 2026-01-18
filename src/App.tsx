import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
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

  // 直接通过 LutSelector 的 onSelect 设置 LUT 文件

  const handleClearFiles = useCallback(() => {
    setVideoFile(null);
    setBatchFiles([]);
    setLutFile(null);
    setProcessedVideoPath(null);
  }, []);

  // 订阅后端批处理事件，实时更新任务列表（每文件与总体）
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

          // 更新总体任务（以 batchId 作为标识）
          setProcessingTasks(prev => {
            const overallId = `batch_${batchId}`;
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

            // 更新当前文件任务
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
        try { unlisten(); } catch {}
      }
    };
  }, []);

  // 批量处理入口：全面改用批处理命令（事件驱动，移除轮询）
  const handleStartBatch = useCallback(async () => {
    if (batchFiles.length === 0 || !lutFile) {
      console.error('需要选择待处理文件（一个或多个）以及 LUT 文件');
      return;
    }

    try {
      // 输出目录：优先使用设置中的目录，否则默认使用首个文件所在目录
      const first = batchFiles[0];
      const outputDirectory = (settings.output_directory && settings.output_directory.trim().length > 0)
        ? settings.output_directory
        : (first.includes('/')
            ? first.substring(0, first.lastIndexOf('/'))
            : first.substring(0, first.lastIndexOf('\\')));

      // 组装批量请求条目
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
      // 添加总体任务条目，等待事件驱动更新
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
    // 这里可以重新触发处理逻辑
  }, []);

  const handleClearCompleted = useCallback(() => {
    setProcessingTasks(prev =>
      prev.filter(task =>
        task.status !== 'completed' && task.status !== 'failed' && task.status !== 'cancelled'
      )
    );
    // 同时清除已处理的视频路径
    const hasCompletedTasks = processingTasks.some(task =>
      task.status === 'completed' || task.status === 'failed'
    );
    if (hasCompletedTasks) {
      setProcessedVideoPath(null);
      handleClearFiles(); // 清除所有文件
    }
  }, [processingTasks]);

  const handleSettingsChange = useCallback((newSettings: ProcessingSettings) => {
     setSettings(newSettings);
   }, []);

  return (
    <div className="app">
      <header className="app-header">
        <h1>Auto Apply LUT</h1>
        <p>自动化视频LUT应用工具</p>
      </header>

      <main className="app-main">
        <div className="app-grid">
          <div className="upload-section">
            <div style={{ marginTop: 16 }}>
              <MultiFileSelector
                title="批量选择待处理视频"
                acceptExtensions={["mp4","mov","avi","mkv","wmv","flv","webm","m4v"]}
                disabled={processingTasks.some(task => task.status === 'processing')}
                onChange={setBatchFiles}
              />
              {batchFiles.length > 0 && (
                <div style={{ marginTop: 8, fontSize: '0.9rem', color: 'var(--color-fg-subtle)' }}>
                  已选择 {batchFiles.length} 个文件
                </div>
              )}
            </div>
            <div style={{ marginTop: 16 }}>
              <LutSelector
                disabled={processingTasks.some(task => task.status === 'processing')}
                onSelect={setLutFile}
              />
            </div>
          </div>

          <div className="preview-section">
             <VideoPreview
               videoPath={videoFile || undefined}
               lutPath={lutFile || undefined}
               onProcessingStart={() => console.log('Processing started')}
               onProcessingComplete={(outputPath) => setProcessedVideoPath(outputPath)}
               onProcessingError={(error) => console.error('Processing error:', error)}
             />
           </div>

          <div className="settings-section">
            <button
              className="btn-settings"
              onClick={() => setIsSettingsOpen(true)}
              disabled={processingTasks.some(task => task.status === 'processing')}
            >
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none">
                <path d="M12 15a3 3 0 100-6 3 3 0 000 6z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                <path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-2 2 2 2 0 01-2-2v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83 0 2 2 0 010-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 01-2-2 2 2 0 012-2h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 010-2.83 2 2 0 012.83 0l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 012-2 2 2 0 012 2v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 0 2 2 0 010 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9a1.65 1.65 0 001.51 1H21a2 2 0 012 2 2 2 0 01-2 2h-.09a1.65 1.65 0 00-1.51 1z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
              </svg>
              处理设置
            </button>

            <button
              className="btn-primary"
              onClick={handleStartBatch}
              disabled={batchFiles.length === 0 || !lutFile || processingTasks.some(task => task.status === 'processing')}
              style={{ marginLeft: '10px' }}
            >
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none">
                <path d="M9 11H5v2h4v-2zm0-4H5v2h4V7zm0 8H5v2h4v-2zm12-8h-4v2h4V7zm0 4h-4v2h4v-2zm0 4h-4v2h4v-2zM14 4H10v16h4V4z" fill="currentColor"/>
              </svg>
              开始批量处理
            </button>
            {(batchFiles.length === 0 || !lutFile || processingTasks.some(task => task.status === 'processing')) && (
              <div style={{ marginTop: 8, fontSize: '0.85rem', color: 'var(--color-fg-subtle)' }}>
                ⚠️ {batchFiles.length === 0
                  ? '请先选择至少一个视频文件'
                  : !lutFile
                  ? '请先选择一个 LUT 文件'
                  : '当前有任务进行中，请稍候或清除已完成'}
              </div>
            )}
          </div>

          <div className="status-section">
            <ProcessingStatus
              tasks={processingTasks}
              onCancelTask={handleCancelTask}
              onRetryTask={handleRetryTask}
              onClearCompleted={handleClearCompleted}
            />

            {/* 显示处理后的视频路径 */}
            {processedVideoPath && (
              <div className="processed-video-info" style={{ marginTop: '20px', padding: '15px', backgroundColor: '#f0f9ff', border: '1px solid #0ea5e9', borderRadius: '8px' }}>
                <h4 style={{ margin: '0 0 10px 0', color: '#0c4a6e' }}>✅ 处理完成</h4>
                <p style={{ margin: '0 0 15px 0', fontSize: '14px', color: '#0369a1', wordBreak: 'break-all' }}>
                  输出文件: {processedVideoPath}
                </p>
                <div style={{ display: 'flex', gap: '10px', flexWrap: 'wrap' }}>
                  <button
                    onClick={async () => {
                      try {
                        await invoke('open_file', { path: processedVideoPath });
                      } catch (error) {
                        console.error('打开视频文件失败:', error);
                        alert('打开视频文件失败: ' + (error instanceof Error ? error.message : '未知错误'));
                      }
                    }}
                    style={{
                      padding: '8px 16px',
                      backgroundColor: '#0ea5e9',
                      color: 'white',
                      border: 'none',
                      borderRadius: '4px',
                      cursor: 'pointer',
                      fontSize: '14px',
                      display: 'flex',
                      alignItems: 'center',
                      gap: '6px'
                    }}
                  >
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
                      <path d="M8 3v3a2 2 0 002 2h6a2 2 0 002-2V3m-1 8a3 3 0 100 6 3 3 0 000-6z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                      <rect x="3" y="6" width="18" height="15" rx="2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                    </svg>
                    打开视频文件
                  </button>
                  <button
                    onClick={async () => {
                      try {
                        // 获取文件所在的目录 - 使用更可靠的方法
                        let folderPath = '';
                        if (processedVideoPath.includes('/')) {
                          // Unix/Linux/macOS 路径
                          folderPath = processedVideoPath.substring(0, processedVideoPath.lastIndexOf('/'));
                        } else if (processedVideoPath.includes('\\')) {
                          // Windows 路径
                          folderPath = processedVideoPath.substring(0, processedVideoPath.lastIndexOf('\\'));
                        }

                        console.log('原始文件路径:', processedVideoPath);
                        console.log('提取的文件夹路径:', folderPath);

                        if (!folderPath) {
                          throw new Error('无法提取文件夹路径');
                        }

                        await invoke('open_folder', { path: folderPath });
                      } catch (error) {
                        console.error('打开文件夹失败:', error);
                        alert('打开文件夹失败: ' + (error instanceof Error ? error.message : '未知错误'));
                      }
                    }}
                    style={{
                      padding: '8px 16px',
                      backgroundColor: '#10b981',
                      color: 'white',
                      border: 'none',
                      borderRadius: '4px',
                      cursor: 'pointer',
                      fontSize: '14px',
                      display: 'flex',
                      alignItems: 'center',
                      gap: '6px'
                    }}
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
        disabled={processingTasks.some(task => task.status === 'processing')}
      />
    </div>
  );
}

export default App;
