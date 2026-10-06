import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { useWorkspace } from "./useWorkspace";
import { DEFAULT_SETTINGS } from "./model";
import type { BatchProgress } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
const mockedInvoke = vi.mocked(invoke);

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

const running: BatchProgress = {
  batch_id: "batch-1",
  total_items: 1,
  completed_items: 0,
  failed_items: 0,
  cancelled_items: 0,
  overall_progress: 32,
  status: "running",
  errors: [],
  items: [
    {
      input_path: "/a.mov",
      output_path: "/out/a.mp4",
      progress: 32,
      status: "running",
    },
  ],
};

beforeEach(() => {
  vi.clearAllMocks();
  mockedInvoke.mockImplementation(async (command) => {
    switch (command) {
      case "get_app_settings":
        return { ...DEFAULT_SETTINGS };
      case "list_lut_library":
        return [];
      case "get_ffmpeg_info":
        return {
          mode: "external",
          static_linked: false,
          binary_path: "/usr/bin/ffmpeg",
          binary_version: "ffmpeg",
          library_versions: null,
        };
      case "get_file_info":
        return { is_directory: false };
      case "get_video_info":
        return {
          path: "/a.mov",
          filename: "a.mov",
          size: 100,
          duration: 10,
          width: 1920,
          height: 1080,
          fps: 24,
          codec: "h264",
          bitrate: 1000,
        };
      case "start_batch_processing":
        return {
          batch_id: "batch-1",
          status: "running",
          total_items: 1,
          message: "",
        };
      case "get_batch_progress":
        return running;
      default:
        return undefined;
    }
  });
});

async function loadedWorkspace() {
  const hook = renderHook(() => useWorkspace());
  await waitFor(() => expect(hook.result.current.loading).toBe(false));
  await act(async () => {
    await hook.result.current.importVideos(["/a.mov"]);
  });
  return hook;
}

