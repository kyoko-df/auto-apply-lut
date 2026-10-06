import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import PhotoPreview from "./PhotoPreview";
import type { PhotoClip, PhotoPreviewResponse } from "../workspace/types";
import i18n from "../i18n";
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const clip: PhotoClip = {
  kind: "photo",
  id: "photo-a",
  path: "/photo-a.png",
  name: "photo-a.png",
  lutPath: "/film.cube",
  lutSpace: "srgb",
  lutFingerprint: "a".repeat(64),
  intensity: 50,
  status: "ready",
  progress: 0,
  sourceInterpretation: { mode: "assign", space: "srgb" },
  info: {
    path: "/photo-a.png",
    filename: "photo-a.png",
    size: 100,
    format: "png",
    width: 3000,
    height: 2000,
    stored_width: 3000,
    stored_height: 2000,
    orientation: 1,
    bit_depth: 16,
    has_alpha: true,
    color_profile: "sRGB",
    color_status: "embedded",
    source_version: "v1",
  },
};
const result = (requestId: string): PhotoPreviewResponse => ({
  request_id: requestId,
  source_version: "v1",
  original_image: "data:image/png;base64,AA==",
  processed_image: "data:image/png;base64,BB==",
  width: 1280,
  height: 853,
  cached: false,
});
beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockReset();
  invoke.mockImplementation(
    (cmd: string, args: { request?: { request_id: string } }) =>
      Promise.resolve(
        cmd === "generate_photo_preview"
          ? result(args.request!.request_id)
          : undefined
      )
  );
});
afterEach(async () => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  await act(async () => {
    await i18n.changeLanguage("zh");
  });
});
const settle = () =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(200);
  });
const props = {
  isDesktop: true,
  onImport: () => {},
  quality: "fast" as const,
  alphaPolicy: { mode: "preserve" as const },
};
describe("photo preview", () => {
  it("decodes structured errors and retranslates the same error after a language change", async () => {
    await i18n.changeLanguage("en");
    invoke.mockImplementation((cmd: string) =>
      cmd === "generate_photo_preview"
        ? Promise.reject(
            "\u001fphoto.lut_needs_srgb\u001f\u001f请明确按 sRGB 输入和输出使用此 LUT"
          )
        : Promise.resolve()
    );
    render(<PhotoPreview clip={clip} {...props} />);
    await settle();
    expect(
      screen.getByText("Confirm this LUT uses sRGB input and output")
    ).toBeVisible();
    expect(screen.queryByText(/photo\.lut_needs_srgb/)).toBeNull();
    await act(async () => {
      await i18n.changeLanguage("ja");
    });
    expect(
      screen.getByText(i18n.t("errors.photo.lut_needs_srgb"))
    ).toBeVisible();
    expect(
      invoke.mock.calls.filter(([cmd]) => cmd === "generate_photo_preview")
    ).toHaveLength(1);
  });
  it("limits 100% regions to visible device pixels and keeps hidden edges reachable by panning", async () => {
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(420);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(240);
    vi.stubGlobal("devicePixelRatio", 2);
    render(<PhotoPreview clip={clip} {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "100% 局部" }));
    await settle();
    const requests = () =>
      invoke.mock.calls.filter(([name]) => name === "generate_photo_preview");
    expect(requests().slice(-1)[0]![1].request.viewport).toEqual({
      kind: "region",
      x: 1080,
      y: 760,
      width: 840,
      height: 480,
    });
    fireEvent.keyDown(
      screen.getByLabelText("100% 局部照片，可以拖动或用方向键移动"),
      { key: "ArrowDown" }
    );
    await settle();
    expect(requests().slice(-1)[0]![1].request.viewport.y).toBe(888);
  });
  it("debounces edits and sends explicit color, fingerprint, fractional strength, and accurate 100% regions", async () => {
    const { rerender } = render(<PhotoPreview clip={clip} {...props} />);
    rerender(<PhotoPreview clip={{ ...clip, intensity: 25 }} {...props} />);
    await settle();
    const calls = invoke.mock.calls.filter(
      ([name]) => name === "generate_photo_preview"
    );
    expect(calls).toHaveLength(1);
    expect(calls[0][1].request).toMatchObject({
      intensity: 0.25,
      lut_fingerprint: clip.lutFingerprint,
      source_interpretation: { mode: "assign", space: "srgb" },
      viewport: { kind: "fit" },
      quality: "fast",
    });
    fireEvent.click(screen.getByRole("button", { name: "100% 局部" }));
    await settle();
    expect(
      invoke.mock.calls
        .filter(([name]) => name === "generate_photo_preview")
        .slice(-1)[0]?.[1].request
    ).toMatchObject({
      quality: "accurate",
      viewport: { kind: "region", width: 1024, height: 1024 },
    });
  });
  it("hides previous photos immediately, ignores late results, and isolates instance cancellation", async () => {
    let finish: ((data: PhotoPreviewResponse) => void) | undefined;
    let firstId = "";
    invoke.mockImplementation(
      (cmd: string, args: { request?: { request_id: string } }) =>
        cmd === "generate_photo_preview"
          ? new Promise((resolve) => {
              firstId = args.request!.request_id;
              finish = resolve;
            })
          : Promise.resolve()
    );
    const { rerender, unmount } = render(
      <PhotoPreview clip={clip} {...props} />
    );
    await settle();
    const previous = finish!;
    const oldId = firstId;
    rerender(
      <PhotoPreview
        clip={{
          ...clip,
          id: "photo-b",
          path: "/photo-b.png",
          name: "photo-b.png",
        }}
        {...props}
      />
    );
    await act(async () => previous(result(oldId)));
    expect(screen.queryByAltText("LUT 调色")).not.toBeInTheDocument();
    await settle();
    await act(async () => finish!(result(firstId)));
    expect(screen.getByAltText("LUT 调色")).toHaveAttribute(
      "src",
      result(firstId).processed_image
    );
    const oldClient = invoke.mock.calls.find(
      ([name]) => name === "generate_photo_preview"
    )![1].request.client_id;
    unmount();
    render(<PhotoPreview clip={clip} {...props} />);
    await settle();
    const newClient = invoke.mock.calls
      .filter(([name]) => name === "generate_photo_preview")
      .slice(-1)[0]![1].request.client_id;
    expect(newClient).not.toBe(oldClient);
    expect(invoke).toHaveBeenCalledWith("cancel_photo_preview", {
      clientId: oldClient,
    });
  });
  it("does not generate native photos in browser mode", async () => {
    render(<PhotoPreview clip={clip} {...props} isDesktop={false} />);
    await settle();
    expect(invoke).not.toHaveBeenCalled();
    expect(screen.getByText(/浏览器仅提供界面预览/)).toBeInTheDocument();
  });
});
