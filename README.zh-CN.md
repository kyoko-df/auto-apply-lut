# LUTlab

[English](README.md) | 中文 | [日本語](README.ja.md)

用于批量视频 LUT 调色的本地桌面工作台。React 19 + TypeScript 构建界面，Tauri 2 / Rust 管理文件与任务，FFmpeg 执行真实视频处理。

## 工作流

1. **导入素材**：多选视频、导入整个文件夹，或拖入视频、LUT 和文件夹。重复素材自动去重；元数据渐进读取，可停止继续添加。
2. **选择风格**：在右侧选择 LUT，调节 0–100% 强度。每个视频独立设置，可用 ⌘/Ctrl 点选、Shift 连选，将当前风格应用到所选或全部素材，并撤销修改。选择「原始色彩」可进行无 LUT 转码。
3. **对比预览**：拖动时间轴定位画面；在原片、分割对比、调色后之间切换。预览由 FFmpeg 生成，无需先导出视频，也不依赖 WebView 的视频解码能力。
4. **批量导出**：设置格式、编码、质量和输出目录。选择导出全部待处理素材或仅所选素材。队列显示实际编码器、平滑速度和预计剩余时间，支持取消、重试失败/取消项目、定位完成文件。更改风格或输出参数后，可重新导出。

界面提供中文深色工作台、素材搜索、LUT 资料库、键盘快捷键与引擎诊断。`⌘/Ctrl + O` 导入视频，`⌘/Ctrl + Enter` 开始导出，`⌘/Ctrl + A` 全选，`⌘/Ctrl + Z` 撤销。素材与队列超过 80 项时按窗口渲染。

## 处理能力

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

## 本地开发

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

## 独立 release 应用

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

## 验证

```bash
pnpm test
pnpm test:release
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

Rust 中的真实视频测试使用可发现的 FFmpeg 生成小型测试素材，覆盖强度像素、1D CUBE、特殊字符路径、多音轨、并行处理、输出不覆盖、硬件编码回退、预览缓存与子进程取消。本地没有可发现的 FFmpeg 时，相关测试会提前返回；发行 CI 通过 `LUTLAB_REQUIRE_MEDIA_TESTS=1` 将缺少引擎变为失败；完整验收应使用系统引擎或设置 `FFMPEG_PATH` / `FFPROBE_PATH` 指向已下载的内置引擎，不能把提前返回算作媒体验收。

手动验收应包含：导入不同分辨率的两个视频与一个 CUBE → 调节强度并对比任意帧 → 应用全部 → 并行导出 → 使用 ffprobe 检查成片时长、尺寸和音轨 → 再次导出确认重名安全 → 取消长任务 → 重试 → 切换 FFmpeg 路径并重新预览。

## 代码结构

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
