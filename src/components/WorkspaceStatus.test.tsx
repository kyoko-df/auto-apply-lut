import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import WorkspaceStatus from "./WorkspaceStatus";
import type { useWorkspace } from "../workspace/useWorkspace";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
const mockedInvoke = vi.mocked(invoke);
const mockedListen = vi.mocked(listen);
const listeners = new Map<string, () => void>();
type Workspace = ReturnType<typeof useWorkspace>;

function workspace(overrides: Partial<Workspace> = {}): Workspace {
  return {
    isDesktop: true,
    isExporting: false,
    cancelError: null,
    settingsError: null,
    workspaceSaveError: null,
    flushWorkspace: vi.fn(async () => {}),
    cancelExport: vi.fn(async () => true),
    cancelImport: vi.fn(),
    setNotice: vi.fn(),
    retrySettings: vi.fn(async () => {}),
    discardSettingsError: vi.fn(),
    retryWorkspaceSave: vi.fn(async () => {}),
    reloadPersistentState: vi.fn(async () => {}),
    ...overrides,
  } as unknown as Workspace;
}

async function emit(event: string) {
  await waitFor(() => expect(listeners.has(event)).toBe(true));
  act(() => listeners.get(event)!());
}

beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  mockedInvoke.mockImplementation(async (command) =>
    command === "startup_status"
      ? { issues: [], temporary_storage: false, backup_paths: [] }
      : undefined,
  );
  mockedListen.mockImplementation(async (event, handler) => {
    listeners.set(event, handler as () => void);
    return () => {
      listeners.delete(event);
    };
  });
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
  HTMLDialogElement.prototype.close = function () {
    this.open = false;
  };
});

describe("workspace exit protection", () => {
  it("requires an explicit choice while encoding and allows continuing without stopping the task", async () => {
    const w = workspace({ isExporting: true });
    render(<WorkspaceStatus workspace={w} />);
    await emit("export-close-requested");
    expect(screen.getByRole("dialog", { name: "退出应用" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "导出仍在进行" })).toBeVisible();
    expect(w.cancelExport).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "继续使用" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(w.flushWorkspace).not.toHaveBeenCalled();
    expect(mockedInvoke).not.toHaveBeenCalledWith("request_app_exit");
  });

  it("waits for a terminal batch, then stops imports and completes storage before requesting exit", async () => {
    let resolveSave!: () => void;
    const saved = new Promise<void>((resolve) => {
      resolveSave = resolve;
    });
    const w = workspace({
      isExporting: true,
      flushWorkspace: vi.fn(() => saved),
    });
    const { rerender } = render(<WorkspaceStatus workspace={w} />);
    await emit("export-close-requested");
    fireEvent.click(screen.getByRole("button", { name: "取消导出并退出" }));
    await waitFor(() => expect(w.cancelExport).toHaveBeenCalledOnce());
    expect(w.flushWorkspace).not.toHaveBeenCalled();
    expect(mockedInvoke).not.toHaveBeenCalledWith("request_app_exit");
    rerender(<WorkspaceStatus workspace={{ ...w, isExporting: false }} />);
    await waitFor(() => expect(w.flushWorkspace).toHaveBeenCalledOnce());
    expect(w.cancelImport).toHaveBeenCalledOnce();
    expect(mockedInvoke).not.toHaveBeenCalledWith("request_app_exit");
    await act(async () => resolveSave());
    expect(mockedInvoke).toHaveBeenCalledWith("request_app_exit");
  });

  it("keeps an idle app open on save failure and retries saving before exit", async () => {
    const flush = vi
      .fn<Workspace["flushWorkspace"]>()
      .mockRejectedValueOnce(new Error("disk full"))
      .mockResolvedValue(undefined);
    const w = workspace({ flushWorkspace: flush });
    render(<WorkspaceStatus workspace={w} />);
    await emit("workspace-close-requested");
    await screen.findByText("Error: disk full");
    expect(mockedInvoke).not.toHaveBeenCalledWith("request_app_exit");
    fireEvent.click(screen.getByRole("button", { name: "重试保存并退出" }));
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("request_app_exit"),
    );
    expect(flush).toHaveBeenCalledTimes(2);
  });

  it("restores the cancellation action after an immediate cancellation failure", async () => {
    const w = workspace({
      isExporting: true,
      cancelExport: vi.fn(async () => false),
    });
    render(<WorkspaceStatus workspace={w} />);
    await emit("export-close-requested");
    fireEvent.click(screen.getByRole("button", { name: "取消导出并退出" }));
    await screen.findByText("取消请求未成功，请重试。后台任务仍在运行。");
    expect(
      screen.getByRole("button", { name: "取消导出并退出" }),
    ).toBeEnabled();
    expect(w.flushWorkspace).not.toHaveBeenCalled();
    expect(mockedInvoke).not.toHaveBeenCalledWith("request_app_exit");
  });

  it("restores the dialog after an early cancellation fails when the batch ID arrives later", async () => {
    const w = workspace({ isExporting: true });
    const { rerender } = render(<WorkspaceStatus workspace={w} />);
    await emit("export-close-requested");
    fireEvent.click(screen.getByRole("button", { name: "取消导出并退出" }));
    await waitFor(() => expect(w.cancelExport).toHaveBeenCalledOnce());
    rerender(
      <WorkspaceStatus workspace={{ ...w, cancelError: "IPC unavailable" }} />,
    );
    await screen.findByText("取消导出失败：IPC unavailable");
    expect(
      screen.getByRole("button", { name: "取消导出并退出" }),
    ).toBeEnabled();
    expect(w.flushWorkspace).not.toHaveBeenCalled();
  });

  it("saves the current workspace before rebuilding storage and reloads persistent settings and library afterward", async () => {
    const order: string[] = [];
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "startup_status")
        return {
          issues: [
            {
              component: "database",
              message: "database damaged",
              recoverable: true,
            },
          ],
          temporary_storage: true,
          backup_paths: [],
        };
      if (command === "recover_startup_storage") {
        order.push("recover");
        return {
          issues: [],
          temporary_storage: false,
          backup_paths: ["/backup/library.db"],
        };
      }
      return undefined;
    });
    const w = workspace({
      flushWorkspace: vi.fn(async () => {
        order.push("save");
      }),
      reloadPersistentState: vi.fn(async () => {
        order.push("reload");
      }),
    });
    render(<WorkspaceStatus workspace={w} />);
    fireEvent.click(
      await screen.findByRole("button", { name: "备份并重建存储" }),
    );
    await waitFor(() => expect(order).toEqual(["save", "recover", "reload"]));
    expect(w.cancelImport).toHaveBeenCalledOnce();
    expect(w.setNotice).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: "success",
        message: expect.stringContaining("重新导入"),
      }),
    );
  });

  it("removes native listeners when the component is unmounted", async () => {
    const { unmount } = render(<WorkspaceStatus workspace={workspace()} />);
    await waitFor(() => expect(listeners.size).toBe(2));
    unmount();
    expect(listeners.size).toBe(0);
  });
});
