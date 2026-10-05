import type {
  AppSettings,
  BatchProgress,
  Clip,
  ClipStatus,
  LookPatch,
  LutLibraryItem,
} from "./types";

export const VIDEO_EXTENSIONS = [
  "mp4",
  "mov",
  "mkv",
  "avi",
  "webm",
  "m4v",
  "wmv",
  "flv",
];
export const LUT_EXTENSIONS = ["cube", "3dl", "csp", "m3d", "look", "lut"];

export const DEFAULT_SETTINGS: AppSettings = {
  default_output_dir: "",
  ffmpeg_path: "",
  max_concurrent_tasks: 2,
  cache_size_mb: 1024,
  hardware_acceleration: true,
  log_level: "info",
  ui_theme: "dark",
  language: "zh-CN",
  output_format: "mp4",
  video_codec: "libx264",
  audio_codec: "aac",
  quality_preset: "balanced",
  resolution: "original",
  fps: null,
  bitrate: "auto",
  lut_intensity: 100,
  lut_error_strategy: "StopOnError",
  color_space: "original",
  two_pass_encoding: false,
  preserve_metadata: true,
  output_bit_depth: "8",
  input_color_space: "auto",
  preview_quality: "fast",
};

export const fileName = (path: string) => path.split(/[\\/]/).pop() || path;
export const extension = (path: string) =>
  fileName(path).split(".").pop()?.toLowerCase() || "";
export const clampPercent = (value: number) =>
  Number.isFinite(value) ? Math.min(100, Math.max(0, value)) : 0;

// Windows drive paths are case insensitive. Keep POSIX casing intact.
export function pathKey(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  return /^[a-z]:\//i.test(normalized) ? normalized.toLowerCase() : normalized;
}

export function normalizeSettings(settings: Partial<AppSettings>): AppSettings {
  const merged = { ...DEFAULT_SETTINGS, ...settings };
  const videoCodec = ["libx264", "libx265", "prores_ks"].includes(
    merged.video_codec,
  )
    ? merged.video_codec
    : DEFAULT_SETTINGS.video_codec;
  const outputFormat =
    videoCodec === "prores_ks"
      ? "mov"
      : ["mp4", "mov", "mkv"].includes(merged.output_format)
        ? merged.output_format
        : DEFAULT_SETTINGS.output_format;
  const audioCodecs =
    outputFormat === "mp4" ? ["aac", "copy"] : ["aac", "copy", "pcm_s16le"];
  return {
    ...merged,
    output_format: outputFormat,
    video_codec: videoCodec,
    audio_codec: audioCodecs.includes(merged.audio_codec)
      ? merged.audio_codec
      : DEFAULT_SETTINGS.audio_codec,
    quality_preset: ["balanced", "high_quality", "fast"].includes(
      merged.quality_preset,
    )
      ? merged.quality_preset
      : DEFAULT_SETTINGS.quality_preset,
    resolution: ["original", "1920x1080", "3840x2160", "1280x720"].includes(
      merged.resolution,
    )
      ? merged.resolution
      : "original",
    hardware_acceleration:
      videoCodec === "prores_ks" ? false : merged.hardware_acceleration,
    max_concurrent_tasks: Math.min(
      4,
      Math.max(1, Math.round(Number(merged.max_concurrent_tasks) || 2)),
    ),
    lut_intensity: clampPercent(merged.lut_intensity),
    // The current export UI promises source frame rate and quality-based encoding.
    // Retired controls must not silently override that promise via an old config.
    fps: null,
    bitrate: "auto",
    // These controls represented metadata retagging in the legacy app, not a color transform.
    color_space: "original",
    two_pass_encoding: false,
    output_bit_depth:
      videoCodec === "prores_ks"
        ? "10"
        : videoCodec === "libx265" && merged.output_bit_depth === "10"
          ? "10"
          : "8",
    input_color_space: ["auto", "rec709", "rec2020-pq", "rec2020-hlg"].includes(
      merged.input_color_space,
    )
      ? merged.input_color_space
      : "auto",
    preview_quality:
      merged.preview_quality === "accurate" ? "accurate" : "fast",
  };
}

export function appendClips(
  existing: Clip[],
  paths: string[],
  intensity = 100,
): Clip[] {
  const known = new Set(existing.map((clip) => pathKey(clip.path)));
  const additions: Clip[] = [];
  for (const path of paths) {
    if (
      !path ||
      !VIDEO_EXTENSIONS.includes(extension(path)) ||
      known.has(pathKey(path))
    )
      continue;
    known.add(pathKey(path));
    additions.push({
      id: pathKey(path),
      path,
      name: fileName(path),
      lutPath: null,
      intensity: clampPercent(intensity),
      status: "ready",
      progress: 0,
    });
  }
  return [...existing, ...additions];
}

