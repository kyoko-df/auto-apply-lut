import { appendClips, clampPercent, isBatchFinished, pathKey } from "./model";
import type {
  BatchProgress,
  Clip,
  VideoInfo,
  WorkspaceSnapshot,
  MediaMode,
  SourceInterpretation,
} from "./types";

const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown): string | undefined =>
  typeof value === "string" ? value : undefined;
const finite = (value: unknown): number | null =>
  typeof value === "number" && Number.isFinite(value) ? value : null;

function restoreInfo(value: unknown, clip: Clip): VideoInfo | undefined {
  if (!record(value)) return undefined;
  return {
    path: clip.path,
    filename: clip.name,
    size: finite(value.size) ?? 0,
    duration: finite(value.duration),
    width: finite(value.width),
    height: finite(value.height),
    fps: finite(value.fps),
    codec: text(value.codec) ?? null,
    bitrate: finite(value.bitrate),
    created_at: text(value.created_at),
    modified_at: text(value.modified_at),
    bit_depth: finite(value.bit_depth),
    pixel_format: text(value.pixel_format),
    color_primaries: text(value.color_primaries),
    color_transfer: text(value.color_transfer),
    color_matrix: text(value.color_matrix),
    color_range: text(value.color_range),
  };
}

function restoreSource(value: unknown): SourceInterpretation {
  if (
    record(value) &&
    value.mode === "assign" &&
    ["srgb", "adobe-rgb", "display-p3"].includes(String(value.space))
  )
    return {
      mode: "assign",
      space: value.space as "srgb" | "adobe-rgb" | "display-p3",
    };
  return { mode: "embedded" };
}

function restoreBatch(value: unknown): BatchProgress | null {
  if (
    !record(value) ||
    typeof value.batch_id !== "string" ||
    !Array.isArray(value.items)
  )
    return null;
  const interrupted = !isBatchFinished(String(value.status));
  const items = value.items
    .filter(record)
    .filter((item) => typeof item.input_path === "string")
    .map((item) => {
      const unfinished = !isBatchFinished(String(item.status));
      return {
        input_path: String(item.input_path),
        output_path: text(item.output_path) ?? "",
        status: unfinished ? "cancelled" : String(item.status).toLowerCase(),
        progress: clampPercent(finite(item.progress) ?? 0),
        error: unfinished ? "应用已退出，此项目待重试。" : text(item.error),
        encoder: text(item.encoder),
        speed: finite(item.speed),
        eta_seconds: undefined,
        message: text(item.message),
      };
    });
  return {
    batch_id: value.batch_id,
    total_items: items.length || finite(value.total_items) || 0,
    completed_items: items.filter((item) => item.status === "completed").length,
    failed_items: items.filter((item) => item.status === "failed").length,
    cancelled_items: items.filter((item) => item.status === "cancelled").length,
    overall_progress: clampPercent(finite(value.overall_progress) ?? 0),
    status: interrupted ? "cancelled" : String(value.status).toLowerCase(),
    errors: Array.isArray(value.errors)
      ? value.errors.filter((item): item is string => typeof item === "string")
      : [],
    items,
  };
}

