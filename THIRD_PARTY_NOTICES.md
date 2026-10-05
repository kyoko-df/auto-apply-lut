# Third-Party Notices

## Bundled FFmpeg engine

The self-contained macOS Apple Silicon build of LUTlab includes the `ffmpeg`
and `ffprobe` command-line executables from the FFmpeg project. LUTlab invokes
them as separate processes for video processing, frame previews and metadata.
`ffplay` is not included or required by the current workspace.

| Item | Bundled build |
| --- | --- |
| Project | [FFmpeg](https://ffmpeg.org/) |
| Copyright | FFmpeg: (c) 2000–2026; FFprobe: (c) 2007–2026; the FFmpeg developers |
| Binary provider | [Martin Riedl FFmpeg builds](https://ffmpeg.martin-riedl.de/) |
| Platform | macOS / arm64 (Apple Silicon) |
| Version | `9.0.2-https://www.martin-riedl.de` |
| Provider build identifier | `1789931890_9.0.2` |
| FFmpeg license | GNU General Public License, version 3 or later (`GPL-3.0-or-later`) |
| License configuration | `--enable-gpl --enable-version3`; no `--enable-nonfree` in the recorded configuration |

This is a GPL-enabled build, including x264 and x265. It must not be described
as an LGPL-only FFmpeg distribution. The complete configure line, FFmpeg library
versions and provider-reported dependency versions are preserved in the
[original build manifest](src-tauri/resources/licenses/ffmpeg-macos-arm64-1789931890_9.0.2-versions.txt).
The imported executables' own `-version` and `-L` output is preserved in the
[runtime verification record](src-tauri/resources/licenses/ffmpeg-macos-arm64-1789931890_9.0.2-runtime.txt).
The manifest describes the provider's whole build; its SDL entry does not mean
that LUTlab ships `ffplay`.

The full license text is included as
[GPL-3.0.txt](src-tauri/resources/licenses/GPL-3.0.txt).
FFmpeg is distributed without warranty; see the license for its terms.
This software is based in part on the work of the Independent JPEG Group.

## Source and build provenance

See [FFMPEG-SOURCE.md](src-tauri/resources/licenses/FFMPEG-SOURCE.md) for the exact
download locations, source references, recorded evidence and outstanding source
delivery work. FFmpeg's [license documentation](https://ffmpeg.org/doxygen/trunk/md_LICENSE.html)
explains how its optional components and configure switches affect licensing.

The checked-in license and manifest are notices and provenance records. They
are **not a complete Corresponding Source archive or a written source offer**.
This local application package has not been prepared for public distribution.
Before distributing it to others, assemble and provide the matching FFmpeg and
statically linked dependency sources, applicable notices, patches, and build
scripts under the relevant licenses. A link to the FFmpeg homepage or an
unpinned build-script branch does not establish that correspondence.

These engine notices do not replace the license terms of LUTlab's other Rust,
JavaScript or operating-system components. A different engine build or platform
requires its own verified manifest, license assessment and source records.
