import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  ArrowRight,
  Check,
  CheckCircle2,
  CircleHelp,
  Clapperboard,
  Cpu,
  FileVideo,
  FolderInput,
  FolderOpen,
  Layers3,
  ListVideo,
  LoaderCircle,
  Plus,
  RotateCcw,
  Search,
  Settings2,
  SlidersHorizontal,
  Upload,
  X,
} from "lucide-react";
import FramePreview, { timecode } from "./components/FramePreview";
import WindowedList from "./components/WindowedList";
import WorkspaceStatus from "./components/WorkspaceStatus";
import ExportInspector from "./components/ExportInspector";
import { useWorkspace } from "./workspace/useWorkspace";
import type { Clip } from "./workspace/types";
import "./App.css";

const clipStatus: Record<Clip["status"], string> = {
  ready: "待导出",
  queued: "排队中",
  processing: "导出中",
  completed: "已完成",
  failed: "失败",
  cancelled: "待重试",
};
const basename = (path: string) => path.split(/[\\/]/).pop() || path;
const filesize = (n = 0) =>
  n >= 1073741824
    ? `${(n / 1073741824).toFixed(1)} GB`
    : n >= 1048576
      ? `${(n / 1048576).toFixed(1)} MB`
      : `${Math.round(n / 1024)} KB`;

