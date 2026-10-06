import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Image as ImageIcon,
  LoaderCircle,
  RotateCcw,
  ScanLine,
} from "lucide-react";
import type {
  AlphaPolicy,
  PhotoClip,
  PhotoPreviewRequest,
  PhotoPreviewResponse,
} from "../workspace/types";
import ImageComparison, { type ComparisonMode } from "./ImageComparison";

export default function PhotoPreview({
  clip,
  isDesktop,
  onImport,
  quality,
  alphaPolicy,
  onRefreshMetadata,
}: {
  clip?: PhotoClip;
  isDesktop: boolean;
  onImport: () => void;
  quality: "fast" | "accurate";
  alphaPolicy: AlphaPolicy;
  onRefreshMetadata?: () => void;
}) {
  const [clientId] = useState(() => `photo-${crypto.randomUUID()}`);
  const [mode, setMode] = useState<ComparisonMode>("split");
  const [split, setSplit] = useState(50);
  const [zoom, setZoom] = useState<"fit" | "region">("fit");
  const [region, setRegion] = useState({ x: 0, y: 0 });
  const [frame, setFrame] = useState<{
    key: string;
    data: PhotoPreviewResponse;
  } | null>(null);
  const stage = useRef<HTMLDivElement>(null);
  const [visiblePixels, setVisiblePixels] = useState({
    width: 1024,
    height: 1024,
  });
  useEffect(() => {
    const element = stage.current;
    if (!element) return;
    const measure = () => {
      if (!element.clientWidth || !element.clientHeight) return;
      const ratio = window.devicePixelRatio || 1;
      setVisiblePixels({
        width: Math.max(1, Math.floor(element.clientWidth * ratio)),
        height: Math.max(1, Math.floor(element.clientHeight * ratio)),
      });
    };
    measure();
    const observer =
      typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(measure)
        : null;
    observer?.observe(element);
    window.addEventListener("resize", measure);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, []);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  const pointer = useRef<{
    x: number;
    y: number;
    left: number;
    top: number;
  } | null>(null);
  const width = clip?.info?.width ?? 0;
  const height = clip?.info?.height ?? 0;
  const regionWidth = Math.min(1024, width, visiblePixels.width);
  const regionHeight = Math.min(1024, height, visiblePixels.height);
  const viewport: PhotoPreviewRequest["viewport"] =
    zoom === "region" && width && height
      ? {
          kind: "region",
          x: Math.max(0, Math.min(width - regionWidth, Math.round(region.x))),
          y: Math.max(0, Math.min(height - regionHeight, Math.round(region.y))),
          width: regionWidth,
          height: regionHeight,
        }
      : { kind: "fit", max_edge: 1280 };
  const params = {
    photo_path: clip?.path ?? "",
    lut_path: clip?.lutPath ?? null,
    intensity: (clip?.intensity ?? 100) / 100,
    source_interpretation: clip?.sourceInterpretation ?? {
      mode: "embedded" as const,
    },
    lut_space: clip?.lutSpace ?? null,
    lut_fingerprint: clip?.lutFingerprint ?? null,
    viewport,
    quality: zoom === "region" ? ("accurate" as const) : quality,
    alpha_policy: alphaPolicy,
  };
  const key = JSON.stringify([
    clip?.id,
    params,
    clip?.info?.source_version,
    clip?.metadataError,
    retry,
  ]);
  const requestGeneration = useRef(0);
  useEffect(() => {
    const generation = ++requestGeneration.current;
    let stale = false;
    if (!clip || !isDesktop || clip.metadataError) {
      setBusy(false);
      return;
    }
    setBusy(true);
    setError("");
    const requestId = crypto.randomUUID();
    const timer = window.setTimeout(async () => {
      try {
        const request: PhotoPreviewRequest = {
          ...params,
          client_id: clientId,
          request_id: requestId,
        };
        const data = await invoke<PhotoPreviewResponse>(
          "generate_photo_preview",
          { request }
        );
        if (
          !stale &&
          generation === requestGeneration.current &&
          data.request_id === requestId
        )
          setFrame({ key, data });
      } catch (e) {
        if (!stale && generation === requestGeneration.current)
          setError(String(e));
      } finally {
        if (!stale && generation === requestGeneration.current) setBusy(false);
      }
    }, 180);
    return () => {
      stale = true;
      window.clearTimeout(timer);
      if (isDesktop)
        void invoke("cancel_photo_preview", { clientId }).catch(() => {});
    };
  }, [key, clientId, isDesktop]);
  useEffect(
    () => () => {
      if (isDesktop)
        void invoke("cancel_photo_preview", { clientId }).catch(() => {});
    },
    [clientId, isDesktop]
  );
  const shown = frame?.key === key ? frame.data : null;
  const chooseZoom = (value: "fit" | "region") => {
    setZoom(value);
    if (value === "region")
      setRegion({
        x: Math.max(0, (width - regionWidth) / 2),
        y: Math.max(0, (height - regionHeight) / 2),
      });
  };
  const move = (dx: number, dy: number) =>
    setRegion((p) => ({
      x: Math.max(0, Math.min(width - regionWidth, p.x + dx)),
      y: Math.max(0, Math.min(height - regionHeight, p.y + dy)),
    }));
  return (
    <section className="preview-panel" aria-label="照片对比预览">
      <div className="panel-toolbar">
        <div className="panel-title">
          <ScanLine size={15} />
          <span>照片预览</span>
          {clip && (
            <span className="muted toolbar-filename" title={clip.name}>
              / {clip.name}
            </span>
          )}
        </div>
        <div className="segmented" aria-label="预览模式">
          {(["original", "split", "graded"] as const).map((value) => (
            <button
              key={value}
              aria-pressed={mode === value}
              onClick={() => setMode(value)}
            >
              {value === "original"
                ? "原图"
                : value === "split"
                ? "对比"
                : "调色后"}
            </button>
          ))}
        </div>
      </div>
      <div
        ref={stage}
        className={`preview-stage photo-preview-stage ${
          clip ? "has-clip" : ""
        }`}
        tabIndex={zoom === "region" ? 0 : undefined}
        aria-label={
          zoom === "region"
            ? "100% 局部照片，可以拖动或用方向键移动"
            : undefined
        }
        onKeyDown={(e) => {
          if (
            zoom !== "region" ||
            (e.target as HTMLElement).matches("input,button")
          )
            return;
          const delta: Record<string, [number, number]> = {
            ArrowLeft: [-128, 0],
            ArrowRight: [128, 0],
            ArrowUp: [0, -128],
            ArrowDown: [0, 128],
          };
          if (delta[e.key]) {
            e.preventDefault();
            move(...delta[e.key]);
          }
        }}
        onPointerDown={(e) => {
          if (
            zoom !== "region" ||
            (e.target as HTMLElement).matches("input,button")
          )
            return;
          pointer.current = {
            x: e.clientX,
            y: e.clientY,
            left: region.x,
            top: region.y,
          };
          e.currentTarget.setPointerCapture(e.pointerId);
        }}
        onPointerMove={(e) => {
          const start = pointer.current;
          if (!start) return;
          const ratio = window.devicePixelRatio || 1;
          setRegion({
            x: Math.max(
              0,
              Math.min(
                width - regionWidth,
                start.left + (start.x - e.clientX) * ratio
              )
            ),
            y: Math.max(
              0,
              Math.min(
                height - regionHeight,
                start.top + (start.y - e.clientY) * ratio
              )
            ),
          });
        }}
        onPointerUp={() => {
          pointer.current = null;
        }}
        onPointerCancel={() => {
          pointer.current = null;
        }}
      >
        {!clip ? (
          <div className="empty-preview">
            <ImageIcon size={42} strokeWidth={1} />
            <span className="eyebrow">PHOTO COLOR WORKSPACE</span>
            <h1>为照片，找到同一种风格。</h1>
            <p>导入照片，比较 LUT 效果，再批量导出。</p>
            <button className="button primary" onClick={onImport}>
              导入照片 <kbd>⌘ O</kbd>
            </button>
            <span className="empty-formats">JPEG · PNG · TIFF</span>
          </div>
        ) : !isDesktop ? (
          <div className="preview-message">
            照片预览需要桌面应用，浏览器仅提供界面预览。
          </div>
        ) : error || clip.metadataError ? (
          <div className="preview-message">
            <p>{error || clip.metadataError}</p>
            <button
              className="button secondary"
              onClick={() => {
                if (clip.metadataError) onRefreshMetadata?.();
                setRetry((v) => v + 1);
              }}
            >
              <RotateCcw size={14} />
              重试预览
            </button>
          </div>
        ) : shown ? (
          <ImageComparison
            original={shown.original_image}
            processed={shown.processed_image}
            mode={mode}
            split={split}
            onSplit={setSplit}
            gradedLabel={
              clip.lutPath && clip.intensity > 0 ? "LUT 调色" : "原色"
            }
            imageSize={{ width: shown.width, height: shown.height }}
            pixelSize={
              zoom === "region"
                ? {
                    width: shown.width / (window.devicePixelRatio || 1),
                    height: shown.height / (window.devicePixelRatio || 1),
                  }
                : undefined
            }
          />
        ) : (
          <div className="preview-message">
            {busy
              ? "正在生成照片预览…"
              : "请选择照片的输入色彩空间与 LUT 用法。"}
          </div>
        )}
        {busy && clip && (
          <div className="preview-loading">
            <LoaderCircle size={16} className="spin" />
            <span>读取照片</span>
          </div>
        )}
      </div>
      <div className="photo-preview-footer">
        <div className="segmented" aria-label="照片缩放">
          <button
            aria-pressed={zoom === "fit"}
            onClick={() => chooseZoom("fit")}
          >
            适应窗口
          </button>
          <button
            aria-pressed={zoom === "region"}
            disabled={!width || !height}
            title={
              !width
                ? "等待照片尺寸读取完成"
                : "查看原尺寸局部，可拖动或用方向键移动"
            }
            onClick={() => chooseZoom("region")}
          >
            100% 局部
          </button>
        </div>
        <span>
          {zoom === "region"
            ? "拖动查看 · 精确效果"
            : quality === "fast"
            ? "快速预览 · 近似效果"
            : "精确预览"}
        </span>
        {clip && (
          <button
            className="text-button"
            disabled={busy || !isDesktop}
            onClick={() => setRetry((v) => v + 1)}
          >
            刷新
          </button>
        )}
      </div>
    </section>
  );
}
