import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
export type ComparisonMode = "original" | "split" | "graded";

export default function ImageComparison({
  original,
  processed,
  mode,
  split,
  onSplit,
  originalLabel,
  gradedLabel,
  pixelSize,
  imageSize,
  originalAlt,
  gradedAlt,
}: {
  original: string;
  processed: string;
  mode: ComparisonMode;
  split: number;
  onSplit: (value: number) => void;
  originalLabel?: string;
  gradedLabel?: string;
  pixelSize?: { width: number; height: number };
  imageSize?: { width: number; height: number };
  originalAlt?: string;
  gradedAlt?: string;
}) {
  const { t } = useTranslation();
  const labels = {
    original: originalLabel ?? t("comparison.original"),
    graded: gradedLabel ?? t("comparison.graded"),
  };
  const ref = useRef<HTMLDivElement>(null);
  const [available, setAvailable] = useState({ width: 0, height: 0 });
  const [loaded, setLoaded] = useState({ source: "", width: 0, height: 0 });
  const source = mode === "original" ? original : processed;
  useEffect(() => {
    const parent = ref.current?.parentElement;
    if (!parent) return;
    const measure = () =>
      setAvailable({ width: parent.clientWidth, height: parent.clientHeight });
    measure();
    const observer =
      typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(measure)
        : null;
    observer?.observe(parent);
    window.addEventListener("resize", measure);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, []);
  const dimensions = imageSize ?? (loaded.source === source ? loaded : null);
  const scale =
    dimensions && available.width && available.height
      ? Math.min(
          available.width / dimensions.width,
          available.height / dimensions.height
        )
      : null;
  const size =
    pixelSize ??
    (dimensions && scale
      ? { width: dimensions.width * scale, height: dimensions.height * scale }
      : undefined);
  return (
    <div
      ref={ref}
      className={`frame-images ${pixelSize ? "photo-pixel-view" : ""}`}
      style={
        size
          ? {
              position: "relative",
              inset: "auto",
              width: size.width,
              height: size.height,
              flex: "none",
            }
          : undefined
      }
    >
      <img
        src={source}
        alt={
          mode === "original"
            ? originalAlt ?? labels.original
            : gradedAlt ?? labels.graded
        }
        draggable={false}
        onLoad={(e) =>
          setLoaded({
            source,
            width: e.currentTarget.naturalWidth,
            height: e.currentTarget.naturalHeight,
          })
        }
      />
      {mode === "split" && (
        <>
          <img
            className="original-overlay"
            src={original}
            alt={originalAlt ?? labels.original}
            draggable={false}
            style={{ clipPath: `inset(0 ${100 - split}% 0 0)` }}
          />
          <span className="frame-label before">{labels.original}</span>
          <span className="frame-label after">{labels.graded}</span>
          <div className="split-line" style={{ left: `${split}%` }}>
            <span>↔</span>
          </div>
          <input
            className="split-control"
            type="range"
            min="0"
            max="100"
            value={split}
            aria-label={t("comparison.ariaSplit")}
            onChange={(e) => onSplit(Number(e.target.value))}
          />
        </>
      )}
    </div>
  );
}
