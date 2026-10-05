# LUTlab

[中文](#中文) | [English](#english) | [日本語](#日本語)

## 中文

用于批量视频 LUT 调色的本地桌面工作台。React 19 + TypeScript 构建界面，Tauri 2 / Rust 管理文件与任务，FFmpeg 执行真实视频处理。

### 工作流

1. **导入素材**：多选视频、导入整个文件夹，或拖入视频、LUT 和文件夹。重复素材自动去重；元数据渐进读取，可停止继续添加。
2. **选择风格**：在右侧选择 LUT，调节 0–100% 强度。每个视频独立设置，可用 ⌘/Ctrl 点选、Shift 连选，将当前风格应用到所选或全部素材，并撤销修改。选择「原始色彩」可进行无 LUT 转码。
3. **对比预览**：拖动时间轴定位画面；在原片、分割对比、调色后之间切换。预览由 FFmpeg 生成，无需先导出视频，也不依赖 WebView 的视频解码能力。
4. **批量导出**：设置格式、编码、质量和输出目录。选择导出全部待处理素材或仅所选素材。队列显示实际编码器、平滑速度和预计剩余时间，支持取消、重试失败/取消项目、定位完成文件。更改风格或输出参数后，可重新导出。

界面提供中文深色工作台、素材搜索、LUT 资料库、键盘快捷键与引擎诊断。`⌘/Ctrl + O` 导入视频，`⌘/Ctrl + Enter` 开始导出，`⌘/Ctrl + A` 全选，`⌘/Ctrl + Z` 撤销。素材与队列超过 80 项时按窗口渲染。

### 处理能力

| 功能     | 行为                                                                                                     |
| -------- | -------------------------------------------------------------------------------------------------------- |
| 导入视频 | MP4、MOV、MKV、AVI、WebM、M4V、WMV、FLV；实际解码能力取决于 FFmpeg                                       |
| LUT      | 3D CUBE、纯 1D CUBE、3DL、CSP，以及现有解析器支持的 M3D / LOOK / LUT 变体；混合 1D + 3D CUBE 会明确拒绝  |
| 导出     | MP4 / MOV / MKV，H.264 / H.265 / ProRes 422 HQ；ProRes 使用 MOV                                          |
| 音频     | 默认 AAC；可复制原始音轨，或在兼容封装下使用 PCM；映射所有输入音轨                                       |
| 尺寸     | 默认原始尺寸；可适配 720p / 1080p / 4K，保持比例并填充，不裁切；奇数尺寸补齐偶数                         |
| 并发     | 1–4 个视频，FFmpeg 线程预算随并发数分配                                                                  |
| 硬件编码 | macOS 尝试 VideoToolbox；其他平台尝试 NVENC / QSV；仅在硬件初始化/能力不支持时回退软件编码；同一引擎缓存确定不可用的编码器                                    |
| 输出安全 | 自动添加 `_lut_applied` 后缀，重名递增；先写同卷临时文件，成功后以不覆盖方式发布；失败和取消清理临时文件 |
| 帧预览   | 180 ms 防抖，1280 像素上限；同客户端新请求取消旧请求；32 MB / 32 项显示缓存 + 256 MiB / 8 项 16-bit 工作帧缓存 + 128 MiB / 8 项 LUT 缓存，最多 2 个预览渲染               |

预览和导出共享 LUT 准备及滤镜构造逻辑，在 16 位 RGB 中混合 LUT 强度。预览 JPEG 与有损导出在压缩精度和缩放上会有差别，不应把预览当作逐像素成片验证。

快速预览先缩小工作帧，准确预览先调色再缩放；调节强度可以复用解码帧与已解析 LUT。引擎发现会缓存成对可执行文件，并在路径、环境或文件版本变化时失效。

**色彩范围**：默认保持输入，不做隐式色域转换；界面展示色域、传递函数和位深。可明确选择 Rec.709 解读，或将 Rec.2020 PQ / HLG 转为 Rec.709 SDR，再应用风格 LUT。此选项作用于整个批次，不同输入空间请分批处理。HEVC 支持 8/10-bit 4:2:0，H.264 为 8-bit，ProRes HQ 为 10-bit 4:2:2。不会自动识别相机 Log，也不提供 HDR / Dolby Vision 母版工作流。

**恢复与边界**：工作区会原子保存素材、参数、选择、输出路径和最近批次。正常退出前等待保存，导出中退出需取消并等待进程结束；异常退出后的未完成任务恢复为待重试，从头重新导出，不做编码断点续传。损坏设置/资料库保留备份并提供恢复入口，保存失败有明确提示。预览仍是可定位的静态帧，不是实时调色视频播放。浏览器模式仅用于查看界面，实际文件处理需要桌面应用。

### 本地开发

需要 Node.js、pnpm、Rust 及平台对应的 Tauri 构建工具。安装 FFmpeg 与 ffprobe，并加入 PATH；也可在应用设置中选择 FFmpeg，同目录下应包含 ffprobe。

```bash
pnpm install
pnpm tauri dev
```

仅查看前端：

```bash
pnpm dev
```

构建：

```bash
pnpm build
pnpm tauri build
# macOS 本地调试应用包
pnpm tauri build --debug --bundles app
```

### 独立 release 应用

**Full 版本内置 FFmpeg 和 ffprobe，使用者不需要安装 Node.js、Rust、Homebrew 或 FFmpeg。** 当前已配置可复现下载的目标为 **Apple Silicon / macOS 12+**，固定使用 FFmpeg 9.0.2 静态构建。系统自带的 WebKit 和系统库仍是平台运行环境。

```bash
# 开发机首次构建：下载固定引擎、验证 SHA256、编译 release 并打包
pnpm release:mac:arm64

# 已准备引擎后的构建
pnpm tauri:build:full:mac:arm64

# 对实际 .app 做独立验收（仅系统 PATH、真实编码/预览、代码签名）
node scripts/verify-macos-release.mjs \
  src-tauri/target/aarch64-apple-darwin/release/bundle/macos/LUTlab.app
```

生成位置：

- 应用：`src-tauri/target/aarch64-apple-darwin/release/bundle/macos/LUTlab.app`
- 安装镜像：`src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/LUTlab_0.1.0_aarch64.dmg`

下载 URL 和 SHA256 固定在 `src-tauri/resources/ffmpeg-manifest.json`，二进制不提交 Git。打包前会检查架构、动态依赖、必要编码器/滤镜，并真实执行 H.264 / HEVC / ProRes + 音频和 JPEG 预览以及 RGB16 FFV1/NUT 工作帧缓存。依赖 Homebrew 动态库的二进制会被拒绝，Full 检查不允许跳过。运行时优先使用应用内成对的 FFmpeg / ffprobe；应用设置中的显式路径覆盖仍有效。

`tauri:build:full:mac` 构建当前 Mac 架构；`tauri:build:full:mac:universal` 需要额外提供 Intel 和 ARM 两套静态引擎及 Rust target。`tauri:build:full:win` 需要在 Windows 构建机提供经验证的 x64 静态引擎，配置已包含离线 WebView2 安装器；本次没有完成 Intel、Windows 或 Linux 实机验收。Lite 版本仍依赖外部 FFmpeg，不属于免依赖发行包。

标签 CI 默认构建 macOS ARM Full **草稿发布**；Windows 是手动任务，需配置已审计的 `FFMPEG_WINDOWS_VENDOR_URL` 和 `FFMPEG_WINDOWS_VENDOR_SHA256`。本地 macOS 包使用 ad-hoc 签名，尚无 Apple Developer ID 签名与公证。对外公开分发前还需补齐对应源码交付材料，详见 [第三方许可与来源记录](THIRD_PARTY_NOTICES.md)。

### 验证

```bash
pnpm test
pnpm test:release
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

Rust 中的真实视频测试使用可发现的 FFmpeg 生成小型测试素材，覆盖强度像素、1D CUBE、特殊字符路径、多音轨、并行处理、输出不覆盖、硬件编码回退、预览缓存与子进程取消。本地没有可发现的 FFmpeg 时，相关测试会提前返回；发行 CI 通过 `LUTLAB_REQUIRE_MEDIA_TESTS=1` 将缺少引擎变为失败；完整验收应使用系统引擎或设置 `FFMPEG_PATH` / `FFPROBE_PATH` 指向已下载的内置引擎，不能把提前返回算作媒体验收。

手动验收应包含：导入不同分辨率的两个视频与一个 CUBE → 调节强度并对比任意帧 → 应用全部 → 并行导出 → 使用 ffprobe 检查成片时长、尺寸和音轨 → 再次导出确认重名安全 → 取消长任务 → 重试 → 切换 FFmpeg 路径并重新预览。

### 代码结构

- `src/App.tsx`：工作台布局、导航、原生拖入与快捷键。
- `src/components/ExportInspector.tsx`：LUT 强度、编码组合、导出参数与输出操作。
- `src/components/FramePreview.tsx`：时间轴、分割预览、防抖与过期结果处理。
- `src/workspace/useWorkspace.ts`：导入、设置持久化、任务快照、轮询、取消与重试。
- `src/workspace/model.ts` / `types.ts`：纯状态转换、设置迁移、前后端数据契约。
- `src-tauri/src/commands/preview.rs`：分层缓存、可取消的帧预览服务。
- `src-tauri/src/commands/workspace.rs` / `startup.rs`：工作区原子保存、安全退出、启动存储恢复。
- `src/components/WorkspaceStatus.tsx` / `WindowedList.tsx`：恢复与退出交互、大列表窗口渲染。
- `src-tauri/src/commands/batch_manager.rs`：批量验证、输出命名、并发调度、逐项状态。
- `src-tauri/src/core/ffmpeg/lut.rs`：预览/导出共享的 LUT 准备与强度滤镜。
- `src-tauri/src/core/ffmpeg/processor.rs`：FFmpeg 子进程、进度、取消、编码回退、安全发布。
- `src-tauri/src/database/`：LUT 资料库与历史任务快照。

---

## English

A local desktop workbench for batch LUT color grading of videos. The interface is built with React 19 + TypeScript, Tauri 2 / Rust manages files and tasks, and FFmpeg performs the actual video processing.

### Workflow

1. **Import footage**: multi-select videos, import an entire folder, or drag in videos, LUTs, and folders. Duplicate clips are deduplicated automatically; metadata loads progressively and importing can be stopped midway.
2. **Choose a look**: pick a LUT in the right panel and adjust 0–100% strength. Each clip has independent settings; ⌘/Ctrl-click and Shift-click to multi-select, apply the current look to selected or all clips, and undo changes. Choose "原始色彩" (original color) for a LUT-free transcode.
3. **Compare previews**: scrub the timeline to locate a frame; switch between original, split comparison, and graded views. Previews are generated by FFmpeg — no export is required first, and they do not depend on WebView video decoding.
4. **Batch export**: set format, codec, quality, and output directory. Export all pending clips or only the selected ones. The queue shows the actual encoder, smoothed speed, and estimated time remaining, with cancel, retry for failed/cancelled items, and reveal-in-Finder for finished files. Re-export after changing the look or output settings.

The UI provides a Chinese dark workspace, clip search, a LUT library, keyboard shortcuts, and engine diagnostics. `⌘/Ctrl + O` imports videos, `⌘/Ctrl + Enter` starts export, `⌘/Ctrl + A` selects all, `⌘/Ctrl + Z` undoes. Lists longer than 80 items are window-rendered.

### Capabilities

| Feature            | Behavior                                                                                                                                                                  |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Video import       | MP4, MOV, MKV, AVI, WebM, M4V, WMV, FLV; actual decoding depends on FFmpeg                                                                                                |
| LUT                | 3D CUBE, pure 1D CUBE, 3DL, CSP, plus M3D / LOOK / LUT variants supported by the existing parser; mixed 1D + 3D CUBE files are explicitly rejected                          |
| Export             | MP4 / MOV / MKV, H.264 / H.265 / ProRes 422 HQ; ProRes uses MOV                                                                                                           |
| Audio              | AAC by default; copy original tracks, or PCM in compatible containers; all input audio tracks are mapped                                                                   |
| Size               | Original size by default; fit to 720p / 1080p / 4K with aspect ratio preserved and padding, no cropping; odd dimensions are rounded up to even                              |
| Concurrency        | 1–4 videos; the FFmpeg thread budget is divided across concurrent jobs                                                                                                     |
| Hardware encoding  | VideoToolbox on macOS; NVENC / QSV on other platforms; software fallback only on hardware initialization/capability failures; encoders proven unusable are cached per engine |
| Output safety      | Automatic `_lut_applied` suffix with collision increments; writes a temporary file on the destination volume, then publishes without overwriting; failures and cancellations clean up temp files |
| Frame preview      | 180 ms debounce, 1280 px cap; a new request from the same client cancels the previous one; 32 MB / 32-entry display cache + 256 MiB / 8-entry 16-bit working-frame cache + 128 MiB / 8-entry LUT cache; at most 2 concurrent preview renders |

Preview and export share LUT preparation and filter-graph construction, blending LUT strength in 16-bit RGB. Preview JPEGs and lossy exports differ in compression precision and scaling — do not treat the preview as pixel-exact validation of the final output.

Fast preview scales the working frame first; accurate preview applies the LUT before scaling. Adjusting strength can reuse the decoded frame and parsed LUT. Engine discovery caches validated executable pairs and invalidates on path, environment, or file-version changes.

**Color range**: the default preserves the source interpretation with no implicit gamut conversion; the UI displays gamut, transfer function, and bit depth. You can explicitly choose Rec.709 interpretation, or convert Rec.2020 PQ / HLG to Rec.709 SDR before applying the creative LUT. This option applies to the whole batch — process different input spaces in separate batches. HEVC supports 8/10-bit 4:2:0, H.264 is 8-bit, and ProRes HQ is 10-bit 4:2:2. Camera Log is not detected automatically, and there is no HDR / Dolby Vision mastering workflow.

**Recovery and limits**: the workspace atomically saves clips, settings, selection, output paths, and the most recent batch. A normal exit waits for the save; quitting during export requires cancelling and waiting for processes to finish. Tasks interrupted by an abnormal exit are restored as retryable and re-exported from the beginning — there is no encode resume. Corrupt settings/library files keep their original bytes as a backup with a recovery entry point, and persistent save failures are surfaced. Previews are seekable still frames, not real-time graded playback. Browser mode is for viewing the UI only; actual file processing requires the desktop app.

### Local development

Requires Node.js, pnpm, Rust, and the platform-specific Tauri build tools. Install FFmpeg and ffprobe on your PATH, or choose FFmpeg in the app settings — ffprobe should live in the same directory.

```bash
pnpm install
pnpm tauri dev
```

Frontend only:

```bash
pnpm dev
```

Build:

```bash
pnpm build
pnpm tauri build
# macOS local debug app bundle
pnpm tauri build --debug --bundles app
```

### Standalone release app

**Full builds embed FFmpeg and ffprobe — users do not need Node.js, Rust, Homebrew, or FFmpeg installed.** The currently configured reproducible-download target is **Apple Silicon / macOS 12+**, pinned to a static FFmpeg 9.0.2 build. The system WebKit and system libraries remain part of the platform runtime.

```bash
# First build on a dev machine: download pinned engines, verify SHA256, compile release and bundle
pnpm release:mac:arm64

# Build once engines are prepared
pnpm tauri:build:full:mac:arm64

# Standalone acceptance on the real .app (system-only PATH, real encode/preview, code signature)
node scripts/verify-macos-release.mjs \
  src-tauri/target/aarch64-apple-darwin/release/bundle/macos/LUTlab.app
```

Outputs:

- App: `src-tauri/target/aarch64-apple-darwin/release/bundle/macos/LUTlab.app`
- Installer image: `src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/LUTlab_0.1.0_aarch64.dmg`

Download URLs and SHA256 are pinned in `src-tauri/resources/ffmpeg-manifest.json`; binaries are not committed to Git. Before bundling, architecture, dynamic dependencies, and required encoders/filters are checked, and real H.264 / HEVC / ProRes + audio encodes, JPEG previews, and RGB16 FFV1/NUT working-frame cache runs are executed. Binaries that depend on Homebrew dynamic libraries are rejected, and Full checks cannot be skipped. At runtime the bundled FFmpeg / ffprobe pair takes precedence; explicit paths configured in app settings still override.

`tauri:build:full:mac` builds for the current Mac architecture; `tauri:build:full:mac:universal` additionally requires both Intel and ARM static engines plus the Rust targets. `tauri:build:full:win` requires a verified x64 static engine on a Windows build machine; the config already includes an offline WebView2 installer; Intel, Windows, and Linux have not completed on-device acceptance this round. Lite builds still depend on an external FFmpeg and are not dependency-free packages.

Tag CI builds a macOS ARM Full **draft release** by default; Windows is a manual job that requires the audited `FFMPEG_WINDOWS_VENDOR_URL` and `FFMPEG_WINDOWS_VENDOR_SHA256` repository variables. Local macOS bundles use ad-hoc signing — there is no Apple Developer ID signing or notarization yet. Public distribution also requires complete matching source delivery; see [Third-party licenses and provenance](THIRD_PARTY_NOTICES.md).

### Verification

```bash
pnpm test
pnpm test:release
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

Real-video Rust tests generate small fixtures with a discoverable FFmpeg, covering strength pixels, 1D CUBE, special-character paths, multi-track audio, parallel processing, no-overwrite output, hardware-encode fallback, preview caching, and child-process cancellation. When no FFmpeg is discoverable, those tests return early; release CI turns a missing engine into a failure via `LUTLAB_REQUIRE_MEDIA_TESTS=1`; full acceptance should use system engines or point `FFMPEG_PATH` / `FFPROBE_PATH` at the downloaded bundled engines — an early return does not count as media verification.

Manual acceptance should cover: import two videos of different resolutions plus a CUBE → adjust strength and compare arbitrary frames → apply to all → export in parallel → check output duration, dimensions, and audio tracks with ffprobe → export again to confirm rename safety → cancel a long job → retry → switch the FFmpeg path and re-preview.

### Code structure

- `src/App.tsx`: workspace layout, navigation, native drag-in and keyboard shortcuts.
- `src/components/ExportInspector.tsx`: LUT strength, codec combinations, export parameters and output actions.
- `src/components/FramePreview.tsx`: timeline, split preview, debounce and stale-result handling.
- `src/workspace/useWorkspace.ts`: import, settings persistence, task snapshots, polling, cancel and retry.
- `src/workspace/model.ts` / `types.ts`: pure state transitions, settings migration, frontend/backend contracts.
- `src-tauri/src/commands/preview.rs`: layered caching, cancellable frame preview service.
- `src-tauri/src/commands/workspace.rs` / `startup.rs`: atomic workspace saves, safe exit, startup storage recovery.
- `src/components/WorkspaceStatus.tsx` / `WindowedList.tsx`: recovery and exit interactions, large-list windowed rendering.
- `src-tauri/src/commands/batch_manager.rs`: batch validation, output naming, concurrency scheduling, per-item state.
- `src-tauri/src/core/ffmpeg/lut.rs`: LUT preparation and strength filters shared by preview and export.
- `src-tauri/src/core/ffmpeg/processor.rs`: FFmpeg child process, progress, cancellation, encode fallback, safe publishing.
- `src-tauri/src/database/`: LUT library and historical task snapshots.

---

## 日本語

バッチ動画 LUT カラーグレーディングのためのローカルデスクトップワークベンチ。UI は React 19 + TypeScript で構築し、Tauri 2 / Rust がファイルとタスクを管理し、FFmpeg が実際の動画処理を実行します。

### ワークフロー

1. **素材のインポート**: 複数の動画を選択、フォルダーごとインポート、または動画・LUT・フォルダーをドラッグ&ドロップ。重複する素材は自動的に除外され、メタデータは段階的に読み込まれ、途中で追加を停止できます。
2. **ルックの選択**: 右側のパネルで LUT を選択し、0–100% の強度を調整。各動画は個別に設定でき、⌘/Ctrl クリックや Shift クリックで複数選択し、現在のルックを選択中またはすべての素材に適用でき、取り消しも可能です。「原始色彩」を選ぶと LUT なしのトランスコードになります。
3. **プレビュー比較**: タイムラインをドラッグしてフレームを定位し、元の映像・分割比較・グレーディング後を切り替えます。プレビューは FFmpeg が生成するため、事前の書き出しは不要で、WebView の動画デコード能力にも依存しません。
4. **バッチ書き出し**: フォーマット、コーデック、品質、出力先を設定。すべての未処理素材または選択中の素材のみを書き出せます。キューには実際のエンコーダー、平滑化された速度、残り時間の予測が表示され、キャンセル、失敗/キャンセル項目の再試行、完了ファイルの表示が可能です。ルックや出力パラメーターの変更後に再書き出しできます。

UI は中国語のダークワークベンチ、素材検索、LUT ライブラリ、キーボードショートカット、エンジン診断を提供します。`⌘/Ctrl + O` で動画をインポート、`⌘/Ctrl + Enter` で書き出し開始、`⌘/Ctrl + A` で全選択、`⌘/Ctrl + Z` で取り消し。素材とキューが 80 項目を超えるとウィンドウ描画されます。

### 処理能力

| 機能                 | 動作                                                                                                                                                                         |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 動画インポート       | MP4、MOV、MKV、AVI、WebM、M4V、WMV、FLV。実際のデコード能力は FFmpeg に依存                                                                                                   |
| LUT                  | 3D CUBE、純粋な 1D CUBE、3DL、CSP、および既存パーサーが対応する M3D / LOOK / LUT バリアント。1D + 3D 混合の CUBE は明示的に拒否                                                |
| 書き出し             | MP4 / MOV / MKV、H.264 / H.265 / ProRes 422 HQ。ProRes は MOV を使用                                                                                                          |
| 音声                 | デフォルト AAC。元の音声トラックをコピー、または互換コンテナで PCM を使用可能。すべての入力音声トラックをマッピング                                                             |
| サイズ               | デフォルトは元のサイズ。720p / 1080p / 4K に合わせ、アスペクト比を維持してパディング、クロップなし。奇数サイズは偶数に補正                                                      |
| 並行処理             | 1–4 本の動画。FFmpeg のスレッド予算は並行数に応じて配分                                                                                                                        |
| ハードウェアエンコード | macOS は VideoToolbox、その他のプラットフォームは NVENC / QSV を試行。ハードウェア初期化/能力非対応時のみソフトウェアエンコードにフォールバック。同一エンジンで使用不可と判明したエンコーダーはキャッシュ |
| 出力の安全性         | `_lut_applied` サフィックスを自動付加し、重名はインクリメント。出力先ボリュームの一時ファイルに書き込み、成功後に上書きしない方式で公開。失敗・キャンセル時は一時ファイルを削除   |
| フレームプレビュー   | 180ms デバウンス、1280 ピクセル上限。同一クライアントの新規リクエストは旧リクエストをキャンセル。32 MB / 32 項目の表示キャッシュ + 256 MiB / 8 項目の 16 ビット作業フレームキャッシュ + 128 MiB / 8 項目の LUT キャッシュ。最大 2 つのプレビューレンダリング |

プレビューと書き出しは LUT 準備とフィルターグラフ構築ロジックを共有し、16 ビット RGB で LUT 強度をブレンドします。プレビュー JPEG と不可逆圧縮の書き出しは圧縮精度とスケーリングで差異があるため、プレビューをピクセル単位の最終検証として扱うべきではありません。

高速プレビューは作業フレームを先に縮小し、正確なプレビューはグレーディング後にスケーリングします。強度の調整はデコード済みフレームと解析済み LUT を再利用できます。エンジン検出は実行可能ファイルのペアをキャッシュし、パス・環境・ファイルバージョンの変化で無効化されます。

**色域**: デフォルトでは入力の解釈を維持し、暗黙の色域変換は行いません。UI には色域、伝達関数、ビット深度が表示されます。Rec.709 解釈を明示的に選択するか、Rec.2020 PQ / HLG を Rec.709 SDR に変換してからクリエイティブ LUT を適用できます。このオプションはバッチ全体に作用するため、異なる入力空間は分けて処理してください。HEVC は 8/10 ビット 4:2:0、H.264 は 8 ビット、ProRes HQ は 10 ビット 4:2:2 に対応。カメラ Log の自動認識や HDR / Dolby Vision マスタリングワークフローはありません。

**復旧と制限**: ワークスペースは素材、パラメーター、選択状態、出力先、最近のバッチをアトミックに保存します。正常終了前に保存を待機し、書き出し中の終了はキャンセルしてプロセスの終了を待つ必要があります。異常終了後の未完了タスクは再試行可能として復元され、最初から再書き出しされます(エンコードの中断点再開はありません)。破損した設定/ライブラリは元のバイトをバックアップとして保持し、復旧手段を提供します。保存失敗は明示的に通知されます。プレビューはシーク可能な静止フレームであり、リアルタイムのグレーディング再生ではありません。ブラウザーモードは UI の確認専用で、実際のファイル処理にはデスクトップアプリが必要です。

### ローカル開発

Node.js、pnpm、Rust、およびプラットフォームに対応する Tauri ビルドツールが必要です。FFmpeg と ffprobe をインストールして PATH に追加するか、アプリ設定で FFmpeg を選択してください(同じディレクトリに ffprobe が必要です)。

```bash
pnpm install
pnpm tauri dev
```

フロントエンドのみ:

```bash
pnpm dev
```

ビルド:

```bash
pnpm build
pnpm tauri build
# macOS ローカルデバッグアプリバンドル
pnpm tauri build --debug --bundles app
```

### スタンドアロンリリースアプリ

**Full 版には FFmpeg と ffprobe が内蔵されており、ユーザーは Node.js、Rust、Homebrew、FFmpeg をインストールする必要がありません。** 現在、再現可能なダウンロードが設定されているターゲットは **Apple Silicon / macOS 12+** で、FFmpeg 9.0.2 の静的ビルドに固定されています。システムの WebKit とシステムライブラリはプラットフォームランタイムの一部です。

```bash
# 開発機での初回ビルド: 固定エンジンのダウンロード、SHA256 検証、release コンパイルとバンドル
pnpm release:mac:arm64

# エンジン準備済みのビルド
pnpm tauri:build:full:mac:arm64

# 実際の .app に対するスタンドアロン検証(システム PATH のみ、実エンコード/プレビュー、コード署名)
node scripts/verify-macos-release.mjs \
  src-tauri/target/aarch64-apple-darwin/release/bundle/macos/LUTlab.app
```

生成場所:

- アプリ: `src-tauri/target/aarch64-apple-darwin/release/bundle/macos/LUTlab.app`
- インストーラーイメージ: `src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/LUTlab_0.1.0_aarch64.dmg`

ダウンロード URL と SHA256 は `src-tauri/resources/ffmpeg-manifest.json` に固定され、バイナリは Git にコミットされません。バンドル前にアーキテクチャ、動的依存関係、必要なエンコーダー/フィルターをチェックし、H.264 / HEVC / ProRes + 音声、JPEG プレビュー、RGB16 FFV1/NUT 作業フレームキャッシュを実際に実行します。Homebrew の動的ライブラリに依存するバイナリは拒否され、Full チェックをスキップできません。実行時はアプリ内の FFmpeg / ffprobe ペアが優先され、アプリ設定の明示的なパス上書きも有効です。

`tauri:build:full:mac` は現在の Mac アーキテクチャ向けにビルドします。`tauri:build:full:mac:universal` には Intel と ARM 両方の静的エンジンと Rust ターゲットが追加で必要です。`tauri:build:full:win` には Windows ビルドマシン上の検証済み x64 静的エンジンが必要で、設定にはオフライン WebView2 インストーラーが含まれています。今回、Intel、Windows、Linux の実機検証は完了していません。Lite 版は引き続き外部 FFmpeg に依存し、依存関係フリーの配布パッケージではありません。

タグ CI はデフォルトで macOS ARM Full の**ドラフトリリース**をビルドします。Windows は手動タスクで、監査済みの `FFMPEG_WINDOWS_VENDOR_URL` と `FFMPEG_WINDOWS_VENDOR_SHA256` リポジトリ変数の設定が必要です。ローカルの macOS パッケージは ad-hoc 署名を使用しており、Apple Developer ID 署名と公証はまだありません。公開配布の前に対応するソースコード提供資料も必要です。詳細は[サードパーティライセンスと出所記録](THIRD_PARTY_NOTICES.md)を参照してください。

### 検証

```bash
pnpm test
pnpm test:release
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

Rust の実動画テストは検出可能な FFmpeg で小型テスト素材を生成し、強度ピクセル、1D CUBE、特殊文字パス、マルチ音声トラック、並列処理、出力の非上書き、ハードウェアエンコードフォールバック、プレビューキャッシュ、子プロセスキャンセルをカバーします。検出可能な FFmpeg がない場合、関連テストは早期リターンします。リリース CI は `LUTLAB_REQUIRE_MEDIA_TESTS=1` でエンジン欠如を失敗に変えます。完全な検証はシステムエンジンを使用するか、`FFMPEG_PATH` / `FFPROBE_PATH` でダウンロード済みのバンドルエンジンを指定してください。早期リターンをメディア検証として扱うことはできません。

手動検証には以下を含めるべきです: 異なる解像度の 2 つの動画と 1 つの CUBE をインポート → 強度を調整して任意のフレームを比較 → すべてに適用 → 並列書き出し → ffprobe で出力の尺、サイズ、音声トラックを確認 → 再書き出しで重名の安全性を確認 → 長時間タスクをキャンセル → 再試行 → FFmpeg パスを切り替えて再プレビュー。

### コード構成

- `src/App.tsx`: ワークベンチレイアウト、ナビゲーション、ネイティブドラッグ&ドロップ、ショートカット。
- `src/components/ExportInspector.tsx`: LUT 強度、エンコード組み合わせ、書き出しパラメーター、出力操作。
- `src/components/FramePreview.tsx`: タイムライン、分割プレビュー、デバウンス、古い結果の処理。
- `src/workspace/useWorkspace.ts`: インポート、設定永続化、タスクスナップショット、ポーリング、キャンセル、再試行。
- `src/workspace/model.ts` / `types.ts`: 純粋な状態遷移、設定マイグレーション、フロントエンド/バックエンド契約。
- `src-tauri/src/commands/preview.rs`: 階層キャッシュ、キャンセル可能なフレームプレビューサービス。
- `src-tauri/src/commands/workspace.rs` / `startup.rs`: ワークスペースのアトミック保存、安全な終了、起動時ストレージ復旧。
- `src/components/WorkspaceStatus.tsx` / `WindowedList.tsx`: 復旧と終了のインタラクション、大規模リストのウィンドウ描画。
- `src-tauri/src/commands/batch_manager.rs`: バッチ検証、出力命名、並行スケジューリング、項目ごとの状態。
- `src-tauri/src/core/ffmpeg/lut.rs`: プレビュー/書き出し共有の LUT 準備と強度フィルター。
- `src-tauri/src/core/ffmpeg/processor.rs`: FFmpeg 子プロセス、進捗、キャンセル、エンコードフォールバック、安全な公開。
- `src-tauri/src/database/`: LUT ライブラリと履歴タスクスナップショット。
