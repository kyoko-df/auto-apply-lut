# Photo pipeline dependencies

Versions below are resolved in `src-tauri/Cargo.lock`; registry checksums are
recorded there. License files were copied without changes from the corresponding
downloaded crates. Little CMS is built statically (`lcms2` feature `static`),
so photo processing does not require a separately installed color engine.

| Dependency | Resolved version | Upstream | License / included notice |
| --- | --- | --- | --- |
| image | 0.25.10 | https://github.com/image-rs/image | MIT or Apache-2.0; `image-0.25.10-LICENSE-*` |
| png | 0.18.1 | https://github.com/image-rs/image-png | MIT or Apache-2.0; `png-0.18.1-LICENSE-*` |
| tiff | 0.11.3 | https://github.com/image-rs/image-tiff | MIT; `tiff-0.11.3-LICENSE.txt` |
| lcms2 | 6.2.0 | https://github.com/kornelski/rust-lcms2 | MIT; `lcms2-6.2.0-LICENSE.txt` |
| lcms2-sys | 4.0.7 | https://github.com/kornelski/rust-lcms2-sys | MIT declared by package; its vendored Little CMS notice is `lcms2-sys-4.0.7-LICENSE.txt` |
| Little CMS | 2.19 (vendored by lcms2-sys) | https://www.littlecms.com/ | MIT; vendored notice above |
| little_exif | 0.6.23 | https://github.com/TechnikTobi/little_exif | MIT or Apache-2.0; `little_exif-0.6.23-LICENSE-*` |
| crc32fast | 1.5.0 | https://github.com/srijs/rust-crc32fast | MIT or Apache-2.0; `crc32fast-1.5.0-LICENSE-*` |

The lcms2-sys registry archive does not contain the `COPYING` file listed in its
package manifest. Before public distribution, resolve that binding notice and
assemble the complete applicable transitive dependency notices. This record
does not replace the existing FFmpeg GPL source-delivery requirements, and does
not establish public distribution readiness.

## LibRaw input backend

LibRaw 0.21.4 is linked from an external precompiled static library. macOS (ARM64/Intel, minimum macOS 15) and Windows x64 use official distributions pinned by URL and SHA256 in `resources/libraw-manifest.json`. Only the matching headers and static library are extracted to a Git-ignored cache; app builds compile `raw_bridge.cpp` and link that library, never the upstream implementation. The official Mac package uses the system zlib; no LibRaw dylib/DLL or PATH-based RAW decoder is required.

Linux uses separately built packages from unchanged upstream source, with the recipe in `scripts/build-libraw.mjs`. The resulting precompiled package can be cached or hosted separately for reuse. Normal app builds only consume it. Audited external packages can also be supplied using `LIBRAW_VENDOR_URL` and `LIBRAW_VENDOR_SHA256` or `prepare:libraw --archive ... --sha256 ...`.

LUTlab selects **CDDL-1.0**. The license and copyright/third-party notices remain in `libraw-0.21.4-LICENSE-CDDL.txt` and `libraw-0.21.4-COPYRIGHT.txt`. `LibRaw-SOURCE.txt` provides exact download locations/checksums for matching sources (included in the official distributions), rather than embedding a source archive in every installer. No LibRaw implementation source is modified. The existing FFmpeg GPL source-delivery and other dependency-notice requirements above still apply. Local packaging alone does not establish public distribution readiness.
