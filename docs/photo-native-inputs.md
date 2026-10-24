# HIF / Sony ARW photo inputs

## Processing contract

Photo mode accepts `.hif`, `.heif`, `.heic` and `.arw` in the picker, drag/drop and folder scan. macOS, Windows and Linux share the same Rust/C++ color pipeline. Only Apple Silicon macOS has been executed locally; platform support in source and CI is not evidence of successful Windows/Linux execution.

- **SDR HEIF:** the explicitly configured FFmpeg and its sibling ffprobe must both be version 9 or later, with HEVC, stream-group probing and xstack/crop support. The backend matches the container's `pitm` primary ID, validates every tile and complete nonoverlapping grid coverage, assembles the coded image and crops to its full active dimensions. It never falls back to stream 0, one tile or a thumbnail. Version alone does not guarantee compatibility: Full packaging runs an actual self-generated RGB16 HEIF-grid decode before accepting the engine. YCbCr matrix and range must be declared and supported. HEVC 8/10/12-bit input is decoded to RGB16; alpha/auxiliary primary images and unsupported codecs fail explicitly.
- **Sony ARW:** externally prepared, statically linked **LibRaw 0.21.4** opens the full sensor RAW, unpacks, develops and returns full RGB16. No thumbnail API or FFmpeg ARW fallback is used. Only single-frame CFA Sony RAW with square pixels is accepted. Standard sensor active-image crop is applied with LibRaw's own crop API; optional user/aspect crops are not substituted. The default recipe uses full-resolution AHD, as-shot white balance, no automatic brightness, exposure/brightness 1, clipped highlights and linear sRGB primaries. Per-photo RAW development controls add pre-demosaic exposure from −2 to +3 EV (`exp_correc` / `exp_shift = 2^EV`, no highlight preservation), plus camera/daylight/automatic white balance. Zero EV and camera WB preserve the existing default recipe. LittleCMS applies the sRGB transfer curve exactly once. RAW cannot be assigned an RGB profile. The UI reports 16-bit developed precision, not nominal sensor precision. No camera-JPEG rendering match is promised. Parameters are validated before discovery/decode, persisted in the existing source-interpretation contract, included in both normalized/display cache keys and captured in immutable batch snapshots. Input copying never applies RAW controls to an RGB photo or an RGB profile assignment to an ARW.
- **Color:** bounded primary-item ICC/nclx parsing follows only the full image's `dimg` references, excluding thumbnails. Valid RGB ICC or supported nclx primaries/transfer are transformed through LittleCMS into SDR sRGB. Recognized sRGB is passed through without an identity round trip; ICC description strings alone do not qualify. Unknown/invalid profiles require explicit sRGB, Adobe RGB or Display P3 assignment. Assignment describes the actual input and is not a substitute for conversion.
- **HDR:** HLG/PQ declarations in primary/tile nclx, ICC cICP or HEVC VUI are rejected, including with manual sRGB assignment. The photo pipeline has no verified HDR-to-SDR tone mapper. Convert HDR explicitly first. S-Log3 and video HDR handling remain in the separate video workflow.
- **Direction:** decode unrotated pixels, validate full stored dimensions, then apply the original orientation exactly once in Rust. All eight HEIF EXIF transforms are tested. Output has corrected dimensions and Orientation=1.
- **LUT and output:** normalized RGB16 enters the existing FFmpeg photo LUT/strength graph. JPEG/PNG/TIFF embeds sRGB ICC. Original paths and existing outputs are never overwritten; temporary outputs are validated before no-clobber publication. Preview cache keys include the selected engine identity and normalization version.
- **Metadata:** HEIF primary Exif uses the existing photo allowlist (camera/date/lens/exposure/artist/copyright; supported GPS only when requested). RAW retains make/model/lens/ISO/exposure/aperture/focal length and the timezone-less capture clock. RAW GPS, UTC offsets, MakerNote, embedded previews, complete XMP and unlisted tags are not copied.

