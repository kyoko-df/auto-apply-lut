import { useTranslation } from "react-i18next";
import type { PreviewScopes } from "../workspace/types";

interface FrameScopesProps {
  enabled: boolean;
  onEnable: (value: boolean) => void;
  mask: boolean;
  onMask: (value: boolean) => void;
  data?: PreviewScopes | null;
}

const colors = ["#ff7474", "#72dd96", "#77abff"];
const channels = ["R", "G", "B"];

export default function FrameScopes({ enabled, onEnable, mask, onMask, data }: FrameScopesProps) {
  const { t } = useTranslation();
  const peak = data ? Math.max(1, ...data.channels.flat()) : 1;
  return (
    <aside className="frame-scopes">
      <button type="button" className="text-button" aria-expanded={enabled} onClick={() => onEnable(!enabled)}>
        {t("scopes.title")}
      </button>
      {enabled && data && <>
        <svg viewBox="0 0 256 64" preserveAspectRatio="none" role="img" aria-label={t("scopes.chart")}>
          {data.channels.map((bins, channel) => (
            <polyline
              key={channel}
              fill="none"
              stroke={colors[channel]}
              points={bins.map((count, i) => `${i},${63 - 62 * count / peak}`).join(" ")}
            />
          ))}
        </svg>
        <span>{t("scopes.count", { count: data.clipped, total: data.sampled })}</span>
        <span className="scope-counts">
          {data.clipped_channels.map((count, i) => (
            <span key={i}>{channels[i]} {count.toLocaleString()}</span>
          ))}
        </span>
        <label>
          <input type="checkbox" checked={mask} onChange={event => onMask(event.target.checked)} /> {t("scopes.overlay")}
        </label>
        <small>{t("scopes.hint")}</small>
      </>}
    </aside>
  );
}
