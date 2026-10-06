import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
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
  FileImage,
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
import PhotoPreview from "./components/PhotoPreview";
import PhotoExportInspector from "./components/PhotoExportInspector";
import WindowedList from "./components/WindowedList";
import WorkspaceStatus from "./components/WorkspaceStatus";
import ExportInspector from "./components/ExportInspector";
import { useWorkspace } from "./workspace/useWorkspace";
import { clipErrorText } from "./workspace/snapshot";
import { errorMessage, errorText, processingMessage } from "./workspace/model";
import { applyLanguagePreference, normalizeLanguage } from "./i18n";
import type { Clip } from "./workspace/types";
import "./App.css";

const basename = (path: string) => path.split(/[\\/]/).pop() || path;
const filesize = (n = 0) =>
  n >= 1073741824
    ? `${(n / 1073741824).toFixed(1)} GB`
    : n >= 1048576
    ? `${(n / 1048576).toFixed(1)} MB`
    : `${Math.round(n / 1024)} KB`;

export default function App() {
  const { t } = useTranslation();
  const w = useWorkspace();
  const clipStatus: Record<Clip["status"], string> = {
    ready: t("status.ready"),
    queued: t("status.queued"),
    processing: t("status.processing"),
    completed: t("status.completed"),
    failed: t("status.failed"),
    cancelled: t("status.cancelled"),
  };
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
  const photoMode = w.mediaMode === "photo";
  const videoInfo = active?.kind === "photo" ? undefined : active?.info;
  const totalDuration = w.clips.reduce(
    (sum, c) => sum + (c.kind === "photo" ? 0 : c.info?.duration || 0),
    0
  );
  const completed = w.clips.filter((c) => c.status === "completed").length;
  const failures = w.clips.filter(
    (c) => c.status === "failed" || c.status === "cancelled"
  ).length;
  const filteredClips = w.clips.filter((c) =>
    c.name.toLowerCase().includes(search.toLowerCase())
  );
  const filteredLuts = w.luts.filter((l) =>
    l.name.toLowerCase().includes(search.toLowerCase())
  );

  visibleIdsRef.current = filteredClips.map((c) => c.id);
  useEffect(() => {
    applyLanguagePreference(w.settings.language);
  }, [w.settings.language]);
  useEffect(() => {
    exportScopeRef.current = "pending";
    setView("workspace");
  }, [w.mediaMode]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (
        !(e.metaKey || e.ctrlKey) ||
        settingsOpen ||
        helpOpen ||
        document.querySelector("dialog[open]")
      )
        return;
      const editing = (e.target as HTMLElement)?.matches(
        "input, textarea, select, [contenteditable=true]"
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
          if (exportScopeRef.current === "selected")
            void wRef.current.exportSelected();
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
      wRef.current.setNotice({ kind: "error", message: errorText(e) });
    }
  }, []);
  const selectLut = (path: string | null) => {
    if (active) w.setClipLook(active.id, { lutPath: path });
    else w.setNotice({ kind: "info", message: t("app.noticeImportFirst") });
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
            message: t("app.noticeDesktopDrop"),
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
          aria-label={t("app.ariaWorkspace")}
        >
          <span className="brand-symbol">
            <Layers3 size={23} strokeWidth={1.8} />
          </span>
          <span>
            LUT<span className="brand-light">lab</span>
            <small>COLOR WORKSPACE</small>
          </span>
        </a>
        <nav className="main-navigation" aria-label={t("app.ariaMainNav")}>
          <button
            className={view === "workspace" ? "selected" : ""}
            onClick={() => setView("workspace")}
          >
            <SlidersHorizontal size={15} />
            {t("app.navWorkspace")}
          </button>
          <button
            className={view === "queue" ? "selected" : ""}
            onClick={() => setView("queue")}
          >
            <ListVideo size={16} />
            {t("app.navQueue")}
            {w.clips.length > 0 && (
              <span className="nav-count">
                {w.isExporting ? w.clips.length - completed : completed}
              </span>
            )}
          </button>
        </nav>
        <div className="header-actions">
          <div
            className="segmented media-mode"
            aria-label={t("app.ariaMediaMode")}
          >
            <button
              aria-pressed={!photoMode}
              disabled={locked || w.isImporting}
              title={locked ? t("app.mediaSwitchLocked") : undefined}
              onClick={() => w.setMediaMode("video")}
            >
              {t("mode.video")}
            </button>
            <button
              aria-pressed={photoMode}
              disabled={locked || w.isImporting}
              title={locked ? t("app.mediaSwitchLocked") : undefined}
              onClick={() => w.setMediaMode("photo")}
            >
              {t("mode.photo")}
            </button>
          </div>
          <button
            className={`engine-status ${w.ffmpeg.status}`}
            onClick={() => setSettingsOpen(true)}
          >
            <span className="status-dot" />
            {w.ffmpeg.status === "ready"
              ? t("app.engineReady")
              : w.ffmpeg.status === "checking"
              ? t("app.engineChecking")
              : w.ffmpeg.status === "browser"
              ? t("app.engineBrowser")
              : t("app.engineConfigure")}
          </button>
          <span className="header-divider" />
          <button
            className="icon-button"
            aria-label={t("app.ariaHelp")}
            onClick={() => setHelpOpen(true)}
          >
            <CircleHelp size={18} />
          </button>
          <button
            className="icon-button"
            aria-label={t("app.ariaSettings")}
            onClick={() => setSettingsOpen(true)}
          >
            <Settings2 size={18} />
          </button>
        </div>
      </header>

      <WorkspaceStatus workspace={w} />
      <div className="workspace-body">
        <aside className="media-sidebar" aria-label={t("app.ariaSidebar")}>
          <div className="sidebar-tabs">
            <button
              className={section === "media" ? "active" : ""}
              onClick={() => {
                setSection("media");
                setSearch("");
              }}
            >
              <Clapperboard size={15} />
              {t("app.clipsTitle")}
              <span>{w.clips.length}</span>
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
                placeholder={
                  section === "media"
                    ? t("app.searchClips")
                    : t("app.searchLuts")
                }
                aria-label={
                  section === "media"
                    ? t("app.searchClips")
                    : t("app.searchLuts")
                }
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
              <kbd>⌕</kbd>
            </label>
            <button
              className="icon-button outlined"
              aria-label={
                section === "media"
                  ? photoMode
                    ? t("app.addPhotos")
                    : t("app.addVideos")
                  : t("app.addLuts")
              }
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
              <span>
                {t("app.selectedCount", { count: w.selectedIds.length })}
              </span>
              <button
                title={t("app.selectAllTitle")}
                onClick={() => w.selectAll(filteredClips.map((c) => c.id))}
              >
                {t("app.selectAll")}
              </button>
              <button
                disabled={!w.selectedIds.length}
                onClick={w.clearSelection}
              >
                {t("app.clearSelection")}
              </button>
              <button
                disabled={locked || !w.selectedIds.length}
                onClick={w.removeSelected}
              >
                {t("app.remove")}
              </button>
              <button
                aria-label={t("app.ariaUndo")}
                title={t("app.undoTitle")}
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
                {t("app.importProgress", {
                  completed: w.importProgress.completed,
                  total: w.importProgress.total,
                })}
              </span>
              <button onClick={w.cancelImport}>{t("app.stopImport")}</button>
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
                    label={t("app.ariaClipsList")}
                  >
                    {(clip, i) => (
                      <div
                        key={clip.id}
                        className={`clip-row ${
                          active?.id === clip.id ? "active" : ""
                        } ${
                          w.selectedIds.includes(clip.id)
                            ? "multi-selected"
                            : ""
                        }`}
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
                          aria-label={t("app.ariaPreviewClip", {
                            name: clip.name,
                          })}
                          aria-current={
                            active?.id === clip.id ? "true" : undefined
                          }
                          aria-pressed={w.selectedIds.includes(clip.id)}
                        >
                          <span className="clip-thumbnail">
                            {clip.kind === "photo" ? (
                              <FileImage size={23} strokeWidth={1.2} />
                            ) : (
                              <FileVideo size={23} strokeWidth={1.2} />
                            )}
                            <span>{String(i + 1).padStart(2, "0")}</span>
                          </span>
                          <span className="clip-copy">
                            <strong title={clip.name}>{clip.name}</strong>
                            <span className="clip-facts">
                              <span
                                title={
                                  clip.info?.width
                                    ? `${clip.info.width} × ${clip.info.height}`
                                    : undefined
                                }
                              >
                                {clip.info?.width
                                  ? `${clip.info.width} × ${clip.info.height}`
                                  : t("app.readingClip")}
                              </span>
                              <i />
                              <span>
                                {clip.kind === "photo"
                                  ? t("app.bitDepth", {
                                      bits: clip.info?.bit_depth ?? "—",
                                    })
                                  : timecode(clip.info?.duration || 0)}
                              </span>
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
                          aria-label={t("app.ariaRemoveClip", {
                            name: clip.name,
                          })}
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
                    <strong>{t("app.emptyClipsTitle")}</strong>
                    <p>
                      {photoMode
                        ? t("app.emptyClipsPhoto")
                        : t("app.emptyClipsVideo")}
                      <br />
                      {t("app.emptyClipsSub")}
                    </p>
                    <button
                      className="text-button"
                      disabled={locked}
                      onClick={() => void w.importVideos()}
                    >
                      {photoMode ? t("app.pickPhotos") : t("app.pickVideos")}{" "}
                      <ArrowRight size={13} />
                    </button>
                  </div>
                )}
                {w.clips.length > 0 && !filteredClips.length && (
                  <p className="no-results">{t("app.noClipMatch")}</p>
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
                    <strong>{t("app.originalColor")}</strong>
                    <small>{t("app.noLutApplied")}</small>
                  </span>
                  {!active?.lutPath && <Check size={14} />}
                </button>
                {filteredLuts.length > 0 && (
                  <WindowedList
                    items={filteredLuts}
                    rowHeight={68}
                    itemKey={(lut) => lut.path}
                    label={t("app.ariaLutList")}
                  >
                    {(lut) => (
                      <div className="lut-list-item" key={lut.path}>
                        <button
                          className={`lut-row ${
                            active?.lutPath === lut.path ? "selected" : ""
                          }`}
                          disabled={locked || !lut.is_valid}
                          onClick={() => selectLut(lut.path)}
                          title={
                            lut.error_message
                              ? errorMessage(lut.error_message)
                              : lut.path
                          }
                        >
                          <span
                            className={`lut-icon ${
                              !lut.is_valid ? "invalid" : ""
                            }`}
                          >
                            <Layers3 size={18} />
                          </span>
                          <span>
                            <strong>{lut.name}</strong>
                            <small>
                              {lut.is_valid
                                ? `${lut.format} · ${lut.category}`
                                : t("app.fileUnavailable")}
                            </small>
                          </span>
                          {active?.lutPath === lut.path && <Check size={14} />}
                        </button>
                        <button
                          className="icon-button lut-remove"
                          aria-label={t("app.ariaRemoveLut", {
                            name: lut.name,
                          })}
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
                    <strong>{t("app.emptyLutsTitle")}</strong>
                    <p>
                      {t("app.emptyLutsLine1")}
                      <br />
                      {t("app.emptyLutsLine2")}
                    </p>
                    <button
                      className="text-button"
                      disabled={locked}
                      onClick={() => void w.importLuts()}
                    >
                      {t("app.importLuts")} <Plus size={13} />
                    </button>
                  </div>
                )}
                {w.luts.length > 0 && !filteredLuts.length && (
                  <p className="no-results">{t("app.noLutMatch")}</p>
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
              {section === "media"
                ? t("app.importClipsFolder")
                : t("app.importLutsFolder")}
            </button>
            <div className="sidebar-summary">
              <span>
                {section === "media"
                  ? photoMode
                    ? t("app.photoCount", { count: w.clips.length })
                    : t("app.clipCount", {
                        count: w.clips.length,
                        duration: timecode(totalDuration),
                      })
                  : t("app.lutCount", { count: w.luts.length })}
              </span>
              {section === "media" && w.clips.length > 0 && (
                <button
                  className="text-button subdued"
                  disabled={locked}
                  onClick={w.clearClips}
                >
                  {t("app.clearAll")}
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
                {view === "workspace"
                  ? t("app.navWorkspace")
                  : t("app.navQueue")}
                <span className="heading-slash">/</span>
                <span className="heading-detail">
                  {view === "workspace"
                    ? t("app.workspaceSub")
                    : t("app.queueSub", { completed, total: w.clips.length })}
                </span>
              </h2>
            </div>
            <button
              className="button secondary import-top"
              disabled={locked}
              onClick={() => void w.importVideos()}
            >
              <Plus size={15} />
              {t("app.importMedia")}
            </button>
          </div>
          {view === "workspace" ? (
            <>
              {photoMode ? (
                <PhotoPreview
                  key={active?.id ?? "photo-empty"}
                  clip={active?.kind === "photo" ? active : undefined}
                  isDesktop={w.isDesktop}
                  quality={w.settings.preview_quality}
                  alphaPolicy={w.settings.photo_options.output.alpha_policy}
                  onImport={() => void w.importVideos()}
                  onRefreshMetadata={() =>
                    active && void w.refreshMetadata(active.id)
                  }
                />
              ) : (
                <FramePreview
                  key={active?.id ?? "empty"}
                  clip={
                    active?.kind !== "photo" ? active ?? undefined : undefined
                  }
                  isDesktop={w.isDesktop}
                  quality={w.settings.preview_quality}
                  inputColorSpace={w.settings.input_color_space}
                  onImport={() => void w.importVideos()}
                />
              )}
              <section className="selection-detail">
                <div>
                  <span className="section-overline">
                    {t("app.currentClip")}
                  </span>
                  <strong>{active?.name || t("app.noClipSelected")}</strong>
                  <span
                    className={`source-path ${
                      active?.metadataError ? "metadata-error" : ""
                    }`}
                    title={
                      active?.metadataError
                        ? errorMessage(active.metadataError)
                        : active?.path
                    }
                  >
                    {active?.metadataError
                      ? t("app.metadataFailed", {
                          message: errorMessage(active.metadataError),
                        })
                      : active?.path ||
                        (photoMode
                          ? t("app.previewHintPhoto")
                          : t("app.previewHintVideo"))}
                  </span>
                </div>
                <div className="metadata-pair">
                  <span>{t("app.codecLabel")}</span>
                  <strong>
                    {active?.kind === "photo"
                      ? active.info?.format.toUpperCase() || "—"
                      : videoInfo?.codec?.toUpperCase() || "—"}
                  </strong>
                </div>
                <div className="metadata-pair">
                  <span>{t("app.sizeLabel")}</span>
                  <strong>
                    {active?.info ? filesize(active.info.size) : "—"}
                  </strong>
                </div>
                <button
                  className="icon-button"
                  aria-label={t("app.ariaShowInFolder")}
                  disabled={!active || !w.isDesktop}
                  onClick={() => active && void openPath(active.path)}
                >
                  <FolderOpen size={17} />
                </button>
              </section>
              {videoInfo && (
                <div className="color-information">
                  <span>
                    {videoInfo.bit_depth
                      ? `${videoInfo.bit_depth}-bit`
                      : t("app.bitDepthUnknown")}{" "}
                    · {videoInfo.color_primaries || t("app.primariesUnknown")} ·{" "}
                    {videoInfo.color_transfer || t("app.transferUnknown")}
                  </span>
                  {(videoInfo.color_transfer === "smpte2084" ||
                    videoInfo.color_transfer === "arib-std-b67") &&
                    w.settings.input_color_space === "auto" && (
                      <strong>{t("app.hdrNotice")}</strong>
                    )}
                  {!videoInfo.color_transfer && (
                    <span>{t("app.colorSpaceHint")}</span>
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
                    <strong>{t("app.addMediaTitle")}</strong>
                    <small>
                      {photoMode
                        ? t("app.addMediaPhoto")
                        : t("app.addMediaVideo")}
                    </small>
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
                    <strong>{t("app.stepLookTitle")}</strong>
                    <small>{t("app.stepLookSub")}</small>
                  </span>
                </div>
                <ArrowRight size={14} />
                <div>
                  <span className={`step-number ${completed ? "done" : ""}`}>
                    {completed ? <Check size={12} /> : "03"}
                  </span>
                  <span>
                    <strong>{t("app.stepExportTitle")}</strong>
                    <small>{t("app.stepExportSub")}</small>
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
                      ? t("app.queueBusyTitle")
                      : w.clips.length
                      ? t("app.queueReadyTitle")
                      : t("app.queueEmptyTitle")}
                  </strong>
                  <p>
                    {w.isExporting
                      ? t("app.queueBusySub")
                      : t("app.queueReadySub")}
                  </p>
                </div>
                {failures > 0 && (
                  <button
                    className="button secondary"
                    disabled={locked}
                    onClick={() => void w.retryFailed()}
                  >
                    <RotateCcw size={14} />
                    {t("app.retryUnfinished")}
                  </button>
                )}
              </div>
              {w.batch && (
                <div className="batch-summary">
                  <div>
                    <span>
                      {w.isExporting
                        ? t("app.batchBusy")
                        : w.batch.status.toLowerCase() === "completed"
                        ? t("app.batchDone")
                        : w.batch.status.toLowerCase() === "cancelled"
                        ? t("app.batchCancelled")
                        : t("app.batchEnded")}
                    </span>
                    <strong>
                      {t("app.batchProgress", {
                        done:
                          w.batch.completed_items +
                          w.batch.failed_items +
                          w.batch.cancelled_items,
                        total: w.batch.total_items,
                      })}{" "}
                      {Math.round(w.batch.overall_progress)}%
                    </strong>
                  </div>
                  <progress
                    max="100"
                    value={w.batch.overall_progress}
                    aria-label={t("app.ariaBatchProgress")}
                  />
                </div>
              )}
              {!w.clips.length && (
                <div className="queue-empty">
                  <ListVideo size={38} strokeWidth={1} />
                  <h3>{t("app.noExportsTitle")}</h3>
                  <p>{t("app.noExportsSub")}</p>
                  <button
                    className="button secondary"
                    onClick={() => void w.importVideos()}
                  >
                    <Plus size={14} />
                    {t("app.addMedia")}
                  </button>
                </div>
              )}
              <div className="queue-list">
                <WindowedList
                  items={w.clips}
                  rowHeight={120}
                  itemKey={(clip) => clip.id}
                  label={t("app.ariaExportList")}
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
                            : t("app.originalColor")}
                          <i />
                          {clip.kind === "photo"
                            ? t("app.bitDepth", {
                                bits: clip.info?.bit_depth ?? "—",
                              })
                            : timecode(clip.info?.duration || 0)}
                        </span>
                        {clip.encoder && (
                          <span
                            className="encoding-details"
                            title={processingMessage(clip.message)}
                          >
                            {clip.encoder}
                            {clip.speed ? ` · ${clip.speed.toFixed(2)}×` : ""}
                            {clip.eta_seconds != null
                              ? ` · ${t("app.etaRemaining", {
                                  time: timecode(clip.eta_seconds),
                                })}`
                              : ""}
                          </span>
                        )}
                        {clip.kind === "photo" &&
                          clip.status === "processing" &&
                          clip.message && (
                            <span className="encoding-details">
                              {processingMessage(clip.message)}
                            </span>
                          )}
                        {clip.error && (
                          <p className="queue-error">
                            {clipErrorText(clip.error)}
                          </p>
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
                            clip.kind !== "photo" &&
                            ` ${Math.round(clip.progress)}%`}
                        </span>
                        {clip.status === "processing" &&
                          clip.kind !== "photo" && (
                            <progress
                              max="100"
                              value={clip.progress}
                              aria-label={t("app.ariaExportProgress", {
                                name: clip.name,
                              })}
                            />
                          )}
                      </div>
                      {clip.status === "completed" && clip.outputPath ? (
                        <button
                          className="icon-button"
                          aria-label={t("app.ariaShowExported", {
                            name: clip.name,
                          })}
                          onClick={() => void openPath(clip.outputPath!)}
                        >
                          <FolderOpen size={17} />
                        </button>
                      ) : clip.status === "failed" ||
                        clip.status === "cancelled" ? (
                        <button
                          className="icon-button"
                          aria-label={t("app.ariaRetryClip", {
                            name: clip.name,
                          })}
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
                  <summary>
                    {t("app.recentBatches", { count: w.history.length })}
                  </summary>
                  {w.history
                    .slice(-10)
                    .reverse()
                    .map((b) => (
                      <div key={b.batch_id}>
                        {t("app.batchSummary", {
                          completed: b.completed_items,
                          failed: b.failed_items,
                          cancelled: b.cancelled_items,
                        })}
                      </div>
                    ))}
                </details>
              )}
            </section>
          )}
        </main>

        {photoMode ? (
          <PhotoExportInspector
            workspace={w}
            onImportLuts={() => void w.importLuts()}
            onStartExport={start}
            onExportSelected={() => {
              setView("queue");
              void w.exportSelected();
            }}
            onExportScopeChange={(scope) => {
              exportScopeRef.current = scope;
            }}
          />
        ) : (
          <ExportInspector
            workspace={w}
            onImportLuts={() => {
              setSection("luts");
              void w.importLuts();
            }}
            onStartExport={start}
            onExportScopeChange={(scope) => {
              exportScopeRef.current = scope;
            }}
            onExportSelected={() => {
              setView("queue");
              void w.exportSelected();
            }}
          />
        )}
      </div>
      <footer className="status-bar">
        <span>
          <span className={`status-dot ${w.isExporting ? "pulsing" : ""}`} />
          {w.loading
            ? t("app.statusLoading")
            : w.isExporting
            ? t("app.statusExporting", { count: completed })
            : w.isImporting
            ? t("app.statusImporting", {
                completed: w.importProgress.completed,
                total: w.importProgress.total,
              })
            : w.isSavingSettings
            ? t("app.statusSaving")
            : t("app.statusReady")}
          {!w.isDesktop && (
            <span className="browser-hint">{t("app.browserOnly")}</span>
          )}
        </span>
        <span>
          {t("app.localFlow")}
          <span className="status-separator">/</span>
          {t("app.parallelTasks", { count: w.settings.max_concurrent_tasks })}
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
          <p>{errorMessage(w.notice.message)}</p>
          <button
            className="icon-button"
            aria-label={t("app.ariaDismissNotice")}
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
            <h2>{t("app.dropTitle")}</h2>
            <p>{photoMode ? t("app.dropPhoto") : t("app.dropVideo")}</p>
          </div>
        </div>
      )}
      {(settingsOpen || helpOpen) && (
        <dialog
          className="app-dialog"
          ref={dialogRef}
          aria-label={settingsOpen ? t("app.ariaSettings") : t("app.ariaHelp")}
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
                <h2>
                  {settingsOpen ? t("app.ariaSettings") : t("app.helpTitle")}
                </h2>
              </div>
              <button
                ref={closeRef}
                className="icon-button"
                aria-label={t("app.ariaCloseDialog")}
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
                    <strong>{t("app.ffmpegTitle")}</strong>
                    <span>
                      {w.ffmpeg.status === "ready"
                        ? t("app.engineConnected")
                        : w.ffmpeg.status === "browser"
                        ? t("app.engineNeedsDesktop")
                        : w.ffmpeg.status === "checking"
                        ? t("app.engineCheckingShort")
                        : t("app.engineUnavailable")}
                    </span>
                  </div>
                  <span className={`engine-status ${w.ffmpeg.status}`}>
                    <span className="status-dot" />
                  </span>
                </div>
                <p className="field-hint">{t("app.ffmpegHint")}</p>
                <div className="engine-path">
                  {w.ffmpeg.info?.binary_path ||
                    w.settings.ffmpeg_path ||
                    t("app.noCustomPath")}
                </div>
                {w.ffmpeg.error && (
                  <p className="queue-error">{errorMessage(w.ffmpeg.error)}</p>
                )}
                <div className="dialog-buttons">
                  <button
                    className="button secondary"
                    disabled={locked || !w.isDesktop}
                    onClick={() => void w.pickFfmpegPath()}
                  >
                    <FolderOpen size={15} />
                    {t("app.pickFfmpeg")}
                  </button>
                  <button
                    className="button secondary"
                    disabled={w.ffmpeg.status === "checking" || !w.isDesktop}
                    onClick={() => void w.refreshEngine()}
                  >
                    <RotateCcw size={14} />
                    {t("app.recheckEngine")}
                  </button>
                </div>
                <div className="dialog-note">
                  <strong>{t("app.previewExportTitle")}</strong>
                  <p>
                    {photoMode
                      ? t("app.previewPhotoDesc")
                      : t("app.previewVideoDesc")}
                  </p>
                </div>
                <div className="dialog-note">
                  <strong>{t("app.languageLabel")}</strong>
                  <select
                    aria-label={t("app.languageLabel")}
                    value={normalizeLanguage(w.settings.language)}
                    disabled={locked}
                    title={locked ? t("app.languageLocked") : undefined}
                    onChange={(e) =>
                      void w.updateSettings({ language: e.target.value })
                    }
                  >
                    <option value="auto">{t("app.languageAuto")}</option>
                    <option value="en">{t("app.languageEn")}</option>
                    <option value="zh">{t("app.languageZh")}</option>
                    <option value="ja">{t("app.languageJa")}</option>
                  </select>
                </div>
              </>
            ) : (
              <>
                <div className="help-step">
                  <span>01</span>
                  <div>
                    <strong>{t("app.helpStep1Title")}</strong>
                    <p>
                      {photoMode
                        ? t("app.helpStep1Photo")
                        : t("app.helpStep1Video")}
                      {t("app.helpStep1Tail")}
                    </p>
                  </div>
                </div>
                <div className="help-step">
                  <span>02</span>
                  <div>
                    <strong>{t("app.helpStep2Title")}</strong>
                    <p>
                      {photoMode
                        ? t("app.helpStep2Photo")
                        : t("app.helpStep2Video")}
                    </p>
                  </div>
                </div>
                <div className="help-step">
                  <span>03</span>
                  <div>
                    <strong>{t("app.helpStep3Title")}</strong>
                    <p>{t("app.helpStep3Desc")}</p>
                  </div>
                </div>
                <div className="shortcut-row">
                  <span>
                    {photoMode
                      ? t("app.helpImportPhotos")
                      : t("app.helpImportVideos")}
                  </span>
                  <kbd>⌘ / Ctrl O</kbd>
                </div>
                <div className="shortcut-row">
                  <span>{t("app.helpStartExport")}</span>
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
