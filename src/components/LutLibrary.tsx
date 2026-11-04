import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight, Check, Plus, Star, X } from "lucide-react";
import { errorMessage } from "../workspace/model";
import { ALL_LUTS, UNCATEGORIZED_LUTS, MAX_CATEGORY_NAME, filterLuts } from "../workspace/lutLibrary";
import type { LutCategory } from "../workspace/types";
import type { useWorkspace } from "../workspace/useWorkspace";
import LutThumbnail from "./LutThumbnail";
import WindowedList from "./WindowedList";

export default function LutLibrary({ workspace: w, search, clearSearch, photoThumbnails, setPhotoThumbnails }: {
  workspace: ReturnType<typeof useWorkspace>;
  search: string;
  clearSearch: () => void;
  photoThumbnails: boolean;
  setPhotoThumbnails: (value: boolean) => void;
}) {
  const { t } = useTranslation();
  const [favorites, setFavorites] = useState(false);
  const [category, setCategory] = useState(ALL_LUTS);
  const [organizing, setOrganizing] = useState(false);
  const [selection, setSelection] = useState<string[]>([]);
  const [destination, setDestination] = useState("");
  const [mode, setMode] = useState<"create" | "manage" | "rename" | "delete" | null>(null);
  const [editing, setEditing] = useState<LutCategory | null>(null);
  const [name, setName] = useState("");
  const [error, setError] = useState("");
  const dialog = useRef<HTMLDialogElement>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const locked = w.loading || w.isExporting || w.isImporting || w.isPresetBusy || w.isLibraryBusy || !w.isDesktop;
  const unavailable = locked || !!w.lutLibraryError;
  const categories = [...w.lutCategories].sort((a, b) => a.name.localeCompare(b.name));
  const filtered = filterLuts(w.luts, search, favorites, category);
  // A batch always contains the visible checked rows, even after filters or favorites change.
  const checked = new Set(selection);
  const selected = filtered.filter(item => checked.has(item.path)).map(item => item.path);
  const categoryNames = new Map(categories.map(item => [item.id, item.name]));
  const active = w.activeClip;
  const photoMode = w.mediaMode === "photo";
  const title = t(`lutLibrary.${mode === "create" ? "createTitle" : mode === "rename" ? "renameTitle" : mode === "delete" ? "deleteTitle" : "manageTitle"}`);
  useEffect(() => {
    if (mode && !dialog.current?.open) dialog.current?.showModal?.();
    else if (!mode) dialog.current?.close?.();
    if (mode === "create" || mode === "rename") nameInput.current?.focus();
  }, [mode]);
  useEffect(() => {
    if (![ALL_LUTS, UNCATEGORIZED_LUTS].includes(category) && !w.lutCategories.some(c => c.id === category))
      setCategory(ALL_LUTS);
    if (destination && destination !== UNCATEGORIZED_LUTS && !w.lutCategories.some(c => c.id === destination))
      setDestination("");
  }, [category, destination, w.lutCategories]);
  useEffect(() => { setSelection([]); }, [search, favorites, category]);

  const perform = async (work: () => Promise<unknown>) => {
    setError("");
    try { await work(); }
    catch (e) {
      const message = errorMessage(e);
      if (mounted.current) setError(message);
      else w.setNotice({ kind: "error", message });
    }
  };
  const openCreate = () => { setName(""); setError(""); setEditing(null); setMode("create"); };
  const edit = (item: LutCategory, next: "rename" | "delete") => {
    setEditing(item); setName(item.name); setError(""); setMode(next);
  };
  const submit = () => perform(async () => {
    if (mode === "create") {
      const created = await w.createLutCategory(name);
      if (mounted.current) { setDestination(created.id); setMode(null); }
    } else if (editing && mode === "rename") {
      await w.renameLutCategory(editing.id, name);
      if (mounted.current) setMode("manage");
    } else if (editing && mode === "delete") {
      await w.deleteLutCategory(editing.id);
      if (mounted.current) setMode("manage");
    }
  });
  const applyLut = (path: string | null) => {
    if (active) w.setClipLook(active.id, { lutPath: path });
    else w.setNotice({ kind: "info", message: t("app.noticeImportFirst") });
  };

  useEffect(() => {
    if (!organizing || unavailable) return;
    const selectResults = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "a") return;
      if (document.querySelector("dialog[open]")) return;
      if (event.target instanceof Element && event.target.closest("input:not([type=checkbox]), textarea, select, [contenteditable=true]")) return;
      event.preventDefault();
      setSelection(filtered.map(item => item.path));
    };
    window.addEventListener("keydown", selectResults);
    return () => window.removeEventListener("keydown", selectResults);
  }, [organizing, unavailable, filtered]);

  return <div className="lut-library-panel">
    <div className="lut-library-tools">
      <div className="lut-library-toolbar">
        <button type="button" className={`library-filter ${favorites ? "active" : ""}`} aria-pressed={favorites}
          onClick={() => setFavorites(value => !value)}><Star size={13} fill={favorites ? "currentColor" : "none"} />{t("lutLibrary.onlyFavorites")}</button>
        <button type="button" className="text-button" disabled={unavailable}
          onClick={openCreate}><Plus size={12} />{t("lutLibrary.newCategory")}</button>
        <button type="button" className="text-button" disabled={unavailable || !categories.length}
          onClick={() => { setError(""); setMode("manage"); }}>{t("lutLibrary.manage")}</button>
      </div>
      <label className="field library-category-filter"><span>{t("lutLibrary.filterLabel")}</span>
        <select aria-label={t("lutLibrary.filterLabel")} value={category} onChange={e => setCategory(e.target.value)}>
          <option value={ALL_LUTS}>{t("lutLibrary.all")}</option>
          <option value={UNCATEGORIZED_LUTS}>{t("lutLibrary.uncategorized")}</option>
          {categories.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
        </select>
      </label>
      <div className="lut-library-toolbar">
        <span className="library-result-count">{t("lutLibrary.matchCount", { count: filtered.length, total: w.luts.length })}</span>
        <button type="button" className="text-button" aria-pressed={organizing} disabled={unavailable || !w.luts.length}
          onClick={() => { setOrganizing(value => !value); setSelection([]); }}>{t(organizing ? "lutLibrary.finishOrganizing" : "lutLibrary.organize")}</button>
      </div>
      {organizing && <div className="library-batch">
        <div className="lut-library-toolbar">
          <span>{t("lutLibrary.selectedCount", { count: selected.length })}</span>
          <button type="button" className="text-button" disabled={unavailable || !filtered.length}
            onClick={() => setSelection(filtered.map(item => item.path))}>{t("lutLibrary.selectVisible")}</button>
          <button type="button" className="text-button" disabled={unavailable || !selection.length}
            onClick={() => setSelection([])}>{t("app.clearSelection")}</button>
        </div>
        <label className="field"><span>{t("lutLibrary.destination")}</span>
          <select aria-label={t("lutLibrary.destination")} value={destination} disabled={unavailable}
            onChange={e => setDestination(e.target.value)}>
            <option value="">{t("lutLibrary.chooseCategory")}</option>
            <option value={UNCATEGORIZED_LUTS}>{t("lutLibrary.uncategorized")}</option>
            {categories.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
          </select>
        </label>
        <button type="button" className="button secondary full-width" disabled={unavailable || !selected.length || !destination}
          onClick={() => void perform(async () => {
            await w.assignLutCategory(selected, destination === UNCATEGORIZED_LUTS ? null : destination);
            if (mounted.current) setSelection([]);
          })}>{t("lutLibrary.assign")}<ArrowRight size={13} /></button>
      </div>}
      {!w.isDesktop && <p className="field-hint">{t("lutLibrary.desktopOnly")}</p>}
      {w.lutLibraryError && <div className="library-read-error" role="alert">
        <p>{t("lutLibrary.readFailed", { message: w.lutLibraryError })}</p>
        <button type="button" className="text-button" disabled={locked}
          onClick={() => void perform(w.retryLutLibrary)}>{t("lutLibrary.retry")}</button>
      </div>}
      {!mode && error && <p className="preset-error" role="alert">{error}</p>}
    </div>
    {active && <div className="lut-preview-note">
      {photoMode ? <label><input type="checkbox" checked={photoThumbnails}
        disabled={locked} onChange={e => setPhotoThumbnails(e.target.checked)} />{t("app.lutPhotoThumbnails")}</label>
        : <span>{t("app.lutVideoThumbnails")}</span>}
    </div>}
    <button type="button" className={`lut-row library-original ${!active?.lutPath ? "selected" : ""}`}
      disabled={w.isExporting || w.loading || w.isPresetBusy || !active} onClick={() => applyLut(null)}>
      <span className="lut-icon neutral"><span className="original-swatch" /></span>
      <span><strong>{t("app.originalColor")}</strong><small>{t("app.noLutApplied")}</small></span>
      {!active?.lutPath && <Check size={14} />}
    </button>
    {filtered.length > 0 && <WindowedList items={filtered} rowHeight={68} itemKey={lut => lut.path} label={t("app.ariaLutList")}>
      {lut => <div className={`lut-list-item ${checked.has(lut.path) ? "library-checked" : ""}`} key={lut.path}>
        {organizing && <input type="checkbox" className="lut-organize-check" checked={checked.has(lut.path)} disabled={unavailable}
          aria-label={t("lutLibrary.selectNamed", { name: lut.name })}
          onChange={e => setSelection(current => e.target.checked ? [...new Set([...current, lut.path])] : current.filter(path => path !== lut.path))} />}
        <button type="button" className={`lut-row ${active?.lutPath === lut.path ? "selected" : ""}`}
          disabled={w.isExporting || w.loading || w.isPresetBusy || !lut.is_valid}
          aria-label={t("lutLibrary.applyNamed", { name: lut.name })} onClick={() => applyLut(lut.path)}
          title={lut.error_message ? errorMessage(lut.error_message) : lut.path}>
          <LutThumbnail clip={active ?? undefined} lut={lut}
            enabled={w.ffmpeg.status === "ready" && !w.isExporting && !w.loading && !w.isPresetBusy && (!photoMode || photoThumbnails)}
            isDesktop={w.isDesktop} inputColorSpace={w.settings.input_color_space}
            alphaPolicy={w.settings.photo_options.output.alpha_policy} engineKey={JSON.stringify(w.ffmpeg)} />
          <span><strong>{lut.name}</strong><small title={categoryNames.get(lut.category_id ?? "")}>
            {lut.is_valid ? `${lut.format} · ${categoryNames.get(lut.category_id ?? "") ?? t("lutLibrary.uncategorized")}` : t("app.fileUnavailable")}
          </small></span>
          {active?.lutPath === lut.path && <Check size={12} />}
        </button>
        <button type="button" className={`icon-button lut-favorite ${lut.is_favorite ? "active" : ""}`} disabled={unavailable}
          aria-label={t(lut.is_favorite ? "lutLibrary.unfavoriteNamed" : "lutLibrary.favoriteNamed", { name: lut.name })}
          aria-pressed={lut.is_favorite} onClick={() => void perform(() => w.setLutFavorite(lut.path, !lut.is_favorite))}>
          <Star size={14} fill={lut.is_favorite ? "currentColor" : "none"} />
        </button>
        <button type="button" className="icon-button lut-remove" aria-label={t("app.ariaRemoveLut", { name: lut.name })}
          disabled={locked} onClick={() => void w.removeLut(lut.path)}><X size={12} /></button>
      </div>}
    </WindowedList>}
    {!w.luts.length && !w.lutLibraryError && <div className="sidebar-empty">
      <strong>{t("app.emptyLutsTitle")}</strong><p>{t("app.emptyLutsLine1")}<br />{t("app.emptyLutsLine2")}</p>
      <button type="button" className="text-button" disabled={locked} onClick={() => void w.importLuts()}>{t("app.importLuts")}<Plus size={13} /></button>
    </div>}
    {w.luts.length > 0 && !filtered.length && <div className="no-results">
      <p>{t("app.noLutMatch")}</p>
      <button type="button" className="text-button" onClick={() => { setFavorites(false); setCategory(ALL_LUTS); clearSearch(); }}>{t("lutLibrary.clearFilters")}</button>
    </div>}
    <dialog ref={dialog} className="app-dialog preset-dialog" aria-label={title}
      onCancel={e => { if (w.isLibraryBusy) e.preventDefault(); else setMode(null); }}
      onClick={e => { if (e.target === e.currentTarget && !w.isLibraryBusy) setMode(null); }}>
      <div className="dialog-content">
        <div className="dialog-heading"><h2>{title}</h2>
          <button type="button" className="icon-button" aria-label={t("lutLibrary.close")} disabled={w.isLibraryBusy} onClick={() => setMode(null)}><X size={18} /></button>
        </div>
        {mode === "manage" ? <>
          <p className="field-hint">{t("lutLibrary.manageHint")}</p>
          <div className="preset-list">
            {!categories.length && <p className="field-hint">{t("lutLibrary.noCategories")}</p>}
            {categories.map(item => <div className="preset-row" key={item.id}>
              <div><strong>{item.name}</strong><span>{t("app.lutCount", { count: w.luts.filter(lut => lut.category_id === item.id).length })}</span></div>
              <div className="preset-row-actions">
                <button type="button" className="text-button" disabled={unavailable} aria-label={t("lutLibrary.renameNamed", { name: item.name })} onClick={() => edit(item, "rename")}>{t("presets.rename")}</button>
                <button type="button" className="text-button" disabled={unavailable} aria-label={t("lutLibrary.deleteNamed", { name: item.name })} onClick={() => edit(item, "delete")}>{t("presets.delete")}</button>
              </div>
            </div>)}
          </div>
          <button type="button" className="text-button" disabled={unavailable} onClick={openCreate}><Plus size={13} />{t("lutLibrary.newCategory")}</button>
        </> : <form onSubmit={e => { e.preventDefault(); if (!unavailable) void submit(); }}>
          {mode === "delete" ? <p className="field-hint">{t("lutLibrary.deleteHint", { name: editing?.name })}</p> :
            <label className="field"><span>{t("lutLibrary.name")}</span><input ref={nameInput} type="text" maxLength={MAX_CATEGORY_NAME * 2} required
              value={name} disabled={unavailable} onChange={e => setName(e.target.value)} /></label>}
          <div className="dialog-buttons">
            <button type="submit" className="button primary" disabled={unavailable || (mode !== "delete" && !name.trim())}>{t(w.isLibraryBusy ? "presets.working" : mode === "delete" ? "presets.delete" : "lutLibrary.save")}</button>
            <button type="button" className="button secondary" disabled={w.isLibraryBusy} onClick={() => { setError(""); setMode(mode === "create" ? null : "manage"); }}>{t("presets.cancel")}</button>
          </div>
        </form>}
        {error && <p className="preset-error" role="alert">{error}</p>}
      </div>
    </dialog>
  </div>;
}
