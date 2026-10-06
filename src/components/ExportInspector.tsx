import { useState } from "react";
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
      w.setNotice({ kind: "info", message: "先导入视频，再为素材选择 LUT。" });
  };

  return (
    <aside className="inspector" aria-label="调色与导出设置">
      <div className="inspector-title">
        <SlidersHorizontal size={15} />
        <strong>风格与输出</strong>
        <span>INSPECTOR</span>
      </div>
      <div className="inspector-scroll">
        <section className="inspector-section look-section">
          <div className="section-heading">
            <h3>调色风格</h3>
            <span className="section-number">01</span>
          </div>
          <div className="field-label">
            <label htmlFor="lut-select">当前 LUT</label>
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
              id="lut-select"
              disabled={!active || locked}
              value={active?.lutPath || ""}
              onChange={(e) => selectLut(e.target.value || null)}
            >
              <option value="">原始色彩 · 不应用 LUT</option>
              {active?.lutPath && !activeLut && (
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
          <div className="look-description">
            <span className="tiny-dot" />
            <span>
              {activeLut
                ? `${activeLut.format} · ${activeLut.category}`
                : active?.lutPath
                ? "来自恢复的工作区，导出前将校验原文件"
                : "选择适合素材色彩空间的 LUT"}
            </span>
          </div>
          <label className="field-label intensity-label" htmlFor="intensity">
            LUT 强度
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
            <span>原片</span>
            <span>完整风格</span>
          </div>
          {selected.length > 1 && (
            <>
              <p className="field-hint selection-hint">
                已选中 {selected.length} 个素材
                {mixedLook ? "，风格或强度存在不同值。" : "。"}
                当前控件编辑正在预览的素材。
              </p>
              <button
                className="button secondary full-width apply-selected"
                disabled={!active || locked}
                onClick={() => w.applyLookToSelected()}
              >
                <Copy size={14} />
                应用到选中的 {selected.length} 个素材
              </button>
            </>
          )}
          <button
            className="button secondary full-width apply-all"
            disabled={!active || w.clips.length < 2 || locked}
            onClick={() => w.applyLookToAll()}
          >
            <Copy size={14} />
            应用到全部素材
          </button>
          <p className="field-hint">
            每个素材可单独设置风格与强度，批量修改可以撤销。
          </p>
          <label className="field preview-quality-field">
            <span>预览精度</span>
            <select
              aria-label="预览精度"
              value={w.settings.preview_quality}
              disabled={locked}
              onChange={(e) =>
                w.updateSettings({
                  preview_quality: e.target
                    .value as AppSettings["preview_quality"],
                })
              }
            >
              <option value="fast">快速 · 优先响应</option>
              <option value="accurate">准确 · 完整精度调色</option>
            </select>
          </label>
          <p className="field-hint">
            预览为可定位的静态帧，准确模式需要更多处理时间。
          </p>
        </section>
        <section className="inspector-section">
          <div className="section-heading">
            <h3>导出设置</h3>
            <span className="section-number">02</span>
          </div>
          <div className="field-grid">
            <label className="field">
              <span>封装格式</span>
              <select
                aria-label="封装格式"
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
              <span>视频编码</span>
              <select
                aria-label="视频编码"
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
            <span>输出位深</span>
            <select
              aria-label="输出位深"
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
                <option value="8">8-bit · 通用兼容</option>
              )}
              {w.settings.video_codec !== "libx264" && (
                <option value="10">10-bit · 更细腻的渐变</option>
              )}
            </select>
          </label>
          <label className="field input-color-field">
            <span>输入色彩处理</span>
            <select
              aria-label="输入色彩处理"
              value={w.settings.input_color_space}
              disabled={locked}
              onChange={(e) =>
                w.updateSettings({
                  input_color_space: e.target
                    .value as AppSettings["input_color_space"],
                })
              }
            >
              <option value="auto">保持输入 · 不转换</option>
              <option value="rec709">按 Rec.709 解读</option>
              <option value="rec2020-pq">Rec.2020 PQ → Rec.709</option>
              <option value="rec2020-hlg">Rec.2020 HLG → Rec.709</option>
            </select>
          </label>
          <p className="field-hint">
            应用于本次全部导出素材；转换发生在 LUT
            之前。不同色彩空间的素材请分批处理。
          </p>
          {hdrInput && w.settings.input_color_space === "auto" && (
            <p className="field-hint color-warning" role="status">
              当前素材为 HDR。保持输入不会自动转换为 SDR，请确认 LUT
              与输入色彩空间匹配。
            </p>
          )}
          <label className="field quality-field">
            <span>输出质量</span>
            <select
              aria-label="输出质量"
              value={w.settings.quality_preset}
              disabled={locked || w.settings.video_codec === "prores_ks"}
              onChange={(e) =>
                w.updateSettings({ quality_preset: e.target.value })
              }
            >
              <option value="high_quality">高质量 · 细节优先</option>
              <option value="balanced">均衡 · 质量与速度</option>
              <option value="fast">快速 · 预览与交付</option>
            </select>
          </label>
          {w.settings.video_codec === "prores_ks" && (
            <p className="field-hint">ProRes 422 HQ 使用固定质量档位。</p>
          )}
          <label className="toggle-row">
            <span>
              <Cpu size={16} />
              <span>
                <strong>硬件加速</strong>
                <small>不可用时自动使用 CPU</small>
              </span>
            </span>
            <input
              type="checkbox"
              role="switch"
              aria-label="硬件加速"
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
            <span>更多设置</span>
            <ChevronDown size={14} className={advanced ? "rotated" : ""} />
          </button>
          {advanced && (
            <div className="advanced-fields">
              <label className="field">
                <span>分辨率</span>
                <select
                  aria-label="分辨率"
                  disabled={locked}
                  value={w.settings.resolution}
                  onChange={(e) =>
                    w.updateSettings({ resolution: e.target.value })
                  }
                >
                  <option value="original">与原片一致</option>
                  <option value="1920x1080">适配 1920 × 1080</option>
                  <option value="3840x2160">适配 3840 × 2160</option>
                  <option value="1280x720">适配 1280 × 720</option>
                </select>
              </label>
              <label className="field">
                <span>音频</span>
                <select
                  aria-label="音频"
                  disabled={locked}
                  value={w.settings.audio_codec}
                  onChange={(e) =>
                    w.updateSettings({ audio_codec: e.target.value })
                  }
                >
                  <option value="aac">AAC · 通用兼容</option>
                  <option value="copy">复制原始音轨</option>
                  {w.settings.output_format !== "mp4" && (
                    <option value="pcm_s16le">PCM · 无损音频</option>
                  )}
                </select>
              </label>
              <label className="field">
                <span>并行任务数</span>
                <select
                  aria-label="并行任务数"
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
                      {n} 个任务{n === 2 ? " · 推荐" : ""}
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
                保留素材元数据
              </label>
              <p className="field-hint">
                保持原始帧率。缩放保留画面比例，不裁切。
              </p>
            </div>
          )}
        </section>
        <section className="inspector-section output-section">
          <div className="section-heading">
            <h3>输出位置</h3>
            <span className="section-number">03</span>
          </div>
          <button
            className="output-folder"
            disabled={locked}
            onClick={() => void w.pickOutputDirectory()}
            title={w.settings.default_output_dir || "保存在各原视频所在文件夹"}
          >
            <Folder size={18} />
            <span>
              <strong>
                {w.settings.default_output_dir
                  ? fileName(w.settings.default_output_dir)
                  : "与原视频相同文件夹"}
              </strong>
              <small>
                {w.settings.default_output_dir || "自动添加 _lut_applied 后缀"}
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
              重置为原视频文件夹
            </button>
          )}
          <p className="safe-output">
            <CheckCheck size={13} />
            自动避免重名，保留原始素材
          </p>
        </section>
      </div>
      <div className="export-action">
        <div>
          <span>
            {w.isExporting
              ? "批量处理进行中"
              : `${exportCount} 个素材${selectedExport ? "已选中" : "待导出"}`}
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
            <span>导出范围</span>
            <select
              aria-label="导出范围"
              value={exportScope}
              disabled={locked}
              onChange={(e) => {
                const scope = e.target.value as "pending" | "selected";
                setExportScope(scope);
                onExportScopeChange?.(scope);
              }}
            >
              <option value="pending">全部待导出 · {exportable} 个</option>
              <option value="selected">
                仅选中素材 · {selected.length} 个
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
              ? "正在停止…"
              : "停止导出"}
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
            {selectedExport ? "导出选中素材" : "批量导出"}
            <ArrowRight size={16} />
          </button>
        )}
        <span className="export-hint">
          {w.isExporting ? "已完成的文件会保留" : "⌘ / Ctrl + Enter 开始导出"}
        </span>
      </div>
    </aside>
  );
}
