import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { useWorkspace } from "../workspace/useWorkspace";
import { errorMessage, errorText } from "../workspace/model";

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
  const { t } = useTranslation();
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
      setExitError(errorText(e));
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
            message: t("statusBar.startupReadFailed", {
              message: errorMessage(e),
            }),
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
      setExitError(
        t("statusBar.cancelExportFailed", {
          message: errorMessage(w.cancelError),
        })
      );
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
        setExitError(t("statusBar.cancelNotAccepted"));
        setExiting(false);
        cancelThenExit.current = false;
      }
    } catch (e) {
      setExitError(errorText(e));
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
        message: t("statusBar.storageRebuilt", {
          paths: result.backup_paths?.join("；") || "",
        }),
      });
    } catch (e) {
      w.setNotice({ kind: "error", message: errorText(e) });
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
              ? t("statusBar.usingTempStorage")
              : t("statusBar.startupAction")}
            {startup.issues.map((i) => errorMessage(i.message)).join("；")}
            {t("statusBar.rebuildHint")}
          </span>
          <button
            disabled={recovering || w.isExporting || w.loading}
            onClick={() => void recover()}
          >
            {recovering
              ? t("statusBar.rebuilding")
              : t("statusBar.rebuildStorage")}
          </button>
        </div>
      ) : null}
      {w.settingsError && (
        <div className="persistence-banner" role="alert">
          <span>
            {t("statusBar.settingsSaveFailed", {
              message: errorMessage(w.settingsError),
            })}
          </span>
          <button onClick={() => void w.retrySettings()}>
            {t("statusBar.retrySave")}
          </button>
          <button onClick={w.discardSettingsError}>
            {t("statusBar.discardChanges")}
          </button>
        </div>
      )}
      {w.workspaceSaveError && (
        <div className="persistence-banner" role="alert">
          <span>
            {t("statusBar.workspaceUnsaved", {
              message: errorMessage(w.workspaceSaveError),
            })}
          </span>
          <button onClick={() => void w.retryWorkspaceSave()}>
            {t("statusBar.retry")}
          </button>
        </div>
      )}
      {exitOpen && (
        <dialog
          ref={dialog}
          className="app-dialog exit-dialog"
          aria-label={t("statusBar.ariaQuit")}
          onCancel={(e) => {
            e.preventDefault();
            if (!exiting) setExitOpen(false);
          }}
        >
          <h2>
            {w.isExporting
              ? t("statusBar.quitBusyTitle")
              : t("statusBar.quitSaveTitle")}
          </h2>
          <p>
            {exiting
              ? t("statusBar.quitBusyDesc")
              : t("statusBar.quitSaveDesc")}
          </p>
          {exitError && (
            <p role="alert" className="queue-error">
              {errorMessage(exitError)}
            </p>
          )}
          <div className="exit-actions">
            <button
              className="button secondary"
              disabled={exiting}
              onClick={() => setExitOpen(false)}
            >
              {t("statusBar.continueUsing")}
            </button>
            <button
              className="button primary"
              disabled={exiting}
              onClick={() =>
                w.isExporting ? void cancelAndExit() : void finishExit()
              }
            >
              {exiting
                ? t("statusBar.quitting")
                : w.isExporting
                ? t("statusBar.cancelAndQuit")
                : t("statusBar.retrySaveAndQuit")}
            </button>
          </div>
        </dialog>
      )}
    </>
  );
}
