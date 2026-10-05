import { useCallback, useEffect, useRef, useState } from "react";
import type { SetStateAction } from "react";
import { restoreWorkspace } from "./snapshot";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  appendClips,
  buildBatchRequest,
  DEFAULT_SETTINGS,
  errorMessage,
  extension,
  isBatchFinished,
  invalidateCompleted,
  rememberOutput,
  LUT_EXTENSIONS,
  mergeLuts,
  normalizeSettings,
  pathKey,
  reconcileBatch,
  updateLook,
  VIDEO_EXTENSIONS,
} from "./model";
import type {
  AppSettings,
  BatchProgress,
  BatchResponse,
  Clip,
  EngineState,
  FfmpegInfo,
  LookPatch,
  LutLibraryItem,
  Notice,
  ScanResult,
  VideoInfo,
  WorkspaceSnapshot,
  SelectionOptions,
} from "./types";

const desktopAvailable = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const desktopMessage =
  "请在桌面应用中使用本地视频、LUT 预览和导出。浏览器仅提供界面预览。";

const EXPORT_SETTING_KEYS: Array<keyof AppSettings> = [
  "default_output_dir",
  "output_format",
  "video_codec",
  "audio_codec",
  "quality_preset",
  "resolution",
  "fps",
  "bitrate",
  "preserve_metadata",
  "output_bit_depth",
  "input_color_space",
];

async function mapLimit<T>(
  items: T[],
  limit: number,
  work: (item: T) => Promise<void>,
) {
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(limit, items.length) }, async () => {
      while (next < items.length) await work(items[next++]);
    }),
  );
}

