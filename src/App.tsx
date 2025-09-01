import { useState, useCallback } from 'react';
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
  color_space: string;
  hardware_acceleration: boolean;
  two_pass_encoding: boolean;
  preserve_metadata: boolean;
  output_directory: string;
}

function App() {
  const [videoFile, setVideoFile] = useState<File | null>(null);
  const [lutFile, setLutFile] = useState<File | null>(null);
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
    color_space: 'rec709',
    hardware_acceleration: true,
    two_pass_encoding: false,
    preserve_metadata: true,
    output_directory: ''
  });

  const handleVideoSelect = useCallback((file: File) => {
    setVideoFile(file);
    setProcessedVideoPath(null);
  }, []);

  const handleLutSelect = useCallback((file: File) => {
    setLutFile(file);
  }, []);

  const handleClearFiles = useCallback(() => {
    setVideoFile(null);
    setLutFile(null);
    setProcessedVideoPath(null);
  }, []);

  const handleProcessVideo = useCallback(async () => {
    if (!videoFile || !lutFile) {
      console.error('需要选择视频文件和LUT文件');
      return;
    }

    const taskId = `task_${Date.now()}`;
    const newTask: ProcessingTask = {
      id: taskId,
      name: `处理 ${videoFile.name}`,
      progress: 0,
      status: 'pending',
      stage: '准备中...'
    };

    setProcessingTasks(prev => [...prev, newTask]);

    try {
      // 更新任务状态为处理中
      setProcessingTasks(prev => 
        prev.map(task => 
          task.id === taskId 
            ? { ...task, status: 'processing' as const, stage: '应用LUT...' }
            : task
        )
      );

      // 调用后端处理视频
      const result = await invoke('apply_lut_to_video', {
        videoPath: videoFile.name, // 实际应该是文件路径
        lutPath: lutFile.name, // 实际应该是文件路径
        settings: settings
      });

      // 模拟进度更新
      for (let progress = 10; progress <= 100; progress += 10) {
        await new Promise(resolve => setTimeout(resolve, 500));
        setProcessingTasks(prev => 
          prev.map(task => 
            task.id === taskId 
              ? { 
                  ...task, 
                  progress,
                  stage: progress < 100 ? `处理中... ${progress}%` : '完成',
                  eta: progress < 100 ? (100 - progress) * 0.5 : undefined,
                  speed: 1.2
                }
              : task
          )
        );
      }

      // 处理完成
      setProcessingTasks(prev => 
        prev.map(task => 
          task.id === taskId 
            ? { ...task, status: 'completed' as const, progress: 100, stage: '已完成' }
            : task
        )
      );

      setProcessedVideoPath(result as string);
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
  }, [videoFile, lutFile, settings]);

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
  }, []);

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
              onVideoSelect={(filePath: string) => {
                // 创建一个模拟的File对象
                const fileName = filePath.split('/').pop() || filePath.split('\\').pop() || 'unknown';
                const mockFile = new File([], fileName, { type: 'video/mp4' });
                handleVideoSelect(mockFile);
              }}
              onLutSelect={(filePath: string) => {
                // 创建一个模拟的File对象
                const fileName = filePath.split('/').pop() || filePath.split('\\').pop() || 'unknown';
                const mockFile = new File([], fileName, { type: 'application/octet-stream' });
                handleLutSelect(mockFile);
              }}
              disabled={processingTasks.some(task => task.status === 'processing')}
            />
          </div>

          <div className="preview-section">
             <VideoPreview
               videoPath={videoFile?.name}
               lutPath={lutFile?.name}
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
          </div>

          <div className="status-section">
            <ProcessingStatus
              tasks={processingTasks}
              onCancelTask={handleCancelTask}
              onRetryTask={handleRetryTask}
              onClearCompleted={handleClearCompleted}
            />
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
