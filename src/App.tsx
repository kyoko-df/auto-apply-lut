import { useEffect, useState, useCallback } from 'react';
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

function App() {
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
    hardware_acceleration: true,
    two_pass_encoding: false,
    preserve_metadata: true,
    output_directory: ''
  });

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
    if (!activeVideoFile && filePaths.length > 0) {
      setActiveVideoFile(filePaths[0]);
    }
    if (filePaths.length === 0) {
      setActiveVideoFile(null);
    }
  }, [activeVideoFile]);

  const handleActiveVideoChange = useCallback((filePath: string | null) => {
    setActiveVideoFile(filePath);
    setProcessedVideoPath(null);
  }, []);

  const handleLutSelect = useCallback((filePaths: string[]) => {
    setLutFiles(filePaths);
  }, []);

  const handleClearFiles = useCallback(() => {
    setActiveVideoFile(null);
    setLutFiles([]);
    setProcessedVideoPath(null);
  }, []);

  const handleProcessVideo = useCallback(async () => {
    if (!activeVideoFile || lutFiles.length === 0) {
      console.error('需要选择视频文件和LUT文件');
      return;
    }

    const taskId = `task_${Date.now()}`;
    const fileName = activeVideoFile.split('/').pop() || activeVideoFile.split('\\').pop() || 'unknown';
    const newTask: ProcessingTask = {
      id: taskId,
      name: `处理 ${fileName}`,
      progress: 0,
      status: 'pending',
      stage: '准备中...'
    };

    setProcessingTasks(prev => [...prev, newTask]);
      console.log('任务已添加:', newTask);
      console.log('当前任务列表:', processingTasks);

    try {
      // 更新任务状态为处理中
      setProcessingTasks(prev => {
        const updated = prev.map(task =>
          task.id === taskId
            ? { ...task, status: 'processing' as const, stage: '应用LUT...' }
            : task
        );
        console.log('任务状态已更新为处理中:', updated.find(t => t.id === taskId));
        return updated;
      });

      // 调用后端处理视频
      console.log('开始处理视频:', { activeVideoFile, lutFiles, settings });
      const result = await invoke('start_video_processing', {
        request: {
          input_path: activeVideoFile,
          output_path: '', // 让后端自动生成输出路径
          lut_paths: lutFiles,
          intensity: settings.lut_intensity / 100.0, // 转换为0-1范围
          hardware_acceleration: settings.hardware_acceleration,
          lut_error_strategy: settings.lut_error_strategy
        }
      });

      console.log('后端返回结果:', result);

      // 检查是否返回了任务ID
      if (result && typeof result === 'object' && 'task_id' in result) {
        const resultWithOutputPath = result as { task_id: string; output_path?: string };
        console.log('开始真实FFmpeg处理，任务ID:', resultWithOutputPath.task_id);
        console.log('预期输出路径:', resultWithOutputPath.output_path);

        let finalOutputPath = resultWithOutputPath.output_path || `${activeVideoFile}_processed.mp4`;
        let completed = false;

        // 轮询后端真实任务进度
        for (;;) {
          try {
            const progressInfo = await invoke('get_task_progress', { taskId: resultWithOutputPath.task_id });
            const pi = progressInfo as { progress: number; status_message?: string };

            setProcessingTasks(prev =>
              prev.map(task =>
                task.id === taskId
                  ? {
                      ...task,
                      progress: Math.round(pi.progress || 0),
                      stage: pi.status_message || '处理中...',
                    }
                  : task
              )
            );

            if ((pi.progress || 0) >= 100) {
              completed = true;
              break;
            }
          } catch (e) {
            console.warn('获取任务进度失败:', e);
          }

          await new Promise(resolve => setTimeout(resolve, 1000));
        }

        // 完成后确认输出文件是否存在
        if (completed && finalOutputPath) {
          try {
            await invoke('get_file_info', { path: finalOutputPath });
            setProcessingTasks(prev =>
              prev.map(task =>
                task.id === taskId
                  ? { ...task, status: 'completed' as const, progress: 100, stage: '已完成' }
                  : task
              )
            );
            setProcessedVideoPath(finalOutputPath);
            console.log('视频处理完成，输出文件:', finalOutputPath);
          } catch {
            console.error('处理完成但未找到输出文件:', finalOutputPath);
            setProcessingTasks(prev =>
              prev.map(task =>
                task.id === taskId
                  ? { ...task, status: 'failed' as const, stage: '处理失败：未找到输出文件' }
                  : task
              )
            );
          }
        }
      }
    } catch (error) {
      console.error('处理视频时出错:', error);
      setProcessingTasks(prev =>
        prev.map(task =>
          task.id === taskId
            ? {
                ...task,
                status: 'failed' as const,
                stage: '处理失败',
                error: error instanceof Error ? error.message : '未知错误'
              }
            : task
        )
      );
    }
  }, [activeVideoFile, lutFiles, settings]);

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
            <FileUpload
              onVideoSelect={handleVideoSelect}
              onActiveVideoChange={handleActiveVideoChange}
              onLutSelect={handleLutSelect}
              lutPaths={lutFiles}
              disabled={processingTasks.some(task => task.status === 'processing')}
            />
          </div>

          <div className="preview-section">
             <VideoPreview
               videoPath={activeVideoFile || undefined}
               lutPaths={lutFiles}
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
              onClick={handleProcessVideo}
              disabled={!activeVideoFile || lutFiles.length === 0 || processingTasks.some(task => task.status === 'processing')}
              style={{ marginLeft: '10px' }}
            >
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none">
                <path d="M9 11H5v2h4v-2zm0-4H5v2h4V7zm0 8H5v2h4v-2zm12-8h-4v2h4V7zm0 4h-4v2h4v-2zm0 4h-4v2h4v-2zM14 4H10v16h4V4z" fill="currentColor"/>
              </svg>
              应用LUT
            </button>
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
