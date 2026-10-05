# LUTlab contributor guide

## Product and scope

LUTlab is a local-first Tauri desktop app for applying LUTs to batches of videos. Keep the Chinese dark workspace focused on importing clips, choosing a look, comparing actual frames, and exporting reliably. User instructions take precedence. Historical files under `docs/superpowers/` are context, not current implementation requirements; do not restore the removed legacy UI or follow old CLAUDE workflows by default.

Current limits must remain explicit: previews are seekable still frames, not real-time graded playback; automatic camera Log detection and HDR/Dolby Vision mastering are not implemented. Explicit Rec.2020 PQ/HLG to Rec.709 SDR conversion is supported; automatic input mode performs no conversion. Browser development mode is UI-only and must never claim that a real export succeeded. Do not fabricate sample jobs, timings, hardware availability or LUT thumbnails.

## Architecture

- `src/App.tsx`: workspace composition, media/library navigation, queue, dialogs, keyboard shortcuts and native drag/drop.
- `src/components/ExportInspector.tsx`: per-clip LUT controls and export settings; only offer meaningful, compatible codec/container combinations.
- `src/components/FramePreview.tsx`: debounced requests, unique preview client per component instance, stale response guards, comparison wipe and frame seeking.
- `src/workspace/types.ts`: typed IPC and UI contracts. Rust request keys are snake_case inside `request`; top-level Tauri arguments use camelCase (for example `batchId`).
- `src/workspace/model.ts`: pure setting migration, clip state transitions, request construction and batch reconciliation.
- `src/workspace/useWorkspace.ts`: imports, bounded metadata work, serialized settings persistence, immutable export snapshots, atomic workspace snapshots, multi-selection/undo, non-overlapping polling, cancellation and retry.
- `src-tauri/src/commands/batch_manager.rs`: validation, unique destination allocation, concurrency scheduling and batch/item terminal states.
- `src-tauri/src/commands/preview.rs`: cancellable FFmpeg frame service, 2-render limit; 32-entry / 32 MiB display LRU, 8-entry / 256 MiB RGB16 FFV1 frame LRU and 8-entry / 128 MiB prepared-LUT LRU. Cache files have RAII lifetime and bounded leases.
- `src/components/WorkspaceStatus.tsx`: startup recovery, persistent save failures and native close coordination.
- `src/components/WindowedList.tsx`: fixed-height window rendering above 80 rows. Keep virtual row geometry consistent with CSS.
- `src-tauri/src/commands/workspace.rs` / `startup.rs`: bounded atomic snapshots, export exit guard and explicit storage recovery.
- `src-tauri/src/core/ffmpeg/color.rs`: shared explicit input interpretation and HDR-to-SDR graph, applied before LUT.
- `src-tauri/src/core/ffmpeg/lut.rs`: shared LUT preparation and filter graph generation. Preview and export must use this implementation.
- `src-tauri/src/core/ffmpeg/processor.rs`: actual encoding, audio mapping, progress, hardware fallback, cancellation and no-clobber publishing.
- `src-tauri/src/core/video/mod.rs`: FFprobe metadata and timeout handling.
- `src-tauri/src/core/ffmpeg/mod.rs`: shared FFmpeg/ffprobe pair discovery. The app bundle takes precedence over environment/PATH; explicit app configuration takes precedence at call sites.
- `src-tauri/src/utils/config.rs`: recoverable config loading and atomic persistence.
- `src-tauri/src/database/`: persisted LUT library and task snapshots. Workspace snapshots restore the UI queue; interrupted tasks become retryable. Exports restart from the beginning, not at the encoded frame.
- Register new commands in `src-tauri/src/commands/mod.rs` and `src-tauri/src/lib.rs`; register managed state in the Tauri builder.

Do not mix encoding logic into React, duplicate IPC interfaces in UI components, or introduce a second independent LUT intensity implementation. Use ordinary React state and the existing workspace hook unless a concrete need justifies another dependency.

## Processing invariants

1. **Protect user media.** Never overwrite input or an existing output. Allocate collision-free names, write on the destination volume to a temporary file, then publish with `persist_noclobber` after successful FFmpeg completion. Failure and cancellation remove incomplete output only.
2. **Keep cancellation truthful.** Lock export startup synchronously. Cancellation before a task/process ID exists must still reach the eventual job. Kill and reap FFmpeg. Report a batch as terminal only after every running item ends. Ignore stale cancellation responses from prior batches.
3. **Keep settings consistent.** Export a captured per-batch snapshot, await pending settings writes, and prohibit conflicting mutations while exporting. Polls must never overlap. A transient IPC failure does not mean a running batch stopped.
4. **Use the current executable configuration.** Honor the current FFmpeg configuration on each operation. Reuse validated engine pairs until environment/file version/explicit config changes; explicit diagnostics invalidate discovery. Do not invalidate discovery for unrelated settings edits. Diagnostics verify FFmpeg and ffprobe without holding config locks while waiting for child processes. Probe commands require bounded timeouts and `kill_on_drop`.
5. **Pass arguments safely.** Use `tokio::process::Command`, never shell interpolation. LUT filter values have a separate FFmpeg escaping grammar; use the shared helper. Support spaces, Unicode, quotes and punctuation in paths.
6. **Be honest about color.** UI intensity is 0–100; Rust intensity is 0–1. Validate finite ranges. Blend in the shared 16-bit RGB graph. Metadata tags are not color transforms. Auto mode preserves source interpretation. Manual PQ/HLG conversion uses linear float tone mapping before the creative LUT and labels the resulting output Rec.709. Unknown input must not be guessed; input conversion is batch-wide.
7. **Keep preview work bounded.** Debounce UI requests; cancel superseded work; discard late responses; release per-client state on all exits. Register cancellation before any discovery await. Cache keys include file identity/version, timestamp, LUT, strength, quality, color conversion and preview size. Full-result cache hits bypass render slots. Fast mode scales before LUT; accurate mode applies LUT before scaling. Never load whole videos into JavaScript memory.
8. **Hardware is optional.** Fall back only for hardware initialization/capability failures; disk, invalid media, filter and audio errors must fail directly. Cache permanently unavailable encoders by executable version and encoder, not all hardware failures. Do not claim that LUT filtering itself runs on the GPU. Bound both batch concurrency and per-process threads.
9. **Respect output constraints.** H.264 uses 8-bit 4:2:0, HEVC offers 8/10-bit 4:2:0, ProRes HQ uses 10-bit 4:2:2. ProRes quality is fixed, so its irrelevant quality control is disabled. Preserve all audio streams; stream-copy still depends on container compatibility.
10. **Recover without data loss.** Preserve the original bytes of corrupt settings before creating defaults. Save atomically and roll back in-memory settings if persistence fails. Never delete an unreadable config to make startup succeed. Settings rollback must preserve subsequent queued edits; failed settings never invalidate completed exports. An unreadable workspace blocks autosave until recovery. Both window-close and OS quit must flush the snapshot; backend task lifetimes protect against an early frontend guard release.

