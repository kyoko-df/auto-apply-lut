import { describe, expect, it } from "vitest";
import { restoreWorkspace } from "./snapshot";

describe("workspace snapshot recovery", () => {
  it("restores RAW settings and preserves storage when RAW parameters are invalid", () => {
    const source = { mode: "raw", exposure_ev: -1.3, white_balance: "auto" };
    const data = { version: 2, mediaMode: "photo", clips: [
      { kind: "photo", path: "/a.arw", sourceInterpretation: source },
    ] };
    expect(restoreWorkspace(data)?.clips[0].sourceInterpretation).toEqual(source);
    expect(() => restoreWorkspace({ ...data, clips: [{ ...data.clips[0],
      sourceInterpretation: { ...source, exposure_ev: 100 } }] })).toThrow();
    expect(() => restoreWorkspace({ ...data, clips: [{ ...data.clips[0], path: "/a.png" }] })).toThrow();
  });
  it("rejects unknown versions and corrupt clip records instead of silently replacing their storage", () => {
    expect(() => restoreWorkspace({ version: 3, clips: [] })).toThrow(
      "原文件已保留"
    );
    expect(() =>
      restoreWorkspace({ version: 1, clips: [{ path: "/a.mov" }, {}] })
    ).toThrow("原文件已保留");
    expect(() =>
      restoreWorkspace({ version: 1, clips: [{ path: "/a.unknown" }] })
    ).toThrow("原文件已保留");
  });

  it("retains successful output paths while treating interrupted files as pending retry", () => {
    const saved = restoreWorkspace({
      version: 1,
      clips: [
        {
          path: "/a.mov",
          status: "completed",
          outputPath: "/exports/a.mp4",
          outputHistory: ["/exports/a-old.mp4"],
          progress: 100,
        },
        {
          path: "/b.mov",
          status: "processing",
          outputPath: "/exports/b.mp4",
          progress: 55,
        },
      ],
      activeId: "/b.mov",
      selectedIds: ["/a.mov", "/missing.mov"],
      history: [],
    });
    expect(saved?.clips[0]).toMatchObject({
      status: "completed",
      outputPath: "/exports/a.mp4",
      outputHistory: ["/exports/a-old.mp4"],
    });
    expect(saved?.clips[1]).toMatchObject({ status: "cancelled", progress: 0 });
    expect(saved?.clips[1].outputPath).toBeUndefined();
    expect(saved?.selectedIds).toEqual(["/a.mov"]);
    expect(saved?.version).toBe(2);
  });
  it("restores photo interpretation and media selections while re-reading photo metadata", () => {
    const saved = restoreWorkspace({
      version: 2,
      mediaMode: "photo",
      clips: [
        { path: "/video.mov" },
        {
          kind: "photo",
          path: "/image.png",
          sourceInterpretation: { mode: "assign", space: "display-p3" },
          lutPath: "/film.cube",
          lutSpace: "srgb",
          lutFingerprint: "a".repeat(64),
          info: { width: 999 },
          status: "processing",
        },
      ],
      activeId: "/video.mov",
      selectedIds: ["/video.mov", "/image.png"],
      mediaSelection: {
        video: { activeId: "/video.mov", selectedIds: ["/video.mov"] },
        photo: { activeId: "/image.png", selectedIds: ["/image.png"] },
      },
    });
    expect(saved).toMatchObject({
      mediaMode: "photo",
      activeId: "/image.png",
      selectedIds: ["/image.png"],
    });
    expect(saved?.clips[1]).toMatchObject({
      kind: "photo",
      sourceInterpretation: { mode: "assign", space: "display-p3" },
      lutSpace: "srgb",
      lutFingerprint: "a".repeat(64),
      status: "cancelled",
    });
    expect(saved?.clips[1].info).toBeUndefined();
    expect(saved?.mediaSelection?.video?.activeId).toBe("/video.mov");
  });
});
