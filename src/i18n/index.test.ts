import { afterEach, describe, expect, it } from "vitest";
import i18n, { applyLanguagePreference } from "./index";

afterEach(async () => {
  Object.defineProperty(navigator, "language", {
    value: "zh-CN",
    configurable: true,
  });
  await i18n.changeLanguage("zh");
});

describe("document language", () => {
  it("synchronizes explicit preferences and legacy regional preferences", async () => {
    applyLanguagePreference("ja");
    expect(i18n.language).toBe("ja");
    expect(document.documentElement.lang).toBe("ja");
    applyLanguagePreference("en-US");
    expect(document.documentElement.lang).toBe("en");
    applyLanguagePreference("zh-CN");
    expect(document.documentElement.lang).toBe("zh-CN");
  });

  it("synchronizes direct language changes and fallback resources", async () => {
    await i18n.changeLanguage("ja");
    expect(document.documentElement.lang).toBe("ja");
    await i18n.changeLanguage("fr");
    expect(document.documentElement.lang).toBe("en");
  });

  it("follows the system preference and falls back for unsupported system languages", () => {
    Object.defineProperty(navigator, "language", {
      value: "ja-JP",
      configurable: true,
    });
    applyLanguagePreference("auto");
    expect(document.documentElement.lang).toBe("ja");
    Object.defineProperty(navigator, "language", {
      value: "de-DE",
      configurable: true,
    });
    applyLanguagePreference("auto");
    expect(document.documentElement.lang).toBe("en");
  });
});
