import { afterEach, expect, it } from "vitest";
import i18n from "../i18n";
import {
  errorMessage,
  errorText,
  photoProfileText,
  processingMessage,
} from "./model";
import { clipErrorText } from "./snapshot";
import type { PhotoInfo } from "./types";

afterEach(async () => {
  await i18n.changeLanguage("zh");
});
const info: PhotoInfo = {
  path: "/photo.png",
  filename: "photo.png",
  size: 100,
  format: "png",
  width: 10,
  height: 10,
  stored_width: 10,
  stored_height: 10,
  bit_depth: 16,
  has_alpha: false,
  orientation: 1,
  color_profile: "Camera vendor ICC",
  color_status: "embedded",
  source_version: "v1",
};

it.each([
  [
    "en",
    [
      "Reading photo information",
      "Decoding and converting to sRGB",
      "Applying LUT",
      "Writing and verifying photo",
    ],
    "Untagged color space",
    "Invalid ICC or channel mismatch",
  ],
  [
    "ja",
    [
      "写真情報を読み込み中",
      "デコードして sRGB に変換中",
      "LUT を適用中",
      "写真を書き込み、検証中",
    ],
    "色空間が未指定",
    "ICC が無効、またはチャンネルが一致しません",
  ],
] as const)(
  "translates native photo stages and built-in profiles in %s while preserving vendor names",
  async (language, stages, unknown, invalid) => {
    await i18n.changeLanguage(language);
    expect(
      ["photo.read", "photo.normalize", "photo.lut", "photo.write"].map(
        processingMessage
      )
    ).toEqual(stages);
    expect(processingMessage("解码并转换到 sRGB")).toBe(stages[1]);
    expect(
      photoProfileText({
        ...info,
        color_status: "unknown",
        color_profile: "未标记色彩空间",
      })
    ).toBe(unknown);
    expect(
      photoProfileText({
        ...info,
        color_status: "invalid",
        color_profile: "无效或通道不匹配的 ICC",
      })
    ).toBe(invalid);
    expect(photoProfileText(info)).toBe("Camera vendor ICC");
  }
);

it("decodes nested persistence errors without leaking the wire protocol", async () => {
  await i18n.changeLanguage("en");
  const reason = "\u001fws.too_large\u001f\u001f工作区超过 8 MB";
  const encoded = `\u001fws.autosave_blocked\u001f${JSON.stringify({
    reason,
  })}\u001f已停止保存`;
  const message = errorMessage(new Error(encoded));
  expect(message).toContain("The workspace file exceeds 8 MB");
  expect(message).not.toContain("\u001f");
  expect(errorText(new Error(encoded))).toBe(encoded);
});

it("translates persisted structured failures at display time", async () => {
  const encoded =
    "\u001fphoto.input_profile_unknown\u001f\u001f照片未标记色彩空间";
  await i18n.changeLanguage("en");
  expect(clipErrorText(encoded)).toBe(
    "The photo has no color profile; explicitly assign its input color space."
  );
  await i18n.changeLanguage("ja");
  expect(clipErrorText(encoded)).toBe(
    "写真にカラープロファイルがありません。入力色空間を明示的に指定してください。"
  );
});
