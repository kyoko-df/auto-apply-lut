import { describe, expect, it } from "vitest";
import {
  appendClips,
  buildBatchRequest,
  DEFAULT_SETTINGS,
  normalizeSettings,
  reconcileBatch,
  updateLook,
} from "./model";
import type { BatchProgress } from "./types";

describe("workspace model", () => {
  it("deduplicates Windows paths, preserves POSIX case and ignores unsupported input", () => {
    const clips = appendClips(
      [],
      [
        "C:\\Footage\\A.MP4",
        "c:/footage/a.mp4",
        "/a/Shot.mov",
        "/a/shot.mov",
        "/a/notes.txt",
      ],
    );
    expect(clips.map((clip) => clip.path)).toEqual([
      "C:\\Footage\\A.MP4",
      "/a/Shot.mov",
      "/a/shot.mov",
    ]);
    expect(appendClips(clips, ["/a/Shot.mov"])).toHaveLength(3);
  });

  it("keeps an export snapshot independent of later LUT and settings changes", () => {
    const clips = appendClips([], ["/a.mov", "/b.mov"]);
    clips[0] = updateLook(clips[0], { lutPath: "/warm.cube", intensity: 35 });
    const settings = { ...DEFAULT_SETTINGS, max_concurrent_tasks: 3 };
    const request = buildBatchRequest(clips, settings);
    clips[0].intensity = 90;
    settings.video_codec = "libx265";
    expect(request.items).toEqual([
      {
        input_path: "/a.mov",
        output_path: "",
        lut_paths: ["/warm.cube"],
        lut_path: "/warm.cube",
        intensity: 0.35,
      },
      {
        input_path: "/b.mov",
        output_path: "",
        lut_paths: [],
        lut_path: null,
        intensity: 1,
      },
    ]);
    expect(request.video_codec).toBe("libx264");
    expect(request.max_concurrent).toBe(3);
  });

  it("reconciles by source path, never by array order or aggregate progress", () => {
    const clips = appendClips([], ["/a.mov", "/b.mov", "/c.mov"]);
    const progress: BatchProgress = {
      batch_id: "batch",
      total_items: 2,
      completed_items: 1,
      failed_items: 0,
      cancelled_items: 0,
      overall_progress: 100,
      status: "running",
      errors: [],
      items: [
        {
          input_path: "/b.mov",
          output_path: "/out/b.mov",
          progress: 99,
          status: "running",
        },
        {
          input_path: "/a.mov",
          output_path: "/out/a.mov",
          progress: 100,
          status: "completed",
        },
      ],
    };
    const updated = reconcileBatch(clips, progress);
    expect(updated.map((clip) => clip.status)).toEqual([
      "completed",
      "processing",
      "ready",
    ]);
    expect(updated[1].outputPath).toBe("/out/b.mov");
    expect(updated[2]).toBe(clips[2]);
  });

  it("invalidates an old export after a look change and protects queued clips", () => {
    const clip = {
      ...appendClips([], ["/a.mov"])[0],
      status: "completed" as const,
      progress: 100,
      outputPath: "/out/a.mov",
    };
    expect(updateLook(clip, { intensity: 100 })).toBe(clip);
    const updated = updateLook(clip, { intensity: 40 });
    expect(updated.status).toBe("ready");
    expect(updated.outputPath).toBeUndefined();
    const queued = { ...updated, status: "queued" as const };
    expect(updateLook(queued, { lutPath: "/look.cube" })).toBe(queued);
  });

  it("constrains legacy settings to supported behavior without false color conversions", () => {
    expect(
      normalizeSettings({
        max_concurrent_tasks: 80,
        color_space: "rec2020",
        two_pass_encoding: true,
        fps: 60,
        bitrate: "2500k",
      }),
    ).toMatchObject({
      max_concurrent_tasks: 4,
      color_space: "original",
      two_pass_encoding: false,
      fps: null,
      bitrate: "auto",
    });
  });

  it("migrates retired export options to visible supported choices", () => {
    expect(
      normalizeSettings({
        output_format: "webm",
        video_codec: "libvpx-vp9",
        audio_codec: "libopus",
        quality_preset: "web_optimized",
        resolution: "4096x2160",
      }),
    ).toMatchObject({
      output_format: "mp4",
      video_codec: "libx264",
      audio_codec: "aac",
      quality_preset: "balanced",
      resolution: "original",
    });
  });

  it("keeps ProRes in MOV with CPU encoding, and disallows invisible PCM in MP4", () => {
    expect(
      normalizeSettings({
        output_format: "mp4",
        video_codec: "prores_ks",
        audio_codec: "pcm_s16le",
        hardware_acceleration: true,
      }),
    ).toMatchObject({
      output_format: "mov",
      video_codec: "prores_ks",
      audio_codec: "pcm_s16le",
      hardware_acceleration: false,
    });
    expect(
      normalizeSettings({
        output_format: "mp4",
        video_codec: "libx264",
        audio_codec: "pcm_s16le",
      }),
    ).toMatchObject({
      output_format: "mp4",
      video_codec: "libx264",
      audio_codec: "aac",
    });
  });

  it("preserves current UI choices while discarding hidden frame-rate and bitrate overrides", () => {
    expect(
      normalizeSettings({
        output_format: "mkv",
        video_codec: "libx265",
        audio_codec: "copy",
        quality_preset: "high_quality",
        resolution: "3840x2160",
        fps: 30,
        bitrate: "12M",
      }),
    ).toMatchObject({
      output_format: "mkv",
      video_codec: "libx265",
      audio_codec: "copy",
      quality_preset: "high_quality",
      resolution: "3840x2160",
      fps: null,
      bitrate: "auto",
    });
  });
  it("preserves all references when a progress poll contains no changes", () => {
    const clips = appendClips([], ["/a.mov"]);
    const batch: BatchProgress = {
      batch_id: "b",
      total_items: 1,
      completed_items: 0,
      failed_items: 0,
      cancelled_items: 0,
      overall_progress: 25,
      status: "running",
      errors: [],
      items: [
        {
          input_path: "/a.mov",
          output_path: "/out/a.mp4",
          status: "running",
          progress: 25,
          encoder: "libx264",
          speed: 2,
          eta_seconds: 5,
        },
      ],
    };
    const first = reconcileBatch(clips, batch);
    expect(reconcileBatch(first, batch)).toBe(first);
    const next = reconcileBatch(first, {
      ...batch,
      items: [{ ...batch.items[0], speed: 3 }],
    });
    expect(next).not.toBe(first);
    expect(next[0].speed).toBe(3);
  });

  it("keeps 10-bit output restricted to HEVC and ProRes", () => {
    expect(
      normalizeSettings({ video_codec: "libx264", output_bit_depth: "10" })
        .output_bit_depth,
    ).toBe("8");
    expect(
      normalizeSettings({ video_codec: "libx265", output_bit_depth: "10" })
        .output_bit_depth,
    ).toBe("10");
    expect(
      normalizeSettings({ video_codec: "prores_ks", output_bit_depth: "8" })
        .output_bit_depth,
    ).toBe("10");
  });
});