File size is limited to 512 MiB; active images to 64 MP and 32768 pixels per side; metadata to 16 MiB and ICC to 4 MiB. Existing shared memory/concurrency budgets apply. FFmpeg children have bounded output and timeouts, and cancellation kills and reaps them. LibRaw cancellation is cooperative at progress callbacks and may wait for a decoding stage; cancellation prevents publication. Full development can be slow in a debug build.

## Dependencies and distribution

The application now requires **macOS 15+**. LibRaw 0.21.4 on macOS ARM64/Intel and Windows x64 comes from official precompiled packages. URLs and SHA256 are fixed in `src-tauri/resources/libraw-manifest.json`. The Mac objects actually declare macOS 15 even though older website descriptions mention an earlier OS. `prepare:libraw` validates checksums, target architecture, every archive member, header version and deployment target. It extracts only the static library, matching headers, provenance and notices into `src-tauri/.cache/libraw/` (Git-ignored). No implementation source is committed or compiled by application builds. macOS links only the system zlib in addition to the library/C++ runtime.

`pnpm prepare:libraw` runs before Tauri development/build; direct Cargo invocations verify the prepared cache offline and give preparation instructions if missing or corrupt. The cache retains a checked package plus per-file digests, so offline builds do not download anything and corrupt dependencies fail explicitly. Both Full and Lite statically link LibRaw and include its CDDL/copyright notices plus `LibRaw-SOURCE.txt` and the dependency manifest. Matching sources are available separately from the exact upstream archives listed in that notice. Existing FFmpeg GPL source delivery and other transitive-notice requirements remain separate.

Linux has no pinned official precompiled package. `pnpm build:libraw --target x86_64-unknown-linux-gnu` is an independent maintainer dependency build: it downloads the pinned source archive into an ignored cache, builds in a temporary directory, emits a checked precompiled package, then removes extracted sources. Normal application builds never invoke this compiler. The resulting binary package stays in the ignored cache and can be hosted separately for reuse. To consume a hosted package directly, set `LIBRAW_VENDOR_URL` / `LIBRAW_VENDOR_SHA256`, or run `pnpm prepare:libraw --target ... --archive package.tar.gz --sha256 ...`. CMake/native C++ tools are needed only for this Linux dependency build; ordinary app builds still compile the project-owned bridge. Linux uses normal Tauri/WebKit, glibc and C++ runtime.

The existing release workflow prepares the official Mac/Windows libraries before testing and packaging. Linux native development retains the separate LibRaw preparation path and uses an externally configured FFmpeg/ffprobe pair; Linux Full packaging is outside this change. Mac Intel, Windows and Linux need their own native runtime verification. No unsupported official library is silently accepted and no system LibRaw fallback is used.

## Repeatable verification

Use the same selected engine pair as the application. For this checkout on Apple Silicon:

```sh
export PATH="$PWD/src-tauri/resources/bin/macos/aarch64:$PATH"
pnpm prepare:libraw
pnpm test
pnpm build
pnpm test:release
pnpm prepare:ffmpeg --target aarch64-apple-darwin
LUTLAB_REQUIRE_MEDIA_TESTS=1 cargo test --offline --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1
cargo check --offline --manifest-path src-tauri/Cargo.toml
```

Normal Rust tests include actual project-owned HEIF grid decoding; real embedded Adobe RGB/Display P3 ICC conversion against LittleCMS reference values; invalid ICC rejection/assignment; numerical sRGB identity and linear-to-sRGB transfer; and primary/thumbnail, tile coverage, HDR, cancellation/timeout/reaping checks. The process-lifecycle test uses a deliberately slow simulated child, not a simulated camera decoder. Frontend tests use mocked IPC and do not prove native image decoding. ELF tests use constructed ELF bytes and do not prove Linux runtime support. Run the full Rust suite serially: the pre-existing exit-guard test assumes no other process-global application operation is active and can race other actual preview tests in a parallel run.

