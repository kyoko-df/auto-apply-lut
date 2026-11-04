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
import type { LutLibraryItem } from "./workspace/types";

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
  is_favorite: false,
  category_id: null,
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
        case "list_lut_categories": return [];
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
  it("favorites, searches and batch categorizes LUTs, then renames and deletes a category without changing the applied look", async () => {
    let items: LutLibraryItem[] = [structuredClone(lut), { ...lut, path: "/luts/cool.cube", name: "Cool" },
      { ...lut, path: "/luts/missing.cube", name: "Missing", is_valid: false }];
    let categories: { id: string; name: string }[] = [];
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      const request = args?.request as { name: string; id: string; lut_path: string; is_favorite: boolean; lut_paths: string[]; category_id: string | null };
      if (command === "list_lut_library") return structuredClone(items);
      if (command === "list_lut_categories") return structuredClone(categories);
      if (command === "set_lut_favorite") { items = items.map(item => item.path === request.lut_path ? { ...item, is_favorite: request.is_favorite } : item); return; }
      if (command === "create_lut_category") { const category = { id: "custom", name: request.name }; categories.push(category); return category; }
      if (command === "rename_lut_category") { categories = categories.map(c => ({ ...c, name: request.name })); return categories[0]; }
      if (command === "assign_lut_category") { items = items.map(item => request.lut_paths.includes(item.path) ? { ...item, category_id: request.category_id } : item); return; }
      if (command === "delete_lut_category") { categories = []; items = items.map(item => ({ ...item, category_id: null })); return; }
      return original(command, args);
    });
    const first = render(<App />);
    await importClips();
    fireEvent.change(screen.getByRole("combobox", { name: "当前 LUT" }), { target: { value: lut.path } });
    fireEvent.click(screen.getByRole("button", { name: "LUT3" }));
    fireEvent.click(screen.getByRole("button", { name: "收藏 LUT Film" }));
    await screen.findByRole("button", { name: "取消收藏 LUT Film" });
    fireEvent.click(screen.getByRole("button", { name: "仅看收藏" }));
    expect(screen.queryByRole("button", { name: "应用 LUT Cool" })).not.toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "搜索 LUT" }), { target: { value: "no match" } });
    expect(screen.queryByRole("button", { name: "应用 LUT Film" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "清除筛选" }));
    expect(screen.getByRole("textbox", { name: "搜索 LUT" })).toHaveValue("");
    fireEvent.click(screen.getByRole("button", { name: "新建分类" }));
    expect(screen.getByRole("textbox", { name: "分类名称" })).toHaveFocus();
    fireEvent.keyDown(window, { key: "Enter", metaKey: true });
    expect(invoke).not.toHaveBeenCalledWith("start_batch_processing", expect.anything());
    fireEvent.change(screen.getByRole("textbox", { name: "分类名称" }), { target: { value: "日常暖色" } });
    fireEvent.click(screen.getByRole("button", { name: "保存分类" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "批量归类" }));
    fireEvent.click(screen.getByRole("button", { name: "全选结果" }));
    expect(screen.getByText("已选 3 个 LUT")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "应用分类" }));
    await waitFor(() => expect(items.every(item => item.category_id === "custom")).toBe(true));
    expect(invoke).toHaveBeenCalledWith("assign_lut_category", { request: { lut_paths: items.map(item => item.path), category_id: "custom" } });
    fireEvent.change(screen.getByRole("combobox", { name: "筛选分类" }), { target: { value: "custom" } });
    fireEvent.click(screen.getByRole("button", { name: "仅看收藏" }));
    fireEvent.change(screen.getByRole("textbox", { name: "搜索 LUT" }), { target: { value: "FILM" } });
    expect(screen.getByRole("button", { name: "应用 LUT Film" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "应用 LUT Cool" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "管理分类" }));
    fireEvent.click(screen.getByRole("button", { name: "重命名分类 日常暖色" }));
    fireEvent.change(screen.getByRole("textbox", { name: "分类名称" }), { target: { value: "暖色收藏" } });
    fireEvent.click(screen.getByRole("button", { name: "保存分类" }));
    await screen.findByRole("button", { name: "删除分类 暖色收藏" });
    fireEvent.click(screen.getByRole("button", { name: "删除分类 暖色收藏" }));
    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    await screen.findByText("暂无自定义分类");
    fireEvent.click(screen.getByRole("button", { name: "关闭分类窗口" }));
    expect(screen.getByRole("combobox", { name: "当前 LUT" })).toHaveValue(lut.path);
    expect(screen.getByRole("combobox", { name: "筛选分类" })).toHaveValue("all");
    expect(screen.getByRole("button", { name: "取消收藏 LUT Film" })).toHaveAttribute("aria-pressed", "true");
    first.unmount();
    render(<App />);
    await waitFor(() => expect(screen.getByRole("button", { name: "LUT3" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "LUT3" }));
    expect(screen.getByRole("button", { name: "取消收藏 LUT Film" })).toHaveAttribute("aria-pressed", "true");
    expect(invoke).not.toHaveBeenCalledWith("remove_lut_from_library", expect.anything());
  }, 10_000);

  it("reports favorite and category write errors without claiming they were saved", async () => {
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation((command: string, args?: Record<string, unknown>) =>
      ["set_lut_favorite", "create_lut_category"].includes(command) ? Promise.reject(new Error("Database is read-only")) : original(command, args));
    render(<App />);
    await waitFor(() => expect(screen.getByRole("button", { name: "LUT1" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "LUT1" }));
    fireEvent.click(screen.getByRole("button", { name: "收藏 LUT Film" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("button", { name: "收藏 LUT Film" })).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(screen.getByRole("button", { name: "新建分类" }));
    fireEvent.change(screen.getByRole("textbox", { name: "分类名称" }), { target: { value: "未保存分类" } });
    fireEvent.click(screen.getByRole("button", { name: "保存分类" }));
    await screen.findByText("Database is read-only");
    expect(screen.getByRole("dialog", { name: "新建 LUT 分类" })).toBeVisible();
    expect(screen.getByRole("textbox", { name: "分类名称" })).toHaveValue("未保存分类");
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("option", { name: "未保存分类" })).not.toBeInTheDocument();
  });

  it("categorizes all search results beyond the rendered window and clears checked rows when the filter changes", async () => {
    const many = Array.from({ length: 120 }, (_, index) => ({ ...lut, path: `/look-${index}.cube`, name: `Look ${index}` }));
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "list_lut_library") return Promise.resolve(many);
      if (command === "list_lut_categories") return Promise.resolve([{ id: "all-looks", name: "全部风格" }]);
      return original(command, args);
    });
    render(<App />);
    await importClips();
    fireEvent.click(screen.getByRole("button", { name: "LUT120" }));
    fireEvent.click(screen.getByRole("button", { name: "批量归类" }));
    expect(screen.getAllByRole("checkbox").length).toBeLessThan(120);
    fireEvent.keyDown(document.body, { key: "a", metaKey: true });
    expect(screen.getByText("已选 120 个 LUT")).toBeInTheDocument();
    fireEvent.change(screen.getByRole("combobox", { name: "归入分类" }), { target: { value: "all-looks" } });
    fireEvent.click(screen.getByRole("button", { name: "应用分类" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("assign_lut_category", { request: { lut_paths: many.map(item => item.path), category_id: "all-looks" } }));
    await waitFor(() => expect(screen.getByText("已选 0 个 LUT")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "全选结果" }));
    fireEvent.change(screen.getByRole("textbox", { name: "搜索 LUT" }), { target: { value: "Look 1" } });
    expect(screen.getByText("已选 0 个 LUT")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "应用分类" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: /素材\s*2$/ }));
    expect(screen.getByText("1 项已选")).toBeInTheDocument();
  });

  it("saves, renames, batch applies, undoes and deletes presets without changing applied exports", async () => {
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "get_photo_lut_fingerprint") return Promise.resolve("a".repeat(64));
      if (command === "validate_lut_file") return Promise.resolve({ is_valid: true, errors: [] });
      return original(command, args);
    });
    render(<App />);
    await importClips();
    fireEvent.change(screen.getByRole("combobox", { name: "当前 LUT" }), { target: { value: lut.path } });
    fireEvent.change(screen.getByRole("slider", { name: /LUT 强度/ }), { target: { value: "45" } });
    fireEvent.click(screen.getByRole("button", { name: "保存当前风格" }));
    expect(screen.getByRole("textbox", { name: "预设名称" })).toHaveFocus();
    fireEvent.keyDown(window, { key: "Enter", metaKey: true });
    expect(invoke).not.toHaveBeenCalledWith("start_batch_processing", expect.anything());
    fireEvent.change(screen.getByRole("textbox", { name: "预设名称" }), { target: { value: "日常暖色" } });
    fireEvent.click(screen.getByRole("button", { name: "保存预设" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "管理预设" }));
    fireEvent.click(screen.getByRole("button", { name: "重命名预设 日常暖色" }));
    fireEvent.change(screen.getByRole("textbox", { name: "预设名称" }), { target: { value: "暖色 45" } });
    fireEvent.click(screen.getByRole("button", { name: "保存预设" }));
    await screen.findByRole("button", { name: "删除预设 暖色 45" });
    fireEvent.click(screen.getByRole("button", { name: "关闭预设窗口" }));
    fireEvent.click(screen.getByRole("button", { name: "预览 b.mp4" }));
    fireEvent.click(screen.getByRole("button", { name: "全选" }));
    fireEvent.change(screen.getByRole("combobox", { name: "预设应用范围" }), { target: { value: "selected" } });
    fireEvent.click(screen.getByRole("button", { name: "应用预设" }));
    await waitFor(() => expect(screen.getByRole("slider", { name: /LUT 强度/ })).toHaveValue("45"));
    fireEvent.click(screen.getByRole("button", { name: "撤销修改" }));
    expect(screen.getByRole("slider", { name: /LUT 强度/ })).toHaveValue("100");
    fireEvent.click(screen.getByRole("button", { name: "应用预设" }));
    await waitFor(() => expect(screen.getByRole("slider", { name: /LUT 强度/ })).toHaveValue("45"));
    await waitFor(() => expect(screen.getByRole("button", { name: "管理预设" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "管理预设" }));
    fireEvent.click(screen.getByRole("button", { name: "删除预设 暖色 45" }));
    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    await screen.findByText("暂无当前类型的预设", { selector: ".preset-list p" });
    fireEvent.click(screen.getByRole("button", { name: "关闭预设窗口" }));
    fireEvent.click(screen.getByRole("button", { name: "批量导出" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("start_batch_processing", {
      request: expect.objectContaining({ items: [
        expect.objectContaining({ lut_path: lut.path, intensity: 0.45 }),
        expect.objectContaining({ lut_path: lut.path, intensity: 0.45 }),
      ] }),
    }));
    expect(invoke).not.toHaveBeenCalledWith("remove_lut_from_library", expect.anything());
  });

  it("keeps photo preset confirmation unchecked when an older confirmation finishes late", async () => {
    let complete!: (hash: string) => void;
    const pending = new Promise<string>(resolve => { complete = resolve; });
    let reads = 0;
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "get_app_settings") return Promise.resolve({ ...DEFAULT_SETTINGS, look_presets: [{
        id: "photo-look", name: "照片风格", media_mode: "photo", lut_path: lut.path,
        lut_fingerprint: "a".repeat(64), intensity: 45, raw: null,
      }] });
      if (command === "get_photo_info") return Promise.resolve({ path: args?.path, filename: "a.jpg", format: "jpeg",
        width: 64, height: 48, bit_depth: 8, has_alpha: false, color_status: "srgb", source_version: "v1" });
      if (command === "get_photo_lut_fingerprint") return ++reads === 1 ? pending : Promise.resolve("a".repeat(64));
      if (command === "validate_lut_file") return Promise.resolve({ is_valid: true, errors: [] });
      return original(command, args);
    });
    render(<App />);
    await waitFor(() => expect(screen.getByRole("button", { name: "照片" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "照片" }));
    open.mockResolvedValueOnce(["/a.jpg"]);
    fireEvent.click(screen.getByRole("button", { name: "导入素材" }));
    await waitFor(() => expect(screen.getByRole("combobox", { name: "当前 LUT" })).toBeEnabled());
    fireEvent.change(screen.getByRole("combobox", { name: "当前 LUT" }), { target: { value: lut.path } });
    fireEvent.click(screen.getByRole("checkbox", { name: "按 sRGB 输入和输出使用此 LUT" }));
    fireEvent.change(screen.getByRole("combobox", { name: "调色预设" }), { target: { value: "photo-look" } });
    fireEvent.click(screen.getByRole("button", { name: "应用预设" }));
    await waitFor(() => expect(screen.getByRole("slider", { name: "LUT 强度" })).toHaveValue("45"));
    await act(async () => { complete("a".repeat(64)); await pending; });
    expect(screen.getByRole("checkbox", { name: "按 sRGB 输入和输出使用此 LUT" })).not.toBeChecked();
    expect(screen.getByRole("button", { name: /^导出 1 张照片/ })).toBeDisabled();
  });

  it("submits real RAW controls with the photo snapshot and locks them during export", async () => {
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_photo_info") return { path: args?.path, filename: "camera.ARW", size: 100,
        format: "arw", width: 7008, height: 4672, bit_depth: 16, has_alpha: false,
        color_status: "srgb", source_version: "v1" };
      if (command === "generate_photo_preview") return { request_id: (args?.request as { request_id: string }).request_id,
        source_version: "v1", width: 160, height: 106, processed_image: "data:image/png;base64,AA==", original_image: "data:image/png;base64,AA==" };
      if (command === "start_photo_batch_processing") return { batch_id: "raw-batch", status: "Running", total_items: 1 };
      return original(command, args);
    });
    render(<App />);
    await waitFor(() => expect(screen.getByRole("button", { name: "照片" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "照片" }));
    open.mockResolvedValueOnce(["/camera.ARW"]);
    fireEvent.click(screen.getByRole("button", { name: "导入素材" }));
    const exposure = await screen.findByRole("slider", { name: "RAW 曝光" });
    fireEvent.change(exposure, { target: { value: "-1.2" } });
    fireEvent.change(screen.getByRole("combobox", { name: "RAW 白平衡" }), { target: { value: "daylight" } });
    fireEvent.click(screen.getByRole("button", { name: /^导出 1 张照片/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("start_photo_batch_processing", {
      request: expect.objectContaining({ items: [expect.objectContaining({ photo: expect.objectContaining({
        source_interpretation: { mode: "raw", exposure_ev: -1.2, white_balance: "daylight" },
      }) })] }),
    }));
    expect(exposure).toBeDisabled();
  });
  it("requires Log confirmation and blocks export shortcuts while the dialog is open", async () => {
    render(<App />);
    await importClips();
    const calls = invoke.mock.calls.filter(([name]) => name === "update_app_settings").length;
    fireEvent.change(screen.getByRole("combobox", { name: "输入色彩处理" }), { target: { value: "sony-slog3-cine" } });
    expect(screen.getByRole("dialog", { name: "确认 Log 输入并转换" })).toBeVisible();
    expect(invoke.mock.calls.filter(([name]) => name === "update_app_settings")).toHaveLength(calls);
    fireEvent.keyDown(document.body, { key: "Enter", metaKey: true });
    expect(invoke.mock.calls.some(([name]) => name === "start_batch_processing")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.getByRole("combobox", { name: "输入色彩处理" })).toHaveValue("auto");
    fireEvent.change(screen.getByRole("combobox", { name: "输入色彩处理" }), { target: { value: "sony-slog3" } });
    fireEvent.click(screen.getByRole("button", { name: "确认 Log 输入并转换" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("update_app_settings", {
      settings: expect.objectContaining({ input_color_space: "sony-slog3" }),
    }));
  });
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
