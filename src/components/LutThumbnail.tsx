import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Layers3, LoaderCircle, TriangleAlert } from "lucide-react";
import { errorMessage } from "../workspace/model";
import type {
  AlphaPolicy, Clip, LutLibraryItem, PhotoPreviewResponse, PreviewFrame,
} from "../workspace/types";

// One library render at a time leaves a render slot for the main comparison.
// Canceled queued work never starts; native services own the bounded caches.
let tail: Promise<void> = Promise.resolve();
function enqueue(work: () => Promise<void>) {
  tail = tail.then(work, work).catch(() => {});
}

export default function LutThumbnail({
  clip, lut, enabled, isDesktop, inputColorSpace, alphaPolicy, engineKey,
}: {
  clip?: Clip;
  lut: LutLibraryItem;
  enabled: boolean;
  isDesktop: boolean;
  inputColorSpace: string;
  alphaPolicy: AlphaPolicy;
  engineKey: string;
}) {
  const { t } = useTranslation();
  const element = useRef<HTMLSpanElement>(null);
  const [visible, setVisible] = useState(false);
  const [result, setResult] = useState<{ key: string; image?: string; error?: string }>();
  const key = JSON.stringify([
    clip?.id, clip?.path, clip?.info, clip?.metadataError,
    clip?.sourceInterpretation, lut.path, lut.updated_at,
    inputColorSpace, alphaPolicy, engineKey,
  ]);
  useEffect(() => {
    if (!element.current || typeof IntersectionObserver === "undefined") {
      setVisible(true);
      return;
    }
    const observer = new IntersectionObserver(([entry]) => setVisible(entry.isIntersecting));
    observer.observe(element.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (!visible || !enabled || !isDesktop || !clip?.info || clip.metadataError || !lut.is_valid) return;
    let stale = false;
    let started = false;
    // Each generation owns its cancellation ID, including across unmount/remount.
    const clientId = `lut-thumb-${crypto.randomUUID()}`;
    const photo = clip.kind === "photo";
    const timer = window.setTimeout(() => enqueue(async () => {
      if (stale) return;
      try {
        let image: string;
        if (photo) {
          const fingerprint = await invoke<string>("get_photo_lut_fingerprint", { path: lut.path });
          if (stale) return;
          started = true;
          const data = await invoke<PhotoPreviewResponse>("generate_photo_preview", {
            request: {
              photo_path: clip.path, lut_path: lut.path, intensity: 1,
              source_interpretation: clip.sourceInterpretation ?? { mode: "embedded" },
              lut_space: "srgb", lut_fingerprint: fingerprint,
              viewport: { kind: "fit", max_edge: 160 }, quality: "fast",
              alpha_policy: alphaPolicy, client_id: clientId, request_id: clientId,
            },
          });
          if (data.request_id !== clientId) return;
          image = data.processed_image;
        } else {
          started = true;
          const data = await invoke<PreviewFrame>("generate_video_preview", {
            request: {
              video_path: clip.path, time_seconds: 0, lut_path: lut.path,
              intensity: 1, max_width: 160, quality: "fast",
              input_color_space: inputColorSpace, client_id: clientId,
            },
          });
          image = data.processed_image;
        }
        if (!stale) setResult({ key, image });
      } catch (error) {
        if (!stale) setResult({ key, error: errorMessage(error) });
      }
    }), 250);
    return () => {
      stale = true;
      window.clearTimeout(timer);
      if (started) void invoke(photo ? "cancel_photo_preview" : "cancel_video_preview", { clientId }).catch(() => {});
    };
  }, [key, visible, enabled, isDesktop, lut.is_valid]);
  const shown = result?.key === key ? result : undefined;
  const available = enabled && isDesktop && clip?.info && !clip.metadataError && lut.is_valid;
  return <span ref={element} className="lut-thumbnail" title={shown?.error}>
    {shown?.image ? <img src={shown.image} alt={t("app.lutThumbnailAlt", { name: lut.name, source: clip?.name })} />
      : shown?.error ? <TriangleAlert size={16} aria-label={shown.error} />
      : available && visible ? <LoaderCircle size={16} className="spin" aria-label={t("app.lutThumbnailLoading")} />
      : <Layers3 size={18} />}
  </span>;
}
