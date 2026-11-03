import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { DEFAULT_SETTINGS } from "./model";
import { useWorkspace } from "./useWorkspace";
import type { AppSettings, LookPreset } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
const ipc = vi.mocked(invoke);
const hash = "a".repeat(64);
const videoPreset: LookPreset = { id: "video-look", name: "Video look", media_mode: "video",
  lut_path: "/luts/暖色's.cube", lut_fingerprint: hash, intensity: 45, raw: null };
let stored: AppSettings;

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(yes => { resolve = yes; });
  return { promise, resolve };
}

beforeEach(() => {
  vi.clearAllMocks();
  stored = structuredClone(DEFAULT_SETTINGS);
  ipc.mockImplementation(async (command, args) => {
    switch (command) {
      case "get_app_settings": return structuredClone(stored);
      case "update_app_settings": stored = structuredClone((args as { settings: AppSettings }).settings); return "saved";
      case "get_ffmpeg_info": return { binary_path: "/bin/ffmpeg" };
      case "list_lut_categories": return [];
        case "list_lut_library": return [];
      case "get_file_info": return { is_directory: false };
      case "get_video_info": return { path: (args as { path: string }).path, width: 64, height: 48, duration: 3 };
      case "get_photo_info": {
        const path = (args as { path: string }).path;
        return { path, filename: path, format: /arw$/i.test(path) ? "arw" : "jpeg", width: 64, height: 48,
          stored_width: 64, stored_height: 48, bit_depth: 16, orientation: 1, has_alpha: false,
          color_status: "srgb", color_profile: "sRGB", source_version: "v1" };
      }
      case "get_photo_lut_fingerprint": return hash;
      case "validate_lut_file": return { is_valid: true, errors: [] };
      default: return undefined;
    }
  });
});

async function load(paths = ["/a.mov", "/b.mov"], photo = false) {
  const hook = renderHook(() => useWorkspace());
  await waitFor(() => expect(hook.result.current.loading).toBe(false));
  if (photo) act(() => hook.result.current.setMediaMode("photo"));
  await act(async () => hook.result.current.importVideos(paths));
  return hook;
}

