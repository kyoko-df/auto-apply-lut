import { errorText, errorMessage } from "../workspace/model";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
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
  const { t } = useTranslation();
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
        if (!stale) setError(errorText(e));
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
    <section className="preview-panel" aria-label={t("previewVideo.aria")}>
      <div className="panel-toolbar">
        <div className="panel-title">
          <ScanLine size={15} />
          <span>{t("previewVideo.title")}</span>
          {clip && (
            <span className="muted toolbar-filename" title={clip.name}>
              / {clip.name}
            </span>
          )}
        </div>
        <div className="segmented" aria-label={t("previewVideo.modeLabel")}>
          <button
            aria-pressed={mode === "original"}
            onClick={() => setMode("original")}
          >
            {t("previewVideo.original")}
          </button>
          <button
            aria-pressed={mode === "split"}
            onClick={() => setMode("split")}
          >
            <ArrowLeftRight size={12} /> {t("previewVideo.compare")}
          </button>
          <button
            aria-pressed={mode === "graded"}
            onClick={() => setMode("graded")}
          >
            {t("previewVideo.graded")}
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
            <h1>{t("previewVideo.heroTitle")}</h1>
            <p>{t("previewVideo.heroSub")}</p>
            <button className="button primary" onClick={onImport}>
              <Film size={15} /> {t("previewVideo.importVideos")} <kbd>⌘ O</kbd>
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
                originalLabel={t("previewVideo.original")}
                originalAlt={t("previewVideo.originalAlt")}
                gradedAlt={t("previewVideo.gradedAlt")}
                gradedLabel={
                  clip.lutPath
                    ? t("previewVideo.lutGraded")
                    : t("previewVideo.noLutLabel")
                }
              />
            )}
            {!frame && !error && (
              <div className="preview-message">
                <LoaderCircle className="spin" size={24} />
                <p>{t("previewVideo.loading")}</p>
              </div>
            )}
            {error && (
              <div className="preview-message error-message">
                <Film size={26} />
                <strong>{t("previewVideo.previewFailed")}</strong>
                <p>{errorMessage(error)}</p>
                <button
                  className="button secondary"
                  onClick={() => setRetry((n) => n + 1)}
                >
                  <RotateCcw size={14} />
                  {t("previewVideo.reload")}
                </button>
              </div>
            )}
            {busy && frame && (
              <span className="preview-loading">
                <LoaderCircle size={12} className="spin" />
                {t("previewVideo.refresh")}
              </span>
            )}
            <div className="preview-bottom-label">
              <span>
                {clip.info?.width && clip.info?.height
                  ? `${clip.info.width} × ${clip.info.height}`
                  : t("previewVideo.readingSpecs")}
              </span>
              <span>
                {t("previewVideo.framePreview")}{" "}
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
          aria-label={t("previewVideo.ariaSeek")}
          min="0"
          max={maxTime || 1}
          step={1 / fps}
          value={time}
          disabled={!clip || !duration}
          onChange={(e) => seek(Number(e.target.value))}
        />
        <button
          className="icon-button"
          aria-label={t("previewVideo.ariaPrev")}
          disabled={!clip || !duration}
          onClick={() => seek(time - 1 / fps)}
        >
          <ChevronLeft size={17} />
        </button>
        <button
          className="icon-button"
          aria-label={t("previewVideo.ariaNext")}
          disabled={!clip || !duration}
          onClick={() => seek(time + 1 / fps)}
        >
          <ChevronRight size={17} />
        </button>
        <button
          className="icon-button"
          aria-label={t("previewVideo.ariaFullscreen")}
          disabled={!clip}
          onClick={fullscreen}
        >
          <Maximize2 size={15} />
        </button>
      </div>
      <div className="preview-footnote">
        <span className="tiny-dot" />
        {clip ? t("previewVideo.hintDesktop") : t("previewVideo.hintBrowser")}
        <span>
          {quality === "accurate"
            ? t("previewVideo.accurateNote")
            : t("previewVideo.fastNote")}
        </span>
      </div>
    </section>
  );
}
