import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { useWorkspace } from "../workspace/useWorkspace";

type StartupStatus = {
  issues: { component: string; message: string; recoverable: boolean }[];
  temporary_storage: boolean;
  backup_paths: string[];
};
export default function WorkspaceStatus({
  workspace: w,
}: {
  workspace: ReturnType<typeof useWorkspace>;
}) {
  const [startup, setStartup] = useState<StartupStatus | null>(null);
  const [exitOpen, setExitOpen] = useState(false);
  const [exiting, setExiting] = useState(false);
  const [exitError, setExitError] = useState("");
  const [recovering, setRecovering] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const current = useRef(w);
  current.current = w;
  const closing = useRef(false);
  const cancelThenExit = useRef(false);
  const finishExit = async () => {
    if (closing.current) return;
    closing.current = true;
    setExiting(true);
    setExitError("");
    try {
      current.current.cancelImport();
      await current.current.flushWorkspace();
      await invoke("request_app_exit");
    } catch (e) {
      setExitError(String(e));
      setExitOpen(true);
      setExiting(false);
      cancelThenExit.current = false;
    } finally {
      closing.current = false;
    }
  };
  useEffect(() => {
    if (!w.isDesktop) return;
    let disposed = false;
    const stops: (() => void)[] = [];
    void invoke<StartupStatus>("startup_status")
      .then((s) => {
        if (!disposed) setStartup(s);
      })
      .catch((e) => {
        if (!disposed)
          current.current.setNotice({
            kind: "error",
            message: `无法读取启动状态：${String(e)}`,
          });
      });
    for (const [event, callback] of [
      [
        "export-close-requested",
        () => {
          setExitError("");
          setExitOpen(true);
        },
      ],
      [
        "workspace-close-requested",
        () => {
          void finishExit();
        },
      ],
    ] as const) {
      void listen(event, callback)
        .then((stop) => (disposed ? stop() : stops.push(stop)))
        .catch(() => {});
    }
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, [w.isDesktop]);
  useEffect(() => {
    if (exitOpen) dialog.current?.showModal?.();
    else dialog.current?.close?.();
  }, [exitOpen]);
  useEffect(() => {
    if (!cancelThenExit.current) return;
    if (!w.isExporting) void finishExit();
    else if (w.cancelError) {
      setExitError(`取消导出失败：${w.cancelError}`);
      setExiting(false);
      cancelThenExit.current = false;
    }
  }, [w.isExporting, w.cancelError, exiting]);
  const cancelAndExit = async () => {
    setExiting(true);
    setExitError("");
    cancelThenExit.current = true;
    try {
      const accepted = await current.current.cancelExport();
      if (accepted === false) {
        setExitError("取消请求未成功，请重试。后台任务仍在运行。");
        setExiting(false);
        cancelThenExit.current = false;
      }
    } catch (e) {
      setExitError(String(e));
      setExiting(false);
      cancelThenExit.current = false;
    }
  };
  const recover = async () => {
    setRecovering(true);
    try {
      w.cancelImport();
      await w.flushWorkspace();
      const result = await invoke<StartupStatus>("recover_startup_storage");
      await w.reloadPersistentState();
      setStartup(result);
      w.setNotice({
        kind: "success",
        message: `存储已重建，原文件已备份。临时资料库中新增的 LUT 请重新导入。${result.backup_paths?.join("；") || ""}`,
      });
    } catch (e) {
      w.setNotice({ kind: "error", message: String(e) });
    } finally {
      setRecovering(false);
    }
  };
  return (
    <>
      {startup?.issues?.length ? (
        <div className="persistence-banner" role="alert">
          <span>
            {startup.temporary_storage
              ? "当前使用临时存储。"
              : "启动需要处理。"}
            {startup.issues.map((i) => i.message).join("；")}
            操作将备份并重建资料库与设置，临时库新增的 LUT 需要重新导入。
          </span>
          <button
            disabled={recovering || w.isExporting || w.loading}
            onClick={() => void recover()}
          >
            {recovering ? "正在重建…" : "备份并重建存储"}
          </button>
        </div>
      ) : null}
      {w.settingsError && (
        <div className="persistence-banner" role="alert">
          <span>设置保存失败，已恢复上次保存的值：{w.settingsError}</span>
          <button onClick={() => void w.retrySettings()}>重试保存</button>
          <button onClick={w.discardSettingsError}>放弃修改</button>
        </div>
      )}
      {w.workspaceSaveError && (
        <div className="persistence-banner" role="alert">
          <span>工作区尚未保存：{w.workspaceSaveError}</span>
          <button onClick={() => void w.retryWorkspaceSave()}>重试</button>
        </div>
      )}
      {exitOpen && (
        <dialog
          ref={dialog}
          className="app-dialog exit-dialog"
          aria-label="退出应用"
          onCancel={(e) => {
            e.preventDefault();
            if (!exiting) setExitOpen(false);
          }}
        >
          <h2>{w.isExporting ? "导出仍在进行" : "保存工作区后退出"}</h2>
          <p>
            {exiting
              ? "正在停止编码并保存工作区，请稍候…"
              : "退出前会等待编码进程停止并保存工作区。已完成文件会保留，未完成任务可在下次打开时重试。"}
          </p>
          {exitError && (
            <p role="alert" className="queue-error">
              {exitError}
            </p>
          )}
          <div className="exit-actions">
            <button
              className="button secondary"
              disabled={exiting}
              onClick={() => setExitOpen(false)}
            >
              继续使用
            </button>
            <button
              className="button primary"
              disabled={exiting}
              onClick={() =>
                w.isExporting ? void cancelAndExit() : void finishExit()
              }
            >
              {exiting
                ? "正在安全退出…"
                : w.isExporting
                  ? "取消导出并退出"
                  : "重试保存并退出"}
            </button>
          </div>
        </dialog>
      )}
    </>
  );
}