export function updateLook(clip: Clip, patch: LookPatch): Clip {
  if (clip.status === "queued" || clip.status === "processing") return clip;
  const lutPath = patch.lutPath === undefined ? clip.lutPath : patch.lutPath;
  const intensity =
    patch.intensity === undefined
      ? clip.intensity
      : clampPercent(patch.intensity);
  if (lutPath === clip.lutPath && intensity === clip.intensity) return clip;
  return {
    ...clip,
    lutPath,
    intensity,
    status: "ready",
    progress: 0,
    error: undefined,
    outputHistory: rememberOutput(clip),
    outputPath: undefined,
    encoder: undefined,
    speed: undefined,
    eta_seconds: undefined,
    message: undefined,
  };
}

export function mergeLuts(
  existing: LutLibraryItem[],
  incoming: LutLibraryItem[],
): LutLibraryItem[] {
  const merged = new Map(existing.map((item) => [pathKey(item.path), item]));
  for (const item of incoming) merged.set(pathKey(item.path), item);
  return [...merged.values()];
}

export function buildBatchRequest(clips: Clip[], settings: AppSettings) {
  return {
    items: clips.map((clip) => ({
      input_path: clip.path,
      output_path: "", // The backend reserves a unique destination; it must never overwrite a source.
      lut_paths: clip.lutPath ? [clip.lutPath] : [],
      lut_path: clip.lutPath,
      intensity: clampPercent(clip.intensity) / 100,
    })),
    output_directory: settings.default_output_dir,
    preserve_structure: false,
    max_concurrent: settings.max_concurrent_tasks,
    hardware_acceleration: settings.hardware_acceleration,
    output_format: settings.output_format,
    video_codec: settings.video_codec,
    audio_codec: settings.audio_codec,
    quality_preset: settings.quality_preset,
    resolution: settings.resolution,
    fps: settings.fps,
    bitrate: settings.bitrate,
    color_space: settings.color_space,
    two_pass_encoding: settings.two_pass_encoding,
    preserve_metadata: settings.preserve_metadata,
    output_bit_depth: settings.output_bit_depth,
    input_color_space: settings.input_color_space,
  };
}

export const isBatchFinished = (status: string) =>
  ["completed", "failed", "cancelled"].includes(status.toLowerCase());

function clipStatus(status: string): ClipStatus {
  switch (status.toLowerCase()) {
    case "completed":
      return "completed";
    case "failed":
      return "failed";
    case "cancelled":
      return "cancelled";
    case "running":
    case "processing":
      return "processing";
    default:
      return "queued";
  }
}

export function rememberOutput(clip: Clip): string[] | undefined {
  const paths = [
    ...new Set([
      ...(clip.outputHistory ?? []),
      ...(clip.status === "completed" && clip.outputPath
        ? [clip.outputPath]
        : []),
    ]),
  ];
  return paths.length ? paths.slice(-20) : undefined;
}

export function invalidateCompleted(clips: Clip[]): Clip[] {
  let changed = false;
  const next = clips.map((clip) => {
    if (clip.status !== "completed") return clip;
    changed = true;
    return {
      ...clip,
      status: "ready" as const,
      progress: 0,
      outputHistory: rememberOutput(clip),
      outputPath: undefined,
      error: undefined,
      encoder: undefined,
      speed: undefined,
      eta_seconds: undefined,
      message: undefined,
    };
  });
  return changed ? next : clips;
}

export function reconcileBatch(clips: Clip[], progress: BatchProgress): Clip[] {
  const byPath = new Map(
    progress.items.map((item) => [pathKey(item.input_path), item]),
  );
  let changed = false;
  const next = clips.map((clip) => {
    const item = byPath.get(pathKey(clip.path));
    if (!item) return clip;
    const status = clipStatus(item.status);
    const patch = {
      status,
      progress: status === "completed" ? 100 : clampPercent(item.progress),
      outputPath: item.output_path || undefined,
      error: item.error || undefined,
      encoder: item.encoder || undefined,
      speed: item.speed ?? undefined,
      eta_seconds: item.eta_seconds ?? undefined,
      message: item.message || undefined,
    };
    if (
      (Object.keys(patch) as Array<keyof typeof patch>).every(
        (key) => clip[key] === patch[key],
      )
    )
      return clip;
    changed = true;
    return { ...clip, ...patch };
  });
  return changed ? next : clips;
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "发生未知错误，请重试。";
}