/** Restores editing state only. An interrupted encoder is never resumed automatically. */
export function restoreWorkspace(value: unknown): WorkspaceSnapshot | null {
  if (value == null) return null;
  if (
    !record(value) ||
    (value.version !== 1 && value.version !== 2) ||
    !Array.isArray(value.clips)
  )
    throw new Error("工作区格式无法识别，原文件已保留。");
  const seen = new Set<string>();
  const clips: Clip[] = [];
  for (const raw of value.clips) {
    if (!record(raw) || typeof raw.path !== "string")
      throw new Error("工作区素材记录损坏，原文件已保留。");
    if (
      value.version === 2 &&
      raw.kind !== undefined &&
      !["photo", "video"].includes(String(raw.kind))
    )
      throw new Error("工作区媒体类型无法识别，原文件已保留。");
    const base = appendClips(
      [],
      [raw.path],
      100,
      value.version === 2 && raw.kind === "photo" ? "photo" : "video"
    )[0];
    if (!base) throw new Error("工作区包含无法识别的素材记录，原文件已保留。");
    if (seen.has(base.id)) continue;
    seen.add(base.id);
    const interrupted = raw.status === "queued" || raw.status === "processing";
    const status = ["ready", "completed", "failed", "cancelled"].includes(
      String(raw.status)
    )
      ? (String(raw.status) as Clip["status"])
      : interrupted
      ? "cancelled"
      : "ready";
    const common = {
      ...base,
      metadataError: text(raw.metadataError),
      lutPath: text(raw.lutPath) ?? null,
      intensity: clampPercent(finite(raw.intensity) ?? 100),
      status,
      progress: interrupted ? 0 : clampPercent(finite(raw.progress) ?? 0),
      outputPath: status === "completed" ? text(raw.outputPath) : undefined,
      outputHistory: Array.isArray(raw.outputHistory)
        ? raw.outputHistory
            .filter((path): path is string => typeof path === "string")
            .slice(-20)
        : undefined,
      error: interrupted ? "上次导出未完成，请重试。" : text(raw.error),
      encoder: text(raw.encoder),
    };
    clips.push(
      base.kind === "photo"
        ? {
            ...common,
            kind: "photo",
            info: undefined,
            sourceInterpretation: restoreSource(raw.sourceInterpretation),
            lutSpace: raw.lutSpace === "srgb" ? "srgb" : null,
            lutFingerprint:
              typeof raw.lutFingerprint === "string" &&
              /^[a-f0-9]{64}$/.test(raw.lutFingerprint)
                ? raw.lutFingerprint
                : null,
          }
        : { ...common, kind: base.kind, info: restoreInfo(raw.info, base) }
    );
  }
  const activeId =
    typeof value.activeId === "string" && seen.has(pathKey(value.activeId))
      ? pathKey(value.activeId)
      : clips[0]?.id ?? null;
  const selectedIds = Array.isArray(value.selectedIds)
    ? [
        ...new Set(
          value.selectedIds
            .filter((id): id is string => typeof id === "string")
            .map(pathKey)
            .filter((id) => seen.has(id))
        ),
      ]
    : activeId
    ? [activeId]
    : [];
  const history = Array.isArray(value.history)
    ? value.history
        .map(restoreBatch)
        .filter((batch): batch is BatchProgress => !!batch)
        .slice(-50)
    : [];
  const batch = restoreBatch(value.batch);
  if (batch && !history.some((item) => item.batch_id === batch.batch_id))
    history.push(batch);
  const mediaMode: MediaMode = value.mediaMode === "photo" ? "photo" : "video";
  const visible = clips.filter(
    (c) => (c.kind === "photo" ? "photo" : "video") === mediaMode
  );
  const scopedActive = visible.some((c) => c.id === activeId)
    ? activeId
    : visible[0]?.id ?? null;
  const scopedSelected = selectedIds.filter((id) =>
    visible.some((c) => c.id === id)
  );
  return {
    version: 2,
    mediaMode,
    mediaSelection: record(value.mediaSelection)
      ? Object.fromEntries(
          (["video", "photo"] as MediaMode[]).map((mode) => {
            const raw =
              value.mediaSelection && record(value.mediaSelection)
                ? value.mediaSelection[mode]
                : null;
            const ids = new Set(
              clips
                .filter(
                  (c) => (c.kind === "photo" ? "photo" : "video") === mode
                )
                .map((c) => c.id)
            );
            return [
              mode,
              {
                activeId:
                  record(raw) &&
                  typeof raw.activeId === "string" &&
                  ids.has(raw.activeId)
                    ? raw.activeId
                    : null,
                selectedIds:
                  record(raw) && Array.isArray(raw.selectedIds)
                    ? raw.selectedIds.filter(
                        (id): id is string =>
                          typeof id === "string" && ids.has(id)
                      )
                    : [],
              },
            ];
          })
        )
      : {
          video: { activeId, selectedIds },
          photo: { activeId: null, selectedIds: [] },
        },
    clips,
    activeId: scopedActive,
    selectedIds: scopedSelected,
    batch,
    history: history.slice(-50),
  };
}
