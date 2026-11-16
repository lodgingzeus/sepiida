//! A small copy of the wallpaper for the settings window's preview.

use std::path::Path;

use image::{ImageReader, imageops::FilterType};

/// An RGBA8 image, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The image at `path`, scaled and cropped to fill `width` × `height`
/// (like a "fill" wallpaper). The format is detected from the contents,
/// as Windows' copy of the wallpaper has no extension.
pub fn thumbnail(path: &Path, width: u32, height: u32) -> Result<Thumbnail, crate::Error> {
    let image = ImageReader::open(path)?.with_guessed_format()?.decode()?;
    let filled = image.resize_to_fill(width.max(1), height.max(1), FilterType::Triangle);
    Ok(Thumbnail {
        width: filled.width(),
        height: filled.height(),
        rgba: filled.into_rgba8().into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};

    use super::*;

    #[test]
    fn fills_the_box_whatever_the_image_shape() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("TranscodedWallpaper");
        DynamicImage::ImageRgb8(RgbImage::from_pixel(400, 100, Rgb([10, 20, 30])))
            .save_with_format(&path, ImageFormat::Png)
            .expect("write image");
        let thumb = thumbnail(&path, 160, 100).expect("thumbnail");
        assert_eq!((thumb.width, thumb.height), (160, 100));
        assert_eq!(thumb.rgba.len(), 160 * 100 * 4);
        assert_eq!(&thumb.rgba[..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn reports_missing_and_corrupt_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(thumbnail(&dir.path().join("missing.png"), 10, 10).is_err());
        let corrupt = dir.path().join("corrupt.png");
        std::fs::write(&corrupt, b"not an image").expect("write file");
        assert!(thumbnail(&corrupt, 10, 10).is_err());
    }
}
