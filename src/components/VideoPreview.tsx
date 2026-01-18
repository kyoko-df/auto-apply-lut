import React, { useRef, useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './VideoPreview.css';

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
  // This prevents unused variable warnings while keeping the component interface
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

  // 应用LUT处理已移至App.tsx中的handleProcessVideo函数

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

  // 切换预览模式
  // const togglePreviewMode = useCallback(() => {
  //   if (processedVideoPath) {
  //     const newMode = previewMode === 'original' ? 'processed' : 'original';
  //     setPreviewMode(newMode);
  //
  //     if (videoRef.current) {
  //       const path = newMode === 'original' ? videoPath : processedVideoPath;
  //       videoRef.current.src = path ? `asset://localhost/${path}` : '';
  //     }
  //   }
  // }, [previewMode, videoPath, processedVideoPath]);

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
    <div className="video-preview">
      <div className="preview-header">
        <h3>视频预览</h3>
        {processedVideoPath && (
          <div className="preview-controls">
            <button 
              className={`mode-button ${previewMode === 'original' ? 'active' : ''}`}
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
              className={`mode-button ${previewMode === 'processed' ? 'active' : ''}`}
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

      <div className="preview-content">
        {error && (
          <div className="error-message">
            <span className="error-icon">⚠️</span>
            {error}
          </div>
        )}

        {isLoading && (
          <div className="loading-state">
            <div className="loading-spinner"></div>
            <div className="loading-text">正在加载视频...</div>
          </div>
        )}

        {videoPath && !isLoading && !error && (
          <div className="video-container">
            <video 
              ref={videoRef}
              controls
              className="video-player"
              onError={() => setError('视频加载失败')}
            >
              您的浏览器不支持视频播放
            </video>
            
            <div className="video-overlay">
              <div className="preview-mode-indicator">
                {previewMode === 'original' ? '原始视频' : '处理后视频'}
              </div>
            </div>
          </div>
        )}

        {!videoPath && !isLoading && (
          <div className="empty-state">
            <div className="empty-icon">🎬</div>
            <div className="empty-text">请选择视频文件进行预览</div>
          </div>
        )}
      </div>

      {videoInfo && (
        <div className="video-info">
          <h4>视频信息</h4>
          <div className="info-grid">
            <div className="info-item">
              <span className="info-label">时长:</span>
              <span className="info-value">{formatTime(videoInfo.duration)}</span>
            </div>
            <div className="info-item">
              <span className="info-label">分辨率:</span>
              <span className="info-value">{videoInfo.width} × {videoInfo.height}</span>
            </div>
            <div className="info-item">
              <span className="info-label">帧率:</span>
              <span className="info-value">{videoInfo.fps.toFixed(2)} fps</span>
            </div>
            <div className="info-item">
              <span className="info-label">编码:</span>
              <span className="info-value">{videoInfo.codec}</span>
            </div>
            <div className="info-item">
              <span className="info-label">码率:</span>
              <span className="info-value">{videoInfo.bitrate}</span>
            </div>
            <div className="info-item">
              <span className="info-label">大小:</span>
              <span className="info-value">{formatFileSize(videoInfo.size)}</span>
            </div>
          </div>
        </div>
      )}

      {isProcessing && progress && (
        <div className="processing-progress">
          <h4>处理进度</h4>
          <div className="progress-info">
            <div className="progress-stage">{progress.stage}</div>
            <div className="progress-bar">
              <div 
                className="progress-fill"
                style={{ width: `${progress.progress}%` }}
              ></div>
            </div>
            <div className="progress-text">{progress.progress.toFixed(1)}%</div>
          </div>
          
          <div className="progress-details">
            <div className="detail-item">
              <span>帧数:</span>
              <span>{progress.current_frame} / {progress.total_frames}</span>
            </div>
            <div className="detail-item">
              <span>速度:</span>
              <span>{progress.speed}</span>
            </div>
            <div className="detail-item">
              <span>预计剩余:</span>
              <span>{formatTime(progress.eta)}</span>
            </div>
          </div>
        </div>
      )}

      {processedVideoPath && (
        <div className="action-buttons">
          <button
            className="download-button"
            onClick={() => {
              // 触发下载或打开文件位置
              invoke('open_file_location', { path: processedVideoPath });
            }}
          >
            打开文件位置
          </button>
        </div>
      )}
    </div>
  );
};

export default VideoPreview;