import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import LutThumbnail from "./LutThumbnail";
import type { Clip, LutLibraryItem } from "../workspace/types";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const clip: Clip = { id: "one", path: "/one.mp4", name: "one.mp4", lutPath: null,
  intensity: 50, status: "ready", progress: 0,
  info: { path: "/one.mp4", filename: "one.mp4", size: 10, duration: 2, width: 160,
    height: 90, fps: 25, codec: "h264", bitrate: null } };
const lut: LutLibraryItem = { path: "/look.cube", name: "Look", size: 10, lut_type: "3D",
  format: "CUBE", category: "3D", is_valid: true, updated_at: "1" };
const props = { clip, lut, enabled: true, isDesktop: true, inputColorSpace: "auto",
  alphaPolicy: { mode: "preserve" as const }, engineKey: "engine1" };
beforeEach(() => { vi.useFakeTimers(); invoke.mockReset(); });
afterEach(async () => { cleanup(); await act(async () => { await Promise.resolve(); }); vi.useRealTimers(); });
const tick = () => act(async () => { await vi.advanceTimersByTimeAsync(300); });

it("renders actual native pixels at full LUT strength and hides the prior source immediately", async () => {
  let finish: (value: unknown) => void = () => {};
  invoke.mockImplementation((name: string) => name === "generate_video_preview"
    ? new Promise(resolve => { finish = resolve; }) : Promise.resolve());
  const { rerender } = render(<LutThumbnail {...props} />);
  await tick();
  const request = invoke.mock.calls.find(([name]) => name === "generate_video_preview")![1].request;
  expect(request).toMatchObject({ video_path: "/one.mp4", max_width: 160, intensity: 1 });
  await act(async () => finish({ processed_image: "data:image/jpeg;base64,b25l" }));
  expect(screen.getByRole("img")).toHaveAttribute("src", "data:image/jpeg;base64,b25l");
  rerender(<LutThumbnail {...props} clip={{ ...clip, id: "two", path: "/two.mp4" }} />);
  expect(screen.queryByRole("img")).toBeNull();
});

it("serializes library renders, cancels an old generation and discards its late response", async () => {
  const pending: Array<(value: unknown) => void> = [];
  invoke.mockImplementation((name: string) => name === "generate_video_preview"
    ? new Promise(resolve => pending.push(resolve)) : Promise.resolve());
  const { rerender } = render(<><LutThumbnail {...props} /><LutThumbnail {...props}
    lut={{ ...lut, path: "/second.cube" }} /></>);
  await tick();
  expect(pending).toHaveLength(1);
  const firstId = invoke.mock.calls[0][1].request.client_id;
  rerender(<LutThumbnail {...props} lut={{ ...lut, path: "/new.cube" }} />);
  expect(invoke).toHaveBeenCalledWith("cancel_video_preview", { clientId: firstId });
  await tick();
  await act(async () => pending[0]({ processed_image: "old" }));
  expect(pending).toHaveLength(2);
  expect(screen.queryByRole("img")).toBeNull();
  const calls = invoke.mock.calls.filter(([name]) => name === "generate_video_preview");
  expect(calls[1][1].request.lut_path).toBe("/new.cube");
  expect(calls[1][1].request.client_id).not.toBe(firstId);
  await act(async () => pending[1]({ processed_image: "new" }));
  expect(screen.getByRole("img")).toHaveAttribute("src", "new");
});

it("does no processing in browser mode or before photo sRGB consent", async () => {
  const { rerender } = render(<LutThumbnail {...props} isDesktop={false} />);
  await tick();
  rerender(<LutThumbnail {...props} enabled={false} clip={{ ...clip, kind: "photo", info: undefined }} />);
  await tick();
  expect(invoke).not.toHaveBeenCalled();
});

it("uses the photo's RAW settings and a fresh LUT fingerprint for consented previews", async () => {
  invoke.mockImplementation(async (name: string, args: { request?: { request_id: string } }) => {
    if (name === "get_photo_lut_fingerprint") return "fresh-hash";
    if (name === "generate_photo_preview") return { request_id: args.request?.request_id, processed_image: "actual-photo" };
  });
  const source = { mode: "raw" as const, exposure_ev: 1, white_balance: "daylight" as const };
  render(<LutThumbnail {...props} clip={{ ...clip, kind: "photo", sourceInterpretation: source,
    info: { path: "/a.arw", filename: "a.arw", size: 100, format: "arw", width: 10, height: 10,
      stored_width: 10, stored_height: 10, bit_depth: 16, has_alpha: false, orientation: 1,
      color_profile: "sRGB", color_status: "srgb", source_version: "v1" } }} />);
  await tick();
  expect(invoke).toHaveBeenCalledWith("generate_photo_preview", { request: expect.objectContaining({
    source_interpretation: source, lut_space: "srgb", lut_fingerprint: "fresh-hash", intensity: 1,
  }) });
  expect(screen.getByRole("img")).toHaveAttribute("src", "actual-photo");
});

it("does not queue work for rows outside the visible scrolling region", async () => {
  let update: IntersectionObserverCallback = () => {};
  vi.stubGlobal("IntersectionObserver", class {
    constructor(callback: IntersectionObserverCallback) { update = callback; }
    observe() {} disconnect() {}
  });
  try {
    invoke.mockResolvedValue({ processed_image: "visible" });
    render(<LutThumbnail {...props} />);
    await tick();
    expect(invoke).not.toHaveBeenCalled();
    act(() => update([{ isIntersecting: true } as IntersectionObserverEntry], {} as IntersectionObserver));
    await tick();
    expect(screen.getByRole("img")).toHaveAttribute("src", "visible");
  } finally { vi.unstubAllGlobals(); }
});
