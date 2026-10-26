import { describe, expect, it } from "vitest";
import { applyPresetLook, presetTargets, validateLookPresets } from "./presets";
import type { Clip, LookPreset, PhotoClip } from "./types";

const preset: LookPreset = { id: "look-1", name: "暖色", media_mode: "video", lut_path: "/风格/film.cube",
  lut_fingerprint: "a".repeat(64), intensity: 45, raw: null };
const clip: Clip = { id: "/a.mov", path: "/a.mov", name: "a.mov", lutPath: null, intensity: 100,
  status: "completed", progress: 100, outputPath: "/out/a.mp4" };

describe("look preset contracts", () => {
  it("validates names, identity, limits, strengths, fingerprints and RAW settings without silently clamping", () => {
    expect(() => validateLookPresets([preset])).not.toThrow();
    for (const patch of [{ name: " " }, { name: "x".repeat(81) }, { name: "x\ny" },
      { intensity: NaN }, { intensity: 101 }, { lut_fingerprint: "bad" },
      { lut_path: null }, { id: "../look" },
      { raw: { mode: "raw", exposure_ev: 0, white_balance: "camera" } }]) {
      expect(() => validateLookPresets([{ ...preset, ...patch } as LookPreset])).toThrow();
    }
    expect(() => validateLookPresets([preset, { ...preset, id: "look-2" }])).toThrow();
    expect(() => validateLookPresets([preset, { ...preset, id: "look-2", media_mode: "photo" }])).not.toThrow();
    expect(() => validateLookPresets(Array.from({ length: 101 }, (_, i) => ({ ...preset, id: `look-${i}`, name: `Look ${i}` })))).toThrow();
  });

  it("invalidates completed output while retaining history", () => {
    const result = applyPresetLook(clip, preset);
    expect(result).toMatchObject({ lutPath: preset.lut_path, intensity: 45, status: "ready", progress: 0,
      outputHistory: ["/out/a.mp4"] });
    expect(result.outputPath).toBeUndefined();
    expect(clip.outputPath).toBe("/out/a.mp4");
  });

  it("preserves each photo's input profile and clears any previous sRGB consent", () => {
    const photo: PhotoClip = { ...clip, kind: "photo", info: undefined, path: "/a.jpg", lutPath: preset.lut_path,
      sourceInterpretation: { mode: "assign" as const, space: "display-p3" as const },
      lutSpace: "srgb" as const, lutFingerprint: preset.lut_fingerprint };
    const result = applyPresetLook(photo, { ...preset, media_mode: "photo" });
    expect(result.sourceInterpretation).toEqual(photo.sourceInterpretation);
    expect(result.lutSpace).toBeNull();
    expect(result.lutFingerprint).toBeNull();
  });

  it("rejects incompatible or unread media as a group before any application", () => {
    const rgb = { ...clip, kind: "photo" as const, path: "/a.jpg", info: {} } as Clip;
    const raw = { ...rgb, path: "/a.ARW" } as Clip;
    const rawPreset: LookPreset = { ...preset, media_mode: "photo", raw: { mode: "raw", exposure_ev: -1, white_balance: "daylight" } };
    expect(() => presetTargets([raw], rawPreset, "photo")).not.toThrow();
    expect(() => presetTargets([raw, rgb], rawPreset, "photo")).toThrow(/ARW/);
    expect(() => presetTargets([rgb], preset, "photo")).toThrow();
    expect(() => presetTargets([{ ...raw, info: undefined }], rawPreset, "photo")).toThrow();
    expect(() => presetTargets([], rawPreset, "photo")).toThrow();
  });
});
