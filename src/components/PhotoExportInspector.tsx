import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import {
  ChevronDown,
  Copy,
  FolderOpen,
  Layers3,
  Plus,
  SlidersHorizontal,
} from "lucide-react";
import type { useWorkspace } from "../workspace/useWorkspace";
import type { PhotoOutput, PhotoSpace } from "../workspace/types";
import { fileName, errorText, photoProfileText } from "../workspace/model";

export default function PhotoExportInspector({
  workspace: w,
  onImportLuts,
  onStartExport,
  onExportSelected,
  onExportScopeChange,
}: {
  workspace: ReturnType<typeof useWorkspace>;
  onImportLuts: () => void;
  onStartExport: () => void;
  onExportSelected?: () => void;
  onExportScopeChange?: (scope: "pending" | "selected") => void;
}) {
  const { t } = useTranslation();
  const [scope, setScope] = useState<"pending" | "selected">("pending");
  const [confirming, setConfirming] = useState(false);
  const live = useRef(w);
  live.current = w;
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const active = w.activeClip?.kind === "photo" ? w.activeClip : null;
  const locked = w.isExporting || w.loading;
  const options = w.settings.photo_options;
  const output = options.output;
  const selected = new Set(w.selectedIds);
  const queue = w.clips.filter((c) =>
    scope === "selected" ? selected.has(c.id) : c.status !== "completed"
  );
  const needsMatte =
    output.format !== "png" &&
    queue.some((c) => c.kind === "photo" && c.info?.has_alpha) &&
    output.alpha_policy.mode === "preserve";
  const needsInterpretation = queue.some(
    (c) =>
      c.kind === "photo" &&
      c.info &&
      ["unknown", "invalid"].includes(c.info.color_status) &&
      c.sourceInterpretation?.mode !== "assign"
  );
  const needsLut = queue.some(
    (c) =>
      c.lutPath &&
      c.intensity > 0 &&
      (!c.lutFingerprint || c.lutSpace !== "srgb")
  );
  const reason = queue.some((c) => c.metadataError || !c.info)
    ? t("photoInspector.errReadInfoFirst")
    : needsMatte
    ? t("photoInspector.errAlphaBg")
    : needsInterpretation
    ? t("photoInspector.errAssignProfile")
    : needsLut
    ? t("photoInspector.errConfirmSrgb")
    : "";
  const setOutput = (next: PhotoOutput) =>
    void w.updateSettings({ photo_options: { ...options, output: next } });
  const confirm = async () => {
    if (!active?.lutPath || locked || !w.isDesktop) return;
    const id = active.id,
      path = active.lutPath;
    setConfirming(true);
    try {
      const fingerprint = await invoke<string>("get_photo_lut_fingerprint", {
        path,
      });
      if (
        mounted.current &&
        live.current.mediaMode === "photo" &&
        live.current.allClips.find((c) => c.id === id)?.lutPath === path
      )
        live.current.setClipLook(id, {
          lutSpace: "srgb",
          lutFingerprint: fingerprint,
        });
    } catch (e) {
      if (mounted.current)
        live.current.setNotice({ kind: "error", message: errorText(e) });
    } finally {
      if (mounted.current) setConfirming(false);
    }
  };
  return (
    <aside className="inspector" aria-label={t("photoInspector.aria")}>
      <div className="inspector-title">
        <SlidersHorizontal size={15} />
        <strong>{t("photoInspector.title")}</strong>
        <span>PHOTO</span>
      </div>
      <div className="inspector-scroll">
        <section className="inspector-section">
          <div className="section-heading">
            <h3>{t("inspector.lookSection")}</h3>
            <span className="section-number">01</span>
          </div>
          <div className="field-label">
            <label htmlFor="photo-lut-select">
              {t("inspector.currentLut")}
            </label>
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
              id="photo-lut-select"
              value={active?.lutPath ?? ""}
              disabled={!active || locked}
              onChange={(e) =>
                active &&
                w.setClipLook(active.id, { lutPath: e.target.value || null })
              }
            >
              <option value="">{t("inspector.originalColorOption")}</option>
              {active?.lutPath &&
                !w.luts.some((l) => l.path === active.lutPath) && (
                  <option value={active.lutPath}>
                    {t("inspector.workspaceLut", {
                      name: fileName(active.lutPath),
                    })}
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
          {active?.lutPath && (
            <div className="photo-lut-interpretation">
              <label className="checkbox-field">
                <input
                  type="checkbox"
                  checked={
                    active.lutSpace === "srgb" && !!active.lutFingerprint
                  }
                  disabled={locked || confirming || !w.isDesktop}
                  onChange={(e) =>
                    e.target.checked
                      ? void confirm()
                      : w.setClipLook(active.id, {
                          lutSpace: null,
                          lutFingerprint: null,
                        })
                  }
                />
                <span>{t("photoInspector.srgbUse")}</span>
              </label>
              <p className="field-hint">{t("photoInspector.srgbHint")}</p>
              {active.lutFingerprint && (
                <button
                  className="text-button"
                  disabled={locked || confirming || !w.isDesktop}
                  onClick={() => void confirm()}
                >
                  {confirming
                    ? t("photoInspector.confirming")
                    : t("photoInspector.reconfirmLut")}
                </button>
              )}
            </div>
          )}
          <div className="strength-heading">
            <label htmlFor="photo-strength">{t("inspector.strength")}</label>
            <span>{active?.intensity ?? 100}%</span>
          </div>
          <input
            id="photo-strength"
            className="strength-slider"
            type="range"
            min="0"
            max="100"
            value={active?.intensity ?? 100}
            disabled={!active || locked}
            onChange={(e) =>
              active &&
              w.setClipLook(active.id, { intensity: Number(e.target.value) })
            }
          />
          <div className="strength-labels">
            <span>{t("photoInspector.originalEnd")}</span>
            <span>{t("inspector.fullLook")}</span>
          </div>
          <button
            className="button secondary full-width apply-selected"
            disabled={!active || locked || w.selectedIds.length < 2}
            onClick={() => w.applyLookToSelected()}
          >
            <Copy size={14} />
            {t("photoInspector.applySelected", { count: w.selectedIds.length })}
          </button>
          <button
            className="button secondary full-width apply-all"
            disabled={!active || locked || w.clips.length < 2}
            onClick={() => w.applyLookToAll()}
          >
            <Copy size={14} />
            {t("photoInspector.applyAll")}
          </button>
          <label className="field">
            <span>{t("inspector.previewQuality")}</span>
            <select
              value={w.settings.preview_quality}
              disabled={locked}
              onChange={(e) =>
                void w.updateSettings({
                  preview_quality: e.target.value as "fast" | "accurate",
                })
              }
            >
              <option value="fast">{t("photoInspector.qualityFast")}</option>
              <option value="accurate">
                {t("photoInspector.qualityAccurate")}
              </option>
            </select>
          </label>
        </section>
        <section className="inspector-section">
          <div className="section-heading">
            <h3>{t("photoInspector.inputSection")}</h3>
            <span className="section-number">02</span>
          </div>
          <label className="field">
            <span>{t("photoInspector.colorSpace")}</span>
            <select
              value={
                active?.sourceInterpretation?.mode === "assign"
                  ? active.sourceInterpretation.space
                  : "embedded"
              }
              disabled={!active || locked}
              onChange={(e) =>
                active &&
                w.setClipLook(active.id, {
                  sourceInterpretation:
                    e.target.value === "embedded"
                      ? { mode: "embedded" }
                      : { mode: "assign", space: e.target.value as PhotoSpace },
                })
              }
            >
              <option value="embedded">{t("photoInspector.embedded")}</option>
              <option value="srgb">{t("photoInspector.srgb")}</option>
              <option value="adobe-rgb">{t("photoInspector.adobeRgb")}</option>
              <option value="display-p3">
                {t("photoInspector.displayP3")}
              </option>
            </select>
          </label>
          <p className="field-hint">
            {t("photoInspector.profileOutput", {
              profile: photoProfileText(active?.info),
            })}
          </p>
          <button
            className="text-button"
            disabled={!active || locked || w.selectedIds.length < 2}
            onClick={() => w.applyInputToSelected()}
          >
            {t("photoInspector.applyInputToSelected")}
          </button>
          <button
            className="text-button"
            disabled={!active || locked || !w.isDesktop}
            onClick={() => active && void w.refreshMetadata(active.id)}
          >
            {t("photoInspector.rereadInfo")}
          </button>
        </section>
        <section className="inspector-section">
          <div className="section-heading">
            <h3>{t("photoInspector.outputSection")}</h3>
            <span className="section-number">03</span>
          </div>
          <label className="field">
            <span>{t("photoInspector.outputFormat")}</span>
            <select
              value={output.format}
              disabled={locked}
              onChange={(e) =>
                setOutput(
                  e.target.value === "jpeg"
                    ? {
                        format: "jpeg",
                        quality: 95,
                        alpha_policy: output.alpha_policy,
                      }
                    : {
                        format: e.target.value as "png" | "tiff",
                        bit_depth: active?.info?.bit_depth === 8 ? 8 : 16,
                        alpha_policy: output.alpha_policy,
                      }
                )
              }
            >
              <option value="jpeg">JPEG</option>
              <option value="png">{t("photoInspector.pngLossless")}</option>
              <option value="tiff">{t("photoInspector.tiffLossless")}</option>
            </select>
          </label>
          {output.format === "jpeg" ? (
            <label className="field">
              <span>
                {t("photoInspector.jpegQuality", { quality: output.quality })}
              </span>
              <input
                type="range"
                min="1"
                max="100"
                value={output.quality}
                disabled={locked}
                onChange={(e) =>
                  setOutput({ ...output, quality: Number(e.target.value) })
                }
              />
            </label>
          ) : (
            <label className="field">
              <span>{t("photoInspector.bitDepth")}</span>
              <select
                value={output.bit_depth}
                disabled={locked}
                onChange={(e) =>
                  setOutput({
                    ...output,
                    bit_depth: Number(e.target.value) as 8 | 16,
                  })
                }
              >
                <option value="8">{t("photoInspector.bit8")}</option>
                <option value="16">{t("photoInspector.bit16")}</option>
              </select>
            </label>
          )}
          <p className="field-hint">{t("photoInspector.bitDepthHint")}</p>
          <label className="checkbox-field">
            <input
              type="checkbox"
              checked={output.alpha_policy.mode === "flatten"}
              disabled={locked}
              onChange={(e) =>
                setOutput({
                  ...output,
                  alpha_policy: e.target.checked
                    ? { mode: "flatten", color: [255, 255, 255] }
                    : { mode: "preserve" },
                })
              }
            />
            <span>{t("photoInspector.alphaBg")}</span>
          </label>
          {output.alpha_policy.mode === "flatten" && (
            <label className="field">
              <span>{t("photoInspector.bgColor")}</span>
              <input
                type="color"
                aria-label={t("photoInspector.ariaBgColor")}
                value={`#${output.alpha_policy.color
                  .map((v) => v.toString(16).padStart(2, "0"))
                  .join("")}`}
                disabled={locked}
                onChange={(e) =>
                  setOutput({
                    ...output,
                    alpha_policy: {
                      mode: "flatten",
                      color: [1, 3, 5].map((i) =>
                        parseInt(e.target.value.slice(i, i + 2), 16)
                      ) as [number, number, number],
                    },
                  })
                }
              />
            </label>
          )}
          <label className="checkbox-field">
            <input
              type="checkbox"
              checked={options.preserve_metadata}
              disabled={locked}
              onChange={(e) =>
                void w.updateSettings({
                  photo_options: {
                    ...options,
                    preserve_metadata: e.target.checked,
                  },
                })
              }
            />
            <span>{t("photoInspector.keepMetadata")}</span>
          </label>
          <label className="checkbox-field">
            <input
              type="checkbox"
              checked={options.preserve_gps}
              disabled={locked || !options.preserve_metadata}
              title={
                !options.preserve_metadata
                  ? t("photoInspector.enableMetadataFirst")
                  : undefined
              }
              onChange={(e) =>
                void w.updateSettings({
                  photo_options: { ...options, preserve_gps: e.target.checked },
                })
              }
            />
            <span>{t("photoInspector.keepGps")}</span>
          </label>
          <label className="field">
            <span>{t("photoInspector.outputLocation")}</span>
            <button
              className="output-directory"
              disabled={locked}
              title={
                w.settings.default_output_dir ||
                t("photoInspector.sourceFolder")
              }
              onClick={() => void w.pickOutputDirectory()}
            >
              <FolderOpen size={15} />
              <span>
                {w.settings.default_output_dir ||
                  t("photoInspector.sourceFolder")}
              </span>
            </button>
          </label>
          <p className="field-hint">{t("photoInspector.outputHint")}</p>
        </section>
      </div>
      <div className="inspector-footer">
        <label className="field">
          <span>{t("inspector.exportScope")}</span>
          <select
            value={scope}
            disabled={locked}
            onChange={(e) => {
              const value = e.target.value as "pending" | "selected";
              setScope(value);
              onExportScopeChange?.(value);
            }}
          >
            <option value="pending">{t("photoInspector.scopeAll")}</option>
            <option value="selected">
              {t("photoInspector.scopeSelected")}
            </option>
          </select>
        </label>
        {reason && (
          <p className="field-hint export-blocked-reason" role="status">
            {reason}
          </p>
        )}
        {w.isExporting ? (
          <button
            className="button primary full-width"
            onClick={() => void w.cancelExport()}
            disabled={w.batch?.status.toLowerCase() === "cancelling"}
          >
            {w.batch?.status.toLowerCase() === "cancelling"
              ? t("photoInspector.cancelling")
              : t("photoInspector.cancelExport")}
          </button>
        ) : (
          <button
            className="button primary full-width"
            disabled={
              locked ||
              !queue.length ||
              !!reason ||
              (w.isDesktop && w.ffmpeg.status !== "ready")
            }
            title={
              reason ||
              (!queue.length ? t("photoInspector.pickPhotosFirst") : undefined)
            }
            onClick={scope === "selected" ? onExportSelected : onStartExport}
          >
            {t("photoInspector.exportPhotos", { count: queue.length })}{" "}
            <kbd>⌘ ↵</kbd>
          </button>
        )}
        {!w.isDesktop && (
          <p className="field-hint">{t("photoInspector.needsDesktop")}</p>
        )}
      </div>
    </aside>
  );
}
