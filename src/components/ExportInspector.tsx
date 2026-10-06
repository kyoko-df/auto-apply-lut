import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { CSSProperties } from "react";
import {
  ArrowDownToLine,
  ArrowRight,
  CheckCheck,
  ChevronDown,
  Copy,
  Cpu,
  Folder,
  Layers3,
  Plus,
  SlidersHorizontal,
  Square,
} from "lucide-react";
import { fileName } from "../workspace/model";
import type { AppSettings } from "../workspace/types";
import type { useWorkspace } from "../workspace/useWorkspace";

interface ExportInspectorProps {
  workspace: ReturnType<typeof useWorkspace>;
  onImportLuts: () => void;
  onStartExport: () => void;
  onExportSelected?: () => void;
  onExportScopeChange?: (scope: "pending" | "selected") => void;
}

export default function ExportInspector({
  workspace: w,
  onImportLuts,
  onStartExport,
  onExportSelected,
  onExportScopeChange,
}: ExportInspectorProps) {
  const { t } = useTranslation();
  const [advanced, setAdvanced] = useState(false);
  const [exportScope, setExportScope] = useState<"pending" | "selected">(
    "pending"
  );
  const locked = w.isExporting || w.loading;
  const active = w.activeClip;
  const activeLut = w.luts.find((lut) => lut.path === active?.lutPath);
  const exportable = w.clips.filter(
    (clip) => clip.status !== "completed"
  ).length;
  const selectedSet = new Set(w.selectedIds);
  const selected = w.clips.filter((clip) => selectedSet.has(clip.id));
  const mixedLook =
    new Set(selected.map((clip) => `${clip.lutPath ?? ""}:${clip.intensity}`))
      .size > 1;
  const selectedExport = exportScope === "selected";
  const exportCount = selectedExport ? selected.length : exportable;
  const hdrInput = ["smpte2084", "arib-std-b67"].includes(
    active?.kind !== "photo" ? active?.info?.color_transfer ?? "" : ""
  );
  const selectLut = (path: string | null) => {
    if (active) w.setClipLook(active.id, { lutPath: path });
    else
      w.setNotice({ kind: "info", message: t("inspector.noticeImportFirst") });
  };

  return (
    <aside className="inspector" aria-label={t("inspector.aria")}>
      <div className="inspector-title">
        <SlidersHorizontal size={15} />
        <strong>{t("inspector.title")}</strong>
        <span>INSPECTOR</span>
      </div>
      <div className="inspector-scroll">
        <section className="inspector-section look-section">
          <div className="section-heading">
            <h3>{t("inspector.lookSection")}</h3>
            <span className="section-number">01</span>
          </div>
          <div className="field-label">
            <label htmlFor="lut-select">{t("inspector.currentLut")}</label>
            <button
              className="text-button"
              disabled={locked}
              onClick={onImportLuts}
            >
              <Plus size={12} />
              {t("inspector.import")}
            </button>
          </div>
          <div className="select-wrap">
            <Layers3 size={15} />
            <select
              id="lut-select"
              disabled={!active || locked}
              value={active?.lutPath || ""}
              onChange={(e) => selectLut(e.target.value || null)}
            >
              <option value="">{t("inspector.originalColorOption")}</option>
              {active?.lutPath && !activeLut && (
                <option value={active.lutPath}>
                  {t("inspector.workspaceLut", { name: fileName(active.lutPath) })}
                </option>
              )}
              {w.luts
                .filter((l) => l.is_valid)
                .map((l) => (
                  <option key={l.path} value={l.path}>
                    {l.name}
                  </option>
                ))}
            </select>
            <ChevronDown size={13} />
          </div>
          <div className="look-description">
            <span className="tiny-dot" />
            <span>
              {activeLut
                ? `${activeLut.format} · ${activeLut.category}`
                : active?.lutPath
                ? t("inspector.restoredLut")
                : t("inspector.lutHint")}
            </span>
          </div>
          <label className="field-label intensity-label" htmlFor="intensity">
            {t("inspector.strength")}
            <span className="value-badge">
              {active?.intensity ?? 100}
              <small>%</small>
            </span>
          </label>
          <input
            id="intensity"
            className="intensity-range"
            type="range"
            min="0"
            max="100"
            step="1"
            value={active?.intensity ?? 100}
            disabled={!active?.lutPath || locked}
            style={
              {
                "--range-fill": `${active?.intensity ?? 100}%`,
              } as CSSProperties
            }
            onChange={(e) =>
              active &&
              w.setClipLook(active.id, {
                intensity: Number(e.target.value),
              })
            }
          />
          <div className="range-labels">
            <span>{t("inspector.originalEnd")}</span>
            <span>{t("inspector.fullLook")}</span>
          </div>
          {selected.length > 1 && (
            <>
              <p className="field-hint selection-hint">
                {t("inspector.selectedCount", { count: selected.length })}
                {mixedLook ? t("inspector.mixedValues") : "。"}
                {t("inspector.editingPreview")}
              </p>
              <button
                className="button secondary full-width apply-selected"
                disabled={!active || locked}
                onClick={() => w.applyLookToSelected()}
              >
                <Copy size={14} />
                {t("inspector.applySelected", { count: selected.length })}
              </button>
            </>
          )}
          <button
            className="button secondary full-width apply-all"
            disabled={!active || w.clips.length < 2 || locked}
            onClick={() => w.applyLookToAll()}
          >
            <Copy size={14} />
            {t("inspector.applyAll")}
          </button>
          <p className="field-hint">
            {t("inspector.applyHint")}
          </p>
          <label className="field preview-quality-field">
            <span>{t("inspector.previewQuality")}</span>
            <select
              aria-label={t("inspector.previewQuality")}
              value={w.settings.preview_quality}
              disabled={locked}
              onChange={(e) =>
                w.updateSettings({
                  preview_quality: e.target
                    .value as AppSettings["preview_quality"],
                })
              }
            >
              <option value="fast">{t("inspector.qualityFast")}</option>
              <option value="accurate">{t("inspector.qualityAccurate")}</option>
            </select>
          </label>
          <p className="field-hint">
            {t("inspector.qualityHint")}
          </p>
        </section>
        <section className="inspector-section">
          <div className="section-heading">
            <h3>{t("inspector.exportSection")}</h3>
            <span className="section-number">02</span>
          </div>
          <div className="field-grid">
            <label className="field">
              <span>{t("inspector.format")}</span>
              <select
                aria-label={t("inspector.format")}
                value={w.settings.output_format}
                disabled={locked}
                onChange={(e) =>
                  w.updateSettings({
                    output_format: e.target.value,
                    ...(e.target.value !== "mov" &&
                    w.settings.video_codec === "prores_ks"
                      ? { video_codec: "libx264", audio_codec: "aac" }
                      : e.target.value === "mp4" &&
                        w.settings.audio_codec === "pcm_s16le"
                      ? { audio_codec: "aac" }
                      : {}),
                  })
                }
              >
                <option value="mp4">MP4</option>
                <option value="mov">MOV</option>
                <option value="mkv">MKV</option>
              </select>
            </label>
            <label className="field">
              <span>{t("inspector.codec")}</span>
              <select
                aria-label={t("inspector.codec")}
                value={w.settings.video_codec}
                disabled={locked}
                onChange={(e) =>
                  w.updateSettings({
                    video_codec: e.target.value,
                    ...(e.target.value === "prores_ks"
                      ? {
                          output_format: "mov",
                          audio_codec: "pcm_s16le",
                          hardware_acceleration: false,
                          output_bit_depth: "10",
                        }
                      : w.settings.video_codec === "prores_ks"
                      ? {
                          audio_codec: "aac",
                          output_bit_depth:
                            e.target.value === "libx265" ? "10" : "8",
                        }
                      : {}),
                  })
                }
              >
                <option value="libx264">H.264</option>
                <option value="libx265">H.265 / HEVC</option>
                <option value="prores_ks">ProRes 422 HQ</option>
              </select>
            </label>
          </div>
          <label className="field bit-depth-field">
            <span>{t("inspector.bitDepth")}</span>
            <select
              aria-label={t("inspector.bitDepth")}
              value={w.settings.output_bit_depth}
              disabled={locked || w.settings.video_codec !== "libx265"}
              onChange={(e) =>
                w.updateSettings({
                  output_bit_depth: e.target
                    .value as AppSettings["output_bit_depth"],
                })
              }
            >
              {w.settings.video_codec !== "prores_ks" && (
                <option value="8">{t("inspector.bit8")}</option>
              )}
              {w.settings.video_codec !== "libx264" && (
                <option value="10">{t("inspector.bit10")}</option>
              )}
            </select>
          </label>
          <label className="field input-color-field">
            <span>{t("inspector.inputColor")}</span>
            <select
              aria-label={t("inspector.inputColor")}
              value={w.settings.input_color_space}
              disabled={locked}
              onChange={(e) =>
                w.updateSettings({
                  input_color_space: e.target
                    .value as AppSettings["input_color_space"],
                })
              }
            >
              <option value="auto">{t("inspector.inputAuto")}</option>
              <option value="rec709">{t("inspector.inputRec709")}</option>
              <option value="rec2020-pq">Rec.2020 PQ → Rec.709</option>
              <option value="rec2020-hlg">Rec.2020 HLG → Rec.709</option>
            </select>
          </label>
          <p className="field-hint">
            {t("inspector.inputColorHint")}
          </p>
          {hdrInput && w.settings.input_color_space === "auto" && (
            <p className="field-hint color-warning" role="status">
              {t("inspector.hdrWarning")}
            </p>
          )}
          <label className="field quality-field">
            <span>{t("inspector.quality")}</span>
            <select
              aria-label={t("inspector.quality")}
              value={w.settings.quality_preset}
              disabled={locked || w.settings.video_codec === "prores_ks"}
              onChange={(e) =>
                w.updateSettings({ quality_preset: e.target.value })
              }
            >
              <option value="high_quality">{t("inspector.qualityHigh")}</option>
              <option value="balanced">{t("inspector.qualityBalanced")}</option>
              <option value="fast">{t("inspector.qualityFastExport")}</option>
            </select>
          </label>
          {w.settings.video_codec === "prores_ks" && (
            <p className="field-hint">{t("inspector.proresFixed")}</p>
          )}
          <label className="toggle-row">
            <span>
              <Cpu size={16} />
              <span>
                <strong>{t("inspector.hwAccel")}</strong>
                <small>{t("inspector.hwAccelSub")}</small>
              </span>
            </span>
            <input
              type="checkbox"
              role="switch"
              aria-label={t("inspector.hwAccel")}
              checked={w.settings.hardware_acceleration}
              disabled={locked || w.settings.video_codec === "prores_ks"}
              onChange={(e) =>
                w.updateSettings({
                  hardware_acceleration: e.target.checked,
                })
              }
            />
          </label>
          <button
            className="advanced-toggle"
            aria-expanded={advanced}
            onClick={() => setAdvanced(!advanced)}
          >
            <span>{t("inspector.moreSettings")}</span>
            <ChevronDown size={14} className={advanced ? "rotated" : ""} />
          </button>
          {advanced && (
            <div className="advanced-fields">
              <label className="field">
                <span>{t("inspector.resolution")}</span>
                <select
                  aria-label={t("inspector.resolution")}
                  disabled={locked}
                  value={w.settings.resolution}
                  onChange={(e) =>
                    w.updateSettings({ resolution: e.target.value })
                  }
                >
                  <option value="original">{t("inspector.resOriginal")}</option>
                  <option value="1920x1080">{t("inspector.resFit", { res: "1920 × 1080" })}</option>
                  <option value="3840x2160">{t("inspector.resFit", { res: "3840 × 2160" })}</option>
                  <option value="1280x720">{t("inspector.resFit", { res: "1280 × 720" })}</option>
                </select>
              </label>
              <label className="field">
                <span>{t("inspector.audio")}</span>
                <select
                  aria-label={t("inspector.audio")}
                  disabled={locked}
                  value={w.settings.audio_codec}
                  onChange={(e) =>
                    w.updateSettings({ audio_codec: e.target.value })
                  }
                >
                  <option value="aac">{t("inspector.audioAac")}</option>
                  <option value="copy">{t("inspector.audioCopy")}</option>
                  {w.settings.output_format !== "mp4" && (
                    <option value="pcm_s16le">{t("inspector.audioPcm")}</option>
                  )}
                </select>
              </label>
              <label className="field">
                <span>{t("inspector.parallel")}</span>
                <select
                  aria-label={t("inspector.parallel")}
                  disabled={locked}
                  value={w.settings.max_concurrent_tasks}
                  onChange={(e) =>
                    w.updateSettings({
                      max_concurrent_tasks: Number(e.target.value),
                    })
                  }
                >
                  {[1, 2, 3, 4].map((n) => (
                    <option key={n} value={n}>
                      {t("inspector.taskCount", { count: n })}{n === 2 ? t("inspector.recommended") : ""}
                    </option>
                  ))}
                </select>
              </label>
              <label className="checkbox-row">
                <input
                  type="checkbox"
                  checked={w.settings.preserve_metadata}
                  disabled={locked}
                  onChange={(e) =>
                    w.updateSettings({
                      preserve_metadata: e.target.checked,
                    })
                  }
                />
                {t("inspector.keepMetadata")}
              </label>
              <p className="field-hint">
                {t("inspector.moreHint")}
              </p>
            </div>
          )}
        </section>
        <section className="inspector-section output-section">
          <div className="section-heading">
            <h3>{t("inspector.outputSection")}</h3>
            <span className="section-number">03</span>
          </div>
          <button
            className="output-folder"
            disabled={locked}
            onClick={() => void w.pickOutputDirectory()}
            title={w.settings.default_output_dir || t("inspector.sameFolder")}
          >
            <Folder size={18} />
            <span>
              <strong>
                {w.settings.default_output_dir
                  ? fileName(w.settings.default_output_dir)
                  : t("inspector.sameAsSource")}
              </strong>
              <small>
                {w.settings.default_output_dir || t("inspector.autoSuffix")}
              </small>
            </span>
            <ChevronDown size={13} />
          </button>
          {w.settings.default_output_dir && (
            <button
              className="text-button subdued reset-directory"
              disabled={locked}
              onClick={() => w.updateSettings({ default_output_dir: "" })}
            >
              {t("inspector.resetOutput")}
            </button>
          )}
          <p className="safe-output">
            <CheckCheck size={13} />
            {t("inspector.outputHint")}
          </p>
        </section>
      </div>
      <div className="export-action">
        <div>
          <span>
            {w.isExporting
              ? t("inspector.exportBusy")
              : `${t("inspector.exportCount", { count: exportCount })}${selectedExport ? t("inspector.exportCountSelected") : t("inspector.exportCountPending")}`}
          </span>
          <span>
            {w.settings.output_format.toUpperCase()}{" "}
            <span className="muted">/</span>{" "}
            {w.settings.video_codec === "libx265"
              ? "HEVC"
              : w.settings.video_codec === "prores_ks"
              ? "ProRes"
              : "H.264"}
          </span>
        </div>
        {!w.isExporting && (selected.length > 0 || selectedExport) && (
          <label className="field export-scope-field">
            <span>{t("inspector.exportScope")}</span>
            <select
              aria-label={t("inspector.exportScope")}
              value={exportScope}
              disabled={locked}
              onChange={(e) => {
                const scope = e.target.value as "pending" | "selected";
                setExportScope(scope);
                onExportScopeChange?.(scope);
              }}
            >
              <option value="pending">{t("inspector.scopeAll", { count: exportable })}</option>
              <option value="selected">
                {t("inspector.scopeSelected", { count: selected.length })}
              </option>
            </select>
          </label>
        )}
        {w.isExporting ? (
          <button
            className="button cancel-button full-width"
            onClick={() => void w.cancelExport()}
            disabled={w.batch?.status.toLowerCase() === "cancelling"}
          >
            <Square size={13} />
            {w.batch?.status.toLowerCase() === "cancelling"
              ? t("inspector.stopping")
              : t("inspector.stopExport")}
          </button>
        ) : (
          <button
            className="button primary full-width export-button"
            disabled={
              locked ||
              !exportCount ||
              !!w.settingsError ||
              !w.isDesktop ||
              w.ffmpeg.status !== "ready"
            }
            onClick={
              selectedExport
                ? onExportSelected ?? (() => void w.exportSelected())
                : onStartExport
            }
          >
            <ArrowDownToLine size={17} />
            {selectedExport ? t("inspector.exportSelected") : t("inspector.exportAll")}
            <ArrowRight size={16} />
          </button>
        )}
        <span className="export-hint">
          {w.isExporting ? t("inspector.keepDone") : t("inspector.shortcutHint")}
        </span>
      </div>
    </aside>
  );
}
