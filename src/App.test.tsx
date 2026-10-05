import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { DEFAULT_SETTINGS } from "./workspace/model";

const { invoke, open, eventHandlers } = vi.hoisted(() => ({
  invoke: vi.fn(),
  open: vi.fn(),
  eventHandlers: new Map<string, () => void>(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, callback: () => void) => {
    eventHandlers.set(name, callback);
    return () => {
      eventHandlers.delete(name);
    };
  }),
}));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    onDragDropEvent: () => Promise.resolve(() => {}),
  }),
}));

const lut = {
  path: "/luts/film.cube",
  name: "Film",
  format: "CUBE",
  category: "3D LUT",
  is_valid: true,
  size: 10,
  lut_type: "ThreeDimensional",
  updated_at: "",
};
const info = {
  duration: 3,
  width: 1920,
  height: 1080,
  fps: 25,
  codec: "h264",
  size: 1024,
};

beforeEach(() => {
  invoke.mockReset();
  open.mockReset();
  eventHandlers.clear();
  invoke.mockImplementation(
    async (command: string, args?: Record<string, unknown>) => {
      switch (command) {
        case "get_app_settings":
          return DEFAULT_SETTINGS;
        case "list_lut_library":
          return [lut];
        case "get_ffmpeg_info":
          return { binary_path: "/bin/ffmpeg" };
        case "get_file_info":
          return { is_directory: false };
        case "get_video_info":
          return { ...info, path: args?.path };
        case "generate_video_preview":
          return {
            original_image: "data:image/jpeg;base64,AA==",
            processed_image: "data:image/jpeg;base64,BB==",
            time_seconds: 0,
            cached: false,
          };
        case "start_batch_processing":
          return { batch_id: "batch", status: "Running", total_items: 2 };
        case "get_batch_progress":
          return {
            batch_id: "batch",
            status: "Running",
            total_items: 2,
            completed_items: 0,
            failed_items: 0,
            cancelled_items: 0,
            overall_progress: 30,
            items: [
              {
                input_path: "/video/a.mp4",
                output_path: "/out/a.mp4",
                status: "Running",
                progress: 60,
              },
              {
                input_path: "/video/b.mp4",
                output_path: "/out/b.mp4",
                status: "Pending",
                progress: 0,
              },
            ],
            errors: [],
          };
        default:
          return null;
      }
    },
  );
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});

async function importClips() {
  open.mockResolvedValueOnce(["/video/a.mp4", "/video/b.mp4"]);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "导入素材" })).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "导入素材" }));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "批量导出" })).toBeEnabled(),
  );
}

