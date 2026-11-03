import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { BookmarkPlus, X } from "lucide-react";
import { errorText, extension, fileName } from "../workspace/model";
import { MAX_PRESET_NAME } from "../workspace/presets";
import type { LookPreset, PresetScope } from "../workspace/types";
import type { useWorkspace } from "../workspace/useWorkspace";

export default function LookPresets({ workspace: w }: { workspace: ReturnType<typeof useWorkspace> }) {
  const { t } = useTranslation();
  const [selectedId, setSelectedId] = useState("");
  const [scope, setScope] = useState<PresetScope>("current");
  const [mode, setMode] = useState<"save" | "manage" | "rename" | "delete" | null>(null);
  const [editing, setEditing] = useState<LookPreset | null>(null);
  const [name, setName] = useState("");
  const [includeRaw, setIncludeRaw] = useState(false);
  const [error, setError] = useState("");
  const dialog = useRef<HTMLDialogElement>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const presets = w.settings.look_presets.filter(p => p.media_mode === w.mediaMode);
  const selected = presets.find(p => p.id === selectedId);
  const locked = w.loading || w.isExporting || w.isPresetBusy || w.isImporting || w.isLibraryBusy || !w.isDesktop;
  const rawAvailable = w.activeClip?.kind === "photo" && extension(w.activeClip.path) === "arw";
  const count = scope === "current" ? Number(!!w.activeClip) : w.selectedIds.length;
  const title = t(`presets.${mode === "save" ? "saveTitle" : mode === "rename" ? "renameTitle" : mode === "delete" ? "deleteTitle" : "manageTitle"}`);

  useEffect(() => {
    if (mode && !dialog.current?.open) dialog.current?.showModal?.();
    else if (!mode) dialog.current?.close?.();
    if (mode === "save" || mode === "rename") nameInput.current?.focus();
  }, [mode]);
  useEffect(() => {
    if (selectedId && !presets.some(p => p.id === selectedId)) setSelectedId("");
  }, [selectedId, presets]);

  const summary = (preset: LookPreset) =>
    `${preset.lut_path ? fileName(preset.lut_path) : t("presets.original")} · ${preset.intensity}%${preset.raw ? ` · ${t("presets.rawBadge")}` : ""}`;
  const openSave = () => {
    setError(""); setName(""); setIncludeRaw(false); setEditing(null); setMode("save");
  };
  const edit = (preset: LookPreset, next: "rename" | "delete") => {
    setError(""); setEditing(preset); setName(preset.name); setMode(next);
  };
  const submit = async () => {
    setError("");
    try {
      if (mode === "save") {
        const id = await w.saveLookPreset(name, includeRaw);
        if (!mounted.current) return;
        setSelectedId(id); setMode(null);
        w.setNotice({ kind: "success", message: t("presets.saved", { name: name.trim() }) });
      } else if (editing && mode === "rename") {
        await w.renameLookPreset(editing.id, name);
        if (mounted.current) setMode("manage");
      } else if (editing && mode === "delete") {
        await w.deleteLookPreset(editing.id);
        if (mounted.current) setMode("manage");
      }
    } catch (e) {
      if (mounted.current) setError(errorText(e));
    }
  };
  const apply = async () => {
    if (!selected) return;
    setError("");
    try { await w.applyLookPreset(selected.id, scope); }
    catch (e) { if (mounted.current) setError(errorText(e)); }
  };

  return <div className="look-presets">
    <label className="field">
      <span>{t("presets.label")}</span>
      <select aria-label={t("presets.label")} value={selectedId}
        disabled={locked || !presets.length} onChange={e => { setSelectedId(e.target.value); setError(""); }}>
        <option value="">{t(presets.length ? "presets.choose" : "presets.empty")}</option>
        {presets.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
      </select>
    </label>
    <div className="preset-toolbar">
      <button type="button" className="text-button" disabled={locked || !w.activeClip?.info || !!w.activeClip.metadataError}
        onClick={openSave}><BookmarkPlus size={13} />{t("presets.saveCurrent")}</button>
      <button type="button" className="text-button" disabled={locked || !presets.length}
        onClick={() => { setError(""); setMode("manage"); }}>{t("presets.manage")}</button>
    </div>
    {selected && <div className="preset-application">
      <p className="field-hint">{summary(selected)}</p>
      {selected.raw && <p className="field-hint">{t("presets.rawOnlyHint")}</p>}
      <label className="field">
        <span>{t("presets.scope")}</span>
        <select aria-label={t("presets.scope")} value={scope} disabled={locked}
          onChange={e => setScope(e.target.value as PresetScope)}>
          <option value="current">{t("presets.current")}</option>
          <option value="selected">{t("presets.selected", { count: w.selectedIds.length })}</option>
        </select>
      </label>
      <button type="button" className="button secondary full-width" disabled={locked || count === 0}
        onClick={() => void apply()}>{t(w.isPresetBusy ? "presets.working" : "presets.apply")}</button>
    </div>}
    {!w.isDesktop && <p className="field-hint">{t("presets.desktopOnly")}</p>}
    {!mode && error && <p className="preset-error" role="alert">{error}</p>}
    <dialog ref={dialog} className="app-dialog preset-dialog" aria-label={title}
      onCancel={e => { if (w.isPresetBusy) e.preventDefault(); else setMode(null); }}
      onClick={e => { if (e.target === e.currentTarget && !w.isPresetBusy) setMode(null); }}>
      <div className="dialog-content">
        <div className="dialog-heading">
          <div><span className="eyebrow">{t(w.mediaMode === "photo" ? "presets.photo" : "presets.video")}</span><h2>{title}</h2></div>
          <button type="button" className="icon-button" aria-label={t("presets.close")} disabled={w.isPresetBusy}
            onClick={() => setMode(null)}><X size={18} /></button>
        </div>
        {mode === "manage" ? <>
          <p className="field-hint">{t("presets.manageHint")}</p>
          <div className="preset-list">
            {presets.length === 0 && <p className="field-hint">{t("presets.empty")}</p>}
            {presets.map(p => <div className="preset-row" key={p.id}>
              <div><strong>{p.name}</strong><span>{summary(p)}</span></div>
              <div className="preset-row-actions">
                <button type="button" className="text-button" disabled={locked} aria-label={t("presets.renameNamed", { name: p.name })}
                  onClick={() => edit(p, "rename")}>{t("presets.rename")}</button>
                <button type="button" className="text-button" disabled={locked} aria-label={t("presets.deleteNamed", { name: p.name })}
                  onClick={() => edit(p, "delete")}>{t("presets.delete")}</button>
              </div>
            </div>)}
          </div>
        </> : <form onSubmit={e => { e.preventDefault(); if (!locked) void submit(); }}>
          {mode === "delete" ? <p className="field-hint">{t("presets.deleteHint", { name: editing?.name })}</p> : <>
            <label className="field"><span>{t("presets.name")}</span>
              <input ref={nameInput} type="text" maxLength={MAX_PRESET_NAME} value={name} disabled={locked}
                onChange={e => setName(e.target.value)} required />
            </label>
            {mode === "save" && <>
              <p className="field-hint">{t("presets.saveHint")}</p>
              {rawAvailable && <label className="checkbox-field preset-raw-option">
                <input type="checkbox" checked={includeRaw} disabled={locked} onChange={e => setIncludeRaw(e.target.checked)} />
                <span>{t("presets.includeRaw")}</span>
              </label>}
              {w.mediaMode === "photo" && <p className="field-hint">{t("presets.photoConfirmation")}</p>}
            </>}
          </>}
          <div className="dialog-buttons">
            <button type="submit" className="button primary" disabled={locked || (mode !== "delete" && !name.trim())}>
              {t(w.isPresetBusy ? "presets.working" : mode === "delete" ? "presets.delete" : "presets.save")}
            </button>
            <button type="button" className="button secondary" disabled={w.isPresetBusy}
              onClick={() => { setError(""); setMode(mode === "save" ? null : "manage"); }}>{t("presets.cancel")}</button>
          </div>
        </form>}
        {error && <p className="preset-error" role="alert">{error}</p>}
      </div>
    </dialog>
  </div>;
}