export function useWorkspace() {
  const isDesktop = desktopAvailable();
  const [clips, setClips] = useState<Clip[]>([]);
  const [activeId, setActiveIdState] = useState<string | null>(null);
  const [selectedIds, setSelectedIdsState] = useState<string[]>([]);
  const [canUndo, setCanUndo] = useState(false);
  const [luts, setLuts] = useState<LutLibraryItem[]>([]);
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [ffmpeg, setFfmpeg] = useState<EngineState>({
    status: isDesktop ? "checking" : "browser",
  });
  const [loading, setLoading] = useState(isDesktop);
  const [isExporting, setIsExporting] = useState(false);
  const [cancelError, setCancelError] = useState<string | null>(null);
  const [batch, setBatchState] = useState<BatchProgress | null>(null);
  const [history, setHistory] = useState<BatchProgress[]>([]);
  const [isImporting, setIsImporting] = useState(false);
  const [importProgress, setImportProgress] = useState({
    completed: 0,
    total: 0,
  });
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [isSavingSettings, setIsSavingSettings] = useState(false);
  const [workspaceSaveError, setWorkspaceSaveError] = useState<string | null>(
    null,
  );
  const [runningBatchId, setRunningBatchId] = useState<string | null>(null);

  const clipsRef = useRef(clips);
  const activeIdRef = useRef(activeId);
  const selectedIdsRef = useRef(selectedIds);
  const selectionAnchorRef = useRef<string | null>(null);
  const batchRef = useRef(batch);
  const historyRef = useRef(history);
  const undoRef = useRef<{
    clips: Clip[];
    activeId: string | null;
    selectedIds: string[];
  } | null>(null);
  const importingRef = useRef(false);
  const importCancelledRef = useRef(false);
  const metadataSlotsRef = useRef(0);
  const metadataWaitersRef = useRef<Array<() => void>>([]);
  const persistedSettingsRef = useRef(settings);
  const failedSettingsRef = useRef<AppSettings | null>(null);
  const settingsRevisionRef = useRef(0);
  const workspaceLoadedRef = useRef(!isDesktop);
  const workspaceWritableRef = useRef(false);
  const workspaceWritesRef = useRef<Promise<void>>(Promise.resolve());
  const persistedWorkspaceKeyRef = useRef("");
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  const lutsRef = useRef(luts);
  const settingsRef = useRef(settings);
  const batchIdRef = useRef<string | null>(null);
  const exportingRef = useRef(false);
  const loadingRef = useRef(isDesktop);
  const cancelRequestedRef = useRef(false);
  const mountedRef = useRef(true);
  const settingsWritesRef = useRef<Promise<void>>(Promise.resolve());
  const guardWritesRef = useRef<Promise<void>>(Promise.resolve());
  const settingsErrorRef = useRef<string | null>(null);
  const engineGenerationRef = useRef(0);
  const metadataGenerationRef = useRef(0);
  const metadataRequestsRef = useRef(new Map<string, Promise<void>>());

  const setExportGuard = useCallback((active: boolean) => {
    const write = guardWritesRef.current
      .catch(() => {})
      .then(() => invoke<void>("set_export_guard", { active }));
    guardWritesRef.current = write;
    return write;
  }, []);

  const setActiveId = useCallback((value: SetStateAction<string | null>) => {
    const next =
      typeof value === "function" ? value(activeIdRef.current) : value;
    activeIdRef.current = next;
    if (mountedRef.current) setActiveIdState(next);
  }, []);

  const setSelectedIds = useCallback((ids: string[]) => {
    selectedIdsRef.current = ids;
    if (mountedRef.current) setSelectedIdsState(ids);
  }, []);

  const setBatch = useCallback(
    (value: SetStateAction<BatchProgress | null>) => {
      const next =
        typeof value === "function" ? value(batchRef.current) : value;
      if (JSON.stringify(next) === JSON.stringify(batchRef.current)) return;
      batchRef.current = next;
      if (mountedRef.current) setBatchState(next);
      if (next?.batch_id && isBatchFinished(next.status)) {
        const entries = [
          ...historyRef.current.filter(
            (item) => item.batch_id !== next.batch_id,
          ),
          next,
        ].slice(-50);
        historyRef.current = entries;
        if (mountedRef.current) setHistory(entries);
      }
    },
    [],
  );

  const getSnapshot = useCallback(
    (): WorkspaceSnapshot => ({
      version: 1,
      clips: clipsRef.current,
      activeId: activeIdRef.current,
      selectedIds: selectedIdsRef.current,
      batch: batchRef.current,
      history: historyRef.current,
    }),
    [],
  );

  const flushWorkspace = useCallback(async () => {
    if (!desktopAvailable() || !workspaceLoadedRef.current) return;
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    await settingsWritesRef.current;
    try {
      await guardWritesRef.current;
    } catch (error) {
      if (exportingRef.current) throw error;
      // A transient failure clearing the exit guard must remain retryable on close.
      await setExportGuard(false);
    }
    if (!workspaceWritableRef.current)
      throw new Error("工作区读取失败，自动保存已暂停以保留原文件。");
    const write = workspaceWritesRef.current
      .catch(() => {})
      .then(async () => {
        // Read refs at execution time so delayed writes cannot replace a newer snapshot.
        let snapshot = getSnapshot();
        let key = JSON.stringify(snapshot);
        while (key !== persistedWorkspaceKeyRef.current) {
          await invoke("save_workspace", { snapshot });
          persistedWorkspaceKeyRef.current = key;
          snapshot = getSnapshot();
          key = JSON.stringify(snapshot);
        }
        if (mountedRef.current) setWorkspaceSaveError(null);
      });
    workspaceWritesRef.current = write;
    try {
      await write;
    } catch (error) {
      if (mountedRef.current) setWorkspaceSaveError(errorMessage(error));
      throw error;
    }
  }, [getSnapshot, setExportGuard]);

  const restoreSavedWorkspace = useCallback(
    async (preserveEdits = false) => {
      let saved = restoreWorkspace(await invoke<unknown>("load_workspace"));
      if (!mountedRef.current) return;
      if (saved && preserveEdits) {
        // A read retry must not discard work imported while storage was unavailable.
        saved = {
          ...saved,
          clips: [
            ...new Map(
              [...saved.clips, ...clipsRef.current].map((clip) => [
                clip.id,
                clip,
              ]),
            ).values(),
          ],
          activeId: activeIdRef.current ?? saved.activeId,
          selectedIds: selectedIdsRef.current.length
            ? selectedIdsRef.current
            : saved.selectedIds,
          batch: batchRef.current ?? saved.batch,
          history: [
            ...new Map(
              [...saved.history, ...historyRef.current].map((item) => [
                item.batch_id,
                item,
              ]),
            ).values(),
          ].slice(-50),
        };
      }
      if (saved) {
        clipsRef.current = saved.clips;
        setClips(saved.clips);
        setActiveId(saved.activeId);
        setSelectedIds(saved.selectedIds);
        selectionAnchorRef.current = saved.activeId;
        batchRef.current = saved.batch;
        setBatchState(saved.batch);
        historyRef.current = saved.history;
        setHistory(saved.history);
        if (
          saved.clips.some((clip) => clip.error === "上次导出未完成，请重试。")
        )
          setNotice({
            kind: "info",
            message:
              "已恢复工作区。上次未完成的项目需要重新导出，不会自动续跑。",
          });
      }
      workspaceWritableRef.current = true;
      workspaceLoadedRef.current = true;
      setWorkspaceSaveError(null);
    },
    [setActiveId, setSelectedIds],
  );

  const retryWorkspaceSave = useCallback(async () => {
    try {
      if (!workspaceWritableRef.current) await restoreSavedWorkspace(true);
      await flushWorkspace();
    } catch (error) {
      setWorkspaceSaveError(errorMessage(error));
    }
  }, [flushWorkspace, restoreSavedWorkspace]);

  const changeClips = useCallback((updater: (current: Clip[]) => Clip[]) => {
    const next = updater(clipsRef.current);
    clipsRef.current = next;
    if (mountedRef.current) setClips(next);
    return next;
  }, []);

  const replaceLuts = useCallback((items: LutLibraryItem[]) => {
    lutsRef.current = items;
    if (mountedRef.current) setLuts(items);
  }, []);

  const requireDesktop = useCallback(() => {
    if (desktopAvailable()) return true;
    setNotice({ kind: "info", message: desktopMessage });
    return false;
  }, []);

  const readVideoMetadata = useCallback(
    (clip: Clip) => {
      const current = clipsRef.current.find((item) => item.id === clip.id);
      if (!current || (current.info && !current.metadataError))
        return Promise.resolve();
      const generation = metadataGenerationRef.current;
      const key = `${generation}:${clip.id}`;
      const pending = metadataRequestsRef.current.get(key);
      if (pending) return pending;
      const request = Promise.resolve().then(async () => {
        if (metadataSlotsRef.current >= 3)
          await new Promise<void>((resolve) =>
            metadataWaitersRef.current.push(resolve),
          );
        else metadataSlotsRef.current += 1;
        try {
          const info = await invoke<VideoInfo>("get_video_info", {
            path: clip.path,
          });
          if (
            mountedRef.current &&
            generation === metadataGenerationRef.current
          ) {
            changeClips((current) =>
              current.map((item) =>
                item.id === clip.id
                  ? { ...item, info, metadataError: undefined }
                  : item,
              ),
            );
          }
        } catch (error) {
          if (
            mountedRef.current &&
            generation === metadataGenerationRef.current
          ) {
            changeClips((current) =>
              current.map((item) =>
                item.id === clip.id
                  ? { ...item, metadataError: errorMessage(error) }
                  : item,
              ),
            );
          }
        } finally {
          metadataRequestsRef.current.delete(key);
          const next = metadataWaitersRef.current.shift();
          if (next) next();
          else metadataSlotsRef.current -= 1;
        }
      });
      metadataRequestsRef.current.set(key, request);
      return request;
    },
    [changeClips],
  );

  const refreshEngine = useCallback(async () => {
    if (!desktopAvailable()) {
      setFfmpeg({ status: "browser" });
      return;
    }
    const generation = ++engineGenerationRef.current;
    setFfmpeg({ status: "checking" });
    try {
      await settingsWritesRef.current;
      const info = await invoke<FfmpegInfo>("get_ffmpeg_info");
      if (mountedRef.current && engineGenerationRef.current === generation) {
        setFfmpeg({ status: "ready", info });
        // Fixing the engine must also recover the duration and seek range of already imported clips.
        const missingMetadata = clipsRef.current.filter(
          (clip) => !clip.info || clip.metadataError,
        );
        await mapLimit(missingMetadata, 3, readVideoMetadata);
      }
    } catch (error) {
      if (mountedRef.current && engineGenerationRef.current === generation)
        setFfmpeg({ status: "error", error: errorMessage(error) });
    }
  }, [readVideoMetadata]);

  useEffect(() => {
    mountedRef.current = true;
    let active = true;
    if (!isDesktop)
      return () => {
        mountedRef.current = false;
      };

    const initialize = async () => {
      const results = await Promise.allSettled([
        invoke<AppSettings>("get_app_settings"),
        invoke<LutLibraryItem[]>("list_lut_library"),
        restoreSavedWorkspace(),
      ]);
      if (!active) return;
      if (results[0].status === "fulfilled") {
        const loaded = normalizeSettings(results[0].value);
        persistedSettingsRef.current = loaded;
        settingsRef.current = loaded;
        setSettings(loaded);
      } else {
        setNotice({
          kind: "error",
          message: `读取设置失败：${errorMessage(results[0].reason)}`,
        });
      }
      if (results[1].status === "fulfilled") replaceLuts(results[1].value);
      else
        setNotice({
          kind: "error",
          message: `读取 LUT 库失败：${errorMessage(results[1].reason)}`,
        });
      if (results[2].status === "rejected") {
        workspaceLoadedRef.current = true;
        setWorkspaceSaveError(
          `读取工作区失败，自动保存已暂停：${errorMessage(results[2].reason)}`,
        );
      }
      loadingRef.current = false;
      setLoading(false);
      void refreshEngine();
    };
    void initialize();
    return () => {
      active = false;
      mountedRef.current = false;
    };
  }, [isDesktop, refreshEngine, replaceLuts, restoreSavedWorkspace]);

  useEffect(() => {
    if (loading || !isDesktop || !workspaceWritableRef.current) return;
    saveTimerRef.current = setTimeout(() => {
      void flushWorkspace().catch(() => {});
    }, 450);
    return () => {
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    };
  }, [
    clips,
    activeId,
    selectedIds,
    batch,
    history,
    loading,
    isDesktop,
    flushWorkspace,
  ]);

  const reloadPersistentState = useCallback(async () => {
    if (!desktopAvailable() || exportingRef.current) return;
    loadingRef.current = true;
    setLoading(true);
    try {
      await settingsWritesRef.current;
      const [storedSettings, storedLuts] = await Promise.all([
        invoke<AppSettings>("get_app_settings"),
        invoke<LutLibraryItem[]>("list_lut_library"),
      ]);
      const next = normalizeSettings(storedSettings);
      if (
        EXPORT_SETTING_KEYS.some(
          (key) => persistedSettingsRef.current[key] !== next[key],
        )
      ) {
        changeClips(invalidateCompleted);
        if (undoRef.current)
          undoRef.current = {
            ...undoRef.current,
            clips: invalidateCompleted(undoRef.current.clips),
          };
      }
      persistedSettingsRef.current = next;
      settingsRef.current = next;
      settingsErrorRef.current = null;
      failedSettingsRef.current = null;
      metadataGenerationRef.current += 1;
      setSettings(next);
      setSettingsError(null);
      replaceLuts(storedLuts);
      await refreshEngine();
    } finally {
      loadingRef.current = false;
      if (mountedRef.current) setLoading(false);
    }
  }, [changeClips, refreshEngine, replaceLuts]);

  // One request at a time. A temporary IPC failure must never unlock an active export.
  useEffect(() => {
    if (!runningBatchId) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let failures = 0;
    const poll = async () => {
      try {
        const progress = await invoke<BatchProgress>("get_batch_progress", {
          batchId: runningBatchId,
        });
        if (!active) return;
        const recovered = failures > 2;
        failures = 0;
        changeClips((current) => reconcileBatch(current, progress));
        setBatch(
          cancelRequestedRef.current && !isBatchFinished(progress.status)
            ? { ...progress, status: "cancelling" }
            : progress,
        );
        if (isBatchFinished(progress.status)) {
          batchIdRef.current = null;
          exportingRef.current = false;
          cancelRequestedRef.current = false;
          setRunningBatchId(null);
          setIsExporting(false);
          void setExportGuard(false).catch(() => {});
          setNotice({
            kind:
              progress.failed_items > 0
                ? "error"
                : progress.cancelled_items > 0
                  ? "info"
                  : "success",
            message:
              progress.failed_items > 0
                ? `导出结束：${progress.completed_items} 个完成，${progress.failed_items} 个失败。可以重试失败项目。`
                : progress.cancelled_items > 0
                  ? `已取消导出，${progress.completed_items} 个文件已完成。`
                  : `${progress.completed_items} 个视频导出完成。`,
          });
          return;
        }
        if (recovered)
          setNotice({ kind: "info", message: "已重新连接导出队列。" });
      } catch (error) {
        if (!active) return;
        failures += 1;
        const message = errorMessage(error);
        if (message.toLowerCase().includes("batch not found")) {
          changeClips((current) =>
            current.map((clip) =>
              ["queued", "processing"].includes(clip.status)
                ? {
                    ...clip,
                    status: "failed",
                    error: "后台队列已失效，请重新导出。",
                  }
                : clip,
            ),
          );
          exportingRef.current = false;
          cancelRequestedRef.current = false;
          batchIdRef.current = null;
          setRunningBatchId(null);
          setIsExporting(false);
          void setExportGuard(false).catch(() => {});
          setBatch((current) =>
            current ? { ...current, status: "failed" } : current,
          );
          setNotice({ kind: "error", message: "后台队列已失效，请重新导出。" });
          return;
        }
        if (failures === 3)
          setNotice({
            kind: "error",
            message: `暂时无法读取导出进度，正在重新连接。后台任务可能仍在运行。${message}`,
          });
      }
      if (active)
        timer = setTimeout(
          () => void poll(),
          Math.min(5000, 750 * Math.max(1, failures)),
        );
    };
    void poll();
    return () => {
      active = false;
      if (timer) clearTimeout(timer);
    };
  }, [runningBatchId, changeClips, setBatch, setExportGuard]);

  const persistSettings = useCallback(
    (next: AppSettings) => {
      const revision = ++settingsRevisionRef.current;
      settingsRef.current = next;
      setSettings(next);
      setIsSavingSettings(true);

      const write = settingsWritesRef.current.then(async () => {
        try {
          if (desktopAvailable())
            await invoke("update_app_settings", { settings: next });
          const previous = persistedSettingsRef.current;
          persistedSettingsRef.current = next;
          if (EXPORT_SETTING_KEYS.some((key) => previous[key] !== next[key])) {
            changeClips(invalidateCompleted);
            // Undoing an older edit must not revive a completed result for obsolete encoding settings.
            if (undoRef.current)
              undoRef.current = {
                ...undoRef.current,
                clips: invalidateCompleted(undoRef.current.clips),
              };
          }
          if (previous.ffmpeg_path !== next.ffmpeg_path)
            metadataGenerationRef.current += 1;
          if (revision === settingsRevisionRef.current) {
            const recovered = !!settingsErrorRef.current;
            settingsErrorRef.current = null;
            failedSettingsRef.current = null;
            if (mountedRef.current) {
              setSettingsError(null);
              if (recovered)
                setNotice((current) =>
                  current?.message.startsWith("设置保存失败")
                    ? { kind: "success", message: "设置已重新保存。" }
                    : current,
                );
            }
          }
        } catch (error) {
          // A stale failure cannot roll back a more recent pending edit.
          if (revision === settingsRevisionRef.current) {
            const message = errorMessage(error);
            settingsErrorRef.current = message;
            failedSettingsRef.current = next;
            settingsRef.current = persistedSettingsRef.current;
            if (mountedRef.current) {
              setSettings(persistedSettingsRef.current);
              setSettingsError(message);
              setNotice({
                kind: "error",
                message: `设置保存失败，已恢复已保存设置：${message}。可以重试未保存的修改。`,
              });
            }
          }
        } finally {
          if (revision === settingsRevisionRef.current && mountedRef.current)
            setIsSavingSettings(false);
        }
      });
      settingsWritesRef.current = write;
      if (next.ffmpeg_path !== persistedSettingsRef.current.ffmpeg_path)
        void write.then(refreshEngine);
      return write;
    },
    [changeClips, refreshEngine],
  );

  const updateSettings = useCallback(
    (patch: Partial<AppSettings>) => {
      if (exportingRef.current || loadingRef.current) return Promise.resolve();
      return persistSettings(
        normalizeSettings({ ...settingsRef.current, ...patch }),
      );
    },
    [persistSettings],
  );

  const retrySettings = useCallback(() => {
    if (exportingRef.current || !failedSettingsRef.current)
      return Promise.resolve();
    return persistSettings(failedSettingsRef.current);
  }, [persistSettings]);

  const discardSettingsError = useCallback(() => {
    if (isSavingSettings) return;
    failedSettingsRef.current = null;
    settingsErrorRef.current = null;
    setSettingsError(null);
    setNotice((current) =>
      current?.message.startsWith("设置保存失败") ? null : current,
    );
  }, [isSavingSettings]);

  const beginImport = useCallback(() => {
    if (
      !requireDesktop() ||
      exportingRef.current ||
      loadingRef.current ||
      importingRef.current
    )
      return false;
    importingRef.current = true;
    importCancelledRef.current = false;
    setIsImporting(true);
    setImportProgress({ completed: 0, total: 0 });
    return true;
  }, [requireDesktop]);

  const endImport = useCallback(() => {
    importingRef.current = false;
    if (mountedRef.current) setIsImporting(false);
  }, []);

  const cancelImport = useCallback(() => {
    if (!importingRef.current) return;
    importCancelledRef.current = true;
    setNotice({
      kind: "info",
      message: "已停止后续导入，正在读取的素材会保留。",
    });
  }, []);

  const addVideos = useCallback(
    async (paths: string[]) => {
      const existingIds = new Set(clipsRef.current.map((clip) => clip.id));
      const candidates = appendClips(
        clipsRef.current,
        paths,
        settingsRef.current.lut_intensity,
      ).filter((clip) => !existingIds.has(clip.id));
      let added = 0;
      setImportProgress((current) => ({
        ...current,
        total: current.total + candidates.length,
      }));
      await mapLimit(candidates, 3, async (clip) => {
        if (
          importCancelledRef.current ||
          exportingRef.current ||
          !mountedRef.current
        )
          return;
        changeClips((current) => [...current, clip]);
        added += 1;
        if (!activeIdRef.current) {
          setActiveId(clip.id);
          setSelectedIds([clip.id]);
          selectionAnchorRef.current = clip.id;
        }
        await readVideoMetadata(clip);
        if (mountedRef.current)
          setImportProgress((current) => ({
            ...current,
            completed: current.completed + 1,
          }));
      });
      return added;
    },
    [changeClips, readVideoMetadata, setActiveId, setSelectedIds],
  );

  const addLuts = useCallback(
    async (paths: string[]) => {
      if (!paths.length || importCancelledRef.current || exportingRef.current)
        return 0;
      const items = await invoke<LutLibraryItem[]>("remember_lut_files", {
        paths: [...new Set(paths)],
      });
      replaceLuts(mergeLuts(lutsRef.current, items));
      return items.length;
    },
    [replaceLuts],
  );

  const importVideos = useCallback(
    async (paths?: string[]) => {
      if (!beginImport()) return;
      try {
        const picked =
          paths ??
          (await open({
            title: "导入视频",
            multiple: true,
            filters: [{ name: "视频", extensions: VIDEO_EXTENSIONS }],
          }));
        if (!picked || importCancelledRef.current) return;
        const rawPaths = Array.isArray(picked) ? picked : [picked];
        const selected = [
          ...new Map(rawPaths.map((path) => [pathKey(path), path])).values(),
        ];
        const discovered = new Map<
          string,
          { videos: string[]; luts: string[] }
        >();
        const issues: string[] = [];
        await mapLimit(selected, 4, async (path) => {
          if (importCancelledRef.current || exportingRef.current) return;
          try {
            const info = await invoke<{ is_directory: boolean }>(
              "get_file_info",
              { path },
            );
            if (info.is_directory) {
              const scanned = await invoke<ScanResult>(
                "scan_directory_for_videos",
                { directory: path },
              );
              discovered.set(path, {
                videos: scanned.video_files.sort((a, b) =>
                  a.localeCompare(b, "zh-CN", { numeric: true }),
                ),
                luts: scanned.lut_files,
              });
            } else if (VIDEO_EXTENSIONS.includes(extension(path)))
              discovered.set(path, { videos: [path], luts: [] });
            else if (LUT_EXTENSIONS.includes(extension(path)))
              discovered.set(path, { videos: [], luts: [path] });
            else issues.push(path);
          } catch (error) {
            issues.push(`${path}：${errorMessage(error)}`);
          }
        });
        // Preserve the selection order even when metadata reads complete out of order.
        const videos = selected.flatMap(
          (path) => discovered.get(path)?.videos ?? [],
        );
        const lookPaths = selected.flatMap(
          (path) => discovered.get(path)?.luts ?? [],
        );
        const videoCount = await addVideos(videos);
        const lutCount = await addLuts(lookPaths);
        if (exportingRef.current) return;
        setNotice({
          kind: issues.length ? "error" : "success",
          message: `${importCancelledRef.current ? "导入已停止，保留 " : "已导入 "}${videoCount} 个视频${lutCount ? `、${lutCount} 个 LUT` : ""}${issues.length ? `；${issues.length} 个文件无法导入：${issues[0]}` : "。重复素材已自动跳过。"}`,
        });
      } catch (error) {
        setNotice({
          kind: "error",
          message: `导入失败：${errorMessage(error)}`,
        });
      } finally {
        endImport();
      }
    },
    [addLuts, addVideos, beginImport, endImport],
  );

  const importDirectory = useCallback(async () => {
    if (!beginImport()) return;
    try {
      const directory = await open({
        title: "导入视频文件夹",
        directory: true,
        multiple: false,
      });
      if (!directory || Array.isArray(directory) || importCancelledRef.current)
        return;
      const result = await invoke<ScanResult>("scan_directory_for_videos", {
        directory,
      });
      const count = await addVideos(
        result.video_files.sort((a, b) =>
          a.localeCompare(b, "zh-CN", { numeric: true }),
        ),
      );
      if (exportingRef.current) return;
      setNotice({
        kind: count ? "success" : "info",
        message: count
          ? `已从文件夹导入 ${count} 个视频。`
          : "文件夹中没有新的视频素材。",
      });
    } catch (error) {
      setNotice({
        kind: "error",
        message: `扫描文件夹失败：${errorMessage(error)}`,
      });
    } finally {
      endImport();
    }
  }, [addVideos, beginImport, endImport]);

  const importLuts = useCallback(
    async (paths?: string[]) => {
      if (!beginImport()) return;
      try {
        const picked =
          paths ??
          (await open({
            title: "导入 LUT",
            multiple: true,
            filters: [{ name: "LUT", extensions: LUT_EXTENSIONS }],
          }));
        if (!picked || importCancelledRef.current) return;
        const count = await addLuts(Array.isArray(picked) ? picked : [picked]);
        setNotice({
          kind: "success",
          message: `已导入 ${count} 个 LUT。选择素材后即可应用风格。`,
        });
      } catch (error) {
        setNotice({
          kind: "error",
          message: `导入 LUT 失败：${errorMessage(error)}`,
        });
      } finally {
        endImport();
      }
    },
    [addLuts, beginImport, endImport],
  );

  const importLutDirectory = useCallback(async () => {
    if (!beginImport()) return;
    try {
      const directory = await open({
        title: "导入 LUT 文件夹",
        directory: true,
        multiple: false,
      });
      if (!directory || Array.isArray(directory) || importCancelledRef.current)
        return;
      const items = await invoke<LutLibraryItem[]>("import_lut_directory", {
        directory,
      });
      replaceLuts(mergeLuts(lutsRef.current, items));
      setNotice({
        kind: "success",
        message: `已导入 ${items.length} 个 LUT。`,
      });
    } catch (error) {
      setNotice({
        kind: "error",
        message: `导入 LUT 文件夹失败：${errorMessage(error)}`,
      });
    } finally {
      endImport();
    }
  }, [beginImport, endImport, replaceLuts]);

  const removeLut = useCallback(
    async (path: string) => {
      if (!beginImport()) return;
      try {
        await invoke("remove_lut_from_library", { lutPath: path });
        undoRef.current = null;
        setCanUndo(false);
        replaceLuts(
          lutsRef.current.filter(
            (item) => pathKey(item.path) !== pathKey(path),
          ),
        );
        changeClips((current) =>
          current.map((clip) =>
            clip.lutPath && pathKey(clip.lutPath) === pathKey(path)
              ? updateLook(clip, { lutPath: null })
              : clip,
          ),
        );
      } catch (error) {
        setNotice({
          kind: "error",
          message: `移除 LUT 失败：${errorMessage(error)}`,
        });
      } finally {
        endImport();
      }
    },
    [beginImport, changeClips, endImport, replaceLuts],
  );

  const rememberUndo = useCallback(() => {
    undoRef.current = {
      clips: clipsRef.current,
      activeId: activeIdRef.current,
      selectedIds: selectedIdsRef.current,
    };
    setCanUndo(true);
  }, []);

  const undo = useCallback(() => {
    const previous = undoRef.current;
    if (!previous || exportingRef.current || loadingRef.current) return;
    undoRef.current = null;
    setCanUndo(false);
    changeClips((current) => {
      const byId = new Map(current.map((clip) => [clip.id, clip]));
      const beforeIds = new Set(previous.clips.map((clip) => clip.id));
      // Metadata and newly imported clips are independent work, never undone by a look edit.
      return [
        ...previous.clips.map((clip) => {
          const latest = byId.get(clip.id);
          return latest
            ? {
                ...clip,
                info: latest.info,
                metadataError: latest.metadataError,
              }
            : clip;
        }),
        ...current.filter((clip) => !beforeIds.has(clip.id)),
      ];
    });
    setActiveId(previous.activeId);
    setSelectedIds(previous.selectedIds);
  }, [changeClips, setActiveId, setSelectedIds]);

  const selectClip = useCallback(
    (id: string, options: SelectionOptions = {}) => {
      if (!clipsRef.current.some((clip) => clip.id === id)) return;
      const known = new Set(clipsRef.current.map((clip) => clip.id));
      const order =
        options.visibleIds?.filter((candidate) => known.has(candidate)) ??
        clipsRef.current.map((clip) => clip.id);
      let ids: string[];
      if (
        options.range &&
        selectionAnchorRef.current &&
        order.includes(selectionAnchorRef.current) &&
        order.includes(id)
      ) {
        const anchor = order.indexOf(selectionAnchorRef.current);
        const target = order.indexOf(id);
        const range = order.slice(
          Math.min(anchor, target),
          Math.max(anchor, target) + 1,
        );
        ids = options.toggle
          ? [...new Set([...selectedIdsRef.current, ...range])]
          : range;
      } else if (options.toggle) {
        ids = selectedIdsRef.current.includes(id)
          ? selectedIdsRef.current.filter((candidate) => candidate !== id)
          : [...selectedIdsRef.current, id];
        selectionAnchorRef.current = id;
      } else {
        ids = [id];
        selectionAnchorRef.current = id;
      }
      setSelectedIds(ids);
      setActiveId(id);
    },
    [setActiveId, setSelectedIds],
  );

  const selectAll = useCallback(
    (ids?: string[]) => {
      const known = new Set(clipsRef.current.map((clip) => clip.id));
      const next = ids
        ? [...new Set(ids.filter((id) => known.has(id)))]
        : [...known];
      setSelectedIds(next);
      if (next.length && !next.includes(activeIdRef.current ?? ""))
        setActiveId(next[0]);
    },
    [setActiveId, setSelectedIds],
  );
  const clearSelection = useCallback(
    () => setSelectedIds([]),
    [setSelectedIds],
  );

  const removeClips = useCallback(
    (ids: string[]) => {
      if (exportingRef.current || loadingRef.current || !ids.length) return;
      const removed = new Set(ids);
      rememberUndo();
      const next = changeClips((current) =>
        current.filter((clip) => !removed.has(clip.id)),
      );
      setSelectedIds(selectedIdsRef.current.filter((id) => !removed.has(id)));
      if (activeIdRef.current && removed.has(activeIdRef.current))
        setActiveId(next[0]?.id ?? null);
    },
    [changeClips, rememberUndo, setActiveId, setSelectedIds],
  );

  const removeClip = useCallback(
    (id: string) => removeClips([id]),
    [removeClips],
  );
  const removeSelected = useCallback(
    () => removeClips(selectedIdsRef.current),
    [removeClips],
  );
  const clearClips = useCallback(
    () => removeClips(clipsRef.current.map((clip) => clip.id)),
    [removeClips],
  );

  const setClipLook = useCallback(
    (id: string, patch: LookPatch) => {
      if (exportingRef.current) return;
      const previous = clipsRef.current;
      const next = previous.map((clip) =>
        clip.id === id ? updateLook(clip, patch) : clip,
      );
      if (next.every((clip, index) => clip === previous[index])) return;
      rememberUndo();
      changeClips(() => next);
    },
    [changeClips, rememberUndo],
  );

  const applyLook = useCallback(
    (ids: string[], sourceId?: string) => {
      if (exportingRef.current) return;
      const source = clipsRef.current.find(
        (clip) => clip.id === (sourceId ?? activeIdRef.current),
      );
      if (!source) return;
      const chosen = new Set(ids);
      const previous = clipsRef.current;
      const next = previous.map((clip) =>
        chosen.has(clip.id)
          ? updateLook(clip, {
              lutPath: source.lutPath,
              intensity: source.intensity,
            })
          : clip,
      );
      if (next.some((clip, index) => clip !== previous[index])) {
        rememberUndo();
        changeClips(() => next);
      }
      setNotice({
        kind: "success",
        message: `已将当前风格和强度应用到 ${chosen.size} 个视频。`,
      });
    },
    [changeClips, rememberUndo],
  );

  const applyLookToAll = useCallback(
    (sourceId?: string) =>
      applyLook(
        clipsRef.current.map((clip) => clip.id),
        sourceId,
      ),
    [applyLook],
  );
  const applyLookToSelected = useCallback(
    (sourceId?: string) => applyLook(selectedIdsRef.current, sourceId),
    [applyLook],
  );

  const cancelExport = useCallback(async () => {
    if (!exportingRef.current || cancelRequestedRef.current) return true;
    setCancelError(null);
    cancelRequestedRef.current = true;
    setBatch((current) =>
      current ? { ...current, status: "cancelling" } : current,
    );
    const batchId = batchIdRef.current;
    if (!batchId) return true; // startExport sends cancellation after it receives the ID.
    try {
      await invoke("cancel_batch", { batchId });
      if (batchIdRef.current === batchId)
        setNotice({
          kind: "info",
          message: "正在停止导出，等待运行中的任务结束…",
        });
      return true;
    } catch (error) {
      if (batchIdRef.current !== batchId) return true;
      setCancelError(errorMessage(error));
      cancelRequestedRef.current = false;
      setBatch((current) =>
        current && !isBatchFinished(current.status)
          ? { ...current, status: "running" }
          : current,
      );
      setNotice({
        kind: "error",
        message: `取消请求失败，请重试：${errorMessage(error)}`,
      });
      return false;
    }
  }, [setBatch]);

  const startExport = useCallback(
    async (ids?: string[]) => {
      if (!requireDesktop() || exportingRef.current || loadingRef.current)
        return;
      if (importingRef.current) importCancelledRef.current = true;
      const chosen = ids ? new Set(ids) : null;
      const queue = clipsRef.current.filter((clip) =>
        chosen ? chosen.has(clip.id) : clip.status !== "completed",
      );
      if (!queue.length) {
        setNotice({
          kind: "info",
          message: "没有待导出的视频。请导入素材或修改风格后重新导出。",
        });
        return;
      }
      const invalidLuts = new Set(
        lutsRef.current
          .filter((lut) => !lut.is_valid)
          .map((lut) => pathKey(lut.path)),
      );
      // Restored clips may reference valid files absent from a rebuilt library.
      // The backend validates every LUT path before any job is started.
      const invalid = queue.find(
        (clip) => clip.lutPath && invalidLuts.has(pathKey(clip.lutPath)),
      );
      if (invalid) {
        setNotice({
          kind: "error",
          message: `“${invalid.name}”的 LUT 不可用，请重新选择风格。`,
        });
        return;
      }
      // The ref locks synchronously, including multiple clicks before React has rendered.
      exportingRef.current = true;
      cancelRequestedRef.current = false;
      setCancelError(null);
      setIsExporting(true);
      setNotice(null);
      // Queue identity is captured now; settings are read after pending writes settle.
      undoRef.current = null;
      setCanUndo(false);
      const queuedIds = new Set(queue.map((clip) => clip.id));
      changeClips((current) =>
        current.map((clip) =>
          queuedIds.has(clip.id)
            ? {
                ...clip,
                status: "queued",
                progress: 0,
                error: undefined,
                outputHistory: rememberOutput(clip),
                outputPath: undefined,
                encoder: undefined,
                speed: undefined,
                eta_seconds: undefined,
                message: undefined,
              }
            : clip,
        ),
      );
      setBatch({
        batch_id: "",
        total_items: queue.length,
        completed_items: 0,
        failed_items: 0,
        cancelled_items: 0,
        overall_progress: 0,
        status: "starting",
        errors: [],
        items: [],
      });
      try {
        await setExportGuard(true);
        await settingsWritesRef.current;
        if (settingsErrorRef.current)
          throw new Error(`请先解决设置保存错误：${settingsErrorRef.current}`);
        const result = await invoke<BatchResponse>("start_batch_processing", {
          request: buildBatchRequest(queue, {
            ...persistedSettingsRef.current,
          }),
        });
        batchIdRef.current = result.batch_id;
        setBatch((current) =>
          current
            ? {
                ...current,
                batch_id: result.batch_id,
                status: cancelRequestedRef.current
                  ? "cancelling"
                  : result.status,
              }
            : current,
        );
        setRunningBatchId(result.batch_id);
        if (cancelRequestedRef.current) {
          try {
            await invoke("cancel_batch", { batchId: result.batch_id });
          } catch (error) {
            if (batchIdRef.current !== result.batch_id) return;
            setCancelError(errorMessage(error));
            cancelRequestedRef.current = false;
            setBatch((current) =>
              current && !isBatchFinished(current.status)
                ? { ...current, status: "running" }
                : current,
            );
            setNotice({
              kind: "error",
              message: `取消请求失败，请重试：${errorMessage(error)}`,
            });
          }
        }
      } catch (error) {
        exportingRef.current = false;
        cancelRequestedRef.current = false;
        setIsExporting(false);
        void setExportGuard(false).catch(() => {});
        const message = errorMessage(error);
        changeClips((current) =>
          current.map((clip) =>
            queuedIds.has(clip.id)
              ? { ...clip, status: "failed", error: message }
              : clip,
          ),
        );
        setBatch((current) =>
          current
            ? {
                ...current,
                status: "failed",
                failed_items: queue.length,
                errors: [message],
              }
            : current,
        );
        setNotice({ kind: "error", message: `无法开始导出：${message}` });
      }
    },
    [changeClips, requireDesktop, setBatch, setExportGuard],
  );

  const exportSelected = useCallback(
    () => startExport(selectedIdsRef.current),
    [startExport],
  );

  const retryFailed = useCallback(
    () =>
      startExport(
        clipsRef.current
          .filter(
            (clip) => clip.status === "failed" || clip.status === "cancelled",
          )
          .map((clip) => clip.id),
      ),
    [startExport],
  );

  const pickOutputDirectory = useCallback(async () => {
    if (!requireDesktop() || exportingRef.current || loadingRef.current) return;
    try {
      const directory = await open({
        title: "选择导出位置",
        directory: true,
        multiple: false,
      });
      if (directory && !Array.isArray(directory))
        await updateSettings({ default_output_dir: directory });
    } catch (error) {
      setNotice({ kind: "error", message: errorMessage(error) });
    }
  }, [requireDesktop, updateSettings]);

  const pickFfmpegPath = useCallback(async () => {
    if (!requireDesktop() || exportingRef.current || loadingRef.current) return;
    try {
      const path = await open({
        title: "选择 FFmpeg 可执行文件",
        multiple: false,
      });
      if (path && !Array.isArray(path))
        await updateSettings({ ffmpeg_path: path });
    } catch (error) {
      setNotice({ kind: "error", message: errorMessage(error) });
    }
  }, [requireDesktop, updateSettings]);

  return {
    clips,
    activeId,
    setActiveId,
    selectedIds,
    selectClip,
    selectAll,
    clearSelection,
    applyLookToSelected,
    removeSelected,
    exportSelected,
    undo,
    canUndo,
    activeClip: clips.find((clip) => clip.id === activeId) ?? null,
    luts,
    settings,
    updateSettings,
    settingsError,
    isSavingSettings,
    retrySettings,
    discardSettingsError,
    workspaceSaveError,
    flushWorkspace,
    retryWorkspaceSave,
    reloadPersistentState,
    isImporting,
    importProgress,
    cancelImport,
    history,
    importVideos,
    importDirectory,
    importLuts,
    importLutDirectory,
    removeClip,
    clearClips,
    removeLut,
    setClipLook,
    applyLookToAll,
    startExport,
    retryFailed,
    cancelExport,
    cancelError,
    batch,
    isExporting,
    notice,
    setNotice,
    ffmpeg,
    refreshEngine,
    pickOutputDirectory,
    pickFfmpegPath,
    loading,
    isDesktop,
  };
}

export default useWorkspace;
