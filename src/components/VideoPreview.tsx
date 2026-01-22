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
const CANVAS_SEGMENT_SECONDS = 6;
const CANVAS_MAX_BUFFER_SECONDS = 45;
const CANVAS_FRAME_WIDTH = 640;
const CANVAS_DECODE_AHEAD_SECONDS = 3;
const CANVAS_MAX_BITMAP_CACHE = 240;

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
  const canvasPrefetchSeqRef = useRef(0);
  const canvasBufferRef = useRef<CanvasBuffer | null>(null);
  const canvasFrameIndexRef = useRef(0);
  const canvasImageCacheRef = useRef<Map<string, HTMLImageElement>>(new Map());
  const canvasBitmapCacheRef = useRef<Map<string, ImageBitmap>>(new Map());
  const canvasBitmapPendingRef = useRef<Map<string, Promise<ImageBitmap | null>>>(new Map());
  const canvasDrawInFlightRef = useRef(false);
  const canvasRafRef = useRef<number | null>(null);
  const canvasPlayStartPerfRef = useRef(0);
  const canvasPlayStartTimeRef = useRef(0);
  const canvasLastDrawnIndexRef = useRef(-1);
  const canvasLastUiUpdatePerfRef = useRef(0);
  const canvasActivePathRef = useRef<string>('');
  const canvasSeekDebounceRef = useRef<number | null>(null);

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

  const drawBitmapToCanvas = useCallback((bmp: ImageBitmap) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const w = bmp.width;
    const h = bmp.height;
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(bmp, 0, 0, canvas.width, canvas.height);
  }, []);

  useEffect(() => {
    canvasBufferRef.current = canvasBuffer;
  }, [canvasBuffer]);

  useEffect(() => {
    canvasFrameIndexRef.current = canvasFrameIndex;
  }, [canvasFrameIndex]);

  const pruneBitmapCache = useCallback(() => {
    const cache = canvasBitmapCacheRef.current;
    while (cache.size > CANVAS_MAX_BITMAP_CACHE) {
      const firstKey = cache.keys().next().value as string | undefined;
      if (!firstKey) break;
      const bmp = cache.get(firstKey);
      if (bmp) bmp.close();
      cache.delete(firstKey);
    }
  }, []);

  const decodeToBitmap = useCallback(async (url: string): Promise<ImageBitmap | null> => {
    if (typeof window === 'undefined') return null;
    if (typeof (window as any).createImageBitmap !== 'function') return null;
    try {
      const imgCache = canvasImageCacheRef.current;
      let img = imgCache.get(url);
      if (!img) {
        img = new Image();
        img.src = url;
        imgCache.set(url, img);
      }
      if (typeof img.decode === 'function') {
        try {
          await img.decode();
        } catch {
          // ignore
        }
      } else if (!img.complete) {
        await new Promise<void>((resolve) => {
          img!.onload = () => resolve();
          img!.onerror = () => resolve();
        });
      }
      const bmp = await createImageBitmap(img);
      return bmp;
    } catch {
      return null;
    }
  }, []);

  const getFrameDrawable = useCallback(
    async (url: string): Promise<{ bmp?: ImageBitmap; img?: HTMLImageElement } | null> => {
      const bmpCache = canvasBitmapCacheRef.current;
      const bmp = bmpCache.get(url);
      if (bmp) {
        bmpCache.delete(url);
        bmpCache.set(url, bmp);
        return { bmp };
      }

      const pending = canvasBitmapPendingRef.current.get(url);
      if (pending) {
        const b = await pending;
        if (b) return { bmp: b };
      } else {
        const p = decodeToBitmap(url);
        canvasBitmapPendingRef.current.set(url, p);
        const b = await p;
        canvasBitmapPendingRef.current.delete(url);
        if (b) {
          bmpCache.set(url, b);
          pruneBitmapCache();
          return { bmp: b };
        }
      }

      const imgCache = canvasImageCacheRef.current;
      let img = imgCache.get(url);
      if (!img) {
        img = new Image();
        img.src = url;
        imgCache.set(url, img);
      }
      if (!img.complete) {
        await new Promise<void>((resolve) => {
          img!.onload = () => resolve();
          img!.onerror = () => resolve();
        });
      }
      return { img };
    },
    [decodeToBitmap, pruneBitmapCache]
  );

  const drawCanvasUrl = useCallback(
    async (url: string) => {
      if (!url) return;
      if (canvasDrawInFlightRef.current) return;
      canvasDrawInFlightRef.current = true;
      try {
        const drawable = await getFrameDrawable(url);
        if (!drawable) return;
        if (drawable.bmp) {
          drawBitmapToCanvas(drawable.bmp);
          return;
        }
        if (drawable.img) {
          drawImageToCanvas(drawable.img);
        }
      } finally {
        canvasDrawInFlightRef.current = false;
      }
    },
    [drawBitmapToCanvas, drawImageToCanvas, getFrameDrawable]
  );

  const stopCanvasPlayback = useCallback(() => {
    if (canvasRafRef.current) {
      window.cancelAnimationFrame(canvasRafRef.current);
      canvasRafRef.current = null;
    }
  }, []);

  const drawCanvasFrameAtIndex = useCallback(
    async (index: number, waitForDecode: boolean) => {
      const buf = canvasBufferRef.current;
      if (!buf) return;
      const url = buf.frameUrls[index];
      if (!url) return;

      if (!waitForDecode) {
        const cached = canvasBitmapCacheRef.current.get(url);
        if (cached) {
          canvasBitmapCacheRef.current.delete(url);
          canvasBitmapCacheRef.current.set(url, cached);
          drawBitmapToCanvas(cached);
          canvasLastDrawnIndexRef.current = index;
          return;
        }
        return;
      }

      const drawable = await getFrameDrawable(url);
      if (!drawable) return;
      if (drawable.bmp) {
        drawBitmapToCanvas(drawable.bmp);
        canvasLastDrawnIndexRef.current = index;
        return;
      }
      if (drawable.img) {
        drawImageToCanvas(drawable.img);
        canvasLastDrawnIndexRef.current = index;
      }
    },
    [drawBitmapToCanvas, drawImageToCanvas, getFrameDrawable]
  );

  const predecodeFrames = useCallback(
    async (urls: string[], startIndex: number, count: number) => {
      const end = Math.min(urls.length, startIndex + count);
      for (let i = startIndex; i < end; i++) {
        const u = urls[i];
        if (!u) continue;
        if (canvasBitmapCacheRef.current.has(u)) continue;
        if (canvasBitmapPendingRef.current.has(u)) continue;
        const p = decodeToBitmap(u);
        canvasBitmapPendingRef.current.set(u, p);
        void p.then((bmp) => {
          canvasBitmapPendingRef.current.delete(u);
          if (!bmp) return;
          canvasBitmapCacheRef.current.set(u, bmp);
          pruneBitmapCache();
        });
      }
    },
    [decodeToBitmap, pruneBitmapCache]
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
          startTime: segmentStart,
          duration: CANVAS_SEGMENT_SECONDS,
          fps: CANVAS_FPS,
          width: CANVAS_FRAME_WIDTH
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

        setCanvasBuffer({ startTime, fps: fpsValue, frameUrls });
        setCanvasFrameIndex(initialIndex);
        setCanvasSeekTime(startTime + initialIndex / fpsValue);
        void predecodeFrames(frameUrls, initialIndex, Math.ceil(fpsValue * CANVAS_DECODE_AHEAD_SECONDS));
      } catch (e) {
        setError(errorToMessage(e, '预取预览帧失败'));
      } finally {
        canvasInFlightRef.current = false;
        setIsCanvasRendering(false);
      }
    },
    [errorToMessage, isTauriEnv, predecodeFrames, resolveVideoSrc]
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
          startTime: segmentStart,
          duration: CANVAS_SEGMENT_SECONDS,
          fps: CANVAS_FPS,
          width: CANVAS_FRAME_WIDTH
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

        if (drop > 0) {
          const dropped = buf.frameUrls.slice(0, drop);
          for (const u of dropped) {
            const bmp = canvasBitmapCacheRef.current.get(u);
            if (bmp) bmp.close();
            canvasBitmapCacheRef.current.delete(u);
            canvasBitmapPendingRef.current.delete(u);
            canvasImageCacheRef.current.delete(u);
          }
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
      setCanvasSeekTime(0);
      setCanvasBuffer(null);
      setCanvasFrameIndex(0);
      canvasImageCacheRef.current.clear();
      for (const bmp of canvasBitmapCacheRef.current.values()) {
        bmp.close();
      }
      canvasBitmapCacheRef.current.clear();
      canvasBitmapPendingRef.current.clear();
      setIsCanvasPlaying(false);
      stopCanvasPlayback();
      loadVideoInfo(videoPath);
    } else {
      setVideoInfo(null);
      setProcessedVideoPath(null);
      setIsProcessing(false); // Reset processing state when video changes
      setVideoSrc('');
    }
  }, [videoPath, loadVideoInfo, stopCanvasPlayback]);

  // 应用LUT处理已移至App.tsx中的handleProcessVideo函数

  useEffect(() => {
    if (!videoPath) return;
    const activePath = previewMode === 'processed' ? (processedVideoPath ?? videoPath) : videoPath;
    if (playerTab === 'ffplay') {
      setIsCanvasPlaying(false);
      stopCanvasPlayback();
      startFfplay(activePath);
      return;
    }
    stopFfplay();
    if (playerTab === 'video') {
      setIsCanvasPlaying(false);
      stopCanvasPlayback();
      applyVideoSource(activePath);
      return;
    }
    setVideoSrc('');
    void seekCanvasTo(activePath, canvasSeekTime);
  }, [
    applyVideoSource,
    canvasSeekTime,
    playerTab,
    previewMode,
    processedVideoPath,
    seekCanvasTo,
    startFfplay,
    stopCanvasPlayback,
    stopFfplay,
    videoPath
  ]);

  useEffect(() => {
    return () => {
      void stopFfplay();
    };
  }, [stopFfplay]);

  useEffect(() => {
    stopCanvasPlayback();
    if (!videoPath) return;
    if (playerTab !== 'canvas') return;
    if (!isCanvasPlaying) return;
    const buf0 = canvasBufferRef.current;
    if (!buf0) {
      setIsCanvasPlaying(false);
      return;
    }

    canvasActivePathRef.current = previewMode === 'processed' ? (processedVideoPath ?? videoPath) : videoPath;
    canvasPlayStartPerfRef.current = performance.now();
    canvasPlayStartTimeRef.current = buf0.startTime + canvasFrameIndexRef.current / buf0.fps;
    canvasLastUiUpdatePerfRef.current = 0;
    canvasLastDrawnIndexRef.current = -1;

    const tick = (now: number) => {
      const buf = canvasBufferRef.current;
      if (!buf) {
        setIsCanvasPlaying(false);
        stopCanvasPlayback();
        return;
      }

      const elapsed = (now - canvasPlayStartPerfRef.current) / 1000;
      const t = canvasPlayStartTimeRef.current + Math.max(0, elapsed);
      const idx = Math.max(0, Math.floor((t - buf.startTime) * buf.fps));

      if (idx >= buf.frameUrls.length) {
        setIsCanvasPlaying(false);
        stopCanvasPlayback();
        return;
      }

      if (idx !== canvasLastDrawnIndexRef.current) {
        void predecodeFrames(buf.frameUrls, idx, Math.ceil(buf.fps * CANVAS_DECODE_AHEAD_SECONDS));
        void drawCanvasFrameAtIndex(idx, false);
      }

      const remaining = buf.frameUrls.length - idx;
      if (remaining <= Math.ceil(buf.fps) && !canvasInFlightRef.current) {
        const nextStart = buf.startTime + buf.frameUrls.length / buf.fps;
        void appendCanvasSegment(canvasActivePathRef.current, nextStart);
      }

      if (now - canvasLastUiUpdatePerfRef.current > 200) {
        canvasLastUiUpdatePerfRef.current = now;
        setCanvasFrameIndex(idx);
        setCanvasSeekTime(t);
      } else {
        canvasFrameIndexRef.current = idx;
      }

      canvasRafRef.current = window.requestAnimationFrame(tick);
    };

    canvasRafRef.current = window.requestAnimationFrame(tick);
    return () => {
      stopCanvasPlayback();
    };
  }, [
    appendCanvasSegment,
    drawCanvasFrameAtIndex,
    isCanvasPlaying,
    playerTab,
    predecodeFrames,
    previewMode,
    processedVideoPath,
    stopCanvasPlayback,
    videoPath
  ]);

  useEffect(() => {
    if (!videoPath) return;
    if (playerTab !== 'canvas') return;
    if (isCanvasPlaying) return;
    const buf = canvasBuffer;
    if (!buf) return;
    const url = buf.frameUrls[canvasFrameIndex];
    if (!url) return;
    void drawCanvasUrl(url).catch((e) => {
      setError(e instanceof Error ? e.message : '渲染预览帧失败');
    });
  }, [canvasBuffer, canvasFrameIndex, drawCanvasUrl, isCanvasPlaying, playerTab, videoPath]);

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
