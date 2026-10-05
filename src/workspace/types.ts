export interface VideoInfo {
  path: string;
  filename: string;
  size: number;
  duration: number | null;
  width: number | null;
  height: number | null;
  fps: number | null;
  codec: string | null;
  bitrate: number | null;
  created_at?: string | null;
  modified_at?: string | null;
  bit_depth?: number | null;
  color_primaries?: string | null;
  color_transfer?: string | null;
  color_matrix?: string | null;
  color_range?: string | null;
  pixel_format?: string | null;
}

export type ClipStatus =
  | "ready"
  | "queued"
  | "processing"
  | "completed"
  | "failed"
  | "cancelled";

export interface Clip {
  id: string;
  path: string;
  name: string;
  info?: VideoInfo;
  metadataError?: string;
  lutPath: string | null;
  intensity: number;
  status: ClipStatus;
  progress: number;
  outputPath?: string;
  outputHistory?: string[];
  encoder?: string;
  speed?: number;
  eta_seconds?: number;
  message?: string;
  error?: string;
}

export interface LutLibraryItem {
  id?: number | null;
  path: string;
  name: string;
  size: number;
  lut_type: string;
  format: string;
  category: string;
  is_valid: boolean;
  error_message?: string | null;
  updated_at: string;
}

export interface AppSettings {
  default_output_dir: string;
  ffmpeg_path: string;
  max_concurrent_tasks: number;
  cache_size_mb: number;
  hardware_acceleration: boolean;
  log_level: string;
  ui_theme: string;
  language: string;
  output_format: string;
  video_codec: string;
  audio_codec: string;
  quality_preset: string;
  resolution: string;
  fps: number | null;
  bitrate: string;
  lut_intensity: number;
  lut_error_strategy: string;
  color_space: string;
  two_pass_encoding: boolean;
  preserve_metadata: boolean;
  output_bit_depth: "8" | "10";
  input_color_space: "auto" | "rec709" | "rec2020-pq" | "rec2020-hlg";
  preview_quality: "fast" | "accurate";
}

export interface FfmpegInfo {
  mode: string;
  static_linked: boolean;
  library_versions: Record<string, string> | null;
  binary_version: string | null;
  binary_path: string | null;
}

export interface EngineState {
  status: "checking" | "ready" | "error" | "browser";
  info?: FfmpegInfo;
  error?: string;
}

export interface BatchItemProgress {
  input_path: string;
  output_path: string;
  status: string;
  progress: number;
  error?: string | null;
  encoder?: string | null;
  speed?: number | null;
  eta_seconds?: number | null;
  message?: string | null;
}

export interface BatchProgress {
  batch_id: string;
  total_items: number;
  completed_items: number;
  failed_items: number;
  cancelled_items: number;
  current_item?: string | null;
  overall_progress: number;
  status: string;
  errors: string[];
  items: BatchItemProgress[];
}

export interface BatchResponse {
  batch_id: string;
  total_items: number;
  status: string;
  message: string;
}

export interface ScanResult {
  video_files: string[];
  lut_files: string[];
  total_size: number;
  estimated_time?: number | null;
}

export interface Notice {
  kind: "info" | "success" | "error";
  message: string;
}

export type LookPatch = Pick<Partial<Clip>, "lutPath" | "intensity">;

export interface WorkspaceSnapshot {
  version: 1;
  clips: Clip[];
  activeId: string | null;
  selectedIds: string[];
  batch: BatchProgress | null;
  history: BatchProgress[];
}

export interface SelectionOptions {
  toggle?: boolean;
  range?: boolean;
  visibleIds?: string[];
}
