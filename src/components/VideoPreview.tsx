import React, { useRef, useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './VideoPreview.css';

interface VideoPreviewProps {
  videoPath?: string;
  lutPaths?: string[];
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

type WebCodecsVideoDecoderConfig = {
  codec: string;
  codedWidth?: number;
  codedHeight?: number;
  description?: ArrayBuffer;
};

type WebCodecsVideoFrame = {
  timestamp: number;
  displayWidth: number;
  displayHeight: number;
  codedWidth: number;
  codedHeight: number;
  close: () => void;
};

const CANVAS_SEEK_STEP = 0.04;

const VideoPreview: React.FC<VideoPreviewProps> = ({
  videoPath,
  lutPaths,
  onProcessingStart,
  onProcessingComplete,
  onProcessingError
}) => {
  void lutPaths;
  void onProcessingStart;
  void onProcessingComplete;
  void onProcessingError;
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
  const isCanvasPlayingRef = useRef(false);
  const [canvasSeekTime, setCanvasSeekTime] = useState(0);
  const canvasSeekTimeRef = useRef(0);
  const [isCanvasPlaying, setIsCanvasPlaying] = useState(false);
  const [isCanvasRendering, setIsCanvasRendering] = useState(false);
  const canvasActivePathRef = useRef<string>('');
  const canvasSeekDebounceRef = useRef<number | null>(null);
  const canvasLastUiUpdatePerfRef = useRef(0);
  const canvasFetchAbortRef = useRef<AbortController | null>(null);
  const canvasDecodeSessionIdRef = useRef(0);
  const canvasDecoderConfigRef = useRef<WebCodecsVideoDecoderConfig | null>(null);
  const canvasMp4TimescaleRef = useRef<number>(0);
  const canvasMp4SamplesRef = useRef<
    Array<{
      is_sync: boolean;
      cts: number;
      dts: number;
      duration: number;
      data: Uint8Array;
    }>
  >([]);
  const canvasPlaybackRafIdRef = useRef<number | null>(null);
  const canvasPlaybackQueueRef = useRef<Array<{ frame: WebCodecsVideoFrame; timestampUs: number }>>([]);
  const canvasPlaybackDecodeDoneRef = useRef(false);
  const canvasPlaybackStartPerfRef = useRef<number | null>(null);
  const canvasPlaybackBaseTimestampUsRef = useRef<number | null>(null);

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

  useEffect(() => {
    isCanvasPlayingRef.current = isCanvasPlaying;
  }, [isCanvasPlaying]);

  useEffect(() => {
    canvasSeekTimeRef.current = canvasSeekTime;
  }, [canvasSeekTime]);

  useEffect(() => {
    if (playerTab === 'canvas') {
      setPlayerTab('video');
    }
  }, [playerTab]);

  const closeCanvasPlaybackFrameQueue = useCallback(() => {
    const q = canvasPlaybackQueueRef.current;
    while (q.length) {
      const item = q.shift();
      if (item) {
        try {
          item.frame.close();
        } catch {
          // ignore
        }
      }
    }
  }, []);

  const stopCanvasPlayback = useCallback(() => {
    canvasDecodeSessionIdRef.current += 1;
    if (canvasPlaybackRafIdRef.current !== null) {
      window.cancelAnimationFrame(canvasPlaybackRafIdRef.current);
      canvasPlaybackRafIdRef.current = null;
    }
    canvasPlaybackStartPerfRef.current = null;
    canvasPlaybackBaseTimestampUsRef.current = null;
    canvasPlaybackDecodeDoneRef.current = false;
    closeCanvasPlaybackFrameQueue();
  }, [closeCanvasPlaybackFrameQueue]);

  const resetCanvasPipeline = useCallback(() => {
    stopCanvasPlayback();
    if (canvasFetchAbortRef.current) {
      try {
        canvasFetchAbortRef.current.abort();
      } catch {
        // ignore
      }
      canvasFetchAbortRef.current = null;
    }
    canvasDecoderConfigRef.current = null;
    canvasMp4TimescaleRef.current = 0;
    canvasMp4SamplesRef.current = [];
    canvasActivePathRef.current = '';
  }, [stopCanvasPlayback]);

  const drawFrameToCanvas = useCallback((frame: WebCodecsVideoFrame) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const w = frame.displayWidth || frame.codedWidth;
    const h = frame.displayHeight || frame.codedHeight;
    if (!w || !h) return;
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    try {
      ctx.drawImage(frame as any, 0, 0, canvas.width, canvas.height);
    } catch {
      // ignore
    }
  }, []);

  const extractCodecDescription = useCallback((mp4boxFile: any, trackId: number, codec: string) => {
    const track = typeof mp4boxFile.getTrackById === 'function' ? mp4boxFile.getTrackById(trackId) : null;
    const entry = track?.trak?.mdia?.minf?.stbl?.stsd?.entries?.[0];
    const box =
      codec.startsWith('avc1') || codec.startsWith('avc3')
        ? entry?.avcC
        : codec.startsWith('hvc1') || codec.startsWith('hev1')
          ? entry?.hvcC
          : null;
    if (!box) return undefined;
    const written = typeof box.write === 'function' ? box.write() : box;
    if (written instanceof ArrayBuffer) {
      const u8 = new Uint8Array(written);
      const copy = new Uint8Array(u8.byteLength);
      copy.set(u8);
      return copy.buffer;
    }
    if (ArrayBuffer.isView(written)) {
      const view = written as ArrayBufferView;
      const u8 = new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
      const copy = new Uint8Array(u8.byteLength);
      copy.set(u8);
      return copy.buffer;
    }
    return undefined;
  }, []);

  const ensureCanvasPipelineReady = useCallback(
    async (path: string) => {
      if (!path) return false;
      if (canvasActivePathRef.current === path && canvasDecoderConfigRef.current && canvasMp4SamplesRef.current.length) {
        return true;
      }

      resetCanvasPipeline();
      canvasActivePathRef.current = path;
      setError(null);

      const ext = path.split('.').pop()?.toLowerCase();
      if (ext !== 'mp4' && ext !== 'm4v') {
        setError('Canvas(WebCodecs) 仅支持 MP4/M4V（请切换“内置”预览）');
        return false;
      }
      const hasWebCodecs = typeof (globalThis as any).VideoDecoder === 'function' && typeof (globalThis as any).EncodedVideoChunk === 'function';
      if (!hasWebCodecs) {
        setError('当前运行环境不支持 WebCodecs（请切换“内置”预览）');
        return false;
      }
      if (videoInfo?.size && videoInfo.size > 700 * 1024 * 1024) {
        setError('该视频文件过大，Canvas(WebCodecs) 预览可能占用大量内存（请切换“内置”预览）');
        return false;
      }

      setIsCanvasRendering(true);
      try {
        const src = await resolveVideoSrc(path);
        if (!src) {
          setError('无法加载视频资源');
          return false;
        }
        const controller = new AbortController();
        canvasFetchAbortRef.current = controller;
        const resp = await fetch(src, { signal: controller.signal });
        if (!resp.ok) {
          setError(`加载视频失败（${resp.status}）`);
          return false;
        }
        const buf = await resp.arrayBuffer();

        const mp4boxModule: any = await import('mp4box');
        const MP4Box: any = mp4boxModule?.default ?? mp4boxModule;
        const mp4boxFile = MP4Box.createFile();

        let trackId: number | null = null;
        let expectedSamples = 0;
        let resolveSamplesReady: (() => void) | null = null;
        const samplesReady = new Promise<void>((resolve) => {
          resolveSamplesReady = resolve;
        });

        const ready = await new Promise<{
          trackId: number;
          codec: string;
          codedWidth: number;
          codedHeight: number;
          timescale: number;
          expectedSamples: number;
        }>((resolve, reject) => {
          mp4boxFile.onError = (e: any) => {
            reject(e);
          };
          mp4boxFile.onReady = (info: any) => {
            const videoTrack = info?.tracks?.find((t: any) => t?.video);
            if (!videoTrack?.id) {
              reject(new Error('未找到视频轨道'));
              return;
            }
            trackId = videoTrack.id;
            expectedSamples = Number(videoTrack.nb_samples ?? 0);
            resolve({
              trackId: videoTrack.id,
              codec: String(videoTrack.codec || ''),
              codedWidth: Number(videoTrack.video?.width ?? 0),
              codedHeight: Number(videoTrack.video?.height ?? 0),
              timescale: Number(videoTrack.timescale ?? 0),
              expectedSamples
            });
          };
          mp4boxFile.onSamples = (id: number, _user: any, samples: any[]) => {
            if (trackId === null || id !== trackId) return;
            for (const s of samples) {
              const data: Uint8Array = s?.data instanceof Uint8Array ? s.data : new Uint8Array(s?.data ?? []);
              canvasMp4SamplesRef.current.push({
                is_sync: !!s?.is_sync,
                cts: Number(s?.cts ?? 0),
                dts: Number(s?.dts ?? 0),
                duration: Number(s?.duration ?? 0),
                data
              });
            }
            if (expectedSamples > 0 && canvasMp4SamplesRef.current.length >= expectedSamples && resolveSamplesReady) {
              resolveSamplesReady();
              resolveSamplesReady = null;
            }
          };

          mp4boxFile.setExtractionOptions = mp4boxFile.setExtractionOptions || mp4boxFile.setExtractionOptions;
          const ab = buf.slice(0);
          (ab as any).fileStart = 0;
          mp4boxFile.appendBuffer(ab);
          mp4boxFile.flush();
        });

        const description = extractCodecDescription(mp4boxFile, ready.trackId, ready.codec);
        const cfg: WebCodecsVideoDecoderConfig = {
          codec: ready.codec,
          codedWidth: ready.codedWidth,
          codedHeight: ready.codedHeight,
          description
        };

        const isConfigSupportedFn = (globalThis as any).VideoDecoder?.isConfigSupported as
          | ((config: WebCodecsVideoDecoderConfig) => Promise<{ supported: boolean }>)
          | undefined;
        if (isConfigSupportedFn) {
          const support = await isConfigSupportedFn(cfg);
          if (!support?.supported) {
            setError('当前运行环境不支持该视频编码（请切换“内置”预览）');
            return false;
          }
        }

        canvasDecoderConfigRef.current = cfg;
        canvasMp4TimescaleRef.current = ready.timescale;

        mp4boxFile.setExtractionOptions(ready.trackId, null, { nbSamples: 1000 });
        mp4boxFile.start();
        mp4boxFile.flush();

        if (ready.expectedSamples > 0) {
          await Promise.race([samplesReady, new Promise<void>((resolve) => window.setTimeout(resolve, 3000))]);
        }

        if (!canvasMp4SamplesRef.current.length) {
          setError('Canvas(WebCodecs) 未能解析到视频帧样本');
          return false;
        }
        return true;
      } catch (e) {
        setError(errorToMessage(e, 'Canvas(WebCodecs) 初始化失败'));
        return false;
      } finally {
        setIsCanvasRendering(false);
      }
    },
    [errorToMessage, extractCodecDescription, resetCanvasPipeline, resolveVideoSrc, videoInfo?.size]
  );

  const seekCanvasTo = useCallback(
    async (path: string, time: number) => {
      const t0 = Math.max(0, time);
      setIsCanvasPlaying(false);
      stopCanvasPlayback();
      setIsCanvasRendering(true);
      try {
        const ok = await ensureCanvasPipelineReady(path);
        if (!ok) {
          return;
        }
        const cfg = canvasDecoderConfigRef.current;
        const timescale = canvasMp4TimescaleRef.current;
        const samples = canvasMp4SamplesRef.current;
        if (!cfg || !timescale || !samples.length) {
          setError('Canvas(WebCodecs) 预览未就绪');
          return;
        }

        const t = videoInfo?.duration ? Math.min(t0, videoInfo.duration) : t0;
        const targetTs = Math.round(t * timescale);
        let targetIndex = samples.findIndex((s) => s.cts >= targetTs);
        if (targetIndex < 0) targetIndex = samples.length - 1;
        let keyIndex = targetIndex;
        while (keyIndex > 0 && !samples[keyIndex].is_sync) keyIndex -= 1;
        while (keyIndex > 0 && !samples[keyIndex].is_sync) keyIndex -= 1;

        const sessionId = ++canvasDecodeSessionIdRef.current;
        let lastFrame: WebCodecsVideoFrame | null = null;
        const decoder = new (globalThis as any).VideoDecoder({
          output: (frame: WebCodecsVideoFrame) => {
            if (sessionId !== canvasDecodeSessionIdRef.current) {
              frame.close();
              return;
            }
            if (lastFrame) {
              try {
                (lastFrame as any).close?.();
              } catch {
                // ignore
              }
            }
            lastFrame = frame;
          },
          error: (err: any) => {
            void err;
          }
        });
        decoder.configure(cfg);

        for (let i = keyIndex; i <= targetIndex; i++) {
          const s = samples[i];
          const timestampUs = Math.max(0, Math.round((s.cts / timescale) * 1_000_000));
          const durationUs = Math.max(0, Math.round((s.duration / timescale) * 1_000_000));
          const chunk = new (globalThis as any).EncodedVideoChunk({
            type: s.is_sync ? 'key' : 'delta',
            timestamp: timestampUs,
            duration: durationUs,
            data: s.data
          });
          decoder.decode(chunk);
        }
        await decoder.flush();
        decoder.close();

        if (lastFrame) {
          drawFrameToCanvas(lastFrame);
          try {
            (lastFrame as any).close?.();
          } catch {
            // ignore
          }
        }

        const actualTime = samples[targetIndex]?.cts ? samples[targetIndex].cts / timescale : t;
        setCanvasSeekTime(actualTime);
        canvasSeekTimeRef.current = actualTime;
      } catch (e) {
        setError(errorToMessage(e, 'Canvas(WebCodecs) 预览失败'));
      } finally {
        setIsCanvasRendering(false);
      }
    },
    [drawFrameToCanvas, ensureCanvasPipelineReady, errorToMessage, stopCanvasPlayback, videoInfo?.duration]
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
    void seekCanvasTo(activePath, canvasSeekTimeRef.current);
  }, [
    applyVideoSource,
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
    if (!videoPath) return;
    if (playerTab !== 'canvas') return;
    if (!isCanvasPlaying) {
      stopCanvasPlayback();
      return;
    }

    canvasActivePathRef.current = previewMode === 'processed' ? (processedVideoPath ?? videoPath) : videoPath;
    const activePath = canvasActivePathRef.current;

    let cancelled = false;
    setIsCanvasRendering(true);
    canvasLastUiUpdatePerfRef.current = 0;

    const start = async () => {
      try {
        const ok = await ensureCanvasPipelineReady(activePath);
        if (!ok) {
          setIsCanvasPlaying(false);
          return;
        }
        const cfg = canvasDecoderConfigRef.current;
        const timescale = canvasMp4TimescaleRef.current;
        const samples = canvasMp4SamplesRef.current;
        if (!cfg || !timescale || !samples.length) {
          setError('Canvas(WebCodecs) 预览未就绪');
          setIsCanvasPlaying(false);
          return;
        }

        const startTime = canvasSeekTimeRef.current;
        const startTs = Math.round(startTime * timescale);
        let startIndex = samples.findIndex((s) => s.cts >= startTs);
        if (startIndex < 0) startIndex = samples.length - 1;
        let keyIndex = startIndex;
        while (keyIndex > 0 && !samples[keyIndex].is_sync) keyIndex -= 1;
        const sessionId = ++canvasDecodeSessionIdRef.current;
        canvasPlaybackQueueRef.current = [];
        canvasPlaybackDecodeDoneRef.current = false;
        canvasPlaybackStartPerfRef.current = null;
        canvasPlaybackBaseTimestampUsRef.current = null;

        const decoder = new (globalThis as any).VideoDecoder({
          output: (frame: WebCodecsVideoFrame) => {
            if (sessionId !== canvasDecodeSessionIdRef.current) {
              frame.close();
              return;
            }
            const timestampUs = Number(frame.timestamp ?? 0);
            canvasPlaybackQueueRef.current.push({ frame, timestampUs });
          },
          error: (err: any) => {
            void err;
          }
        });
        decoder.configure(cfg);

        const render = () => {
          if (cancelled) return;
          if (sessionId !== canvasDecodeSessionIdRef.current) return;
          if (!isCanvasPlayingRef.current) return;

          const now = performance.now();
          const q = canvasPlaybackQueueRef.current;
          if (q.length && canvasPlaybackStartPerfRef.current === null) {
            canvasPlaybackStartPerfRef.current = now;
            canvasPlaybackBaseTimestampUsRef.current = q[0].timestampUs;
          }
          const startPerf = canvasPlaybackStartPerfRef.current;
          const baseTs = canvasPlaybackBaseTimestampUsRef.current;

          if (startPerf !== null && baseTs !== null) {
            while (q.length) {
              const item = q[0];
              const due = startPerf + (item.timestampUs - baseTs) / 1000;
              if (now + 1 < due) break;
              q.shift();
              drawFrameToCanvas(item.frame);
              try {
                item.frame.close();
              } catch {
                // ignore
              }
              if (now - canvasLastUiUpdatePerfRef.current > 120) {
                canvasLastUiUpdatePerfRef.current = now;
                const tNow = item.timestampUs / 1_000_000;
                canvasSeekTimeRef.current = tNow;
                setCanvasSeekTime(tNow);
              }
            }
          }

          if (canvasPlaybackDecodeDoneRef.current && !canvasPlaybackQueueRef.current.length) {
            setIsCanvasPlaying(false);
            return;
          }
          canvasPlaybackRafIdRef.current = window.requestAnimationFrame(render);
        };
        canvasPlaybackRafIdRef.current = window.requestAnimationFrame(render);

        for (let i = keyIndex; i < samples.length; i++) {
          if (cancelled) return;
          if (sessionId !== canvasDecodeSessionIdRef.current) return;
          while (decoder.decodeQueueSize > 8) {
            if (cancelled) return;
            if (sessionId !== canvasDecodeSessionIdRef.current) return;
            await new Promise<void>((resolve) => window.setTimeout(resolve, 10));
          }
          const s = samples[i];
          const timestampUs = Math.max(0, Math.round((s.cts / timescale) * 1_000_000));
          const durationUs = Math.max(0, Math.round((s.duration / timescale) * 1_000_000));
          const chunk = new (globalThis as any).EncodedVideoChunk({
            type: s.is_sync ? 'key' : 'delta',
            timestamp: timestampUs,
            duration: durationUs,
            data: s.data
          });
          decoder.decode(chunk);
        }
        await decoder.flush();
        decoder.close();
        canvasPlaybackDecodeDoneRef.current = true;
      } catch (e) {
        setError(errorToMessage(e, 'Canvas(WebCodecs) 播放失败'));
        setIsCanvasPlaying(false);
      } finally {
        setIsCanvasRendering(false);
      }
    };

    void start();

    return () => {
      cancelled = true;
      stopCanvasPlayback();
      setIsCanvasRendering(false);
    };
  }, [
    drawFrameToCanvas,
    ensureCanvasPipelineReady,
    errorToMessage,
    isCanvasPlaying,
    playerTab,
    previewMode,
    processedVideoPath,
    stopCanvasPlayback,
    videoPath
  ]);

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
                step={CANVAS_SEEK_STEP}
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
