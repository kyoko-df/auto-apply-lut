import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach } from "vitest";
import { cleanup } from "@testing-library/react";
import i18n from "../i18n";

// Tests assert against the Chinese source strings; keep them stable.
// jsdom's navigator.language is en-US, and the app follows the system
// language by default, so pin the system language to Chinese.
Object.defineProperty(window.navigator, "language", {
  value: "zh-CN",
  configurable: true,
});
void i18n.changeLanguage("zh");

beforeEach(async () => {
  await i18n.changeLanguage("zh");
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
    writable: true,
  });
});

afterEach(() => {
  cleanup();
});
