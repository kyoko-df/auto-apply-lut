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

type CanvasBuffer = {
  startTime: number;
  fps: number;
  frameUrls: string[];
};

const CANVAS_FPS = 12;
const CANVAS_SEGMENT_SECONDS = 4;
const CANVAS_MAX_BUFFER_SECONDS = 20;

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
  const [playerTab, setPlayerTab] = useState<'video' | 'canvas' | 'ffplay'>('video');
  const [isFfplayStarting, setIsFfplayStarting] = useState(false);
  const [processedVideoPath, setProcessedVideoPath] = useState<string | null>(null);
  const [videoSrc, setVideoSrc] = useState<string>('');
  const [error, setError] = useState<string | null>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [canvasSeekTime, setCanvasSeekTime] = useState(0);
  const [canvasBuffer, setCanvasBuffer] = useState<CanvasBuffer | null>(null);
  const [canvasFrameIndex, setCanvasFrameIndex] = useState(0);
  const [isCanvasPlaying, setIsCanvasPlaying] = useState(false);
  const [isCanvasRendering, setIsCanvasRendering] = useState(false);
  const canvasInFlightRef = useRef(false);
  const canvasPlaybackTimerRef = useRef<number | null>(null);
  const canvasPrefetchSeqRef = useRef(0);
  const canvasBufferRef = useRef<CanvasBuffer | null>(null);
  const canvasFrameIndexRef = useRef(0);
  const canvasImageCacheRef = useRef<Map<string, HTMLImageElement>>(new Map());
  const canvasSeekDebounceRef = useRef<number | null>(null);

  const isTauriEnv = useCallback(
    () => typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__,
    []
  );

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
      setError(e instanceof Error ? e.message : '启动 ffplay 失败');
    } finally {
      setIsFfplayStarting(false);
    }
  }, []);

  const drawImageToCanvas = useCallback((img: HTMLImageElement) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const w = img.naturalWidth || img.width;
    const h = img.naturalHeight || img.height;
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  }, []);

  useEffect(() => {
    canvasBufferRef.current = canvasBuffer;
  }, [canvasBuffer]);

  useEffect(() => {
    canvasFrameIndexRef.current = canvasFrameIndex;
  }, [canvasFrameIndex]);

  const drawCanvasUrl = useCallback(
    async (url: string) => {
      const cache = canvasImageCacheRef.current;
      let img = cache.get(url);
      if (!img) {
        img = new Image();
        img.src = url;
        cache.set(url, img);
      }
      if (!img.complete) {
        await new Promise<void>((resolve) => {
          img!.onload = () => resolve();
          img!.onerror = () => resolve();
        });
      }
      drawImageToCanvas(img);
    },
    [drawImageToCanvas]
  );

  const loadCanvasSegment = useCallback(
    async (path: string, segmentStart: number, seekTime?: number) => {
      if (!isTauriEnv()) {
        setError('Canvas 预览仅在 Tauri 环境可用');
        return;
      }
      if (!path) return;
      if (canvasInFlightRef.current) return;
      canvasInFlightRef.current = true;
      setIsCanvasRendering(true);
      setError(null);
      const seq = ++canvasPrefetchSeqRef.current;
      try {
        const res = await invoke<{
          start_time: number;
          fps: number;
          frames: Array<{ image_path: string; timestamp: number }>;
        }>('prefetch_preview_frames', {
          path,
          start_time: segmentStart,
          duration: CANVAS_SEGMENT_SECONDS,
          fps: CANVAS_FPS,
          width: 960
        });

        if (seq !== canvasPrefetchSeqRef.current) return;

        const urls = await Promise.all(
          res.frames.map(async (f) => {
            const u = await resolveVideoSrc(f.image_path);
            return u;
          })
        );
        const frameUrls = urls.filter((u): u is string => !!u);
        if (frameUrls.length === 0) {
          setError('未能生成预览帧');
          setCanvasBuffer(null);
          return;
        }

        const startTime = Number.isFinite(res.start_time) ? res.start_time : segmentStart;
        const fpsValue = Number.isFinite(res.fps) && res.fps > 0 ? res.fps : CANVAS_FPS;
        const initialTime = typeof seekTime === 'number' ? seekTime : startTime;
        const initialIndex = Math.max(
          0,
          Math.min(frameUrls.length - 1, Math.floor((initialTime - startTime) * fpsValue))
        );

        for (const u of frameUrls.slice(0, Math.min(8, frameUrls.length))) {
          const cache = canvasImageCacheRef.current;
          if (cache.has(u)) continue;
          const img = new Image();
          img.src = u;
          cache.set(u, img);
        }

        setCanvasBuffer({ startTime, fps: fpsValue, frameUrls });
        setCanvasFrameIndex(initialIndex);
        setCanvasSeekTime(startTime + initialIndex / fpsValue);
      } catch (e) {
        setError(e instanceof Error ? e.message : '预取预览帧失败');
      } finally {
        canvasInFlightRef.current = false;
        setIsCanvasRendering(false);
      }
    },
    [isTauriEnv, resolveVideoSrc]
  );

  const appendCanvasSegment = useCallback(
    async (path: string, segmentStart: number) => {
      if (!isTauriEnv()) return;
      if (!path) return;
      if (canvasInFlightRef.current) return;
      const buf = canvasBufferRef.current;
      if (!buf) return;

      const expectedStart = buf.startTime + buf.frameUrls.length / buf.fps;
      if (Math.abs(expectedStart - segmentStart) > 0.5) return;

      canvasInFlightRef.current = true;
      try {
        const res = await invoke<{
          start_time: number;
          fps: number;
          frames: Array<{ image_path: string; timestamp: number }>;
        }>('prefetch_preview_frames', {
          path,
          start_time: segmentStart,
          duration: CANVAS_SEGMENT_SECONDS,
          fps: CANVAS_FPS,
          width: 960
        });

        const urls = await Promise.all(res.frames.map((f) => resolveVideoSrc(f.image_path)));
        const moreUrls = urls.filter((u): u is string => !!u);
        if (moreUrls.length === 0) return;

        const maxFrames = Math.max(1, Math.floor(CANVAS_MAX_BUFFER_SECONDS * (buf.fps || CANVAS_FPS)));
        const currentIndex = canvasFrameIndexRef.current;

        let drop = 0;
        const newLen = buf.frameUrls.length + moreUrls.length;
        if (newLen > maxFrames) {
          drop = Math.min(newLen - maxFrames, currentIndex);
        }

        setCanvasBuffer((prev) => {
          if (!prev) return prev;
          if (Math.abs(prev.startTime + prev.frameUrls.length / prev.fps - segmentStart) > 0.5) return prev;
          const merged = [...prev.frameUrls, ...moreUrls];
          const trimmed = drop > 0 ? merged.slice(drop) : merged;
          const nextStart = prev.startTime + drop / prev.fps;
          return { ...prev, startTime: nextStart, frameUrls: trimmed };
        });
        if (drop > 0) {
          setCanvasFrameIndex((i) => Math.max(0, i - drop));
        }
      } catch {
        // ignore
      } finally {
        canvasInFlightRef.current = false;
      }
    },
    [isTauriEnv, resolveVideoSrc]
  );

  const seekCanvasTo = useCallback(
    async (path: string, time: number) => {
      const t = Math.max(0, time);
      setIsCanvasPlaying(false);
      setCanvasSeekTime(t);
      const buf = canvasBufferRef.current;
      if (buf) {
        const end = buf.startTime + buf.frameUrls.length / buf.fps;
        if (t >= buf.startTime && t <= end) {
          const idx = Math.max(0, Math.min(buf.frameUrls.length - 1, Math.floor((t - buf.startTime) * buf.fps)));
          setCanvasFrameIndex(idx);
          return;
        }
      }
      await loadCanvasSegment(path, Math.max(0, t - 0.1), t);
    },
    [loadCanvasSegment]
  );

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
      setError('无法加载视频信息');
    } finally {
      setIsLoading(false);
    }
  }, [applyVideoSource]);

  // 当视频路径改变时加载视频信息
  useEffect(() => {
    if (videoPath) {
      setPreviewMode('original');
      setPlayerTab('video');
      setCanvasSeekTime(0);
      setCanvasBuffer(null);
      setCanvasFrameIndex(0);
      canvasImageCacheRef.current.clear();
      setIsCanvasPlaying(false);
      loadVideoInfo(videoPath);
    } else {
      setVideoInfo(null);
      setProcessedVideoPath(null);
      setIsProcessing(false); // Reset processing state when video changes
      setVideoSrc('');
    }
  }, [videoPath, loadVideoInfo]);

  // 应用LUT处理已移至App.tsx中的handleProcessVideo函数

  useEffect(() => {
    if (!videoPath) return;
    const activePath = previewMode === 'processed' ? (processedVideoPath ?? videoPath) : videoPath;
    if (playerTab === 'ffplay') {
      startFfplay(activePath);
      return;
    }
    stopFfplay();
    if (playerTab === 'video') {
      applyVideoSource(activePath);
      return;
    }
    setVideoSrc('');
    void seekCanvasTo(activePath, canvasSeekTime);
  }, [applyVideoSource, canvasSeekTime, playerTab, previewMode, processedVideoPath, seekCanvasTo, startFfplay, stopFfplay, videoPath]);

  useEffect(() => {
    return () => {
      void stopFfplay();
    };
  }, [stopFfplay]);

  useEffect(() => {
    if (!videoPath) return;
    if (playerTab !== 'canvas') return;
    if (canvasPlaybackTimerRef.current) {
      window.clearInterval(canvasPlaybackTimerRef.current);
      canvasPlaybackTimerRef.current = null;
    }
    if (!isCanvasPlaying) return;
    const fps = canvasBuffer?.fps ?? CANVAS_FPS;
    const interval = window.setInterval(() => {
      setCanvasFrameIndex((prev) => {
        const buf = canvasBufferRef.current;
        if (!buf) return prev;
        const next = prev + 1;
        if (next >= buf.frameUrls.length) {
          window.setTimeout(() => setIsCanvasPlaying(false), 0);
          return prev;
        }
        return next;
      });
    }, 1000 / fps);
    canvasPlaybackTimerRef.current = interval;
    return () => {
      if (canvasPlaybackTimerRef.current) {
        window.clearInterval(canvasPlaybackTimerRef.current);
        canvasPlaybackTimerRef.current = null;
      }
    };
  }, [canvasBuffer?.fps, isCanvasPlaying, playerTab, videoPath]);

  useEffect(() => {
    if (!videoPath) return;
    if (playerTab !== 'canvas') return;
    const buf = canvasBuffer;
    if (!buf) return;
    const url = buf.frameUrls[canvasFrameIndex];
    if (!url) return;
    void drawCanvasUrl(url).catch((e) => {
      setError(e instanceof Error ? e.message : '渲染预览帧失败');
    });
  }, [canvasBuffer, canvasFrameIndex, drawCanvasUrl, playerTab, videoPath]);

  useEffect(() => {
    if (!videoPath) return;
    if (playerTab !== 'canvas') return;
    const buf = canvasBuffer;
    if (!buf) return;
    const t = buf.startTime + canvasFrameIndex / buf.fps;
    if (Number.isFinite(t)) setCanvasSeekTime(t);
    if (!isCanvasPlaying) return;
    const remaining = buf.frameUrls.length - canvasFrameIndex;
    if (remaining > Math.ceil(buf.fps)) return;
    const activePath = previewMode === 'processed' ? (processedVideoPath ?? videoPath) : videoPath;
    const nextStart = buf.startTime + buf.frameUrls.length / buf.fps;
    void appendCanvasSegment(activePath, nextStart);
  }, [appendCanvasSegment, canvasBuffer, canvasFrameIndex, isCanvasPlaying, playerTab, previewMode, processedVideoPath, videoPath]);

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
        {videoPath && (
          <div className="preview-controls">
            <button
              className={`mode-button ${playerTab === 'video' ? 'active' : ''}`}
              onClick={() => setPlayerTab('video')}
            >
              内置
            </button>
            <button
              className={`mode-button ${playerTab === 'canvas' ? 'active' : ''}`}
              onClick={() => setPlayerTab('canvas')}
            >
              Canvas
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

        {videoPath && !isLoading && !error && playerTab === 'canvas' && (
          <div style={{ width: '100%' }}>
            <div className="video-container">
              <canvas ref={canvasRef} className="video-player" />
              <div className="video-overlay">
                <div className="preview-mode-indicator">
                  {isCanvasRendering ? '渲染中…' : 'Canvas 预览'}
                </div>
              </div>
            </div>
            <div className="action-buttons" style={{ marginTop: 12 }}>
              <button
                className="apply-button"
                onClick={() => setIsCanvasPlaying(v => !v)}
                disabled={isCanvasRendering}
              >
                {isCanvasPlaying ? '暂停' : '播放'}
              </button>
              <button
                className="download-button"
                onClick={() => {
                  setIsCanvasPlaying(false);
                  if (canvasSeekDebounceRef.current) {
                    window.clearTimeout(canvasSeekDebounceRef.current);
                    canvasSeekDebounceRef.current = null;
                  }
                  const path = previewMode === 'processed' ? (processedVideoPath ?? videoPath) : videoPath;
                  void seekCanvasTo(path, 0);
                }}
              >
                复位
              </button>
            </div>
            <div style={{ padding: '0 12px', marginTop: 12 }}>
              <input
                type="range"
                min={0}
                max={videoInfo?.duration ?? 0}
                step={0.04}
                value={canvasSeekTime}
                onChange={(e) => {
                  setIsCanvasPlaying(false);
                  const t = Number(e.target.value);
                  setCanvasSeekTime(t);
                  if (canvasSeekDebounceRef.current) {
                    window.clearTimeout(canvasSeekDebounceRef.current);
                  }
                  const path = previewMode === 'processed' ? (processedVideoPath ?? videoPath) : videoPath;
                  canvasSeekDebounceRef.current = window.setTimeout(() => {
                    void seekCanvasTo(path, t);
                  }, 150);
                }}
                style={{ width: '100%' }}
                disabled={!videoInfo?.duration}
              />
              <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 6, color: '#6b7280', fontSize: '0.85rem', fontWeight: 500 }}>
                <span>{formatTime(canvasSeekTime)}</span>
                <span>{videoInfo?.duration ? formatTime(videoInfo.duration) : '--:--'}</span>
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
