import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en";
import zh from "./locales/zh";
import ja from "./locales/ja";

export type Language = "en" | "zh" | "ja";
export type LanguagePreference = "auto" | Language;
export const SUPPORTED_LANGUAGES: Language[] = ["en", "zh", "ja"];

export function normalizeLanguage(
  raw: string | null | undefined
): LanguagePreference {
  if (!raw) return "auto";
  const lower = raw.toLowerCase();
  if (lower === "auto" || lower === "system") return "auto";
  if (lower.startsWith("zh")) return "zh";
  if (lower.startsWith("ja")) return "ja";
  if (lower.startsWith("en")) return "en";
  return "auto";
}

export function detectSystemLanguage(): Language {
  const nav = (navigator.language || "").toLowerCase();
  if (nav.startsWith("zh")) return "zh";
  if (nav.startsWith("ja")) return "ja";
  return "en";
}

export function resolveLanguage(pref: LanguagePreference): Language {
  return pref === "auto" ? detectSystemLanguage() : pref;
}

export function applyLanguagePreference(
  raw: string | null | undefined
): Language {
  const language = resolveLanguage(normalizeLanguage(raw));
  if (i18n.language !== language) void i18n.changeLanguage(language);
  return language;
}

function syncDocumentLanguage(language: string) {
  if (typeof document === "undefined") return;
  const resolved = normalizeLanguage(i18n.resolvedLanguage || language);
  const supported = resolveLanguage(resolved);
  document.documentElement.lang = supported === "zh" ? "zh-CN" : supported;
}

i18n.on("languageChanged", syncDocumentLanguage);

void i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    zh: { translation: zh },
    ja: { translation: ja },
  },
  lng: detectSystemLanguage(),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
  returnEmptyString: false,
});
syncDocumentLanguage(i18n.language || detectSystemLanguage());

export default i18n;