describe("workspace integration", () => {
  it("applies an independent look to the whole batch and submits the settings snapshot", async () => {
    render(<App />);
    await importClips();
    fireEvent.change(screen.getByLabelText("当前 LUT", { exact: false }), {
      target: { value: lut.path },
    });
    fireEvent.change(screen.getByLabelText("LUT 强度", { exact: false }), {
      target: { value: "65" },
    });
    fireEvent.click(screen.getByRole("button", { name: "应用到全部素材" }));
    fireEvent.click(screen.getByRole("button", { name: "批量导出" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("start_batch_processing", {
        request: expect.objectContaining({
          max_concurrent: 2,
          items: [
            expect.objectContaining({
              input_path: "/video/a.mp4",
              lut_paths: [lut.path],
              intensity: 0.65,
            }),
            expect.objectContaining({
              input_path: "/video/b.mp4",
              lut_paths: [lut.path],
              intensity: 0.65,
            }),
          ],
        }),
      }),
    );
    expect(
      screen.getByRole("button", { name: "停止导出" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "视频编码" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "停止导出" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("cancel_batch", { batchId: "batch" }),
    );
  });

  it("makes codec/container combinations usable and exposes advanced export settings", async () => {
    render(<App />);
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "视频编码" })).toBeEnabled(),
    );
    fireEvent.change(screen.getByRole("combobox", { name: "视频编码" }), {
      target: { value: "prores_ks" },
    });
    expect(screen.getByRole("combobox", { name: "封装格式" })).toHaveValue(
      "mov",
    );
    expect(screen.getByRole("switch", { name: "硬件加速" })).not.toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "更多设置" }));
    expect(screen.getByRole("combobox", { name: "音频" })).toHaveValue(
      "pcm_s16le",
    );
    fireEvent.change(screen.getByRole("combobox", { name: "封装格式" }), {
      target: { value: "mp4" },
    });
    expect(screen.getByRole("combobox", { name: "视频编码" })).toHaveValue(
      "libx264",
    );
    expect(screen.getByRole("combobox", { name: "音频" })).toHaveValue("aac");
  });

  it("opens a keyboard accessible settings dialog and displays engine information", async () => {
    render(<App />);
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "处理引擎就绪" }),
      ).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "应用设置" }));
    expect(
      screen.getByRole("dialog", { name: "应用设置" }),
    ).toBeInTheDocument();
    expect(screen.getByText("/bin/ffmpeg")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "关闭窗口" })).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "关闭窗口" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("selects only visible search results with the select-all shortcut", async () => {
    render(<App />);
    await importClips();
    fireEvent.click(screen.getByRole("button", { name: "清除选择" }));
    fireEvent.change(screen.getByRole("textbox", { name: "搜索素材" }), {
      target: { value: "a.mp4" },
    });
    fireEvent.keyDown(document.body, { key: "a", metaKey: true });
    expect(screen.getByText("1 项已选")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "预览 a.mp4" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    fireEvent.change(screen.getByRole("textbox", { name: "搜索素材" }), {
      target: { value: "" },
    });
    expect(screen.getByRole("button", { name: "预览 b.mp4" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("uses the selected export scope for the keyboard export shortcut", async () => {
    render(<App />);
    await importClips();
    fireEvent.click(screen.getByRole("button", { name: "预览 b.mp4" }));
    fireEvent.change(screen.getByRole("combobox", { name: "导出范围" }), {
      target: { value: "selected" },
    });
    fireEvent.keyDown(document.body, { key: "Enter", ctrlKey: true });
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("start_batch_processing", {
        request: expect.objectContaining({
          items: [expect.objectContaining({ input_path: "/video/b.mp4" })],
        }),
      }),
    );
  });

  it("does not import, export or undo underneath the native exit confirmation dialog", async () => {
    render(<App />);
    await importClips();
    fireEvent.change(screen.getByLabelText("当前 LUT", { exact: false }), {
      target: { value: lut.path },
    });
    fireEvent.change(screen.getByLabelText("LUT 强度", { exact: false }), {
      target: { value: "45" },
    });
    await waitFor(() =>
      expect(eventHandlers.has("export-close-requested")).toBe(true),
    );
    act(() => eventHandlers.get("export-close-requested")!());
    expect(screen.getByRole("dialog", { name: "退出应用" })).toBeVisible();
    const opened = open.mock.calls.length;
    fireEvent.keyDown(document.body, { key: "o", metaKey: true });
    fireEvent.keyDown(document.body, { key: "Enter", metaKey: true });
    fireEvent.keyDown(document.body, { key: "z", metaKey: true });
    expect(open).toHaveBeenCalledTimes(opened);
    expect(
      invoke.mock.calls.some(
        ([command]) => command === "start_batch_processing",
      ),
    ).toBe(false);
    expect(screen.getByLabelText("LUT 强度", { exact: false })).toHaveValue(
      "45",
    );
  });

  it("clearly reports desktop requirements without fabricating browser processing", () => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "导入素材" }));
    expect(screen.getByRole("status")).toHaveTextContent(
      "浏览器仅提供界面预览",
    );
    expect(screen.getByRole("button", { name: "批量导出" })).toBeDisabled();
    expect(invoke).not.toHaveBeenCalled();
  });
});
