//! The app's logo and window icon.

use std::sync::Arc;

use eframe::egui::{ColorImage, Context, IconData, TextureHandle, TextureOptions};

/// White on transparent, so the UI can paint it in the theme's foreground color.
const LOGO_PNG: &[u8] = include_bytes!("../../../assets/logo.png");

/// The logo on a dark rounded square, for the window, taskbar, and installers.
const ICON_PNG: &[u8] = include_bytes!("../../../assets/icon.png");

/// The window and taskbar icon. `None` if the embedded image cannot be decoded.
pub fn window_icon() -> Option<Arc<IconData>> {
    eframe::icon_data::from_png_bytes(ICON_PNG)
        .ok()
        .map(Arc::new)
}

/// Upload the logo as a texture. `None` if the embedded image cannot be decoded; the app
/// then simply runs without a logo.
pub fn load_logo(ctx: &Context) -> Option<TextureHandle> {
    let logo = eframe::icon_data::from_png_bytes(LOGO_PNG).ok()?;
    let size = [logo.width as usize, logo.height as usize];
    let image = ColorImage::from_rgba_unmultiplied(size, &logo.rgba);
    Some(ctx.load_texture("logo", image, TextureOptions::LINEAR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_images_decode() {
        let icon = window_icon().expect("the window icon decodes");
        assert_eq!((icon.width, icon.height), (256, 256));
        let ctx = Context::default();
        let logo = load_logo(&ctx).expect("the logo decodes");
        assert!(logo.aspect_ratio() > 1.0, "the logo is wider than tall");
    }
}
