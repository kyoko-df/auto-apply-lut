# Third-Party Notices

This project can optionally bundle FFmpeg binaries in the **Full** distribution.
You are responsible for ensuring license compliance for the specific FFmpeg builds you ship.

## FFmpeg

- Project: [FFmpeg](https://ffmpeg.org/)
- License: LGPL or GPL depending on build configuration and enabled components.
- Bundled in: Full builds only (see `src-tauri/tauri.full.conf.json`)

### What to record for your release

Fill in (or attach) the following for each platform/arch you distribute:
- FFmpeg version (first line of `ffmpeg -version`)
- Build configuration flags (the `configuration:` line from `ffmpeg -version`)
- Source code offer / source URL matching the exact build
- License text(s) shipped with your app (LGPL/GPL as applicable)

### Runtime usage

The app uses:
- `ffmpeg` for processing/export
- `ffprobe` for metadata probing
- `ffplay` for preview playback