describe("desktop workspace", () => {
  it("shows a finished batch only in its media mode while preserving shared history", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    mockedInvoke.mockImplementation((command, args) =>
      command === "get_batch_progress"
        ? Promise.resolve({
            ...running,
            status: "completed",
            completed_items: 1,
            overall_progress: 100,
            items: running.items.map((item) => ({
              ...item,
              status: "completed",
              progress: 100,
            })),
          })
        : original(command, args)
    );
    const { result } = await loadedWorkspace();
    await act(async () => result.current.startExport());
    await waitFor(() => expect(result.current.isExporting).toBe(false));
    expect(result.current.batch?.batch_id).toBe("batch-1");
    await act(async () => result.current.setMediaMode("photo"));
    expect(result.current.batch).toBeNull();
    expect(result.current.history).toHaveLength(1);
    await act(async () => result.current.setMediaMode("video"));
    expect(result.current.batch?.batch_id).toBe("batch-1");
  });
  it("imports photos independently, remembers media selections, and freezes a cancellable photo batch", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    const start = deferred<{
      batch_id: string;
      status: string;
      total_items: number;
      message: string;
    }>();
    mockedInvoke.mockImplementation((command, args) => {
      if (command === "get_photo_info")
        return Promise.resolve({
          path: (args as { path: string }).path,
          filename: "a.png",
          size: 100,
          format: "png",
          width: 800,
          height: 600,
          stored_width: 800,
          stored_height: 600,
          bit_depth: 16,
          has_alpha: true,
          orientation: 1,
          color_profile: "sRGB",
          color_status: "embedded",
          source_version: "v1",
        });
      if (command === "start_photo_batch_processing") return start.promise;
      return original(command, args);
    });
    const { result } = await loadedWorkspace();
    await act(async () => result.current.setMediaMode("photo"));
    await act(async () =>
      result.current.importVideos(["/a.png", "/b.tif", "/wrong.mov"])
    );
    expect(result.current.clips.map((c) => c.path)).toEqual([
      "/a.png",
      "/b.tif",
    ]);
    expect(result.current.allClips).toHaveLength(3);
    await act(async () => result.current.setMediaMode("video"));
    expect(result.current.activeClip?.path).toBe("/a.mov");
    await act(async () => result.current.setMediaMode("photo"));
    expect(result.current.activeClip?.path).toBe("/a.png");
    act(() =>
      result.current.setClipLook("/a.png", {
        intensity: 50,
        lutPath: "/film.cube",
        lutSpace: "srgb",
        lutFingerprint: "a".repeat(64),
      })
    );
    let exporting!: Promise<void>;
    act(() => {
      exporting = result.current.startExport();
      void result.current.startExport();
      result.current.setClipLook("/a.png", { intensity: 10 });
    });
    await waitFor(() =>
      expect(
        mockedInvoke.mock.calls.filter(
          ([name]) => name === "start_photo_batch_processing"
        )
      ).toHaveLength(1)
    );
    expect(
      mockedInvoke.mock.calls.find(
        ([name]) => name === "start_photo_batch_processing"
      )?.[1]
    ).toMatchObject({
      request: {
        items: [
          {
            input_path: "/a.png",
            intensity: 0.5,
            photo: {
              lut_space: "srgb",
              lut_fingerprint: "a".repeat(64),
              source_version: "v1",
            },
          },
          { input_path: "/b.tif" },
        ],
        photo_options: { output: { format: "png", bit_depth: 16 } },
      },
    });
    await act(async () => result.current.cancelExport());
    await act(async () => {
      start.resolve({
        batch_id: "photo-batch",
        status: "running",
        total_items: 2,
        message: "",
      });
      await exporting;
    });
    expect(mockedInvoke).toHaveBeenCalledWith("cancel_batch", {
      batchId: "photo-batch",
    });
    expect(result.current.clips[0].intensity).toBe(50);
  });
  it("undo restores the edited media mode and keeps metadata read after the edit", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    mockedInvoke.mockImplementation((command, args) =>
      command === "get_photo_info"
        ? Promise.resolve({
            path: "/photo.png",
            filename: "photo.png",
            size: 100,
            format: "png",
            width: 20,
            height: 30,
            stored_width: 20,
            stored_height: 30,
            bit_depth: 16,
            has_alpha: false,
            orientation: 1,
            color_profile: "sRGB",
            color_status: "embedded",
            source_version: "v1",
          })
        : original(command, args)
    );
    const { result } = await loadedWorkspace();
    act(() => result.current.setClipLook("/a.mov", { intensity: 40 }));
    await act(async () => result.current.setMediaMode("photo"));
    await act(async () => result.current.importVideos(["/photo.png"]));
    act(() => result.current.undo());
    expect(result.current.mediaMode).toBe("video");
    expect(result.current.activeClip?.path).toBe("/a.mov");
    expect(result.current.activeClip?.intensity).toBe(100);
    expect(
      result.current.allClips.find((c) => c.kind === "photo")?.info?.width
    ).toBe(20);
  });
  it("locks immediately against duplicate starts and keeps the submitted look immutable", async () => {
    const start = deferred<{
      batch_id: string;
      status: string;
      total_items: number;
      message: string;
    }>();
    const original = mockedInvoke.getMockImplementation()!;
    mockedInvoke.mockImplementation((command, args) =>
      command === "start_batch_processing"
        ? start.promise
        : original(command, args)
    );
    const { result } = await loadedWorkspace();
    let request!: Promise<void>;
    act(() => {
      request = result.current.startExport();
      void result.current.startExport();
      result.current.setClipLook("/a.mov", { intensity: 10 });
      result.current.removeClip("/a.mov");
    });
    await waitFor(() =>
      expect(
        mockedInvoke.mock.calls.filter(
          ([cmd]) => cmd === "start_batch_processing"
        )
      ).toHaveLength(1)
    );
    expect(result.current.clips[0].intensity).toBe(100);
    expect(result.current.isExporting).toBe(true);
    await act(async () => {
      start.resolve({
        batch_id: "batch-1",
        status: "running",
        total_items: 1,
        message: "",
      });
      await request;
    });
    await waitFor(() =>
      expect(result.current.clips[0].status).toBe("processing")
    );
  });

  it("honors cancellation even when the batch ID has not arrived yet", async () => {
    const start = deferred<{
      batch_id: string;
      status: string;
      total_items: number;
      message: string;
    }>();
    const original = mockedInvoke.getMockImplementation()!;
    mockedInvoke.mockImplementation((command, args) =>
      command === "start_batch_processing"
        ? start.promise
        : original(command, args)
    );
    const { result } = await loadedWorkspace();
    let request!: Promise<void>;
    act(() => {
      request = result.current.startExport();
    });
    await act(async () => {
      await result.current.cancelExport();
    });
    expect(
      mockedInvoke.mock.calls.filter(([cmd]) => cmd === "cancel_batch")
    ).toHaveLength(0);
    await act(async () => {
      start.resolve({
        batch_id: "batch-1",
        status: "running",
        total_items: 1,
        message: "",
      });
      await request;
    });
    expect(mockedInvoke).toHaveBeenCalledWith("cancel_batch", {
      batchId: "batch-1",
    });
    expect(result.current.isExporting).toBe(true);
    expect(result.current.batch?.status.toLowerCase()).toBe("cancelling");
  });

  it("preserves cancellation while an older running poll arrives and immediately restores retry after cancellation failure", async () => {
    const poll = deferred<BatchProgress>();
    const cancel = deferred<void>();
    const original = mockedInvoke.getMockImplementation()!;
    mockedInvoke.mockImplementation((command, args) => {
      if (command === "get_batch_progress") return poll.promise;
      if (command === "cancel_batch") return cancel.promise;
      return original(command, args);
    });
    const { result } = await loadedWorkspace();
    await act(async () => {
      await result.current.startExport();
    });
    let cancellation!: Promise<boolean>;
    act(() => {
      cancellation = result.current.cancelExport();
    });
    await act(async () => {
      poll.resolve(running);
    });
    expect(result.current.batch?.status.toLowerCase()).toBe("cancelling");
    await act(async () => {
      cancel.reject("IPC unavailable");
      await cancellation;
    });
    expect(result.current.batch?.status.toLowerCase()).toBe("running");
    expect(result.current.isExporting).toBe(true);
    await act(async () => {
      await result.current.cancelExport();
    });
    expect(
      mockedInvoke.mock.calls.filter(([cmd]) => cmd === "cancel_batch")
    ).toHaveLength(2);
  });

  it("restores cancellation retry when an early cancellation fails after start returns", async () => {
    const start = deferred<{
      batch_id: string;
      status: string;
      total_items: number;
      message: string;
    }>();
    const original = mockedInvoke.getMockImplementation()!;
    mockedInvoke.mockImplementation((command, args) => {
      if (command === "start_batch_processing") return start.promise;
      if (command === "cancel_batch") return Promise.reject("IPC unavailable");
      return original(command, args);
    });
    const { result } = await loadedWorkspace();
    let request!: Promise<void>;
    act(() => {
      request = result.current.startExport();
    });
    await act(async () => {
      await result.current.cancelExport();
    });
    await act(async () => {
      start.resolve({
        batch_id: "batch-1",
        status: "running",
        total_items: 1,
        message: "",
      });
      await request;
    });
    expect(result.current.batch?.status.toLowerCase()).toBe("running");
    expect(result.current.isExporting).toBe(true);
  });

  it("ignores a late cancellation error from an earlier batch after a new export begins", async () => {
    const firstPoll = deferred<BatchProgress>();
    const cancellation = deferred<void>();
    const original = mockedInvoke.getMockImplementation()!;
    let starts = 0;
    mockedInvoke.mockImplementation((command, args) => {
      if (command === "start_batch_processing")
        return Promise.resolve({
          batch_id: `batch-${++starts}`,
          status: "running",
          total_items: 1,
          message: "",
        });
      if (command === "get_batch_progress")
        return (args as { batchId: string }).batchId === "batch-1"
          ? firstPoll.promise
          : Promise.resolve({ ...running, batch_id: "batch-2" });
      if (command === "cancel_batch") return cancellation.promise;
      return original(command, args);
    });
    const { result } = await loadedWorkspace();
    await act(async () => {
      await result.current.startExport();
    });
    let cancelling!: Promise<boolean>;
    act(() => {
      cancelling = result.current.cancelExport();
    });
    await act(async () => {
      firstPoll.resolve({
        ...running,
        status: "completed",
        completed_items: 1,
        items: [{ ...running.items[0], status: "completed", progress: 100 }],
      });
    });
    expect(result.current.isExporting).toBe(false);
    await act(async () => {
      await result.current.startExport(["/a.mov"]);
    });
    await act(async () => {
      cancellation.reject("obsolete cancellation failure");
      await cancelling;
    });
    expect(result.current.batch?.batch_id).toBe("batch-2");
    expect(result.current.batch?.status.toLowerCase()).toBe("running");
    expect(result.current.notice).toBeNull();
  });

  it("releases a rejected start for retry, preserving the actual backend error", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    mockedInvoke.mockImplementation((command, args) =>
      command === "start_batch_processing"
        ? Promise.reject("encoder unavailable")
        : original(command, args)
    );
    const { result } = await loadedWorkspace();
    await act(async () => {
      await result.current.startExport();
    });
    expect(result.current.isExporting).toBe(false);
    expect(result.current.clips[0]).toMatchObject({
      status: "failed",
      error: "encoder unavailable",
    });
    expect(result.current.notice?.message).toContain("encoder unavailable");
    mockedInvoke.mockImplementation(original);
    await act(async () => {
      await result.current.retryFailed();
    });
    await waitFor(() =>
      expect(result.current.clips[0].status).toBe("processing")
    );
  });

  it("does not poll concurrently and waits for terminal status rather than 100 percent", async () => {
    const poll = deferred<BatchProgress>();
    const original = mockedInvoke.getMockImplementation()!;
    mockedInvoke.mockImplementation((command, args) =>
      command === "get_batch_progress" ? poll.promise : original(command, args)
    );
    const { result } = await loadedWorkspace();
    await act(async () => {
      await result.current.startExport();
    });
    expect(
      mockedInvoke.mock.calls.filter(([cmd]) => cmd === "get_batch_progress")
    ).toHaveLength(1);
    expect(result.current.isExporting).toBe(true);
    await act(async () => {
      poll.resolve({ ...running, overall_progress: 100 });
    });
    expect(result.current.isExporting).toBe(true);
    expect(result.current.clips[0].status).toBe("processing");
  });

  it("uses item outcomes at completion and makes a changed export configuration ready again", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    mockedInvoke.mockImplementation((command, args) =>
      command === "get_batch_progress"
        ? Promise.resolve({
            ...running,
            status: "completed",
            completed_items: 1,
            overall_progress: 100,
            items: [
              { ...running.items[0], progress: 100, status: "completed" },
            ],
          })
        : original(command, args)
    );
    const { result } = await loadedWorkspace();
    await act(async () => {
      await result.current.startExport();
    });
    await waitFor(() => expect(result.current.isExporting).toBe(false));
    expect(result.current.clips[0]).toMatchObject({
      status: "completed",
      outputPath: "/out/a.mp4",
    });
    await act(async () => {
      await result.current.updateSettings({ video_codec: "libx265" });
    });
    expect(result.current.clips[0].status).toBe("ready");
  });

  it("routes mixed dropped paths through native directory scanning and LUT validation", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    const lut = {
      path: "/looks/warm.cube",
      name: "Warm",
      is_valid: true,
      size: 512,
      lut_type: "Lut3D",
      format: "CUBE",
      category: "",
      updated_at: "",
    };
    mockedInvoke.mockImplementation(async (command, args) => {
      if (command === "get_file_info")
        return { is_directory: (args as { path: string }).path === "/footage" };
      if (command === "scan_directory_for_videos")
        return {
          video_files: ["/b.mov", "/a.mov"],
          lut_files: [],
          total_size: 1000,
        };
      if (command === "remember_lut_files") return [lut];
      return original(command, args);
    });
    const { result } = renderHook(() => useWorkspace());
    await waitFor(() => expect(result.current.loading).toBe(false));
    await act(async () => {
      await result.current.importVideos([
        "/footage",
        "/a.mov",
        "/looks/warm.cube",
      ]);
    });
    expect(result.current.clips.map((clip) => clip.path)).toEqual([
      "/a.mov",
      "/b.mov",
    ]);
    expect(result.current.luts).toEqual([lut]);
    expect(mockedInvoke).toHaveBeenCalledWith("scan_directory_for_videos", {
      directory: "/footage",
    });
    expect(mockedInvoke).toHaveBeenCalledWith("remember_lut_files", {
      paths: ["/looks/warm.cube"],
    });
  });

  it("serializes settings writes and waits for them before starting native processing", async () => {
    const firstWrite = deferred<void>();
    const original = mockedInvoke.getMockImplementation()!;
    let writes = 0;
    mockedInvoke.mockImplementation((command, args) => {
      if (command === "update_app_settings")
        return ++writes === 1 ? firstWrite.promise : Promise.resolve();
      return original(command, args);
    });
    const { result } = await loadedWorkspace();
    let exportPromise!: Promise<void>;
    act(() => {
      void result.current.updateSettings({ output_format: "mov" });
      void result.current.updateSettings({ video_codec: "prores_ks" });
      exportPromise = result.current.startExport();
    });
    await waitFor(() => expect(writes).toBe(1));
    expect(
      mockedInvoke.mock.calls.filter(
        ([cmd]) => cmd === "start_batch_processing"
      )
    ).toHaveLength(0);
    await act(async () => {
      firstWrite.resolve();
      await exportPromise;
    });
    expect(writes).toBe(2);
    const request = mockedInvoke.mock.calls.find(
      ([cmd]) => cmd === "start_batch_processing"
    )?.[1];
    expect(request).toMatchObject({
      request: { output_format: "mov", video_codec: "prores_ks" },
    });
  });

  it("retries missing metadata after correcting the FFmpeg path", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    let failMetadata = true;
    mockedInvoke.mockImplementation((command, args) =>
      command === "get_video_info" && failMetadata
        ? Promise.reject("ffprobe not found")
        : original(command, args)
    );
    const { result } = await loadedWorkspace();
    expect(result.current.activeClip?.metadataError).toBe("ffprobe not found");
    failMetadata = false;
    await act(async () => {
      await result.current.updateSettings({ ffmpeg_path: "/new/ffmpeg" });
    });
    await waitFor(() =>
      expect(
        result.current.activeClip?.kind !== "photo"
          ? result.current.activeClip?.info?.duration
          : undefined
      ).toBe(10)
    );
    expect(result.current.activeClip?.metadataError).toBeUndefined();
  });

  it("limits bulk metadata reads to three in flight and deduplicates dropped paths before probing", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    const reads: Array<ReturnType<typeof deferred<unknown>>> = [];
    let inFlight = 0;
    let maxInFlight = 0;
    mockedInvoke.mockImplementation((command, args) => {
      if (command !== "get_video_info") return original(command, args);
      inFlight += 1;
      maxInFlight = Math.max(maxInFlight, inFlight);
      const next = deferred<unknown>();
      reads.push(next);
      return next.promise.finally(() => {
        inFlight -= 1;
      });
    });
    const { result } = renderHook(() => useWorkspace());
    await waitFor(() => expect(result.current.loading).toBe(false));
    let imported!: Promise<void>;
    act(() => {
      imported = result.current.importVideos([
        "/a.mov",
        "/b.mov",
        "/c.mov",
        "/d.mov",
        "/e.mov",
        "/a.mov",
      ]);
    });
    await waitFor(() => expect(reads).toHaveLength(3));
    await act(async () => {
      reads[0].resolve({ duration: 10 });
    });
    await waitFor(() => expect(reads).toHaveLength(4));
    await act(async () => {
      reads[1].resolve({ duration: 10 });
    });
    await waitFor(() => expect(reads).toHaveLength(5));
    await act(async () => {
      reads.slice(2).forEach((read) => read.resolve({ duration: 10 }));
      await imported;
    });
    expect(maxInFlight).toBe(3);
    expect(result.current.clips).toHaveLength(5);
    expect(
      mockedInvoke.mock.calls.filter(([cmd]) => cmd === "get_file_info")
    ).toHaveLength(5);
  });

  it("rolls a failed settings write back without invalidating completed files, then retries the draft", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    let rejectWrite = true;
    mockedInvoke.mockImplementation((command, args) => {
      if (command === "get_batch_progress")
        return Promise.resolve({
          ...running,
          status: "completed",
          completed_items: 1,
          items: [{ ...running.items[0], status: "completed", progress: 100 }],
        });
      if (command === "update_app_settings" && rejectWrite)
        return Promise.reject("disk full");
      return original(command, args);
    });
    const { result } = await loadedWorkspace();
    await act(async () => {
      await result.current.startExport();
    });
    await waitFor(() => expect(result.current.isExporting).toBe(false));
    await act(async () => {
      await result.current.updateSettings({ video_codec: "libx265" });
    });
    expect(result.current.settings.video_codec).toBe("libx264");
    expect(result.current.settingsError).toBe("disk full");
    expect(result.current.clips[0]).toMatchObject({
      status: "completed",
      outputPath: "/out/a.mp4",
    });
    rejectWrite = false;
    await act(async () => {
      await result.current.retrySettings();
    });
    expect(result.current.settings.video_codec).toBe("libx265");
    expect(result.current.settingsError).toBeNull();
    expect(result.current.clips[0]).toMatchObject({
      status: "ready",
      outputHistory: ["/out/a.mp4"],
    });
    expect(result.current.history[0].items[0].output_path).toBe("/out/a.mp4");
  });

  it("does not let an earlier failed save overwrite a newer edit and rolls the last failure back to the last success", async () => {
    const first = deferred<void>();
    const second = deferred<void>();
    const original = mockedInvoke.getMockImplementation()!;
    let writes = 0;
    mockedInvoke.mockImplementation((command, args) =>
      command === "update_app_settings"
        ? ++writes === 1
          ? first.promise
          : second.promise
        : original(command, args)
    );
    const { result } = await loadedWorkspace();
    let done!: Promise<void>;
    act(() => {
      void result.current.updateSettings({ output_format: "mov" });
      done = result.current.updateSettings({ video_codec: "libx265" });
    });
    await act(async () => {
      first.reject("first failed");
    });
    expect(result.current.settings).toMatchObject({
      output_format: "mov",
      video_codec: "libx265",
    });
    await act(async () => {
      second.resolve();
      await done;
    });
    expect(result.current.settingsError).toBeNull();
    mockedInvoke.mockImplementation((command, args) =>
      command === "update_app_settings"
        ? Promise.reject("third failed")
        : original(command, args)
    );
    await act(async () => {
      await result.current.updateSettings({ output_format: "mkv" });
    });
    expect(result.current.settings).toMatchObject({
      output_format: "mov",
      video_codec: "libx265",
    });
  });

  it("restores a workspace without resuming interrupted encoders and flushes the latest look and selection", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    mockedInvoke.mockImplementation((command, args) =>
      command === "load_workspace"
        ? Promise.resolve({
            version: 1,
            clips: [
              {
                id: "/a.mov",
                path: "/a.mov",
                name: "a.mov",
                lutPath: "/warm.cube",
                intensity: 45,
                status: "processing",
                progress: 70,
              },
            ],
            activeId: "/a.mov",
            selectedIds: ["/a.mov"],
            batch: running,
            history: [],
          })
        : original(command, args)
    );
    const { result } = renderHook(() => useWorkspace());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.clips[0]).toMatchObject({
      status: "cancelled",
      lutPath: "/warm.cube",
      intensity: 45,
      progress: 0,
    });
    expect(result.current.isExporting).toBe(false);
    expect(result.current.batch?.status).toBe("cancelled");
    expect(
      mockedInvoke.mock.calls.some(([cmd]) => cmd === "start_batch_processing")
    ).toBe(false);
    await act(async () => {
      result.current.setClipLook("/a.mov", { intensity: 60 });
      await result.current.flushWorkspace();
    });
    const saves = mockedInvoke.mock.calls.filter(
      ([cmd]) => cmd === "save_workspace"
    );
    expect(saves.slice(-1)[0]?.[1]).toMatchObject({
      snapshot: {
        version: 2,
        selectedIds: ["/a.mov"],
        clips: [{ intensity: 60, status: "ready" }],
      },
    });
  });

  it("never overwrites an unreadable workspace during normal autosave or close", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    mockedInvoke.mockImplementation((command, args) =>
      command === "load_workspace"
        ? Promise.reject("permission denied")
        : original(command, args)
    );
    const { result } = renderHook(() => useWorkspace());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.workspaceSaveError).toContain("自动保存已暂停");
    await expect(result.current.flushWorkspace()).rejects.toThrow(
      "工作区读取失败"
    );
    expect(
      mockedInvoke.mock.calls.some(([cmd]) => cmd === "save_workspace")
    ).toBe(false);
  });

  it("serializes workspace writes and follows an in-flight save with the latest editing state", async () => {
    const first = deferred<void>();
    const original = mockedInvoke.getMockImplementation()!;
    let writes = 0;
    mockedInvoke.mockImplementation((command, args) =>
      command === "save_workspace"
        ? ++writes === 1
          ? first.promise
          : Promise.resolve()
        : original(command, args)
    );
    const { result } = await loadedWorkspace();
    let flush!: Promise<void>;
    act(() => {
      flush = result.current.flushWorkspace();
    });
    await waitFor(() => expect(writes).toBe(1));
    act(() => {
      result.current.setClipLook("/a.mov", { intensity: 25 });
    });
    await act(async () => {
      first.resolve();
      await flush;
    });
    expect(writes).toBe(2);
    expect(
      mockedInvoke.mock.calls
        .filter(([cmd]) => cmd === "save_workspace")
        .slice(-1)[0]?.[1]
    ).toMatchObject({ snapshot: { clips: [{ intensity: 25 }] } });
  });

  it("selects ranges within filtered rows, applies only to selected clips, and supports one undo", async () => {
    const { result } = await loadedWorkspace();
    await act(async () => {
      await result.current.importVideos(["/b.mov", "/c.mov", "/d.mov"]);
    });
    act(() => {
      result.current.setClipLook("/a.mov", { intensity: 42 });
      result.current.selectClip("/a.mov");
      result.current.selectClip("/d.mov", {
        range: true,
        visibleIds: ["/a.mov", "/c.mov", "/d.mov"],
      });
      result.current.applyLookToSelected("/a.mov");
    });
    expect(result.current.selectedIds).toEqual(["/a.mov", "/c.mov", "/d.mov"]);
    expect(result.current.clips.map((clip) => clip.intensity)).toEqual([
      42, 100, 42, 42,
    ]);
    act(() => {
      result.current.undo();
    });
    expect(result.current.clips.map((clip) => clip.intensity)).toEqual([
      42, 100, 100, 100,
    ]);
    expect(result.current.canUndo).toBe(false);
    act(() => {
      result.current.removeSelected();
    });
    expect(result.current.clips.map((clip) => clip.id)).toEqual(["/b.mov"]);
    act(() => {
      result.current.undo();
    });
    expect(result.current.clips).toHaveLength(4);
  });

  it("allows editing resolved clips during import and starts only the captured clips when exporting", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    const reads: Array<ReturnType<typeof deferred<unknown>>> = [];
    mockedInvoke.mockImplementation((command, args) => {
      if (command !== "get_video_info") return original(command, args);
      const next = deferred<unknown>();
      reads.push(next);
      return next.promise;
    });
    const { result } = renderHook(() => useWorkspace());
    await waitFor(() => expect(result.current.loading).toBe(false));
    let imported!: Promise<void>;
    act(() => {
      imported = result.current.importVideos([
        "/a.mov",
        "/b.mov",
        "/c.mov",
        "/d.mov",
        "/e.mov",
      ]);
    });
    await waitFor(() => expect(reads).toHaveLength(3));
    expect(result.current.loading).toBe(false);
    expect(result.current.isImporting).toBe(true);
    act(() => {
      result.current.setClipLook("/a.mov", { intensity: 30 });
    });
    expect(result.current.clips[0].intensity).toBe(30);
    await act(async () => {
      await result.current.startExport();
    });
    await act(async () => {
      reads.forEach((read) => read.resolve({ duration: 10 }));
      await imported;
    });
    expect(reads).toHaveLength(3);
    expect(result.current.clips).toHaveLength(3);
    const request = mockedInvoke.mock.calls.find(
      ([cmd]) => cmd === "start_batch_processing"
    )?.[1] as { request: { items: unknown[] } };
    expect(request.request.items).toHaveLength(3);
  });

  it("cancels only pending imports and keeps already started metadata reads", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    const reads: Array<ReturnType<typeof deferred<unknown>>> = [];
    mockedInvoke.mockImplementation((command, args) => {
      if (command !== "get_video_info") return original(command, args);
      const next = deferred<unknown>();
      reads.push(next);
      return next.promise;
    });
    const { result } = renderHook(() => useWorkspace());
    await waitFor(() => expect(result.current.loading).toBe(false));
    let imported!: Promise<void>;
    act(() => {
      imported = result.current.importVideos([
        "/a.mov",
        "/b.mov",
        "/c.mov",
        "/d.mov",
      ]);
    });
    await waitFor(() => expect(reads).toHaveLength(3));
    act(() => {
      result.current.cancelImport();
    });
    await act(async () => {
      reads.forEach((read) => read.resolve({ duration: 10 }));
      await imported;
    });
    expect(result.current.clips).toHaveLength(3);
    expect(
      result.current.clips.every(
        (clip) => clip.kind !== "photo" && clip.info?.duration === 10
      )
    ).toBe(true);
    expect(result.current.isImporting).toBe(false);
    expect(result.current.importProgress).toEqual({ completed: 3, total: 4 });
  });

  it("serializes exit protection across a completed batch and an immediate new start", async () => {
    const releaseGuard = deferred<void>();
    const original = mockedInvoke.getMockImplementation()!;
    let starts = 0;
    mockedInvoke.mockImplementation((command, args) => {
      if (command === "start_batch_processing")
        return Promise.resolve({
          batch_id: `batch-${++starts}`,
          status: "running",
          total_items: 1,
          message: "",
        });
      if (command === "get_batch_progress")
        return Promise.resolve(
          starts === 1
            ? {
                ...running,
                status: "completed",
                completed_items: 1,
                items: [
                  { ...running.items[0], status: "completed", progress: 100 },
                ],
              }
            : { ...running, batch_id: "batch-2" }
        );
      if (
        command === "set_export_guard" &&
        (args as { active: boolean }).active === false
      )
        return releaseGuard.promise;
      return original(command, args);
    });
    const { result } = await loadedWorkspace();
    await act(async () => {
      await result.current.startExport();
    });
    await waitFor(() => expect(result.current.isExporting).toBe(false));
    let second!: Promise<void>;
    act(() => {
      second = result.current.startExport(["/a.mov"]);
    });
    expect(starts).toBe(1);
    await act(async () => {
      releaseGuard.resolve();
      await second;
    });
    expect(starts).toBe(2);
    expect(
      mockedInvoke.mock.calls
        .filter(([cmd]) => cmd === "set_export_guard")
        .map(([, args]) => (args as { active: boolean }).active)
    ).toEqual([true, false, true]);
  });

  it("merges recovered storage with clips edited during a workspace read failure", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    let readable = false;
    mockedInvoke.mockImplementation((command, args) => {
      if (command === "load_workspace")
        return readable
          ? Promise.resolve({
              version: 1,
              clips: [{ path: "/old.mov", intensity: 70, status: "ready" }],
              activeId: "/old.mov",
              selectedIds: ["/old.mov"],
              batch: null,
              history: [],
            })
          : Promise.reject("disk unavailable");
      return original(command, args);
    });
    const { result } = await loadedWorkspace();
    act(() => {
      result.current.setClipLook("/a.mov", { intensity: 35 });
    });
    readable = true;
    await act(async () => {
      await result.current.retryWorkspaceSave();
    });
    expect(
      result.current.clips.map((clip) => [clip.path, clip.intensity])
    ).toEqual([
      ["/old.mov", 70],
      ["/a.mov", 35],
    ]);
    expect(result.current.activeId).toBe("/a.mov");
    expect(result.current.workspaceSaveError).toBeNull();
  });

  it("keeps preview-only settings from invalidating outputs and prevents undo from reviving obsolete completed results", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    mockedInvoke.mockImplementation((command, args) =>
      command === "get_batch_progress"
        ? Promise.resolve({
            ...running,
            status: "completed",
            completed_items: 1,
            items: [
              { ...running.items[0], status: "completed", progress: 100 },
            ],
          })
        : original(command, args)
    );
    const { result } = await loadedWorkspace();
    await act(async () => {
      await result.current.startExport();
    });
    await waitFor(() => expect(result.current.isExporting).toBe(false));
    const completed = result.current.clips[0];
    await act(async () => {
      await result.current.updateSettings({ preview_quality: "accurate" });
    });
    expect(result.current.clips[0]).toBe(completed);
    act(() => {
      result.current.setClipLook("/a.mov", { intensity: 50 });
    });
    await act(async () => {
      await result.current.updateSettings({ video_codec: "libx265" });
    });
    act(() => {
      result.current.undo();
    });
    expect(result.current.clips[0]).toMatchObject({
      status: "ready",
      intensity: 100,
      outputHistory: ["/out/a.mp4"],
    });
    expect(result.current.clips[0].outputPath).toBeUndefined();
  });

  it("reloads settings and library after storage recovery while retaining per-clip LUT references for backend validation", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    let restored = false;
    mockedInvoke.mockImplementation((command, args) => {
      if (command === "get_app_settings" && restored)
        return Promise.resolve({
          ...DEFAULT_SETTINGS,
          video_codec: "libx265",
          output_bit_depth: "10",
        });
      if (command === "list_lut_library")
        return Promise.resolve(
          restored ? [] : [{ path: "/warm.cube", name: "warm", is_valid: true }]
        );
      return original(command, args);
    });
    const { result } = await loadedWorkspace();
    act(() => {
      result.current.setClipLook("/a.mov", { lutPath: "/warm.cube" });
    });
    restored = true;
    await act(async () => {
      await result.current.reloadPersistentState();
    });
    expect(result.current.settings).toMatchObject({
      video_codec: "libx265",
      output_bit_depth: "10",
    });
    expect(result.current.luts).toEqual([]);
    expect(result.current.clips[0].lutPath).toBe("/warm.cube");
    await act(async () => {
      await result.current.startExport();
    });
    expect(mockedInvoke).toHaveBeenCalledWith("start_batch_processing", {
      request: expect.objectContaining({
        video_codec: "libx265",
        items: [expect.objectContaining({ lut_path: "/warm.cube" })],
      }),
    });
  });

  it("can retry a failed exit-guard release before flushing and closing", async () => {
    const original = mockedInvoke.getMockImplementation()!;
    let releaseAttempts = 0;
    mockedInvoke.mockImplementation((command, args) => {
      if (command === "get_batch_progress")
        return Promise.resolve({
          ...running,
          status: "completed",
          completed_items: 1,
          items: [{ ...running.items[0], status: "completed", progress: 100 }],
        });
      if (
        command === "set_export_guard" &&
        (args as { active: boolean }).active === false &&
        ++releaseAttempts === 1
      )
        return Promise.reject("IPC unavailable");
      return original(command, args);
    });
    const { result } = await loadedWorkspace();
    await act(async () => {
      await result.current.startExport();
    });
    await waitFor(() => expect(result.current.isExporting).toBe(false));
    await act(async () => {
      await result.current.flushWorkspace();
    });
    expect(releaseAttempts).toBe(2);
    expect(
      mockedInvoke.mock.calls.some(([command]) => command === "save_workspace")
    ).toBe(true);
  });

  it("shows the browser limitation without opening fake files or invoking native work", async () => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    const { result } = renderHook(() => useWorkspace());
    await act(async () => {
      await result.current.importVideos(["/a.mov"]);
      await result.current.startExport();
    });
    expect(result.current.ffmpeg.status).toBe("browser");
    expect(result.current.clips).toEqual([]);
    expect(result.current.notice?.message).toContain("桌面应用");
    expect(mockedInvoke).not.toHaveBeenCalled();
  });
});
