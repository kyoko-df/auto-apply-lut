import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { DEFAULT_SETTINGS } from "./model";
import { useWorkspace } from "./useWorkspace";
import { ALL_LUTS, UNCATEGORIZED_LUTS, categoryName, filterLuts } from "./lutLibrary";
import type { LutCategory, LutLibraryItem } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
const ipc = vi.mocked(invoke);
const first: LutLibraryItem = { path: "/luts/暖色's.cube", name: "Warm Film", format: "CUBE", category: "3D LUT",
  size: 100, lut_type: "ThreeDimensional", updated_at: "unchanged-content-time", is_valid: true, is_favorite: false, category_id: null };
let library: LutLibraryItem[], categories: LutCategory[];

beforeEach(() => {
  ipc.mockReset();
  library = [structuredClone(first), { ...first, path: "/luts/cool.cube", name: "Cool Film" }];
  categories = [{ id: "night", name: "Night" }];
  ipc.mockImplementation(async (command, args) => {
    const request = (args as { request?: Record<string, unknown> } | undefined)?.request ?? {};
    switch (command) {
      case "get_app_settings": return structuredClone(DEFAULT_SETTINGS);
      case "list_lut_library": return structuredClone(library);
      case "list_lut_categories": return structuredClone(categories);
      case "get_ffmpeg_info": return { binary_path: "/bin/ffmpeg" };
      case "get_file_info": return { is_directory: false };
      case "get_video_info": return { path: (args as { path: string }).path, duration: 3, width: 64, height: 48 };
      case "set_lut_favorite": library = library.map(item => item.path === request.lut_path ? { ...item, is_favorite: request.is_favorite as boolean } : item); return;
      case "create_lut_category": {
        const category = { id: `category-${categories.length}`, name: request.name as string };
        categories.push(category); return category;
      }
      case "rename_lut_category": categories = categories.map(c => c.id === request.id ? { ...c, name: request.name as string } : c); return categories.find(c => c.id === request.id);
      case "delete_lut_category": {
        const id = (args as { categoryId: string }).categoryId;
        categories = categories.filter(c => c.id !== id);
        library = library.map(item => item.category_id === id ? { ...item, category_id: null } : item); return;
      }
      case "assign_lut_category": library = library.map(item => (request.lut_paths as string[]).includes(item.path) ? { ...item, category_id: request.category_id as string | null } : item); return;
      case "start_batch_processing": return { batch_id: "complete", status: "Running", total_items: 1 };
      case "get_batch_progress": return { batch_id: "complete", status: "completed", total_items: 1, completed_items: 1,
        failed_items: 0, cancelled_items: 0, overall_progress: 100, errors: [],
        items: [{ input_path: "/a.mov", output_path: "/out/a.mp4", status: "completed", progress: 100 }] };
      default: return undefined;
    }
  });
});

async function load() {
  const hook = renderHook(() => useWorkspace());
  await waitFor(() => expect(hook.result.current.loading).toBe(false));
  return hook;
}

it("combines name, favorite and category filters and validates Unicode names", () => {
  library[0] = { ...library[0], is_favorite: true, category_id: "night" };
  expect(filterLuts(library, "FILM", false, ALL_LUTS)).toHaveLength(2);
  expect(filterLuts(library, " warm ", true, "night")).toEqual([library[0]]);
  expect(filterLuts(library, "cool", true, "night")).toEqual([]);
  expect(filterLuts(library, "", false, UNCATEGORIZED_LUTS)).toEqual([library[1]]);
  expect(categoryName("  晚霞's / 分类  ", categories)).toBe("晚霞's / 分类");
  expect(categoryName("🎞".repeat(80), [])).toHaveLength(160);
  expect(() => categoryName("night", categories)).toThrow(/同名/);
  expect(() => categoryName("x".repeat(81), [])).toThrow();
});

it("persists favorites and category management across sessions while preserving completed exports and engine state", async () => {
  const hook = await load();
  const { result } = hook;
  await act(async () => { await result.current.importVideos(["/a.mov"]); });
  act(() => result.current.setClipLook("/a.mov", { lutPath: first.path, intensity: 50 }));
  await act(async () => result.current.startExport());
  await waitFor(() => expect(result.current.clips[0].status).toBe("completed"));
  const clip = result.current.clips[0], settings = result.current.settings;
  const discoveryCalls = ipc.mock.calls.filter(([command]) => command === "get_ffmpeg_info").length;
  await act(async () => {
    await result.current.setLutFavorite(first.path, true);
    const category = await result.current.createLutCategory(" 日常暖色 ");
    await result.current.assignLutCategory(library.map(item => item.path), category.id);
    await result.current.renameLutCategory(category.id, "日常风格");
  });
  expect(result.current.clips[0]).toBe(clip);
  expect(result.current.settings).toBe(settings);
  expect(result.current.luts[0].updated_at).toBe(first.updated_at);
  expect(ipc.mock.calls.filter(([command]) => command === "get_ffmpeg_info")).toHaveLength(discoveryCalls);
  const storedCategory = categories.find(c => c.name === "日常风格")!;
  hook.unmount();
  const next = await load();
  expect(next.result.current.luts[0].is_favorite).toBe(true);
  expect(next.result.current.luts.every(item => item.category_id === storedCategory.id)).toBe(true);
  await act(async () => { await next.result.current.deleteLutCategory(storedCategory.id); });
  expect(next.result.current.luts.every(item => item.category_id === null)).toBe(true);
  expect(next.result.current.luts[0].is_favorite).toBe(true);
  expect(ipc).not.toHaveBeenCalledWith("remove_lut_from_library", expect.anything());
  expect(ipc).not.toHaveBeenCalledWith("update_app_settings", expect.anything());
});

