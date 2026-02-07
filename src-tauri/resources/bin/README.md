# Bundled FFmpeg binaries (Full build only)

This directory is packaged into the **Full** app bundle via `bundle.resources` in
`src-tauri/tauri.full.conf.json`.

Expected layout:

```
src-tauri/resources/bin/
  windows/x86_64/
    ffmpeg.exe
    ffprobe.exe
    ffplay.exe
  macos/aarch64/
    ffmpeg
    ffprobe
    ffplay
  macos/x86_64/
    ffmpeg
    ffprobe
    ffplay
```

Notes:
- macOS binaries must be executable (`chmod +x`).
- Lite builds do not include these files and will fall back to system-installed FFmpeg.

