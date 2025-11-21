import React, { useRef, useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Film, AlertCircle, Loader2, FileVideo, Clock, Monitor, Activity, HardDrive, FileType } from 'lucide-react';

interface VideoPreviewProps {
  videoPath?: string;
  lutPath?: string;
  onProcessingStart?: () => void;
  onProcessingComplete?: (outputPath: string) => void;
  onProcessingError?: (error: string) => void;
}

interface VideoInfo {
  duration: number;
  width: number;
  height: number;
  fps: number;
  codec: string;
  bitrate: string;
  size: number;
}

interface ProcessingProgress {
  stage: string;
  progress: number;
  eta: number;
  current_frame: number;
  total_frames: number;
  fps: number;
  bitrate: string;
  speed: string;
}

const VideoPreview: React.FC<VideoPreviewProps> = ({
  videoPath,
  lutPath,
  onProcessingStart,
  onProcessingComplete,
  onProcessingError
}) => {
  // Use parameters to avoid unused warnings
  console.log('VideoPreview props:', { videoPath, lutPath });

  // Mock usage of callback props to avoid warnings
  const mockCallbacks = { onProcessingStart, onProcessingComplete, onProcessingError };
  void mockCallbacks;

  const videoRef = useRef<HTMLVideoElement>(null);
  const [videoInfo, setVideoInfo] = useState<VideoInfo | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [progress, setProgress] = useState<ProcessingProgress | null>(null);
  const [previewMode, setPreviewMode] = useState<'original' | 'processed'>('original');
  const [processedVideoPath, setProcessedVideoPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // 加载视频信息
  const loadVideoInfo = useCallback(async (path: string) => {
    try {
      setIsLoading(true);
      setError(null);

      const info = await invoke<VideoInfo>('get_video_info', { path });
      setVideoInfo(info);

      // 设置视频源
      if (videoRef.current) {
        videoRef.current.src = `asset://localhost/${path}`;
      }
    } catch (err) {
      console.error('Failed to load video info:', err);
      setError('无法加载视频信息');
    } finally {
      setIsLoading(false);
    }
  }, []);

  // 当视频路径改变时加载视频信息
  useEffect(() => {
    if (videoPath) {
      loadVideoInfo(videoPath);
    } else {
      setVideoInfo(null);
      setProcessedVideoPath(null);
      setIsProcessing(false); // Reset processing state when video changes
      if (videoRef.current) {
        videoRef.current.src = '';
      }
    }
  }, [videoPath, loadVideoInfo]);

  // 监听处理进度
  useEffect(() => {
    if (!isProcessing) return;

    const interval = setInterval(async () => {
      try {
        const currentProgress = await invoke<ProcessingProgress>('get_processing_progress');
        setProgress(currentProgress);
      } catch (err) {
        // 忽略进度获取错误
      }
    }, 1000);

    return () => clearInterval(interval);
  }, [isProcessing]);

  // 格式化时间
  const formatTime = (seconds: number): string => {
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const secs = Math.floor(seconds % 60);

    if (hours > 0) {
      return `${hours}:${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
    }
    return `${minutes}:${secs.toString().padStart(2, '0')}`;
  };

  // 格式化文件大小
  const formatFileSize = (bytes: number): string => {
    const units = ['B', 'KB', 'MB', 'GB'];
    let size = bytes;
    let unitIndex = 0;

    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex++;
    }

    return `${size.toFixed(1)} ${units[unitIndex]}`;
  };

  return (
    <div className="flex flex-col h-full space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium text-[var(--color-text-secondary)]">视频预览</h3>
        {processedVideoPath && (
          <div className="flex p-0.5 bg-[var(--color-border)] rounded-lg">
            <button
              className={`px-3 py-1 text-xs font-medium rounded-md transition-all ${previewMode === 'original' ? 'bg-[var(--color-surface)] shadow-sm text-[var(--color-text-primary)]' : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'}`}
              onClick={() => {
                setPreviewMode('original');
                if (videoRef.current && videoPath) {
                  videoRef.current.src = `asset://localhost/${videoPath}`;
                }
              }}
            >
              原始
            </button>
            <button
              className={`px-3 py-1 text-xs font-medium rounded-md transition-all ${previewMode === 'processed' ? 'bg-[var(--color-surface)] shadow-sm text-[var(--color-text-primary)]' : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'}`}
              onClick={() => {
                setPreviewMode('processed');
                if (videoRef.current && processedVideoPath) {
                  videoRef.current.src = `asset://localhost/${processedVideoPath}`;
                }
              }}
            >
              处理后
            </button>
          </div>
        )}
      </div>

      <div className="relative aspect-video bg-black rounded-xl overflow-hidden shadow-lg border border-[var(--color-border)] group">
        {error ? (
          <div className="absolute inset-0 flex flex-col items-center justify-center text-[var(--color-danger)] bg-[var(--color-surface)]">
            <AlertCircle size={32} className="mb-2" />
            <span className="text-sm font-medium">{error}</span>
          </div>
        ) : isLoading ? (
          <div className="absolute inset-0 flex flex-col items-center justify-center text-[var(--color-text-secondary)] bg-[var(--color-surface)]">
            <Loader2 size={32} className="animate-spin mb-2" />
            <span className="text-sm">正在加载视频...</span>
          </div>
        ) : videoPath ? (
          <>
            <video
              ref={videoRef}
              controls
              className="w-full h-full object-contain"
              onError={() => setError('视频加载失败')}
            >
              您的浏览器不支持视频播放
            </video>

            <div className="absolute top-4 left-4 px-2 py-1 bg-black/50 backdrop-blur-md rounded text-xs font-medium text-white pointer-events-none opacity-0 group-hover:opacity-100 transition-opacity">
              {previewMode === 'original' ? '原始视频' : '处理后视频'}
            </div>
          </>
        ) : (
          <div className="absolute inset-0 flex flex-col items-center justify-center text-[var(--color-text-tertiary)] bg-[var(--color-surface)]">
            <Film size={48} className="mb-3 opacity-50" />
            <div className="text-sm">请选择视频文件进行预览</div>
          </div>
        )}
      </div>

      {videoInfo && (
        <div className="apple-card bg-[var(--color-surface)] p-4">
          <h4 className="text-xs font-medium text-[var(--color-text-secondary)] uppercase tracking-wider mb-3">视频信息</h4>
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-4">
            <div className="flex items-center gap-2">
              <Clock size={14} className="text-[var(--color-text-tertiary)]" />
              <div className="flex flex-col">
                <span className="text-[10px] text-[var(--color-text-secondary)]">时长</span>
                <span className="text-xs font-medium text-[var(--color-text-primary)]">{formatTime(videoInfo.duration)}</span>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <Monitor size={14} className="text-[var(--color-text-tertiary)]" />
              <div className="flex flex-col">
                <span className="text-[10px] text-[var(--color-text-secondary)]">分辨率</span>
                <span className="text-xs font-medium text-[var(--color-text-primary)]">{videoInfo.width} × {videoInfo.height}</span>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <Activity size={14} className="text-[var(--color-text-tertiary)]" />
              <div className="flex flex-col">
                <span className="text-[10px] text-[var(--color-text-secondary)]">帧率</span>
                <span className="text-xs font-medium text-[var(--color-text-primary)]">{videoInfo.fps.toFixed(2)} fps</span>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <FileType size={14} className="text-[var(--color-text-tertiary)]" />
              <div className="flex flex-col">
                <span className="text-[10px] text-[var(--color-text-secondary)]">编码</span>
                <span className="text-xs font-medium text-[var(--color-text-primary)]">{videoInfo.codec}</span>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <Activity size={14} className="text-[var(--color-text-tertiary)]" />
              <div className="flex flex-col">
                <span className="text-[10px] text-[var(--color-text-secondary)]">码率</span>
                <span className="text-xs font-medium text-[var(--color-text-primary)]">{videoInfo.bitrate}</span>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <HardDrive size={14} className="text-[var(--color-text-tertiary)]" />
              <div className="flex flex-col">
                <span className="text-[10px] text-[var(--color-text-secondary)]">大小</span>
                <span className="text-xs font-medium text-[var(--color-text-primary)]">{formatFileSize(videoInfo.size)}</span>
              </div>
            </div>
          </div>
        </div>
      )}

      {isProcessing && progress && (
        <div className="apple-card bg-[var(--color-surface)] p-4 border-[var(--color-accent)] border-opacity-20">
          <div className="flex items-center justify-between mb-2">
            <h4 className="text-xs font-medium text-[var(--color-text-secondary)] uppercase tracking-wider">处理进度</h4>
            <span className="text-xs font-bold text-[var(--color-accent)]">{progress.progress.toFixed(1)}%</span>
          </div>

          <div className="h-1.5 bg-[var(--color-background)] rounded-full overflow-hidden mb-3">
            <div
              className="h-full bg-[var(--color-accent)] transition-all duration-300 ease-apple"
              style={{ width: `${progress.progress}%` }}
            />
          </div>

          <div className="grid grid-cols-3 gap-2 text-xs">
            <div className="text-[var(--color-text-secondary)]">
              <span className="block text-[10px] text-[var(--color-text-tertiary)]">阶段</span>
              {progress.stage}
            </div>
            <div className="text-[var(--color-text-secondary)]">
              <span className="block text-[10px] text-[var(--color-text-tertiary)]">速度</span>
              {progress.speed}
            </div>
            <div className="text-[var(--color-text-secondary)] text-right">
              <span className="block text-[10px] text-[var(--color-text-tertiary)]">预计剩余</span>
              {formatTime(progress.eta)}
            </div>
          </div>
        </div>
      )}

      {processedVideoPath && (
        <div className="flex justify-end">
          <button
            className="apple-button flex items-center gap-2"
            onClick={() => {
              invoke('open_file_location', { path: processedVideoPath });
            }}
          >
            <FileVideo size={16} />
            打开文件位置
          </button>
        </div>
      )}
    </div>
  );
};

export default VideoPreview;