import i18n from "../i18n";
import { extension, updateLook } from "./model";
import type { Clip, LookPreset, MediaMode } from "./types";

export const MAX_LOOK_PRESETS = 100;
export const MAX_PRESET_NAME = 80;

export function validateLookPresets(presets: LookPreset[]): void {
  if (presets.length > MAX_LOOK_PRESETS) throw new Error(i18n.t("presets.limit"));
  const ids = new Set<string>(), names = new Set<string>();
  for (const preset of presets) {
    if (!preset.id || preset.id.length > 128 || !/^[\w-]+$/.test(preset.id) || ids.has(preset.id))
      throw new Error(i18n.t("presets.invalid"));
    ids.add(preset.id);
    if (!preset.name.trim() || preset.name !== preset.name.trim() ||
      [...preset.name].length > MAX_PRESET_NAME || /[\u0000-\u001f\u007f-\u009f]/.test(preset.name))
      throw new Error(i18n.t("presets.badName"));
    const name = `${preset.media_mode}:${preset.name.toLowerCase()}`;
    if (names.has(name)) throw new Error(i18n.t("presets.duplicateName"));
    names.add(name);
    if (!["video", "photo"].includes(preset.media_mode) ||
      !Number.isFinite(preset.intensity) || preset.intensity < 0 || preset.intensity > 100)
      throw new Error(i18n.t("presets.invalid"));
    if (preset.lut_path === null ? preset.lut_fingerprint !== null :
      !preset.lut_path.trim() || [...preset.lut_path].length > 4096 || preset.lut_path.includes("\0") ||
      !/^[a-f0-9]{64}$/.test(preset.lut_fingerprint ?? ""))
      throw new Error(i18n.t("presets.invalid"));
    if (preset.raw && (preset.media_mode !== "photo" || preset.raw.mode !== "raw" ||
      !Number.isFinite(preset.raw.exposure_ev) || preset.raw.exposure_ev < -2 || preset.raw.exposure_ev > 3 ||
      !["camera", "daylight", "auto"].includes(preset.raw.white_balance)))
      throw new Error(i18n.t("presets.invalid"));
  }
}

export function presetTargets(clips: Clip[], preset: LookPreset, mode: MediaMode): void {
  if (preset.media_mode !== mode || clips.some(c => (c.kind === "photo" ? "photo" : "video") !== mode))
    throw new Error(i18n.t("presets.wrongMedia"));
  if (!clips.length) throw new Error(i18n.t("presets.noTargets"));
  if (clips.some(c => !c.info || c.metadataError || ["queued", "processing"].includes(c.status)))
    throw new Error(i18n.t("presets.readInfoFirst"));
  if (preset.raw && clips.some(c => c.kind !== "photo" || extension(c.path) !== "arw"))
    throw new Error(i18n.t("presets.rawOnly"));
}

export function applyPresetLook(clip: Clip, preset: LookPreset): Clip {
  return updateLook(clip, {
    lutPath: preset.lut_path,
    intensity: preset.intensity,
    ...(clip.kind === "photo" ? { lutSpace: null, lutFingerprint: null } : {}),
    ...(preset.raw ? { sourceInterpretation: { ...preset.raw } } : {}),
  });
}