export default function App() {
  const w = useWorkspace();
  const [section, setSection] = useState<"media" | "luts">("media");
  const [view, setView] = useState<"workspace" | "queue">("workspace");
  const [search, setSearch] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [dragging, setDragging] = useState(false);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const wRef = useRef(w);
  wRef.current = w;
  const exportScopeRef = useRef<"pending" | "selected">("pending");
  const visibleIdsRef = useRef<string[]>([]);
  const locked = w.isExporting || w.loading;
  const active = w.activeClip;
  const totalDuration = w.clips.reduce(
    (sum, c) => sum + (c.info?.duration || 0),
    0,
  );
  const completed = w.clips.filter((c) => c.status === "completed").length;
  const failures = w.clips.filter(
    (c) => c.status === "failed" || c.status === "cancelled",
  ).length;
  const filteredClips = w.clips.filter((c) =>
    c.name.toLowerCase().includes(search.toLowerCase()),
  );
  const filteredLuts = w.luts.filter((l) =>
    l.name.toLowerCase().includes(search.toLowerCase()),
  );

  visibleIdsRef.current = filteredClips.map(c => c.id);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || settingsOpen || helpOpen || document.querySelector("dialog[open]")) return;
      const editing = (e.target as HTMLElement)?.matches(
        "input, textarea, select, [contenteditable=true]",
      );
      if (!editing && e.key.toLowerCase() === "a") {
        e.preventDefault();
        wRef.current.selectAll(visibleIdsRef.current);
      }
      if (!editing && e.key.toLowerCase() === "z") {
        e.preventDefault();
        wRef.current.undo();
      }
      if (e.key.toLowerCase() === "o") {
        e.preventDefault();
        if (!wRef.current.isExporting) void wRef.current.importVideos();
      }
      if (e.key === "Enter") {
        e.preventDefault();
        if (!wRef.current.isExporting) {
          setView("queue");
          if (exportScopeRef.current === "selected") void wRef.current.exportSelected();
          else void wRef.current.startExport();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [settingsOpen, helpOpen]);

  useEffect(() => {
    if (!w.isDesktop) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebviewWindow()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter") setDragging(true);
        if (event.payload.type === "leave") setDragging(false);
        if (event.payload.type === "drop") {
          setDragging(false);
          if (!wRef.current.isExporting)
            void wRef.current.importVideos(event.payload.paths);
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [w.isDesktop]);

  useEffect(() => {
    if (settingsOpen || helpOpen) {
      dialogRef.current?.showModal?.();
      closeRef.current?.focus();
    } else dialogRef.current?.close?.();
  }, [settingsOpen, helpOpen]);

  const openPath = useCallback(async (path: string, folder = false) => {
    try {
      await invoke(folder ? "open_folder" : "open_file_location", { path });
    } catch (e) {
      wRef.current.setNotice({ kind: "error", message: String(e) });
    }
  }, []);
  const selectLut = (path: string | null) => {
    if (active) w.setClipLook(active.id, { lutPath: path });
    else
      w.setNotice({ kind: "info", message: "先导入视频，再为素材选择 LUT。" });
  };
  const start = () => {
    setView("queue");
    void w.startExport();
  };

  return (
    <div
      className="studio-app"
      onDragOver={(e) => {
        e.preventDefault();
        if (!w.isDesktop) setDragging(true);
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node))
          setDragging(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        setDragging(false);
        if (!w.isDesktop)
          w.setNotice({
            kind: "info",
            message: "请在桌面应用中拖入素材，浏览器仅用于界面预览。",
          });
      }}
    >
      <header className="app-header">
        <a
          className="brand"
          href="#"
          onClick={(e) => {
            e.preventDefault();
            setView("workspace");
          }}
          aria-label="LUT Lab 工作台"
        >
          <span className="brand-symbol">
            <Layers3 size={23} strokeWidth={1.8} />
          </span>
          <span>
            LUT<span className="brand-light">lab</span>
            <small>COLOR WORKSPACE</small>
          </span>
        </a>
        <nav className="main-navigation" aria-label="主导航">
          <button
            className={view === "workspace" ? "selected" : ""}
            onClick={() => setView("workspace")}
          >
            <SlidersHorizontal size={15} />
            调色工作台
          </button>
          <button
            className={view === "queue" ? "selected" : ""}
            onClick={() => setView("queue")}
          >
            <ListVideo size={16} />
            导出队列
            {w.clips.length > 0 && (
              <span className="nav-count">
                {w.isExporting ? w.clips.length - completed : completed}
              </span>
            )}
          </button>
        </nav>
        <div className="header-actions">
          <button
            className={`engine-status ${w.ffmpeg.status}`}
            onClick={() => setSettingsOpen(true)}
          >
            <span className="status-dot" />
            {w.ffmpeg.status === "ready"
              ? "处理引擎就绪"
              : w.ffmpeg.status === "checking"
                ? "检测处理引擎"
                : w.ffmpeg.status === "browser"
                  ? "浏览器预览"
                  : "配置处理引擎"}
          </button>
          <span className="header-divider" />
          <button
            className="icon-button"
            aria-label="使用帮助"
            onClick={() => setHelpOpen(true)}
          >
            <CircleHelp size={18} />
          </button>
          <button
            className="icon-button"
            aria-label="应用设置"
            onClick={() => setSettingsOpen(true)}
          >
            <Settings2 size={18} />
          </button>
        </div>
      </header>

      <WorkspaceStatus workspace={w} />
      <div className="workspace-body">
        <aside className="media-sidebar" aria-label="素材与 LUT 资料库">
          <div className="sidebar-tabs">
            <button
              className={section === "media" ? "active" : ""}
              onClick={() => {
                setSection("media");
                setSearch("");
              }}
            >
              <Clapperboard size={15} />
              素材<span>{w.clips.length}</span>
            </button>
            <button
              className={section === "luts" ? "active" : ""}
              onClick={() => {
                setSection("luts");
                setSearch("");
              }}
            >
              <Layers3 size={15} />
              LUT<span>{w.luts.length}</span>
            </button>
          </div>
          <div className="sidebar-tools">
            <label className="search-field">
              <Search size={14} />
              <input
                placeholder={section === "media" ? "搜索素材…" : "搜索 LUT…"}
                aria-label={section === "media" ? "搜索素材" : "搜索 LUT"}
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
              <kbd>⌕</kbd>
            </label>
            <button
              className="icon-button outlined"
              aria-label={section === "media" ? "添加视频" : "添加 LUT"}
              disabled={locked}
              onClick={() =>
                section === "media"
                  ? void w.importVideos()
                  : void w.importLuts()
              }
            >
              <Plus size={17} />
            </button>
          </div>
          {section === "media" && (w.clips.length > 0 || w.canUndo) && (
            <div className="selection-toolbar">
              <span>{w.selectedIds.length} 项已选</span>
              <button
                title="全选当前搜索结果"
                onClick={() => w.selectAll(filteredClips.map((c) => c.id))}
              >
                全选
              </button>
              <button
                disabled={!w.selectedIds.length}
                onClick={w.clearSelection}
              >
                清除选择
              </button>
              <button
                disabled={locked || !w.selectedIds.length}
                onClick={w.removeSelected}
              >
                移除
              </button>
              <button
                aria-label="撤销修改"
                title="撤销 ⌘Z"
                disabled={locked || !w.canUndo}
                onClick={w.undo}
              >
                <RotateCcw size={13} />
              </button>
            </div>
          )}
          {w.isImporting && (
            <div className="import-progress" role="status">
              <span>
                读取素材 {w.importProgress.completed} / {w.importProgress.total}
              </span>
              <button onClick={w.cancelImport}>停止添加</button>
            </div>
          )}
          <div className="sidebar-list">
            {section === "media" ? (
              <>
                {filteredClips.length > 0 && (
                  <WindowedList
                    items={filteredClips}
                    rowHeight={92}
                    itemKey={(clip) => clip.id}
                    label="素材列表"
                  >
                    {(clip, i) => (
                      <div
                        key={clip.id}
                        className={`clip-row ${active?.id === clip.id ? "active" : ""} ${w.selectedIds.includes(clip.id) ? "multi-selected" : ""}`}
                      >
                        <button
                          className="clip-select"
                          onClick={(event) => {
                            w.selectClip(clip.id, {
                              toggle: event.metaKey || event.ctrlKey,
                              range: event.shiftKey,
                              visibleIds: filteredClips.map((c) => c.id),
                            });
                            setView("workspace");
                          }}
                          aria-label={`预览 ${clip.name}`}
                          aria-current={
                            active?.id === clip.id ? "true" : undefined
                          }
                          aria-pressed={w.selectedIds.includes(clip.id)}
                        >
                          <span className="clip-thumbnail">
                            <FileVideo size={23} strokeWidth={1.2} />
                            <span>{String(i + 1).padStart(2, "0")}</span>
                          </span>
                          <span className="clip-copy">
                            <strong title={clip.name}>{clip.name}</strong>
                            <span>
                              {clip.info?.width
                                ? `${clip.info.width} × ${clip.info.height}`
                                : "读取素材"}
                              <i />
                              {timecode(clip.info?.duration || 0)}
                            </span>
                            <span className={`clip-state ${clip.status}`}>
                              {clip.status === "completed" ? (
                                <Check size={10} />
                              ) : clip.status === "processing" ? (
                                <LoaderCircle size={10} className="spin" />
                              ) : (
                                <span className="tiny-dot" />
                              )}
                              {clipStatus[clip.status]}
                              {clip.lutPath && (
                                <span className="lut-indicator">LUT</span>
                              )}
                            </span>
                          </span>
                        </button>
                        <button
                          className="clip-remove icon-button"
                          aria-label={`移除 ${clip.name}`}
                          disabled={locked}
                          onClick={() => w.removeClip(clip.id)}
                        >
                          <X size={12} />
                        </button>
                        {(clip.status === "processing" ||
                          clip.status === "queued") && (
                          <div
                            className="clip-progress"
                            style={{ width: `${clip.progress}%` }}
                          />
                        )}
                      </div>
                    )}
                  </WindowedList>
                )}
                {!w.clips.length && (
                  <div className="sidebar-empty">
                    <div className="empty-icon">
                      <FolderInput size={24} strokeWidth={1.3} />
                    </div>
                    <strong>素材，从这里开始</strong>
                    <p>
                      拖入多个视频或整个文件夹
                      <br />
                      一次完成所有素材的调色
                    </p>
                    <button
                      className="text-button"
                      disabled={locked}
                      onClick={() => void w.importVideos()}
                    >
                      选择视频 <ArrowRight size={13} />
                    </button>
                  </div>
                )}
                {w.clips.length > 0 && !filteredClips.length && (
                  <p className="no-results">没有匹配的素材</p>
                )}
              </>
            ) : (
              <>
                <button
                  className={`lut-row ${!active?.lutPath ? "selected" : ""}`}
                  disabled={locked || !active}
                  onClick={() => selectLut(null)}
                >
                  <span className="lut-icon neutral">
                    <ScanIcon />
                  </span>
                  <span>
                    <strong>原始色彩</strong>
                    <small>不应用 LUT</small>
                  </span>
                  {!active?.lutPath && <Check size={14} />}
                </button>
                {filteredLuts.length > 0 && (
                  <WindowedList
                    items={filteredLuts}
                    rowHeight={68}
                    itemKey={(lut) => lut.path}
                    label="LUT 列表"
                  >
                    {(lut) => (
                      <div className="lut-list-item" key={lut.path}>
                        <button
                          className={`lut-row ${active?.lutPath === lut.path ? "selected" : ""}`}
                          disabled={locked || !lut.is_valid}
                          onClick={() => selectLut(lut.path)}
                          title={lut.error_message || lut.path}
                        >
                          <span
                            className={`lut-icon ${!lut.is_valid ? "invalid" : ""}`}
                          >
                            <Layers3 size={18} />
                          </span>
                          <span>
                            <strong>{lut.name}</strong>
                            <small>
                              {lut.is_valid
                                ? `${lut.format} · ${lut.category}`
                                : "文件不可用"}
                            </small>
                          </span>
                          {active?.lutPath === lut.path && <Check size={14} />}
                        </button>
                        <button
                          className="icon-button lut-remove"
                          aria-label={`移除 LUT ${lut.name}`}
                          disabled={locked}
                          onClick={() => w.removeLut(lut.path)}
                        >
                          <X size={12} />
                        </button>
                      </div>
                    )}
                  </WindowedList>
                )}
                {!w.luts.length && (
                  <div className="sidebar-empty">
                    <strong>建立你的风格资料库</strong>
                    <p>
                      导入 .cube、.3dl 等 LUT
                      <br />
                      在所有视频间自由复用
                    </p>
                    <button
                      className="text-button"
                      disabled={locked}
                      onClick={() => void w.importLuts()}
                    >
                      导入 LUT <Plus size={13} />
                    </button>
                  </div>
                )}
                {w.luts.length > 0 && !filteredLuts.length && (
                  <p className="no-results">没有匹配的 LUT</p>
                )}
              </>
            )}
          </div>
          <div className="sidebar-bottom">
            <button
              className="button secondary full-width"
              disabled={locked}
              onClick={() =>
                section === "media"
                  ? void w.importDirectory()
                  : void w.importLutDirectory()
              }
            >
              <FolderInput size={15} />
              导入{section === "media" ? "素材" : "LUT"}文件夹
            </button>
            <div className="sidebar-summary">
              <span>
                {section === "media"
                  ? `${w.clips.length} 个素材 · ${timecode(totalDuration)}`
                  : `${w.luts.length} 个 LUT`}
              </span>
              {section === "media" && w.clips.length > 0 && (
                <button
                  className="text-button subdued"
                  disabled={locked}
                  onClick={w.clearClips}
                >
                  清空
                </button>
              )}
            </div>
          </div>
        </aside>

        <main className="main-workspace">
          <div className="workspace-heading">
            <div>
              <div className="eyebrow">
                {view === "workspace" ? "COLOR WORKSPACE" : "RENDER QUEUE"}
              </div>
              <h2>
                {view === "workspace" ? "调色工作台" : "导出队列"}
                <span className="heading-slash">/</span>
                <span className="heading-detail">
                  {view === "workspace"
                    ? "预览、调整，统一你的影像风格"
                    : `${completed} / ${w.clips.length} 个素材已完成`}
                </span>
              </h2>
            </div>
            <button
              className="button secondary import-top"
              disabled={locked}
              onClick={() => void w.importVideos()}
            >
              <Plus size={15} />
              导入素材
            </button>
          </div>
          {view === "workspace" ? (
            <>
              <FramePreview
                key={active?.id ?? "empty"}
                clip={active ?? undefined}
                isDesktop={w.isDesktop}
                quality={w.settings.preview_quality}
                inputColorSpace={w.settings.input_color_space}
                onImport={() => void w.importVideos()}
              />
              <section className="selection-detail">
                <div>
                  <span className="section-overline">当前素材</span>
                  <strong>{active?.name || "尚未选择素材"}</strong>
                  <span
                    className={`source-path ${active?.metadataError ? "metadata-error" : ""}`}
                    title={active?.metadataError || active?.path}
                  >
                    {active?.metadataError
                      ? `读取素材信息失败：${active.metadataError}`
                      : active?.path ||
                        "导入后即可预览，不需要等待整个视频处理完成"}
                  </span>
                </div>
                <div className="metadata-pair">
                  <span>编码</span>
                  <strong>{active?.info?.codec?.toUpperCase() || "—"}</strong>
                </div>
                <div className="metadata-pair">
                  <span>文件大小</span>
                  <strong>
                    {active?.info ? filesize(active.info.size) : "—"}
                  </strong>
                </div>
                <button
                  className="icon-button"
                  aria-label="在文件夹中显示原视频"
                  disabled={!active || !w.isDesktop}
                  onClick={() => active && void openPath(active.path)}
                >
                  <FolderOpen size={17} />
                </button>
              </section>
              {active?.info && (
                <div className="color-information">
                  <span>
                    {active.info.bit_depth
                      ? `${active.info.bit_depth}-bit`
                      : "位深未知"}{" "}
                    · {active.info.color_primaries || "色域未标记"} ·{" "}
                    {active.info.color_transfer || "传递函数未标记"}
                  </span>
                  {(active.info.color_transfer === "smpte2084" ||
                    active.info.color_transfer === "arib-std-b67") &&
                    w.settings.input_color_space === "auto" && (
                      <strong>
                        HDR 素材：使用 SDR 风格 LUT 前，请选择对应的 HDR →
                        Rec.709 转换。
                      </strong>
                    )}
                  {!active.info.color_transfer && (
                    <span>请确认素材与 LUT 的输入色彩空间匹配。</span>
                  )}
                </div>
              )}
              <section className="workflow-strip">
                <div>
                  <span
                    className={`step-number ${w.clips.length ? "done" : ""}`}
                  >
                    {w.clips.length ? <Check size={12} /> : "01"}
                  </span>
                  <span>
                    <strong>添加素材</strong>
                    <small>多选视频或拖入文件夹</small>
                  </span>
                </div>
                <ArrowRight size={14} />
                <div>
                  <span
                    className={`step-number ${active?.lutPath ? "done" : ""}`}
                  >
                    {active?.lutPath ? <Check size={12} /> : "02"}
                  </span>
                  <span>
                    <strong>调整风格</strong>
                    <small>选择 LUT，实时对比画面</small>
                  </span>
                </div>
                <ArrowRight size={14} />
                <div>
                  <span className={`step-number ${completed ? "done" : ""}`}>
                    {completed ? <Check size={12} /> : "03"}
                  </span>
                  <span>
                    <strong>批量导出</strong>
                    <small>设置一次，处理全部素材</small>
                  </span>
                </div>
              </section>
            </>
          ) : (
            <section className="queue-panel">
              <div className="queue-heading">
                <div>
                  <strong>
                    {w.isExporting
                      ? "正在处理素材"
                      : w.clips.length
                        ? "你的导出任务"
                        : "队列准备就绪"}
                  </strong>
                  <p>
                    {w.isExporting
                      ? "你可以继续查看素材，导出将在后台进行。"
                      : "每个任务独立处理，失败的素材可以单独重试。"}
                  </p>
                </div>
                {failures > 0 && (
                  <button
                    className="button secondary"
                    disabled={locked}
                    onClick={() => void w.retryFailed()}
                  >
                    <RotateCcw size={14} />
                    重试未完成
                  </button>
                )}
              </div>
              {w.batch && (
                <div className="batch-summary">
                  <div>
                    <span>
                      {w.isExporting
                        ? "任务处理进度"
                        : w.batch.status.toLowerCase() === "completed"
                          ? "批次已完成"
                          : w.batch.status.toLowerCase() === "cancelled"
                            ? "批次已取消"
                            : "批次已结束"}
                    </span>
                    <strong>
                      {w.batch.completed_items +
                        w.batch.failed_items +
                        w.batch.cancelled_items}{" "}
                      / {w.batch.total_items} 已结束 ·{" "}
                      {Math.round(w.batch.overall_progress)}%
                    </strong>
                  </div>
                  <progress
                    max="100"
                    value={w.batch.overall_progress}
                    aria-label="批次总进度"
                  />
                </div>
              )}
              {!w.clips.length && (
                <div className="queue-empty">
                  <ListVideo size={38} strokeWidth={1} />
                  <h3>还没有导出任务</h3>
                  <p>添加素材并设置调色风格后，即可开始批量导出。</p>
                  <button
                    className="button secondary"
                    onClick={() => void w.importVideos()}
                  >
                    <Plus size={14} />
                    添加素材
                  </button>
                </div>
              )}
              <div className="queue-list">
                <WindowedList
                  items={w.clips}
                  rowHeight={120}
                  itemKey={(clip) => clip.id}
                  label="导出任务列表"
                >
                  {(clip, i) => (
                    <article className="queue-row" key={clip.id}>
                      <span className="queue-index">
                        {String(i + 1).padStart(2, "0")}
                      </span>
                      <div className="queue-file">
                        <strong>{clip.name}</strong>
                        <span>
                          {clip.lutPath
                            ? `${basename(clip.lutPath)} · ${clip.intensity}%`
                            : "原始色彩"}
                          <i />
                          {timecode(clip.info?.duration || 0)}
                        </span>
                        {clip.encoder && (
                          <span
                            className="encoding-details"
                            title={clip.message}
                          >
                            {clip.encoder}
                            {clip.speed ? ` · ${clip.speed.toFixed(2)}×` : ""}
                            {clip.eta_seconds != null
                              ? ` · 预计剩余 ${timecode(clip.eta_seconds)}`
                              : ""}
                          </span>
                        )}
                        {clip.error && (
                          <p className="queue-error">{clip.error}</p>
                        )}
                        {clip.outputPath && clip.status === "completed" && (
                          <span
                            className="queue-output"
                            title={clip.outputPath}
                          >
                            {clip.outputPath}
                          </span>
                        )}
                      </div>
                      <div className={`queue-state ${clip.status}`}>
                        <span>
                          {clip.status === "processing" && (
                            <LoaderCircle size={12} className="spin" />
                          )}
                          {clip.status === "completed" && (
                            <CheckCircle2 size={12} />
                          )}
                          {clipStatus[clip.status]}
                          {clip.status === "processing" &&
                            ` ${Math.round(clip.progress)}%`}
                        </span>
                        {clip.status === "processing" && (
                          <progress
                            max="100"
                            value={clip.progress}
                            aria-label={`${clip.name} 导出进度`}
                          />
                        )}
                      </div>
                      {clip.status === "completed" && clip.outputPath ? (
                        <button
                          className="icon-button"
                          aria-label={`显示导出文件 ${clip.name}`}
                          onClick={() => void openPath(clip.outputPath!)}
                        >
                          <FolderOpen size={17} />
                        </button>
                      ) : clip.status === "failed" ||
                        clip.status === "cancelled" ? (
                        <button
                          className="icon-button"
                          aria-label={`重试 ${clip.name}`}
                          disabled={locked}
                          onClick={() => void w.startExport([clip.id])}
                        >
                          <RotateCcw size={15} />
                        </button>
                      ) : (
                        <span className="queue-action-space" />
                      )}
                    </article>
                  )}
                </WindowedList>
              </div>
              {w.history.length > 0 && (
                <details className="batch-history">
                  <summary>最近批次 · {w.history.length}</summary>
                  {w.history.slice(-10).reverse().map((b) => (
                    <div key={b.batch_id}>
                      {b.completed_items} 成功 / {b.failed_items} 失败 /{" "}
                      {b.cancelled_items} 待重试
                    </div>
                  ))}
                </details>
              )}
            </section>
          )}
        </main>

        <ExportInspector
          workspace={w}
          onImportLuts={() => {
            setSection("luts");
            void w.importLuts();
          }}
          onStartExport={start}
          onExportScopeChange={scope => { exportScopeRef.current = scope; }}
          onExportSelected={() => {
            setView("queue");
            void w.exportSelected();
          }}
        />
      </div>
      <footer className="status-bar">
        <span>
          <span className={`status-dot ${w.isExporting ? "pulsing" : ""}`} />
          {w.loading
            ? "正在读取素材与设置…"
            : w.isExporting
              ? `正在导出 · ${completed} 个已完成`
              : w.isImporting
                ? `正在导入 · ${w.importProgress.completed}/${w.importProgress.total}`
                : w.isSavingSettings
                  ? "正在保存设置…"
                  : "准备就绪"}
          {!w.isDesktop && (
            <span className="browser-hint">
              浏览器仅预览界面，视频处理请使用桌面应用
            </span>
          )}
        </span>
        <span>
          本地工作流<span className="status-separator">/</span>
          {w.settings.max_concurrent_tasks} 个并行任务
          <span className="status-separator">/</span>LUTlab{" "}
          <span className="muted">0.1.0</span>
        </span>
      </footer>
      {w.notice && (
        <div
          className={`toast ${w.notice.kind}`}
          role={w.notice.kind === "error" ? "alert" : "status"}
        >
          <span>
            {w.notice.kind === "success" ? (
              <CheckCircle2 size={17} />
            ) : w.notice.kind === "error" ? (
              <CircleHelp size={17} />
            ) : (
              <LoaderCircle size={17} />
            )}
          </span>
          <p>{w.notice.message}</p>
          <button
            className="icon-button"
            aria-label="关闭提示"
            onClick={() => w.setNotice(null)}
          >
            <X size={15} />
          </button>
        </div>
      )}
      {dragging && (
        <div className="drop-overlay">
          <div>
            <Upload size={44} strokeWidth={1.2} />
            <h2>松开，加入工作台</h2>
            <p>视频、LUT 或整个文件夹</p>
          </div>
        </div>
      )}
      {(settingsOpen || helpOpen) && (
        <dialog
          className="app-dialog"
          ref={dialogRef}
          aria-label={settingsOpen ? "应用设置" : "使用帮助"}
          onCancel={() => {
            setSettingsOpen(false);
            setHelpOpen(false);
          }}
          onClick={(e) => {
            if (e.target === e.currentTarget) {
              setSettingsOpen(false);
              setHelpOpen(false);
            }
          }}
        >
          <div className="dialog-content">
            <div className="dialog-heading">
              <div>
                <span className="eyebrow">
                  {settingsOpen ? "PREFERENCES" : "QUICK START"}
                </span>
                <h2>{settingsOpen ? "应用设置" : "从素材到成片"}</h2>
              </div>
              <button
                ref={closeRef}
                className="icon-button"
                aria-label="关闭窗口"
                onClick={() => {
                  setSettingsOpen(false);
                  setHelpOpen(false);
                }}
              >
                <X size={19} />
              </button>
            </div>
            {settingsOpen ? (
              <>
                <div className="engine-detail">
                  <Cpu size={24} />
                  <div>
                    <strong>FFmpeg 处理引擎</strong>
                    <span>
                      {w.ffmpeg.status === "ready"
                        ? "已连接 · 可以预览与导出"
                        : w.ffmpeg.status === "browser"
                          ? "需要启动桌面应用"
                          : w.ffmpeg.status === "checking"
                            ? "正在检测…"
                            : "处理引擎不可用"}
                    </span>
                  </div>
                  <span className={`engine-status ${w.ffmpeg.status}`}>
                    <span className="status-dot" />
                  </span>
                </div>
                <p className="field-hint">
                  优先使用应用内的媒体引擎，也可发现系统中的
                  FFmpeg。手动选择时，同目录下需包含 ffprobe。
                </p>
                <div className="engine-path">
                  {w.ffmpeg.info?.binary_path ||
                    w.settings.ffmpeg_path ||
                    "未配置自定义路径"}
                </div>
                {w.ffmpeg.error && (
                  <p className="queue-error">{w.ffmpeg.error}</p>
                )}
                <div className="dialog-buttons">
                  <button
                    className="button secondary"
                    disabled={locked || !w.isDesktop}
                    onClick={() => void w.pickFfmpegPath()}
                  >
                    <FolderOpen size={15} />
                    选择 FFmpeg
                  </button>
                  <button
                    className="button secondary"
                    disabled={w.ffmpeg.status === "checking" || !w.isDesktop}
                    onClick={() => void w.refreshEngine()}
                  >
                    <RotateCcw size={14} />
                    重新检测
                  </button>
                </div>
                <div className="dialog-note">
                  <strong>预览与导出</strong>
                  <p>
                    预览使用缩小的真实视频帧与 LUT
                    运算；导出使用设置的完整分辨率。LUT 不会自动识别相机 Log
                    或执行 HDR 色调映射，请使用与素材输入色彩空间匹配的 LUT。
                  </p>
                </div>
              </>
            ) : (
              <>
                <div className="help-step">
                  <span>01</span>
                  <div>
                    <strong>导入素材与 LUT</strong>
                    <p>
                      视频支持多选，也可以拖入文件夹。切换左侧 LUT
                      标签页，管理风格资料库。
                    </p>
                  </div>
                </div>
                <div className="help-step">
                  <span>02</span>
                  <div>
                    <strong>查看真实调色效果</strong>
                    <p>
                      选中素材，选择 LUT
                      并调节强度。拖动时间轴选帧，用中央分割线比较原片与调色画面。使用「应用到全部素材」统一风格。
                    </p>
                  </div>
                </div>
                <div className="help-step">
                  <span>03</span>
                  <div>
                    <strong>批量导出与检查</strong>
                    <p>
                      选择格式、质量和输出目录，点击批量导出。导出队列可查看进度、重试失败任务，或定位已完成的文件。
                    </p>
                  </div>
                </div>
                <div className="shortcut-row">
                  <span>导入视频</span>
                  <kbd>⌘ / Ctrl O</kbd>
                </div>
                <div className="shortcut-row">
                  <span>开始导出</span>
                  <kbd>⌘ / Ctrl Enter</kbd>
                </div>
              </>
            )}
          </div>
        </dialog>
      )}
    </div>
  );
}
function ScanIcon() {
  return <span className="original-swatch" />;
}
