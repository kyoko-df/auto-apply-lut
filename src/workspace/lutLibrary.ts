import i18n from "../i18n";
import type { LutCategory, LutLibraryItem } from "./types";

export const ALL_LUTS = "all";
export const UNCATEGORIZED_LUTS = "uncategorized";
export const MAX_CATEGORY_NAME = 80;

export function categoryName(value: string, categories: LutCategory[], exceptId?: string): string {
  const name = value.trim();
  if (!name || [...name].length > MAX_CATEGORY_NAME || /[\u0000-\u001f\u007f-\u009f]/.test(name))
    throw new Error(i18n.t("lutLibrary.badName"));
  if (categories.some(c => c.id !== exceptId && c.name.toLowerCase() === name.toLowerCase()))
    throw new Error(i18n.t("lutLibrary.duplicateName"));
  return name;
}

export function filterLuts(items: LutLibraryItem[], search: string, favorites: boolean, category: string): LutLibraryItem[] {
  const query = search.trim().toLowerCase();
  return items.filter(item => item.name.toLowerCase().includes(query) &&
    (!favorites || item.is_favorite) &&
    (category === ALL_LUTS || (category === UNCATEGORIZED_LUTS ? !item.category_id : item.category_id === category)));
}