Generate the optional eight orientation fixtures from synthetic pixels (Xcode tools are needed for this test generator only; ImageIO is not the photo input backend):

```sh
xcrun swift scripts/generate-heif-fixtures.swift /tmp/lutlab-format-fixtures
LUTLAB_PHOTO_FIXTURES=/tmp/lutlab-format-fixtures cargo test --offline --manifest-path src-tauri/Cargo.toml --lib generated_heif -- --ignored --nocapture
```

The synthetic encoder emits unspecified nclx. Tests verify automatic refusal, explicit sRGB assignment, full dimensions, pixel ordering and every orientation, plus actual photo preview/region geometry. They do not substitute for Sony-camera tests.

Download public camera samples outside the checkout using these names; do not commit or redistribute them:

| Local name | Source | Expected input |
| --- | --- | --- |
| `a7c2.arw` | [Photography Blog A7C II ARW](https://img.photographyblog.com/reviews/sony_a7c_ii/sample_images/sony_a7c_ii_01.arw) | Full active 7008 × 4672 RAW; embedded preview is 1616 × 1080 |
| `a7c2-hlg.heif` | [A7C II HLG sample](https://github.com/dmitry907/hlg-to-sdr-hdr/blob/a6af542e8320f629e1d45fbe6b397e284ad52d31/samples/DSC03288.heif) | HLG, orientation 8; must be refused |
| `20241120-214155-cam0-000-000004-0.HIF` | [libheif #1406 attachment](https://github.com/strukturag/libheif/issues/1406) | Sony ILX-LR1 SDR, full 9504 × 6336, 10-bit |

```sh
LUTLAB_PHOTO_FIXTURES=/tmp/lutlab-format-fixtures LUTLAB_REQUIRE_MEDIA_TESTS=1 \
  cargo test --offline --manifest-path src-tauri/Cargo.toml --lib actual_sony_heif_arw_and_hdr_boundary \
  -- --ignored --nocapture --test-threads=1
LUTLAB_PHOTO_FIXTURES=/tmp/lutlab-format-fixtures TZ=America/Los_Angeles \
  cargo test --offline --manifest-path src-tauri/Cargo.toml --lib actual_sony_primary_color_metadata -- --ignored --nocapture
```

ARW can also be verified independently with only `a7c2.arw` in the fixture directory:

```sh
LUTLAB_PHOTO_FIXTURES=/tmp/lutlab-format-fixtures LUTLAB_REQUIRE_MEDIA_TESTS=1 TZ=America/Los_Angeles \
  cargo test --offline --manifest-path src-tauri/Cargo.toml --lib actual_sony_arw -- --ignored --nocapture --test-threads=1
```

The ARW-only tests verify the exact public sample SHA256, full 7008 × 4672 RGB16 development, numerical 25% LUT PNG16 export, sRGB ICC/capture metadata, unchanged source and existing-output hashes, and the production preview command's fast/accurate fit views, cached display and 100% region beyond the embedded thumbnail's bounds. Preview assertions compare decoded pixels because separately created ICC profiles can have different creation timestamps. They use the default application memory budget. Samples and generated outputs remain outside the repository; missing samples are failures when these tests are explicitly invoked.

These tests fail if camera fixtures or the compatible engine are absent. The combined pipeline test verifies full dimensions, non-8-bit precision, actual 25% LUT pixels against an analytic reference, RGB16 PNG/sRGB ICC/Orientation=1, capture metadata, unchanged source identity, collision refusal and HLG refusal with an sRGB assignment. Six photo fixture tests and the existing manual 4K benchmark are ignored in the ordinary suite and must be reported separately.

## LUT thumbnails, RAW controls and Sony Log verification (2026-10-24)

Added actual current-media LUT library thumbnails (visible rows only, one background render, independent cancellation IDs, shared native preview caches). Photos require explicit sRGB library-preview consent, separate from export LUT confirmation. ARW exposure and WB are carried in `SourceInterpretation::Raw`, so main previews, thumbnails, full-size exports and workspace recovery use the same settings. Parameters never cross between ARW development and RGB profile assignment when copying input settings.

Sony's standard S-Log3 / S-Gamut3.Cine and S-Log3 / S-Gamut3 video conversions are manual and batch-wide. The confirmation states full-range native codes and BT.709 YCbCr matrix. Source interpretation happens before working-frame caching; auto and Sony full-range frames have distinct cache identities. Float linear-light decoding and gamut conversion retain highlights until fixed Mobius SDR mapping, then apply the Rec.709 transfer curve and the shared creative LUT. Out-of-gamut values clip. Limited-range transcodes, VENICE-specific primaries, automatic Log detection and HDR mastering remain outside this implementation. See the [Sony technical definition](https://www.sony.jp/ls-camera/knowledge/pdf/TechnicalSummary_for_S-Gamut3Cine_S-Gamut3_S-Log3_V1_00.pdf) and [LibRaw processing parameter documentation](https://www.libraw.org/docs/API-datastruct.html).

Executed on Apple Silicon with the bundled FFmpeg/ffprobe 9.0.2 and verified static LibRaw 0.21.4:

- Frontend: 94 tests passed; production TypeScript/Vite build passed. Includes Log confirmation and keyboard guard, RAW export snapshots and locking, snapshot recovery, thumbnail visibility/serialization/cancellation/stale responses and photo consent.
- Rust required-media suite, serial: 222 passed, 9 ignored. Includes independent Sony black/18%-gray/color-patch/highlight pixel references, special-character LUT paths, source-range cache separation, preview/export ordering and HEVC 10-bit SDR tags/audio, plus existing fractional LUT, concurrency, cancellation and no-overwrite regressions. Log media here is generated, not real camera footage.
- Explicitly executed `actual_sony_arw_development_controls` and `actual_sony_arw_preview_development_cache` using the exact A7C II ARW SHA256 recorded below. Full 7008 × 4672 decoding, −1 EV linear-light ratio, camera/daylight/automatic WB differences, full PNG16 export matching the adjusted decode, unchanged source hash, 100% region preview and cache invalidation/hits passed. The other seven ignored tests were not rerun in this change.
- Release-tool suite: 19 passed. Offline Cargo check, Rust formatting and Git whitespace checks passed. Full macOS debug packaging and both original/relocated app engine, dependency and ad-hoc-signature verification passed. These package checks do not exercise desktop IPC.
- Native desktop inspection used a relocated debug app and an isolated data directory after the Mac was unlocked. Imported three actual CUBE files and two generated videos (720 × 480 and 480 × 800, Unicode/spaces/apostrophe paths), checked source-specific LUT thumbnails and portrait comparison geometry, applied the same 50% inverse LUT and exported both concurrently through VideoToolbox. Both MP4s retained their dimensions, four-second duration and two audio streams; decoded first-frame RGB values were 128–129, matching the analytic half-inverse result. Re-export allocated a `(2)` filename and left both previous output hashes unchanged.
- Desktop Sony Log checks covered both gamut confirmation messages, cancellation preserving the prior selection, the conversion preview and an actual three-second synthetic S-Log3 export with audio and Rec.709 matrix/transfer/primaries. The confirmation shortcut did not start a batch. Real-camera ARW preview, −1.1 EV/daylight controls, all three real photo LUT thumbnails, explicit photo-library thumbnail consent and the separate per-LUT sRGB export confirmation were exercised. The adjusted RAW with 50% Warm LUT exported through desktop IPC to a full 7008 × 4672, 16-bit RGB PNG with ICC and Exif; original video/ARW source hashes were unchanged. Export locked the RAW controls. Normal quit/restart restored exposure, WB, LUT, strength, consent and completed output, and rendered the adjusted comparison again.
- A long-action-label layout issue in the RAW inspector was corrected by stacking the input actions. The latest build was checked in the desktop app at approximately 800 × 680: action labels remained separate and the export button stayed visible. Production frontend build and original/relocated package verification passed again after this layout correction.
- Windows/Linux/Intel runtime, real Sony Log camera footage and optimized-release performance remain unverified. Camera samples, generated media, logs and application bundles remain outside tracked source.

To repeat the new real-camera checks with the documented fixture directory:

```sh
LUTLAB_PHOTO_FIXTURES=/tmp/lutlab-format-fixtures LUTLAB_REQUIRE_MEDIA_TESTS=1 \
  cargo test --offline --manifest-path src-tauri/Cargo.toml --lib actual_sony_arw_development_controls \
  -- --ignored --nocapture --test-threads=1
LUTLAB_PHOTO_FIXTURES=/tmp/lutlab-format-fixtures LUTLAB_REQUIRE_MEDIA_TESTS=1 \
  cargo test --offline --manifest-path src-tauri/Cargo.toml --lib actual_sony_arw_preview_development_cache \
  -- --ignored --nocapture --test-threads=1
```

## ARW merge verification (2026-10-17)

Explicitly executed both ARW-only ignored tests outside the ordinary suite on Apple Silicon, with official static LibRaw 0.21.4, bundled FFmpeg 9.0.2 and `TZ=America/Los_Angeles`. Both passed using the exact 70,238,208-byte public A7C II ARW whose SHA256 is recorded below. Full 7008 × 4672 RGB16 development, numerical 25% LUT PNG16 export, sRGB ICC, capture metadata, collision refusal and source/existing-output SHA256 protection passed. Fast/accurate fit previews, display-cache hits with both render slots occupied and a 96 × 64 region at sensor coordinates (4000, 2000) passed through the production preview command. The corrected preview test took 70.56 seconds in this debug build; this is one test run, not a performance benchmark.

A freshly built Full debug application was copied to a temporary directory, given its own bundle identifier and `LUTLAB_DATA_DIR`, and exercised through the native macOS UI. Import, actual-frame comparison in fit and 100% region views, sRGB LUT confirmation and 25% PNG16 export completed. Independent ffprobe and PNG/EXIF inspection verified the published 7008 × 4672 `rgb48be` output, embedded RGB ICC, Orientation=1, SONY make, original capture clock and absent GPS. The source SHA256 was unchanged. No camera sample or test export was added to Git.

The ordinary required-media Rust suite passed 219 tests with 7 ignored; the two ARW-only tests above were explicitly executed separately. Offline Cargo check, Rust formatting and whitespace checks for the current edits passed. Full debug packaging included the real bundled-engine checks. No additional ARW processing defect was found; the single-tile HEIF fix has a non-ignored real decode/export regression test. Other ARW cameras/compression variants, Windows/Linux/Intel Mac runtime, macOS 15 hardware, optimized release performance and the remaining five ignored tests were not revalidated in this run.

## Scope cleanup verification (2026-10-11)

Removed the additional Linux Full packaging/ELF tooling and duplicate portable CI. Linux native LibRaw preparation remains available. Removed unused RAW preview dimensions from both sides of the C ABI, the unread manifest target list and the duplicate preview engine fingerprint. Full packages inherit macOS 15 from the base configuration. Only this documentation file is exempted from the docs ignore rule, so README links survive a clean checkout.

Validation: 19 release-tool tests, 85 frontend tests and the production frontend build passed; 218 Rust library tests passed (5 ignored). Both ignored Sony sample tests were run separately and passed (242.97 s), including full ARW/HIF RGB16 LUT/export, metadata and source/output protection. Offline Cargo check and formatting passed. Full debug Mac packaging and both built/relocated bundle verifications passed; packaged minimum macOS remains 15.0. Windows/Linux runtime and macOS 15 hardware were not revalidated.

## Precompiled dependency verification (2026-10-10)

Executed on Apple Silicon macOS 26.6.2 with official LibRaw 0.21.4 static objects and bundled FFmpeg 9.0.2:

- Release tooling: 21 tests passed, including dependency tampering, wrong architecture/deployment target and failed replacement preserving the previous cache.
- Frontend: 85 tests and the TypeScript/Vite build passed. Rust: 218 library tests passed, 5 ignored; offline check and formatting passed after removing vendored sources.
- The ignored real Sony pipeline test was run separately and passed (235.10 s): 7008×4672 ARW development and 9504×6336 SDR HIF decode, analytic RGB16 LUT at 25%, full PNG16 output, ICC/metadata, source protection/collision refusal and HDR refusal.
- Full debug Mac bundle and its relocated copy passed the existing engine/dependency/ad-hoc-signature checks. Main Mach-O and `LSMinimumSystemVersion` both declare 15.0. Its dependencies include only system libraries/frameworks; there is no LibRaw dylib or embedded implementation/source archive. License, copyright, source-download notice and manifest are included.
- Mac Intel and Windows official static archives were prepared and structurally validated only. Windows/Linux builds and runtime, older macOS 15 machines, optimized release and desktop GUI interaction were not tested. The Linux dependency build has not been executed locally.

## Historical evidence before the precompiled dependency update (2026-10-09)

Executed in `/Users/dafuchen/Develop/video/auto-apply-lut` on Apple Silicon macOS, bundled FFmpeg 9.0.2 and static LibRaw 0.21.4.

| Check | Result |
| --- | --- |
| Frontend / production build | 85 tests passed; TypeScript/Vite build passed |
| Rust library suite, serial, required real media | 218 passed, 5 ignored; ignored tests listed above |
| Real Sony camera pipeline | Passed: full ARW + SDR HIF, RGB16 analytic LUT/export, metadata, collision/source protection; 261.68 s debug |
| Real camera metadata with `TZ=America/Los_Angeles` | Passed; ARW capture clock retained as `2023:08:09 13:52:26` |
| Generated HEIF orientation + actual preview | 2 tests passed; all eight transforms, full image and region geometry |
| Release script suite | 18 passed; ELF checks are structural, not Linux runtime |
| Offline compile / Git whitespace | Passed |
| Mac Full debug app build | Passed; matching LibRaw source/CDDL/notices verified inside bundle |
| Built and relocated Mac app validation | Both passed with system-only PATH: native dependencies, full RGB16 HEIF grid, video/audio/preview/cache runtime, ad-hoc signature |

A parallel Rust run had 217 passed and one pre-existing exit-guard test failure while another global application operation was active. The final serial suite passed all 218; portable CI uses the serial invocation. Desktop GUI interaction and optimized release builds were not tested. Bundle verification exercises its engines/signature/dependencies; it is not a camera decode through desktop IPC. Native camera decode is verified separately through the production Rust processor.

The FFmpeg 9.0.2 audit identified complete SDR and HLG TileGrid groups. Direct stream 0 is only a tile. Default automatic assembly works, but a simple `-vf` conflicts with FFmpeg's derived complex graph; the backend therefore assembles the validated primary grid explicitly. FFmpeg drops ICC/cICP output tags and does not tone-map HLG automatically, so color parsing and SDR conversion remain explicit. FFmpeg ARW decoded zero frames; LibRaw developed the actual sensor data.

Camera sources remained byte-identical (SHA256):

- A7C II ARW: `32059163a294a43b37e4c6a487d922205e48af1631920c544b3e2b3994bfbe40`
- A7C II HLG HEIF: `42452fec9e56303d62b7b4a05dd4a4f9318f04afc5c3b9d49e975be03ee097ae`
- ILX-LR1 SDR HIF: `2d1451171a41e47f63cc8eb2af3f1c97bcaeb1ca4a0218db1a69823aabf9f6b0`

No real A7C II **SDR** HIF was available. Other ARW variants, Windows/Linux native builds/runs, Mac Intel, older macOS, desktop GUI interaction and the manual 4K benchmark were not verified. HDR-to-SDR remains unsupported. No system software was installed, camera samples were not added to Git, and no commit, push or deployment was performed.
