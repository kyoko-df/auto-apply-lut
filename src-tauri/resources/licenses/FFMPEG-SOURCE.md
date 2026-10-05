# FFmpeg engine provenance and source status

Recorded on 2026-10-05 for the local macOS arm64 LUTlab package.

## Imported provider build

- Provider build: `1789931890_9.0.2`.
- Version: `9.0.2-https://www.martin-riedl.de`.
- Compiler reported by the provider: Apple clang 14.0.0
  (`clang-1400.0.29.102`).
- Executables used by LUTlab: `ffmpeg` and `ffprobe`; no `ffplay`.
- [FFmpeg archive](https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2/ffmpeg.zip)
  and [FFprobe archive](https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2/ffprobe.zip).
- [Provider versions manifest](https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2/versions.txt),
  saved verbatim as `ffmpeg-macos-arm64-1789931890_9.0.2-versions.txt`.
- Manifest SHA-256:
  `fc92572e752e09b20e7ff28d64fb7096f7bc1a0d589b2e40f17ac91b7fd0ca61`.
- The imported executables were run with `-version` and `-L`. Both report
  GNU GPL version 3 or later. Their output, sizes and executable SHA-256 hashes
  are preserved in `ffmpeg-macos-arm64-1789931890_9.0.2-runtime.txt`.
  These hashes describe imported bytes before application signing; signing can
  change executable bytes. Download archive checksums are recorded separately in
  `../ffmpeg-manifest.json`.

The manifest records `--enable-gpl` and `--enable-version3`, with no
`--enable-nonfree`. Its full configuration and dependency version list are the
evidence for this specific imported build; do not substitute the configuration
of a Homebrew/system FFmpeg or another download.

## License text and upstream references

- `GPL-3.0.txt` is the unmodified text retrieved from the
  [Free Software Foundation](https://www.gnu.org/licenses/gpl-3.0.txt).
  SHA-256: `3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986`.
- [FFmpeg 9.0.2 release source](https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz)
  is the upstream release source reference. This source archive alone does not
  include every external library or establish how the provider built its tools.
- [Provider build-script repository](https://git.martin-riedl.de/ffmpeg/build-script)
  contains the build procedure and dependency references. The repository's
  current branch is a moving reference; the manifest does not identify an exact
  build-script commit for this binary build.
- [FFmpeg licensing documentation](https://ffmpeg.org/doxygen/trunk/md_LICENSE.html)
  and [FFmpeg legal information](https://ffmpeg.org/legal.html).

## Work required before distributing the package to others

This repository currently records provenance and includes the GPLv3 text. It
does not contain or host a verified complete Corresponding Source archive for
these binaries, and does not make a written source offer.

For a distribution, obtain and preserve the exact FFmpeg source used, any
provider patches, the precise build-script revision and configuration, and the
matching source of all statically included external components. Preserve their
copyright/license notices and the scripts required to build and install the
covered code. The provider's manifest includes an imprecise `x264 0.165.x`
version, so that line alone is not enough to select the corresponding x264
source revision. A list of dependency versions is not a substitute for source.

Verify that this source collection corresponds to both shipped executables and
make it available with the binary distribution using an applicable GPLv3
Section 6 source-delivery method. Retain any source modifications made for that
release and include the relevant third-party notices. Confirm the resulting
package and its download page point recipients to the actual source delivery,
not only to upstream project homepages.
