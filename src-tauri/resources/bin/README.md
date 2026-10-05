# Bundled media engine

The Full application includes **ffmpeg + ffprobe**. Frame preview does not use
ffplay. Downloaded executables are ignored by Git; their pinned archive URLs and
SHA256 values are tracked in `../ffmpeg-manifest.json`.

```text
macos/aarch64/{ffmpeg,ffprobe}
macos/x86_64/{ffmpeg,ffprobe}
windows/x86_64/{ffmpeg.exe,ffprobe.exe}
```

`pnpm fetch:ffmpeg` fetches the audited macOS arm64 9.0.2 build. That build requires
macOS 12+. Other architectures must be supplied separately. A vendor directory
passed with `FFMPEG_VENDOR_DIR` must have the same layout shown above.

`pnpm prepare:ffmpeg --target aarch64-apple-darwin` checks executable architecture,
external dependencies, required codecs/filters, actual encode/probe and JPEG
preview. Full builds run this check automatically; it cannot be skipped. Native
builds require only their own architecture; Universal builds require both macOS
directories. Only the host architecture receives executable smoke tests.

Homebrew binaries with external dylib dependencies are rejected. Copying a
dynamic FFmpeg executable here does not make an application standalone.

See `../licenses/FFMPEG-SOURCE.md` for version/license records and corresponding
source requirements before public distribution. Lite builds still require a
separately installed engine.