## UI rules

Use `src/index.css` and `src/App.css` for the graphite/orange visual system. Preserve a single primary action, legible density, keyboard focus rings, labeled controls, independent scrolling regions and reduced-motion support. Main layout targets desktop windows of at least 800 × 680. The export action must stay reachable at shorter heights. Every visible control must execute a real action or explain why it is disabled.

Keep media display based on rendered image dimensions, including rotated and non-square-pixel inputs. Do not let a previous clip frame appear labeled as a new clip. Per-instance preview client IDs prevent an old unmount cancellation from terminating a new instance's request.

## Commands and verification

```bash
pnpm install
pnpm dev                       # Browser UI only
pnpm tauri dev                 # Desktop app
pnpm test                      # React integration + workspace state tests
pnpm test:release              # Binary architecture/dependency checks + pinned download checks
pnpm build                     # Strict TypeScript + production frontend
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
pnpm tauri build --debug --bundles app  # macOS local app bundle
pnpm release:mac:arm64          # Pinned FFmpeg download + standalone optimized macOS release
pnpm benchmark:media           # Repeatable 4K benchmark through production VideoProcessor
```

Use installed FFmpeg and ffprobe for meaningful media tests. Local media tests can return early if FFmpeg is unavailable; do not call that end-to-end verification. Release CI sets LUTLAB_REQUIRE_MEDIA_TESTS=1 and fails without real engines. Tests must exercise behavior and failure paths, not mirror CSS strings or internal implementation shapes. Do not broaden tests for purely cosmetic changes without a reason.

For processing changes, cover actual generated video with fractional LUT strength, audio preservation, concurrent jobs, cancellation and no-overwrite behavior. For preview changes, cover stale results, bounds, cache invalidation and process cleanup. For queue changes, cover double-click startup, early cancellation, failed starts, transient polling errors and late responses from old batches.

Before delivery, run appropriate tests, inspect the real desktop UI when possible, and report the exact limits of validation. A useful smoke test imports two generated videos of different sizes, selects a CUBE, applies 50% strength to both, exports concurrently and checks the outputs with ffprobe. Place generated media outside the source tree. Never commit `dist/`, Cargo `target/`, caches, logs or test exports.

## Standalone distribution

- Full releases require both ffmpeg and ffprobe; ffplay is not required. Never describe a Lite bundle or copied Homebrew executable as standalone.
- `scripts/fetch-ffmpeg.mjs` downloads only pinned URLs from `src-tauri/resources/ffmpeg-manifest.json`, verifies SHA256 and static dependency metadata before installing. The current pinned provider build is macOS arm64 FFmpeg 9.0.2 and requires macOS 12+. Keep the app minimum version compatible with the actual Mach-O engine deployment target.
- `scripts/prepare-ffmpeg.mjs` validates the selected target before bundling. Do not bypass it. Native builds need one architecture; universal builds need both. Non-native structural checks do not count as runtime validation.
- `scripts/validate-bundled-binary.mjs` parses Mach-O/PE and rejects non-system dynamic libraries. Preserve this safeguard when changing binary suppliers.
- `scripts/verify-macos-release.mjs <app-path>` checks the packaged main executable and engine, then encodes real samples with a system-only PATH and verifies the application signature. Validate a relocated .app as well as development binaries. macOS engines are at `Contents/Resources/bin/macos/<arch>`.
- Include licenses and provenance records under `src-tauri/resources/licenses/`. Public distribution also requires complete matching source delivery for the GPL engine and static dependencies; the current notices do not constitute a source archive or written offer. Do not claim public distribution readiness from a successful local build alone.
- Current macOS signing is ad-hoc, not Developer ID/notarized. Intel, Windows and Linux require their own native verification. Windows Full uses an offline WebView2 installer to avoid requiring users to fetch it separately.
- macOS CI verifies both the built and relocated app before uploading to a draft. The webview uses an explicit CSP; keep unused file mutation/process commands unregistered and logs confined to their designated directory.
- The release workflow creates drafts. Preserve the review point before public publication. Downloaded binaries remain Git-ignored and are reproduced from the manifest or an explicitly audited vendor archive.
