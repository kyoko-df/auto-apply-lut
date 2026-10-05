import { describe, expect, it } from "vitest";
import { restoreWorkspace } from "./snapshot";

describe("workspace snapshot recovery", () => {
  it("rejects unknown versions and corrupt clip records instead of silently replacing their storage", () => {
    expect(() => restoreWorkspace({ version: 2, clips: [] })).toThrow(
      "原文件已保留",
    );
    expect(() =>
      restoreWorkspace({ version: 1, clips: [{ path: "/a.mov" }, {}] }),
    ).toThrow("原文件已保留");
    expect(() =>
      restoreWorkspace({ version: 1, clips: [{ path: "/a.unknown" }] }),
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
  });
});
