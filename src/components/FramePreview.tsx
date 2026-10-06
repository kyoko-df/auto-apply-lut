import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowLeftRight,
  ChevronLeft,
  ChevronRight,
  Film,
  LoaderCircle,
  Maximize2,
  RotateCcw,
  ScanLine,
} from "lucide-react";
import ImageComparison from "./ImageComparison";
import type { VideoClip, PreviewFrame } from "../workspace/types";

export const timecode = (seconds = 0) => {
  const total = Math.max(0, Math.floor(seconds));
  return `${Math.floor(total / 60)
    .toString()
    .padStart(2, "0")}:${(total % 60).toString().padStart(2, "0")}`;
};

export default function FramePreview({
  clip,
  isDesktop,
  onImport,
  quality = "fast",
  inputColorSpace = "auto",
}: {
  clip?: VideoClip;
  isDesktop: boolean;
  onImport: () => void;
  quality?: "fast" | "accurate";
  inputColorSpace?: string;
}) {
  const [clientId] = useState(() => `workspace-${crypto.randomUUID()}`);
  const [time, setTime] = useState(0);
  const [mode, setMode] = useState<"split" | "original" | "graded">("split");
  const [split, setSplit] = useState(50);
  const [frame, setFrame] = useState<PreviewFrame | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  const viewport = useRef<HTMLDivElement>(null);
  const duration = clip?.info?.duration ?? 0;
  const fps = clip?.info?.fps || 25;
  const maxTime = Math.max(0, duration - 1 / fps);
  const timeRef = useRef(0);
  const pathRef = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (pathRef.current !== clip?.path) {
      pathRef.current = clip?.path;
      timeRef.current = 0;
      setTime(0);
      setFrame(null);
      setError("");
    }
  }, [clip?.path]);

  useEffect(() => {
    if (!clip || !isDesktop) {
      setBusy(false);
      if (isDesktop)
        void invoke("cancel_video_preview", { clientId }).catch(() => {});
      return;
    }
    let stale = false;
    setBusy(true);
    const timer = window.setTimeout(async () => {
      try {
        const result = await invoke<PreviewFrame>("generate_video_preview", {
          request: {
            video_path: clip.path,
            time_seconds: timeRef.current,
            lut_path: clip.lutPath,
            intensity: clip.intensity / 100,
            max_width: 1280,
            client_id: clientId,
            quality,
            input_color_space: inputColorSpace,
          },
        });
        if (!stale) {
          setFrame(result);
          setError("");
        }
      } catch (e) {
        if (!stale) setError(String(e));
      } finally {
        if (!stale) setBusy(false);
      }
    }, 180);
    return () => {
      stale = true;
      window.clearTimeout(timer);
    };
  }, [
    clip?.path,
    clip?.lutPath,
    clip?.intensity,
    time,
    isDesktop,
    retry,
    quality,
    inputColorSpace,
  ]);

  useEffect(
    () => () => {
      if (isDesktop)
        void invoke("cancel_video_preview", { clientId }).catch(() => {});
    },
    [isDesktop, clientId]
  );
  const seek = (value: number) => {
    const next = Math.min(maxTime, Math.max(0, value));
    timeRef.current = next;
    setTime(next);
  };
  const fullscreen = () => {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void viewport.current?.requestFullscreen?.();
  };

  return (
    <section className="preview-panel" aria-label="视频对比预览">
      <div className="panel-toolbar">
        <div className="panel-title">
          <ScanLine size={15} />
          <span>画面预览</span>
          {clip && (
            <span className="muted toolbar-filename" title={clip.name}>
              / {clip.name}
            </span>
          )}
        </div>
        <div className="segmented" aria-label="预览模式">
          <button
            aria-pressed={mode === "original"}
            onClick={() => setMode("original")}
          >
            原片
          </button>
          <button
            aria-pressed={mode === "split"}
            onClick={() => setMode("split")}
          >
            <ArrowLeftRight size={12} /> 对比
          </button>
          <button
            aria-pressed={mode === "graded"}
            onClick={() => setMode("graded")}
          >
            调色后
          </button>
        </div>
      </div>
      <div className={`preview-stage ${clip ? "has-clip" : ""}`} ref={viewport}>
        {!clip ? (
          <div className="empty-preview">
            <div className="frame-mark" aria-hidden="true">
              <div className="frame-mark-inner">
                <Film size={38} strokeWidth={1} />
              </div>
              <span className="frame-corner tl" />
              <span className="frame-corner tr" />
              <span className="frame-corner bl" />
              <span className="frame-corner br" />
            </div>
            <span className="eyebrow">YOUR NEXT LOOK STARTS HERE</span>
            <h1>让每一帧，风格一致。</h1>
            <p>导入视频，选择 LUT，预览并批量导出。</p>
            <button className="button primary" onClick={onImport}>
              <Film size={15} /> 导入视频 <kbd>⌘ O</kbd>
            </button>
            <span className="empty-formats">MP4 · MOV · MKV · AVI · WEBM</span>
          </div>
        ) : (
          <>
            {frame && !error && (
              <ImageComparison
                original={frame.original_image}
                processed={frame.processed_image}
                mode={mode}
                split={split}
                onSplit={setSplit}
                originalLabel="原片"
                originalAlt="原始视频帧"
                gradedAlt="应用 LUT 后的视频帧"
                gradedLabel={clip.lutPath ? "LUT 调色" : "原色"}
              />
            )}
            {!frame && !error && (
              <div className="preview-message">
                <LoaderCircle className="spin" size={24} />
                <p>正在读取视频画面</p>
              </div>
            )}
            {error && (
              <div className="preview-message error-message">
                <Film size={26} />
                <strong>暂时无法生成预览</strong>
                <p>{error}</p>
                <button
                  className="button secondary"
                  onClick={() => setRetry((n) => n + 1)}
                >
                  <RotateCcw size={14} />
                  重新加载
                </button>
              </div>
            )}
            {busy && frame && (
              <span className="preview-loading">
                <LoaderCircle size={12} className="spin" />
                更新画面
              </span>
            )}
            <div className="preview-bottom-label">
              <span>
                {clip.info?.width && clip.info?.height
                  ? `${clip.info.width} × ${clip.info.height}`
                  : "读取规格中"}
              </span>
              <span>
                帧预览 ·{" "}
                {clip.info?.fps
                  ? `${Number(clip.info.fps.toFixed(2))} fps`
                  : "—"}
              </span>
            </div>
          </>
        )}
      </div>
      <div className="transport">
        <div className="transport-time">
          <span>{timecode(time)}</span>
          <span className="muted">/ {timecode(duration)}</span>
        </div>
        <input
          className="timeline"
          type="range"
          aria-label="预览时间点"
          min="0"
          max={maxTime || 1}
          step={1 / fps}
          value={time}
          disabled={!clip || !duration}
          onChange={(e) => seek(Number(e.target.value))}
        />
        <button
          className="icon-button"
          aria-label="上一帧"
          disabled={!clip || !duration}
          onClick={() => seek(time - 1 / fps)}
        >
          <ChevronLeft size={17} />
        </button>
        <button
          className="icon-button"
          aria-label="下一帧"
          disabled={!clip || !duration}
          onClick={() => seek(time + 1 / fps)}
        >
          <ChevronRight size={17} />
        </button>
        <button
          className="icon-button"
          aria-label="全屏预览"
          disabled={!clip}
          onClick={fullscreen}
        >
          <Maximize2 size={15} />
        </button>
      </div>
      <div className="preview-footnote">
        <span className="tiny-dot" />
        {clip
          ? "拖动时间轴查看任意帧，拖动分割线对比调色效果"
          : "本地处理 · 原始素材始终保留"}
        <span>
          {quality === "accurate"
            ? "准确预览 · 先调色后缩放"
            : "快速预览 · 先缩放后调色"}
        </span>
      </div>
    </section>
  );
}
