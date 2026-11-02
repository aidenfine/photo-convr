# macOS releases

## Build a local DMG

Build prerequisites (for developers only): macOS, Rust, Xcode command line tools,
Python 3.11+, and Homebrew's libheif with HEVC support:

```sh
brew install libheif
make dmg
```

The output is:

```text
dist/Photo Convr.app
dist/Photo-Convr-0.1.0-macOS-arm64.dmg
dist/Photo-Convr-0.1.0-macOS-arm64.dmg.sha256
```

The version comes from Cargo.toml; the architecture comes from the build target.
Open the DMG and drag **Photo Convr.app** into **Applications**. End users do not
need Rust, Homebrew, Python, or a terminal. Nothing is published automatically.
`make app` builds only the app bundle. `make help` lists the available commands.

Local builds use an ad-hoc signature. They are not Apple-notarized releases.
Downloaded copies can trigger Gatekeeper checks. For public downloads, use the
Developer ID and notarization workflow below.

## What gets packaged

- The release Rust executable, a generated monochrome conversion icon, and app metadata.
- The HEIC decoder and its recursively discovered non-system native libraries.
- Optional libheif codec plugins, when the installed libheif provides them.
- Third-party license files and dependency manifests under `Contents/Resources/ThirdParty`.
- An Applications shortcut and installation instructions in the DMG.
- A SHA-256 checksum next to the DMG.

The app looks for its bundled HEIC decoder before installed development tools.
Native library references are rewritten relative to the bundle, and the builder
rejects remaining absolute dependencies outside Apple's system libraries. It
verifies signatures and decodes the test HEIC using the bundled helper with
Homebrew removed from PATH. Disk images are generated without mounting a virtual
disk, then compressed and checksum-verified by `hdiutil`.

`build-info.json` records architecture and minimum supported macOS version.
The minimum is the highest deployment target of **all** bundled native files;
it is not guessed from the Rust executable alone. The current machine's Homebrew
libraries require **macOS 26**, so its current arm64 DMG requires **macOS 26 or later**.
For older macOS support, build libheif and its dependencies against your desired
minimum OS and supply that `heif-convert` using `HEIF_CONVERT`.

The script builds one architecture at a time. To build for Intel, run on an Intel
Mac with Intel libheif, or install the Rust target and supply matching native
libraries. The builder rejects architecture mismatches rather than creating a
broken installer. It does not build a universal binary.

```sh
make dmg TARGET=x86_64-apple-darwin HEIF_CONVERT=/path/to/intel/heif-convert
```

A local build reuses the normal `target/release` output. An explicit TARGET uses
Cargo's corresponding target-specific output directory. Cargo.lock is respected.
The build uses the selected native library installation, so preserve that
installation or use a controlled build environment for reproducible releases.

## Signed, notarized public release

You need an Apple Developer membership, a **Developer ID Application** certificate
with its private key in your Keychain, and a notarytool Keychain profile. Store
credentials through Apple's tool, not in the Makefile or repository:

```sh
xcrun notarytool store-credentials photo-convr-notary
```

Choose a bundle identifier you control before the first public release:

```sh
make release \
  BUNDLE_ID=com.yourcompany.photoconvr \
  CODE_SIGN_IDENTITY='Developer ID Application: Your Name (TEAMID)' \
  NOTARY_PROFILE=photo-convr-notary
```

`make release` requires both signing settings before it starts. It signs embedded
code and the app with hardened runtime and timestamps, submits an app ZIP to
Apple, staples the accepted app, builds and signs the DMG, submits the DMG, and
staples and validates the DMG ticket. This command contacts Apple; `make dmg`
does not submit anything for notarization. Cargo may download dependencies on a
first build. The release output replaces the local DMG only after successful
creation, signing, notarization (when requested), and verification.

Optional settings: `BUILD_NUMBER` (defaults to the Cargo version), `BUNDLE_ID`,
`TARGET`, `HEIF_CONVERT`, and `PYTHON` (the Python executable used by Make).

Before uploading a public release, test a browser-downloaded DMG on a clean Mac
of the supported architecture and OS version, including a HEIC conversion. Review
the packaged dependency licenses and provide any required corresponding source
for redistributed native components. Native Homebrew SPDX manifests include
upstream source URLs; bundling notices alone does not fulfill every source
redistribution obligation. Upload the final DMG and its checksum to a release
page when ready. Publishing is a separate step.

Apple references: [Developer ID distribution](https://developer.apple.com/macos/distribution/)
and [notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow).

## Windows releases

Build on Windows x64 with Rust's MSVC toolchain, Visual Studio C++ build tools,
Python 3.11+, and MSYS2 MINGW64's `mingw-w64-x86_64-libheif` package:

```powershell
python scripts/package_windows.py --msys-prefix C:/msys64/mingw64
```

The script builds the locked Cargo dependencies for `x86_64-pc-windows-msvc`,
stages a portable folder, verifies HEIC decoding without MSYS2 on PATH, and
creates a ZIP and SHA-256 checksum in `dist`. A failed decoder check prevents
publishing a new ZIP. No upload happens locally. The Tests workflow uploads the
ZIP and checksum as GitHub Actions artifacts after the Windows checks pass.

Use a dedicated, minimal MSYS2 installation for release builds: the package
includes **all DLLs in that prefix's bin directory**, optional libheif plugins,
and all notices from `share/licenses`. This favors complete runtime dependencies
over archive size. The current MSYS2 decoder is named
[`heif-dec.exe`](https://packages.msys2.org/packages/mingw-w64-x86_64-libheif).
The app looks beside its own executable before searching PATH and suppresses
the helper's console window.

The ZIP is unsigned and targets Windows x64; ARM64 and 32-bit packages are not
built. Before a public release, test the extracted app on a clean Windows machine,
including drag and drop, dialogs, HEIC import, and replacing an existing output.
Review native dependency licenses and provide any required corresponding source;
license notices alone do not fulfill every source redistribution obligation.
Record the MSYS2 package versions used for a public release. Signing and public
release publishing are separate steps.