describe("persisted look presets", () => {
  it("does not claim a new undoable edit when a preset already matches", async () => {
    stored.look_presets = [videoPreset];
    const { result } = await load();
    act(() => result.current.setClipLook("/a.mov", { lutPath: videoPreset.lut_path, intensity: 45 }));
    await act(async () => { expect(await result.current.applyLookPreset(videoPreset.id, "current")).toBe(0); });
    expect(result.current.notice?.message).toContain("无需修改");
    act(() => result.current.undo());
    expect(result.current.clips[0].lutPath).toBeNull();
  });
  it("saves only look settings, restores presets across sessions, applies to selected media and undoes the whole edit", async () => {
    stored = { ...stored, input_color_space: "sony-slog3-cine", video_codec: "libx265", output_bit_depth: "10" };
    const first = await load();
    act(() => first.result.current.setClipLook("/a.mov", { lutPath: videoPreset.lut_path, intensity: 45 }));
    let id = "";
    await act(async () => { id = await first.result.current.saveLookPreset("  日常暖色  "); });
    expect(stored.look_presets[0]).toEqual({ ...videoPreset, id, name: "日常暖色" });
    expect(stored.input_color_space).toBe("sony-slog3-cine");
    first.unmount();
    const { result } = await load();
    expect(result.current.settings.look_presets[0].name).toBe("日常暖色");
    act(() => result.current.selectAll());
    await act(async () => { await result.current.applyLookPreset(id, "selected"); });
    expect(result.current.clips.every(c => c.intensity === 45 && c.lutPath === videoPreset.lut_path)).toBe(true);
    expect(result.current.settings.video_codec).toBe("libx265");
    expect(result.current.settings.input_color_space).toBe("sony-slog3-cine");
    act(() => result.current.undo());
    expect(result.current.clips.every(c => c.intensity === 100 && c.lutPath === null)).toBe(true);
  });

  it("makes RAW settings optional, refuses a mixed RGB/ARW target and requires fresh photo LUT consent", async () => {
    const { result } = await load(["/a.ARW", "/b.jpg"], true);
    act(() => {
      result.current.selectClip("/a.ARW");
      result.current.setClipLook("/a.ARW", { lutPath: videoPreset.lut_path, intensity: 50,
        sourceInterpretation: { mode: "raw", exposure_ev: -1, white_balance: "daylight" },
        lutSpace: "srgb", lutFingerprint: hash });
    });
    let rawId = "", lookId = "";
    await act(async () => { rawId = await result.current.saveLookPreset("含 RAW", true); });
    await act(async () => { lookId = await result.current.saveLookPreset("仅风格"); });
    expect(stored.look_presets[0].raw).toMatchObject({ exposure_ev: -1, white_balance: "daylight" });
    expect(stored.look_presets[1].raw).toBeNull();
    act(() => result.current.selectAll());
    const before = result.current.clips;
    await act(async () => { await expect(result.current.applyLookPreset(rawId, "selected")).rejects.toThrow(/ARW/); });
    expect(result.current.clips).toStrictEqual(before);
    await act(async () => { await result.current.applyLookPreset(lookId, "selected"); });
    expect(result.current.clips.every(c => c.intensity === 50 && c.lutSpace === null && c.lutFingerprint === null)).toBe(true);
    expect(result.current.clips.find(c => c.path === "/b.jpg")?.sourceInterpretation).toEqual({ mode: "embedded" });
    expect(result.current.clips.find(c => c.path === "/a.ARW")?.sourceInterpretation).toMatchObject({ exposure_ev: -1 });
    act(() => result.current.undo());
    expect(result.current.clips.find(c => c.path === "/a.ARW")?.lutFingerprint).toBe(hash);
    expect(result.current.clips.find(c => c.path === "/b.jpg")?.lutPath).toBeNull();
    act(() => result.current.selectClip("/a.ARW"));
    act(() => result.current.setClipLook("/a.ARW", { sourceInterpretation: { mode: "raw", exposure_ev: 2, white_balance: "auto" } }));
    await act(async () => { await result.current.applyLookPreset(rawId, "current"); });
    expect(result.current.activeClip?.sourceInterpretation).toMatchObject({ exposure_ev: -1, white_balance: "daylight" });
  });

  it.each(["missing", "changed", "invalid"])("rejects a %s LUT without changing any target", async kind => {
    stored.look_presets = [videoPreset];
    const { result } = await load();
    act(() => result.current.selectAll());
    const before = result.current.clips;
    const original = ipc.getMockImplementation()!;
    ipc.mockImplementation((command, args) => {
      if (command === "get_photo_lut_fingerprint") return kind === "missing"
        ? Promise.reject(new Error("File not found")) : Promise.resolve(kind === "changed" ? "b".repeat(64) : hash);
      if (command === "validate_lut_file" && kind === "invalid") return Promise.resolve({ is_valid: false, errors: ["Invalid CUBE"] });
      return original(command, args);
    });
    await act(async () => { await expect(result.current.applyLookPreset(videoPreset.id, "selected")).rejects.toThrow(); });
    expect(result.current.clips).toStrictEqual(before);
    expect(result.current.canUndo).toBe(false);
  });

  it("locks async preset operations synchronously and flushes them before closing", async () => {
    stored.look_presets = [videoPreset];
    const { result } = await load();
    const fingerprint = deferred<string>();
    const original = ipc.getMockImplementation()!;
    ipc.mockImplementation((command, args) => command === "get_photo_lut_fingerprint" ? fingerprint.promise : original(command, args));
    let applying!: Promise<number>, closing!: Promise<void>;
    act(() => { applying = result.current.applyLookPreset(videoPreset.id, "current"); });
    expect(result.current.isPresetBusy).toBe(true);
    await act(async () => {
      await expect(result.current.applyLookPreset(videoPreset.id, "current")).rejects.toThrow();
      await result.current.startExport();
      result.current.setClipLook("/a.mov", { intensity: 10 });
      result.current.removeClip("/a.mov");
      result.current.selectClip("/b.mov");
      result.current.setMediaMode("photo");
    });
    expect(result.current.activeId).toBe("/a.mov");
    expect(result.current.mediaMode).toBe("video");
    expect(ipc).not.toHaveBeenCalledWith("start_batch_processing", expect.anything());
    let closed = false;
    act(() => { closing = result.current.flushWorkspace().then(() => { closed = true; }); });
    expect(closed).toBe(false);
    await act(async () => { fingerprint.resolve(hash); await applying; await closing; });
    expect(result.current.clips[0].intensity).toBe(45);
    expect(closed).toBe(true);
    expect(ipc).toHaveBeenCalledWith("save_workspace", { snapshot: expect.objectContaining({
      clips: expect.arrayContaining([expect.objectContaining({ id: "/a.mov", intensity: 45 })]),
    }) });
  });

  it("rolls back failed saves, renames and deletes; successful management does not change clips or LUTs", async () => {
    stored.look_presets = [videoPreset];
    const { result } = await load();
    const original = ipc.getMockImplementation()!;
    ipc.mockImplementation((command, args) => command === "update_app_settings" ? Promise.reject(new Error("Disk full")) : original(command, args));
    await act(async () => { await expect(result.current.saveLookPreset("未保存")).rejects.toThrow(/Disk full/); });
    expect(result.current.settings.look_presets).toEqual([videoPreset]);
    await act(async () => { await expect(result.current.renameLookPreset(videoPreset.id, "新名字")).rejects.toThrow(); });
    await act(async () => { await expect(result.current.deleteLookPreset(videoPreset.id)).rejects.toThrow(); });
    expect(stored.look_presets).toEqual([videoPreset]);
    ipc.mockImplementation(original);
    const before = result.current.clips;
    await act(async () => { await result.current.renameLookPreset(videoPreset.id, "新名字"); });
    expect(stored.look_presets[0].name).toBe("新名字");
    await act(async () => { await expect(result.current.saveLookPreset("新名字")).rejects.toThrow(/同名/); });
    await act(async () => { await result.current.deleteLookPreset(videoPreset.id); });
    expect(stored.look_presets).toEqual([]);
    expect(result.current.clips).toStrictEqual(before);
    expect(ipc).not.toHaveBeenCalledWith("remove_lut_from_library", expect.anything());
  });

  it("rejects photo presets in video mode even if called outside the picker", async () => {
    stored.look_presets = [{ ...videoPreset, media_mode: "photo" }];
    const { result } = await load();
    await act(async () => { await expect(result.current.applyLookPreset(videoPreset.id, "current")).rejects.toThrow(/混用/); });
    expect(result.current.clips[0].lutPath).toBeNull();
  });
});
