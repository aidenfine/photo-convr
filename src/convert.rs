use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

pub struct Photo {
    pub path: PathBuf,
    pub image: DynamicImage,
    pub format: String,
    pub bytes: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Png,
    Jpeg,
    WebP,
    Avif,
    Gif,
    Tiff,
    Bmp,
    Ico,
    Tga,
    Pnm,
    Qoi,
    Farbfeld,
}
impl Output {
    pub const ALL: [Self; 12] = [
        Self::Png,
        Self::Jpeg,
        Self::WebP,
        Self::Avif,
        Self::Gif,
        Self::Tiff,
        Self::Bmp,
        Self::Ico,
        Self::Tga,
        Self::Pnm,
        Self::Qoi,
        Self::Farbfeld,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPG / JPEG",
            Self::WebP => "WebP",
            Self::Avif => "AVIF",
            Self::Gif => "GIF",
            Self::Tiff => "TIFF",
            Self::Bmp => "BMP",
            Self::Ico => "ICO",
            Self::Tga => "TGA",
            Self::Pnm => "PPM / PNM",
            Self::Qoi => "QOI",
            Self::Farbfeld => "Farbfeld",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Avif => "avif",
            Self::Gif => "gif",
            Self::Tiff => "tiff",
            Self::Bmp => "bmp",
            Self::Ico => "ico",
            Self::Tga => "tga",
            Self::Pnm => "ppm",
            Self::Qoi => "qoi",
            Self::Farbfeld => "ff",
        }
    }
    fn format(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Jpeg => ImageFormat::Jpeg,
            Self::WebP => ImageFormat::WebP,
            Self::Avif => ImageFormat::Avif,
            Self::Gif => ImageFormat::Gif,
            Self::Tiff => ImageFormat::Tiff,
            Self::Bmp => ImageFormat::Bmp,
            Self::Ico => ImageFormat::Ico,
            Self::Tga => ImageFormat::Tga,
            Self::Pnm => ImageFormat::Pnm,
            Self::Qoi => ImageFormat::Qoi,
            Self::Farbfeld => ImageFormat::Farbfeld,
        }
    }
}
pub fn load(path: &Path) -> Result<Photo, String> {
    (|| -> Result<Photo, Box<dyn std::error::Error>> {
        let mut file = File::open(path)?;
        let bytes = file.metadata()?.len();
        if crate::heic::detected(&mut file)? {
            return Ok(Photo {
                path: path.canonicalize()?,
                image: crate::heic::decode(path)?,
                format: "HEIC".into(),
                bytes,
            });
        }
        let reader = ImageReader::new(BufReader::new(file)).with_guessed_format()?;
        // Prefer magic bytes; use the extension only for formats without a signature.
        let mut reader = reader;
        if reader.format().is_none() && let Ok(format) = ImageFormat::from_path(path) { reader.set_format(format); }
        let format = reader.format().map(|f| format!("{f:?}").to_uppercase()).unwrap_or_else(|| "Unknown".into());
        let mut decoder = reader.into_decoder()?;
        let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
        let mut image = DynamicImage::from_decoder(decoder)?;
        image.apply_orientation(orientation);
        Ok(Photo { path: path.canonicalize()?, image, format, bytes })
    })().map_err(|e| format!("Couldn't open this image: {e}. RAW, SVG and AVIF input are not supported by this build."))
}

fn white_background(image: &DynamicImage) -> DynamicImage {
    let rgba = image.to_rgba8();
    let rgb = image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let a = u32::from(p[3]);
        image::Rgb([0, 1, 2].map(|c| ((u32::from(p[c]) * a + 255 * (255 - a) + 127) / 255) as u8))
    });
    DynamicImage::ImageRgb8(rgb)
}

pub fn save(photo: &Photo, path: &Path, output: Output, quality: u8) -> Result<PathBuf, String> {
    (|| -> Result<PathBuf, Box<dyn std::error::Error>> {
        if path.canonicalize().is_ok_and(|p| p == photo.path) {
            return Err("Choose a different filename to keep your original photo".into());
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        {
            let mut writer = BufWriter::new(temp.as_file_mut());
            let converted = match output {
                Output::Jpeg | Output::Pnm => white_background(&photo.image),
                Output::Farbfeld => DynamicImage::ImageRgba16(photo.image.to_rgba16()),
                Output::Ico => DynamicImage::ImageRgba8(
                    photo
                        .image
                        .resize(
                            photo.image.width().min(256),
                            photo.image.height().min(256),
                            image::imageops::FilterType::Lanczos3,
                        )
                        .to_rgba8(),
                ),
                _ => DynamicImage::ImageRgba8(photo.image.to_rgba8()),
            };
            if output == Output::Jpeg {
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, quality)
                    .encode_image(&converted)?;
            } else {
                converted.write_to(&mut writer, output.format())?;
            }
            writer.flush()?;
        }
        temp.as_file().sync_all()?;
        temp.persist(path)?;
        Ok(path.to_path_buf())
    })()
    .map_err(|e| format!("Couldn't save the photo: {e}"))
}

pub fn size_label(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_output_encodes_and_preserves_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original.png");
        DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            12,
            8,
            image::Rgba([200, 100, 40, 128]),
        ))
        .save(&path)
        .unwrap();
        let original = std::fs::read(&path).unwrap();
        let photo = load(&path).unwrap();
        assert_eq!(photo.format, "PNG");
        for output in Output::ALL {
            let dest = dir.path().join(format!("converted.{}", output.extension()));
            save(&photo, &dest, output, 90).unwrap_or_else(|e| panic!("{}: {e}", output.label()));
            assert!(dest.metadata().unwrap().len() > 0);
            if output != Output::Avif {
                let decoded = image::open(&dest).unwrap();
                assert_eq!((decoded.width(), decoded.height()), (12, 8));
            }
        }
        assert!(save(&photo, &path, Output::Png, 90).is_err());
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
    #[test]
    fn detects_content_and_rejects_invalid_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("actually-a-png.jpg");
        DynamicImage::new_rgb8(2, 3)
            .save_with_format(&path, ImageFormat::Png)
            .unwrap();
        assert_eq!(load(&path).unwrap().format, "PNG");
        std::fs::write(&path, b"not an image").unwrap();
        assert!(load(&path).is_err());
    }
    #[test]
    fn ico_resizes_large_images_and_preserves_aspect_ratio() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("original.png");
        DynamicImage::new_rgba8(512, 256).save(&source).unwrap();
        let photo = load(&source).unwrap();
        let destination = dir.path().join("icon.ico");

        save(&photo, &destination, Output::Ico, 90).unwrap();

        let decoded = image::open(destination).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (256, 128));
        assert_eq!((photo.image.width(), photo.image.height()), (512, 256));
    }

    #[test]
    fn failed_save_preserves_destination_and_removes_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("original.png");
        DynamicImage::new_rgb8(2, 3).save(&source).unwrap();
        let mut photo = load(&source).unwrap();
        // PNG cannot encode an image with zero dimensions.
        photo.image = DynamicImage::new_rgba8(0, 0);
        let destination = dir.path().join("existing.png");
        let original = b"existing destination contents";
        std::fs::write(&destination, original).unwrap();

        assert!(save(&photo, &destination, Output::Png, 90).is_err());

        assert_eq!(std::fs::read(destination).unwrap(), original);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
        assert!(load(&source).is_ok());
    }

    #[test]
    fn alpha_flattens_onto_white() {
        let input = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            1,
            image::Rgba([0, 0, 0, 0]),
        ));
        assert_eq!(
            white_background(&input).to_rgb8().get_pixel(0, 0).0,
            [255; 3]
        );
    }
}
