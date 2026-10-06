import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import App from "./App";
import { DEFAULT_SETTINGS } from "./workspace/model";
import type { useWorkspace } from "./workspace/useWorkspace";

const { workspaceHook } = vi.hoisted(() => ({ workspaceHook: vi.fn() }));
vi.mock("./workspace/useWorkspace", () => ({ useWorkspace: workspaceHook }));
type Workspace = ReturnType<typeof useWorkspace>;
let workspace: Workspace;

beforeEach(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
  HTMLDialogElement.prototype.close = function () {
    this.open = false;
  };
  workspace = {
    clips: [],
    allClips: [],
    luts: [],
    selectedIds: [],
    history: [],
    activeClip: null,
    activeId: null,
    mediaMode: "video",
    settings: { ...DEFAULT_SETTINGS, language: "zh" },
    ffmpeg: { status: "browser" },
    loading: false,
    isDesktop: false,
    isExporting: false,
    isImporting: false,
    batch: null,
    notice: null,
    updateSettings: vi.fn(async () => {}),
  } as unknown as Workspace;
  workspaceHook.mockImplementation(() => workspace);
});

it.each([
  { isExporting: true, loading: false },
  { isExporting: false, loading: true },
])("disables language selection with an explanation while busy: %s", (busy) => {
  workspace = { ...workspace, ...busy };
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "应用设置" }));
  const language = screen.getByRole("combobox", { name: "界面语言" });
  expect(language).toBeDisabled();
  expect(language).toHaveAttribute(
    "title",
    "加载或导出期间无法切换语言，请等待处理结束。"
  );
});

it("saves a language choice while idle and synchronizes the document after settings update", async () => {
  const { rerender } = render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "应用设置" }));
  const language = screen.getByRole("combobox", { name: "界面语言" });
  expect(language).toBeEnabled();
  fireEvent.change(language, { target: { value: "en" } });
  expect(workspace.updateSettings).toHaveBeenCalledWith({ language: "en" });
  workspace = {
    ...workspace,
    settings: { ...workspace.settings, language: "en" },
  };
  await act(async () => {
    rerender(<App />);
  });
  expect(document.documentElement.lang).toBe("en");
  expect(screen.getByRole("combobox", { name: "Language" })).toHaveValue("en");
});