it("does not update UI state after failed favorite, rename, create, delete or assignment writes", async () => {
  const { result } = await load();
  const before = result.current.luts, beforeCategories = result.current.lutCategories;
  const original = ipc.getMockImplementation()!;
  ipc.mockImplementation((command, args) => ["set_lut_favorite", "rename_lut_category", "create_lut_category", "delete_lut_category", "assign_lut_category"].includes(command)
    ? Promise.reject(new Error("Disk full")) : original(command, args));
  await act(async () => {
    await expect(result.current.setLutFavorite(first.path, true)).rejects.toThrow(/Disk full/);
    await expect(result.current.renameLutCategory("night", "新名称")).rejects.toThrow(/Disk full/);
    await expect(result.current.createLutCategory("新分类")).rejects.toThrow(/Disk full/);
    await expect(result.current.deleteLutCategory("night")).rejects.toThrow(/Disk full/);
    await expect(result.current.assignLutCategory([first.path], "night")).rejects.toThrow(/Disk full/);
  });
  expect(result.current.luts).toBe(before);
  expect(result.current.lutCategories).toBe(beforeCategories);
  expect(result.current.isLibraryBusy).toBe(false);
});

it("locks pending organization synchronously against imports and exports and awaits it on close", async () => {
  const { result } = await load();
  await act(async () => { await result.current.importVideos(["/a.mov"]); });
  let resolve!: () => void;
  const pending = new Promise<void>(yes => { resolve = yes; });
  const original = ipc.getMockImplementation()!;
  ipc.mockImplementation((command, args) => command === "set_lut_favorite" ? pending : original(command, args));
  let changing!: Promise<void>;
  act(() => { changing = result.current.setLutFavorite(first.path, true); });
  expect(result.current.isLibraryBusy).toBe(true);
  await act(async () => {
    await expect(result.current.createLutCategory("New")).rejects.toThrow();
    await expect(result.current.saveLookPreset("Preset")).rejects.toThrow();
    await result.current.startExport();
    await result.current.importLuts(["/new.cube"]);
  });
  expect(ipc).not.toHaveBeenCalledWith("start_batch_processing", expect.anything());
  expect(ipc).not.toHaveBeenCalledWith("remember_lut_files", expect.anything());
  let closed = false, closing!: Promise<void>;
  act(() => { closing = result.current.flushWorkspace().then(() => { closed = true; }); });
  expect(closed).toBe(false);
  await act(async () => { resolve(); await changing; await closing; });
  expect(result.current.luts[0].is_favorite).toBe(true);
  expect(closed).toBe(true);
});

it("blocks writes after a library read failure and recovers through an explicit reload", async () => {
  let fails = true;
  const original = ipc.getMockImplementation()!;
  ipc.mockImplementation((command, args) => command === "list_lut_categories" && fails
    ? Promise.reject(new Error("Read failed")) : original(command, args));
  const { result } = await load();
  expect(result.current.lutLibraryError).toContain("Read failed");
  await act(async () => { await expect(result.current.setLutFavorite(first.path, true)).rejects.toThrow(); });
  expect(ipc).not.toHaveBeenCalledWith("set_lut_favorite", expect.anything());
  fails = false;
  await act(async () => { await result.current.retryLutLibrary(); await result.current.setLutFavorite(first.path, true); });
  expect(result.current.lutLibraryError).toBeNull();
  expect(result.current.luts[0].is_favorite).toBe(true);
});

it("rejects stale selections, unknown categories and browser mutations without IPC writes", async () => {
  const hook = await load();
  await act(async () => {
    await expect(hook.result.current.assignLutCategory([first.path, "/missing.cube"], "night")).rejects.toThrow();
    await expect(hook.result.current.assignLutCategory([first.path], "missing")).rejects.toThrow();
    await expect(hook.result.current.createLutCategory("night")).rejects.toThrow();
  });
  expect(ipc).not.toHaveBeenCalledWith("assign_lut_category", expect.anything());
  hook.unmount();
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  const browser = await load();
  await act(async () => { await expect(browser.result.current.createLutCategory("Browser")).rejects.toThrow(); });
  expect(ipc).not.toHaveBeenCalledWith("create_lut_category", expect.anything());
});
