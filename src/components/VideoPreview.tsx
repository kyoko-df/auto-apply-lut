import React, { useRef, useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './VideoPreview.css';

interface VideoPreviewProps {
  videoPath?: string;
  processedVideoPath?: string;
  lutPaths?: string[];
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

const VideoPreview: React.FC<VideoPreviewProps> = ({
  videoPath,
  processedVideoPath,
  lutPaths,
}) => {
  void lutPaths;
  const videoRef = useRef<HTMLVideoElement>(null);
  const [videoInfo, setVideoInfo] = useState<VideoInfo | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [previewMode, setPreviewMode] = useState<'original' | 'processed'>('original');
  const [playerTab, setPlayerTab] = useState<'video' | 'ffplay'>('video');
  const [isFfplayStarting, setIsFfplayStarting] = useState(false);
  const [videoSrc, setVideoSrc] = useState<string>('');
  const [error, setError] = useState<string | null>(null);

  const isTauriEnv = useCallback(
    () => typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__,
    []
  );

  const errorToMessage = useCallback((e: unknown, fallback: string) => {
    if (typeof e === 'string' && e.trim()) return e;
    if (e && typeof e === 'object') {
      const anyE = e as any;
      if (typeof anyE.message === 'string' && anyE.message.trim()) return anyE.message;
      if (typeof anyE.error === 'string' && anyE.error.trim()) return anyE.error;
      if (typeof anyE.toString === 'function') {
        const s = String(anyE.toString());
        if (s && s !== '[object Object]') return s;
      }
    }
    return fallback;
  }, []);

  const buildAssetUrl = useCallback((path: string) => {
    const normalized = path.replace(/\\/g, '/').replace(/^\/+/, '');
    return `asset://localhost/${encodeURI(normalized)}`;
  }, []);

  const guessVideoMimeType = useCallback((path: string) => {
    const ext = path.split('.').pop()?.toLowerCase();
    switch (ext) {
      case 'mp4':
      case 'm4v':
        return 'video/mp4';
      case 'mov':
        return 'video/quicktime';
      case 'webm':
        return 'video/webm';
      case 'ogv':
        return 'video/ogg';
      default:
        return '';
    }
  }, []);

  const resolveVideoSrc = useCallback(
    async (path: string) => {
      if (!isTauriEnv()) return '';
      try {
        const mod = await import('@tauri-apps/api/core');
        const convertFileSrc = (mod as any).convertFileSrc;
        if (typeof convertFileSrc === 'function') {
          return convertFileSrc(path);
        }
      } catch {
        // ignore
      }
      return buildAssetUrl(path);
    },
    [buildAssetUrl, isTauriEnv]
  );

  const applyVideoSource = useCallback(
    async (path?: string | null) => {
      if (!path) {
        setVideoSrc('');
        return;
      }
      const src = await resolveVideoSrc(path);
      setVideoSrc(src);
    },
    [resolveVideoSrc]
  );

  const stopFfplay = useCallback(async () => {
    try {
      await invoke('stop_ffplay');
    } catch {
      // ignore
    }
  }, []);

  const startFfplay = useCallback(async (path?: string | null) => {
    if (!path) return;
    try {
      setIsFfplayStarting(true);
      setError(null);
      await invoke('play_with_ffplay', { path });
    } catch (e) {
      setError(errorToMessage(e, '启动 ffplay 失败'));
    } finally {
      setIsFfplayStarting(false);
    }
  }, [errorToMessage]);

  // 加载视频信息
  const loadVideoInfo = useCallback(async (path: string) => {
    try {
      setIsLoading(true);
      setError(null);
      
      const info = await invoke<VideoInfo>('get_video_info', { path });
      setVideoInfo(info);
      await applyVideoSource(path);
    } catch (err) {
      console.error('Failed to load video info:', err);
      setError(errorToMessage(err, '无法加载视频信息'));
    } finally {
      setIsLoading(false);
    }
  }, [applyVideoSource, errorToMessage]);

  // 当视频路径改变时加载视频信息
  useEffect(() => {
    if (videoPath) {
      setPreviewMode('original');
      setPlayerTab('video');
      loadVideoInfo(videoPath);
    } else {
      setVideoInfo(null);
      setVideoSrc('');
    }
  }, [videoPath, loadVideoInfo]);

  useEffect(() => {
    if (!videoPath) return;
    const activePath = previewMode === 'processed' ? (processedVideoPath ?? videoPath) : videoPath;
    if (playerTab === 'ffplay') {
      startFfplay(activePath);
      return;
    }
    stopFfplay();
    applyVideoSource(activePath);
  }, [
    applyVideoSource,
    playerTab,
    previewMode,
    processedVideoPath,
    startFfplay,
    stopFfplay,
    videoPath
  ]);

  useEffect(() => {
    return () => {
      void stopFfplay();
    };
  }, [stopFfplay]);

  useEffect(() => {
    if (!videoRef.current) return;
    if (!videoSrc) return;
    if (isLoading) return;
    try {
      videoRef.current.load();
    } catch {
      // ignore
    }
  }, [videoSrc, isLoading]);

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
        {videoPath && (
          <div className="preview-controls">
            <button
              className={`mode-button ${playerTab === 'video' ? 'active' : ''}`}
              onClick={() => setPlayerTab('video')}
            >
              内置
            </button>
            <button
              className={`mode-button ${playerTab === 'ffplay' ? 'active' : ''}`}
              onClick={() => setPlayerTab('ffplay')}
            >
              ffplay
            </button>
            {processedVideoPath && (
              <>
                <button 
                  className={`mode-button ${previewMode === 'original' ? 'active' : ''}`}
                  onClick={() => {
                    setPreviewMode('original');
                  }}
                >
                  原始
                </button>
                <button 
                  className={`mode-button ${previewMode === 'processed' ? 'active' : ''}`}
                  onClick={() => {
                    setPreviewMode('processed');
                  }}
                >
                  处理后
                </button>
              </>
            )}
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

        {videoPath && !isLoading && !error && playerTab === 'video' && (
          <div className="video-container">
            <video 
              ref={videoRef}
              controls
              className="video-player"
              onError={(e) => {
                const mediaError = e.currentTarget.error;
                if (mediaError?.code) {
                  setError(`视频加载失败（错误码 ${mediaError.code}）`);
                } else {
                  setError('视频加载失败');
                }
              }}
            >
              {videoSrc && (
                <source
                  key={videoSrc}
                  src={videoSrc}
                  type={guessVideoMimeType(previewMode === 'original' ? (videoPath ?? '') : (processedVideoPath ?? ''))}
                />
              )}
              您的浏览器不支持视频播放
            </video>
            
            <div className="video-overlay">
              <div className="preview-mode-indicator">
                {previewMode === 'original' ? '原始视频' : '处理后视频'}
              </div>
            </div>
          </div>
        )}

        {videoPath && !isLoading && !error && playerTab === 'ffplay' && (
          <div className="empty-state">
            <div className="empty-icon">🎞️</div>
            <div className="empty-text">{isFfplayStarting ? '正在启动 ffplay…' : '使用 ffplay 播放（外部窗口）'}</div>
            <div className="action-buttons">
              <button
                className="apply-button"
                onClick={async () => {
                  const path = previewMode === 'processed' ? (processedVideoPath ?? videoPath) : videoPath;
                  await startFfplay(path);
                }}
                disabled={isFfplayStarting}
              >
                重新播放
              </button>
              <button
                className="download-button"
                onClick={async () => {
                  await stopFfplay();
                }}
              >
                停止播放
              </button>
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

      {processedVideoPath && (
        <div className="action-buttons">
          <button
            className="download-button"
            onClick={async () => {
              try {
                await invoke('open_file_location', { path: processedVideoPath });
              } catch (e) {
                setError(errorToMessage(e, '打开文件位置失败'));
              }
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
