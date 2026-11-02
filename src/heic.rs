//! HEIC import through the platform decoder, without uploading the source.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::Command,
};

#[cfg(windows)]
const HELPER: &str = "heif-dec.exe";
#[cfg(not(windows))]
const HELPER: &str = "heif-convert";

pub fn detected(file: &mut File) -> std::io::Result<bool> {
    let mut header = [0_u8; 4096];
    let len = file.read(&mut header)?;
    file.seek(SeekFrom::Start(0))?;
    Ok(is_heic(&header[..len]))
}

fn is_heic(header: &[u8]) -> bool {
    if header.len() < 16 || &header[4..8] != b"ftyp" {
        return false;
    }
    let size = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
    if size < 16 {
        return false;
    }
    let brands = &header[8..size.min(header.len())];
    brands
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .any(|(index, brand)| {
            index != 1
                && matches!(
                    brand,
                    b"heic" | b"heix" | b"hevc" | b"hevx" | b"heim" | b"heis" | b"hevm" | b"hevs"
                )
        })
}

pub fn decode(path: &Path) -> Result<image::DynamicImage, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let png = directory.path().join("decoded.png");
    // Absolute paths prevent filenames beginning with '-' being parsed as options.
    let source = path.canonicalize()?;
    #[cfg(target_os = "macos")]
    let native_ok = Command::new("/usr/bin/sips")
        .env("TMPDIR", directory.path())
        .args(["-s", "format", "png"])
        .arg(&source)
        .arg("--out")
        .arg(&png)
        .output()
        .is_ok_and(|output| output.status.success() && png.exists());
    #[cfg(not(target_os = "macos"))]
    let native_ok = false;
    if !native_ok {
        if png.exists() {
            std::fs::remove_file(&png)?;
        }
        let bundled = std::env::current_exe().ok().and_then(|exe| {
            let helper = exe.parent()?.join(HELPER);
            helper.is_file().then_some(helper)
        });
        let installed = [
            "/opt/homebrew/bin/heif-convert",
            "/usr/local/bin/heif-convert",
        ]
        .into_iter()
        .find(|tool| Path::new(tool).is_file())
        .unwrap_or(HELPER);
        let tool = bundled.unwrap_or_else(|| installed.into());
        let mut command = Command::new(&tool);
        // GUI releases must not flash a console for each imported photo.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
            if let Some(parent) = tool.parent() {
                let plugins = parent.join("HeifPlugins");
                if plugins.is_dir() {
                    command.env("LIBHEIF_PLUGIN_PATH", plugins);
                }
            }
        }
        if let Some(contents) = tool.parent().and_then(|dir| dir.parent()) {
            let plugins = contents.join("Resources/HeifPlugins");
            if plugins.is_dir() {
                command.env("LIBHEIF_PLUGIN_PATH", plugins);
            }
        }
        let result = command.arg(&source).arg(&png).output()
            .map_err(|e| format!("HEIC decoding requires {HELPER} from libheif when the native decoder is unavailable. Install libheif with HEVC decoding enabled: {e}"))?;
        if !result.status.success() {
            return Err(format!(
                "HEIC decoding failed: {} {}",
                String::from_utf8_lossy(&result.stderr).trim(),
                String::from_utf8_lossy(&result.stdout).trim()
            )
            .into());
        }
    }
    // Both decoders apply HEIF rotation/mirroring before writing the PNG.
    let png = if png.exists() {
        png
    } else {
        // libheif numbers outputs when a container holds multiple images.
        let mut frames: Vec<_> = std::fs::read_dir(directory.path())?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
            .collect();
        frames.sort();
        frames
            .into_iter()
            .next()
            .ok_or("HEIC decoder produced no image")?
    };
    Ok(image::ImageReader::open(png)?.decode()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_real_heic_with_misleading_extension_and_exports_png() {
        let dir = tempfile::tempdir().unwrap();
        let renamed = dir.path().join("photo with spaces.jpg");
        std::fs::write(&renamed, include_bytes!("../tests/fixtures/gradient.heic")).unwrap();
        let original = std::fs::read(&renamed).unwrap();
        let photo = crate::convert::load(&renamed).unwrap();
        assert_eq!(photo.format, "HEIC");
        assert_eq!((photo.image.width(), photo.image.height()), (64, 48));
        let output = dir.path().join("converted.png");
        crate::convert::save(&photo, &output, crate::convert::Output::Png, 90).unwrap();
        assert_eq!(
            image::open(output).unwrap().to_rgba8(),
            photo.image.to_rgba8()
        );
        assert_eq!(std::fs::read(&renamed).unwrap(), original);
        std::fs::write(&renamed, b"\0\0\0\x18ftypheic\0\0\0\0mif1heic").unwrap();
        assert!(crate::convert::load(&renamed).is_err());
    }

    #[test]
    #[ignore = "set HEIC_TEST_FILE to an existing HEIC photo to exercise the platform decoder"]
    fn imports_external_heic() {
        let fixture = std::env::var_os("HEIC_TEST_FILE").expect("set HEIC_TEST_FILE");
        let photo = crate::convert::load(Path::new(&fixture)).unwrap();
        assert_eq!(photo.format, "HEIC");
        assert!(photo.image.width() > 0 && photo.image.height() > 0);
        let dir = tempfile::tempdir().unwrap();
        crate::convert::save(
            &photo,
            &dir.path().join("converted.jpg"),
            crate::convert::Output::Jpeg,
            90,
        )
        .unwrap();
    }

    #[test]
    fn checks_major_and_compatible_brands_without_confusing_avif() {
        assert!(is_heic(b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"));
        assert!(is_heic(b"\0\0\0\x18ftypmif1\0\0\0\0mif1heix"));
        assert!(!is_heic(b"\0\0\0\x18ftypavif\0\0\0\0mif1avif"));
        assert!(!is_heic(b"ftypheic"));
        assert!(!is_heic(b"\0\0\0\x08ftypheic\0\0\0\0"));
    }
}
