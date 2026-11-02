//! Pictures for characters and worlds: whatever the user picks is shrunk and stored as PNG,
//! so the library stays small and the UI only ever has to read one format.

use std::io::Cursor;

use image::{ImageFormat, ImageReader};

/// The longest side of a stored picture, in pixels.
pub const MAX_SIDE: u32 = 512;

/// The largest file accepted, so a huge file cannot freeze or exhaust the app.
pub const MAX_FILE_BYTES: usize = 25 * 1024 * 1024;

/// Decode a picture (PNG, JPEG, or WebP), shrink it to fit within [`MAX_SIDE`] pixels, and
/// encode it as PNG. Pictures that already fit are kept at their size.
pub fn prepare(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!(
            "That image is too large ({} MB). Pick one under {} MB.",
            bytes.len() / (1024 * 1024),
            MAX_FILE_BYTES / (1024 * 1024)
        ));
    }
    let decoded = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("Could not read the image: {e}"))?
        .decode()
        .map_err(|e| format!("That file is not a supported image (PNG, JPEG, or WebP): {e}"))?;
    let fitted = if decoded.width() > MAX_SIDE || decoded.height() > MAX_SIDE {
        decoded.thumbnail(MAX_SIDE, MAX_SIDE)
    } else {
        decoded
    };
    let mut out = Vec::new();
    fitted
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|e| format!("Could not store the image: {e}"))?;
    Ok(out)
}

/// The picture an exported card gets when its character has none: a plain dark panel.
pub fn placeholder() -> Result<Vec<u8>, String> {
    let image = image::RgbaImage::from_pixel(400, 600, image::Rgba([38, 38, 46, 255]));
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|e| format!("Could not make a picture: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, RgbaImage};

    use super::*;

    fn encoded(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            width,
            height,
            image::Rgba([200, 30, 30, 255]),
        ));
        let mut out = Vec::new();
        // JPEG has no alpha channel.
        let image = if format == ImageFormat::Jpeg {
            DynamicImage::ImageRgb8(image.to_rgb8())
        } else {
            image
        };
        image.write_to(&mut Cursor::new(&mut out), format).unwrap();
        out
    }

    fn size_of(png: &[u8]) -> (u32, u32) {
        let image = image::load_from_memory_with_format(png, ImageFormat::Png).unwrap();
        (image.width(), image.height())
    }

    #[test]
    fn big_pictures_shrink_to_fit_and_keep_their_proportions() {
        let out = prepare(&encoded(2000, 1000, ImageFormat::Png)).unwrap();
        assert_eq!(size_of(&out), (512, 256));
        let tall = prepare(&encoded(300, 1500, ImageFormat::Png)).unwrap();
        assert_eq!(size_of(&tall), (102, 512));
    }

    #[test]
    fn small_pictures_keep_their_size() {
        let out = prepare(&encoded(64, 48, ImageFormat::Png)).unwrap();
        assert_eq!(size_of(&out), (64, 48));
    }

    #[test]
    fn jpeg_input_comes_out_as_png() {
        let out = prepare(&encoded(100, 100, ImageFormat::Jpeg)).unwrap();
        assert_eq!(&out[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(size_of(&out), (100, 100));
    }

    #[test]
    fn things_that_are_not_images_are_refused_with_a_clear_message() {
        let error = prepare(b"this is not an image").unwrap_err();
        assert!(error.contains("not a supported image"), "{error}");
        assert!(prepare(&[]).is_err());
    }

    #[test]
    fn oversized_files_are_refused_before_decoding() {
        let error = prepare(&vec![0; MAX_FILE_BYTES + 1]).unwrap_err();
        assert!(error.contains("too large"), "{error}");
    }
}
