**WARNING**: AI was used to build this. Works well and beats uploading photos to the internet to convert them.

# Photo Convr

A native Rust desktop app for converting photos on your own computer. A compact black-and-white interface, a small photo thumbnail, drag and drop, a native file picker, and an output format dropdown. No account, server, or upload.

## Run

Install a current stable Rust toolchain, then run:

```sh
cargo run --release
```

The initial build downloads dependencies from crates.io. Once built, the app works offline. The executable is at `target/release/photo-convr` (`photo-convr.exe` on Windows).

On Linux, the desktop build needs development libraries for your window system and graphics backend, plus a working XDG desktop portal for native file dialogs. On macOS, install the Xcode command line tools. On Windows, use the MSVC Rust toolchain and Visual Studio C++ build tools.

## Build a Mac installer

```sh
make dmg
```

This creates an app bundle and a compressed DMG in `dist/`, including the HEIC
fallback decoder. Build prerequisites: macOS, Rust, Xcode command line tools,
Python 3.11+, and `brew install libheif`. Users of the packaged app need none of
these developer tools.

`make app` builds just the app; `make check` runs verification. See
[macOS release instructions](packaging/README.md) for signing, notarization,
architecture selection, minimum macOS requirements, and public distribution.

## Use

1. Drop a photo onto the window or click **Choose a photo**.
2. See the preview, detected format, dimensions, and file size.
3. Pick an output format. JPEG also has a quality slider.
4. Click **Convert & save**, then choose the destination.

Decoding and encoding run in background threads. Format detection reads file contents, falling back to the extension for formats without a signature. EXIF orientation is applied. Saving writes a temporary file and replaces the destination only after successful encoding. Saving over the original path is blocked.

## Formats and limits

Outputs: **PNG, JPEG, WebP, AVIF, GIF, TIFF, BMP, ICO, TGA, PPM/PNM, QOI, Farbfeld**.

Inputs include HEIC/HEIF, PNG, JPEG, WebP, GIF, TIFF, BMP, ICO, TGA, PNM, QOI, HDR, EXR, DDS, and Farbfeld. Camera RAW, SVG, and AVIF decoding are not included. AVIF encoding is included. This app does not claim to support every photo format.

- HEIC/HEIF import uses the built-in macOS decoder (`sips`), with no extra installation for supported files. If the native decoder fails, packaged macOS apps use their bundled `heif-convert` fallback. For source builds, install it with `brew install libheif`. On Linux and Windows, install `heif-convert` from libheif with an HEVC decoder and put it on PATH. HEIC is an input format; output choices are listed above. Temporary decoded files stay local and are automatically deleted.
- Conversion exports a single still image; animation and multi-page files use the first frame/page.
- Metadata is not copied. Color profiles are not preserved or managed.
- Most outputs use 8-bit color; Farbfeld uses 16-bit RGBA. This is not a lossless archival workflow for high bit depth sources.
- JPEG and PPM flatten transparency onto white. GIF quantizes colors. WebP output is lossless.
- ICO output fits inside 256 × 256 pixels. Other outputs retain the oriented image dimensions.
- Very large or unsupported images report an error. AVIF encoding may take longer than other formats.

## Checks

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Tests cover all output encoders, round trips for formats with enabled decoders, source preservation, content-based detection, invalid input, and alpha flattening.

Implementation uses [eframe/egui](https://docs.rs/eframe/0.33.3/eframe/), [image](https://docs.rs/image/0.25.9/image/), and [rfd](https://docs.rs/rfd/0.15.4/rfd/).
