import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import FramePreview from "./FramePreview";
import type { Clip } from "../workspace/types";
import i18n from "../i18n";
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const clip: Clip = {
  id: "a",
  path: "/a.mp4",
  name: "a.mp4",
  intensity: 70,
  lutPath: "/film.cube",
  status: "ready",
  progress: 0,
  info: {
    path: "/a.mp4",
    filename: "a.mp4",
    duration: 10,
    width: 1920,
    height: 1080,
    fps: 25,
    codec: "h264",
    size: 100,
    bitrate: null,
  },
};
const result = {
  original_image: "data:image/jpeg;base64,AA==",
  processed_image: "data:image/jpeg;base64,BB==",
  time_seconds: 0,
  cached: false,
};
beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockReset();
  invoke.mockResolvedValue(result);
});
afterEach(async () => {
  vi.useRealTimers();
  await act(async () => {
    await i18n.changeLanguage("zh");
  });
});
async function settle() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(200);
  });
}
describe("real frame preview", () => {
  it("shows decoded errors in English and updates existing errors in Japanese", async () => {
    await i18n.changeLanguage("en");
    invoke.mockRejectedValue(
      new Error("\u001fpreview.timeout\u001f\u001f预览超时")
    );
    render(<FramePreview clip={clip} isDesktop onImport={() => {}} />);
    await settle();
    expect(screen.getByText(i18n.t("errors.preview.timeout"))).toBeVisible();
    expect(screen.queryByText(/preview\.timeout/)).toBeNull();
    await act(async () => {
      await i18n.changeLanguage("ja");
    });
    expect(screen.getByText(i18n.t("errors.preview.timeout"))).toBeVisible();
  });
  it("debounces LUT changes and submits fractional intensity and selected time", async () => {
    const { rerender } = render(
      <FramePreview clip={clip} isDesktop onImport={() => {}} />
    );
    rerender(
      <FramePreview
        clip={{ ...clip, intensity: 40 }}
        isDesktop
        onImport={() => {}}
      />
    );
    await settle();
    expect(
      invoke.mock.calls.filter(([name]) => name === "generate_video_preview")
    ).toHaveLength(1);
    expect(invoke).toHaveBeenCalledWith("generate_video_preview", {
      request: expect.objectContaining({
        video_path: "/a.mp4",
        intensity: 0.4,
        lut_path: "/film.cube",
        time_seconds: 0,
      }),
    });
    fireEvent.change(screen.getByRole("slider", { name: "预览时间点" }), {
      target: { value: "5" },
    });
    await settle();
    expect(invoke).toHaveBeenLastCalledWith("generate_video_preview", {
      request: expect.objectContaining({ time_seconds: 5 }),
    });
    expect(screen.getByAltText("应用 LUT 后的视频帧")).toHaveAttribute(
      "src",
      result.processed_image
    );
    fireEvent.click(screen.getByRole("button", { name: "原片" }));
    expect(screen.getByAltText("原始视频帧")).toHaveAttribute(
      "src",
      result.original_image
    );
  });
  it("discards stale responses and cancels an in-flight preview when cleared", async () => {
    let finish: ((value: typeof result) => void) | undefined;
    invoke.mockImplementation((cmd: string) =>
      cmd === "generate_video_preview"
        ? new Promise((resolve) => {
            finish = resolve;
          })
        : Promise.resolve()
    );
    const { rerender } = render(
      <FramePreview clip={clip} isDesktop onImport={() => {}} />
    );
    await settle();
    rerender(<FramePreview isDesktop onImport={() => {}} />);
    await act(async () => {
      finish?.(result);
    });
    expect(
      screen.queryByAltText("应用 LUT 后的视频帧")
    ).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("cancel_video_preview", {
      clientId: expect.stringMatching(/^workspace-/),
    });
  });
});
