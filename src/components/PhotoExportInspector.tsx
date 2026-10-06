import { useEffect, useRef, useState } from "react";
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
import { fileName } from "../workspace/model";

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
    ? "请先完成照片信息读取，并处理不支持或损坏的照片。"
    : needsMatte
    ? "透明照片导出此格式时，需要明确选择合成背景。"
    : needsInterpretation
    ? "请为未标记或无效色彩配置的照片指定输入解释。"
    : needsLut
    ? "请确认所选 LUT 的输入和输出按 sRGB 使用。"
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
        live.current.setNotice({ kind: "error", message: String(e) });
    } finally {
      if (mounted.current) setConfirming(false);
    }
  };
  return (
    <aside className="inspector" aria-label="照片调色与导出设置">
      <div className="inspector-title">
        <SlidersHorizontal size={15} />
        <strong>照片风格与输出</strong>
        <span>PHOTO</span>
      </div>
      <div className="inspector-scroll">
        <section className="inspector-section">
          <div className="section-heading">
            <h3>调色风格</h3>
            <span className="section-number">01</span>
          </div>
          <div className="field-label">
            <label htmlFor="photo-lut-select">当前 LUT</label>
            <button
              className="text-button"
              disabled={locked}
              onClick={onImportLuts}
            >
              <Plus size={12} />
              导入
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
              <option value="">原始色彩 · 不应用 LUT</option>
              {active?.lutPath &&
                !w.luts.some((l) => l.path === active.lutPath) && (
                  <option value={active.lutPath}>
                    {fileName(active.lutPath)} · 工作区 LUT
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
                <span>按 sRGB 输入和输出使用此 LUT</span>
              </label>
              <p className="field-hint">
                适用于照片创意 LUT；相机 Log 与 HDR 转换 LUT 暂不支持。
              </p>
              {active.lutFingerprint && (
                <button
                  className="text-button"
                  disabled={locked || confirming || !w.isDesktop}
                  onClick={() => void confirm()}
                >
                  {confirming ? "读取中…" : "重新读取并确认 LUT"}
                </button>
              )}
            </div>
          )}
          <div className="strength-heading">
            <label htmlFor="photo-strength">LUT 强度</label>
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
            <span>原始色彩</span>
            <span>完整风格</span>
          </div>
          <button
            className="button secondary full-width apply-selected"
            disabled={!active || locked || w.selectedIds.length < 2}
            onClick={() => w.applyLookToSelected()}
          >
            <Copy size={14} />
            应用到选中的 {w.selectedIds.length} 张照片
          </button>
          <button
            className="button secondary full-width apply-all"
            disabled={!active || locked || w.clips.length < 2}
            onClick={() => w.applyLookToAll()}
          >
            <Copy size={14} />
            应用到全部照片
          </button>
          <label className="field">
            <span>预览精度</span>
            <select
              value={w.settings.preview_quality}
              disabled={locked}
              onChange={(e) =>
                void w.updateSettings({
                  preview_quality: e.target.value as "fast" | "accurate",
                })
              }
            >
              <option value="fast">快速 · 选风格</option>
              <option value="accurate">精确 · 确认效果</option>
            </select>
          </label>
        </section>
        <section className="inspector-section">
          <div className="section-heading">
            <h3>输入色彩</h3>
            <span className="section-number">02</span>
          </div>
          <label className="field">
            <span>照片色彩空间</span>
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
              <option value="embedded">使用内嵌色彩配置</option>
              <option value="srgb">明确按 sRGB 解释</option>
              <option value="adobe-rgb">明确按 Adobe RGB 解释</option>
              <option value="display-p3">明确按 Display P3 解释</option>
            </select>
          </label>
          <p className="field-hint">
            {active?.info?.color_profile ?? "等待照片信息"}。输出统一转换为
            sRGB。
          </p>
          <button
            className="text-button"
            disabled={!active || locked || w.selectedIds.length < 2}
            onClick={() => w.applyInputToSelected()}
          >
            将当前输入解释应用到已选照片
          </button>
          <button
            className="text-button"
            disabled={!active || locked || !w.isDesktop}
            onClick={() => active && void w.refreshMetadata(active.id)}
          >
            重新读取照片信息
          </button>
        </section>
        <section className="inspector-section">
          <div className="section-heading">
            <h3>照片输出</h3>
            <span className="section-number">03</span>
          </div>
          <label className="field">
            <span>输出格式</span>
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
              <option value="png">PNG · 无损</option>
              <option value="tiff">TIFF · 无损</option>
            </select>
          </label>
          {output.format === "jpeg" ? (
            <label className="field">
              <span>JPEG 质量 · {output.quality}</span>
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
              <span>输出位深</span>
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
                <option value="8">8 位</option>
                <option value="16">16 位</option>
              </select>
            </label>
          )}
          <p className="field-hint">
            保持原始尺寸。8 位照片导出为 16 位不会恢复额外细节。
          </p>
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
            <span>透明照片合成背景</span>
          </label>
          {output.alpha_policy.mode === "flatten" && (
            <label className="field">
              <span>背景色</span>
              <input
                type="color"
                aria-label="透明照片背景色"
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
            <span>保留主要拍摄信息</span>
          </label>
          <label className="checkbox-field">
            <input
              type="checkbox"
              checked={options.preserve_gps}
              disabled={locked || !options.preserve_metadata}
              title={
                !options.preserve_metadata ? "先启用拍摄信息保留" : undefined
              }
              onChange={(e) =>
                void w.updateSettings({
                  photo_options: { ...options, preserve_gps: e.target.checked },
                })
              }
            />
            <span>同时保留定位信息</span>
          </label>
          <label className="field">
            <span>导出位置</span>
            <button
              className="output-directory"
              disabled={locked}
              title={w.settings.default_output_dir || "源照片所在文件夹"}
              onClick={() => void w.pickOutputDirectory()}
            >
              <FolderOpen size={15} />
              <span>{w.settings.default_output_dir || "源照片所在文件夹"}</span>
            </button>
          </label>
          <p className="field-hint">
            另存为新文件，不覆盖原照片或已有输出。首版不包含 RAW 显影。
          </p>
        </section>
      </div>
      <div className="inspector-footer">
        <label className="field">
          <span>导出范围</span>
          <select
            value={scope}
            disabled={locked}
            onChange={(e) => {
              const value = e.target.value as "pending" | "selected";
              setScope(value);
              onExportScopeChange?.(value);
            }}
          >
            <option value="pending">全部待导出照片</option>
            <option value="selected">仅已选照片</option>
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
              ? "正在取消，等待清理…"
              : "取消照片导出"}
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
              (!queue.length ? "请先导入或选择待导出的照片" : undefined)
            }
            onClick={scope === "selected" ? onExportSelected : onStartExport}
          >
            导出 {queue.length} 张照片 <kbd>⌘ ↵</kbd>
          </button>
        )}
        {!w.isDesktop && (
          <p className="field-hint">真实照片处理需要桌面应用。</p>
        )}
      </div>
    </aside>
  );
}
