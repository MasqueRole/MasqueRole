//! The app logo, drawn from a texture.

use egui::{Image, Response, TextureHandle, Ui, Widget};

use crate::theme::ThemeExt;

/// A white-on-transparent texture, painted in the theme's foreground color so it reads in
/// both light and dark mode. Keeps the texture's aspect ratio.
#[must_use = "add the icon with `ui.add(...)`"]
pub struct Logo<'a> {
    texture: &'a TextureHandle,
    width: f32,
}

impl<'a> Logo<'a> {
    /// A logo from `texture`, 24 logical pixels wide until [`Logo::width`] says otherwise.
    pub fn new(texture: &'a TextureHandle) -> Self {
        Self {
            texture,
            width: 24.0,
        }
    }

    /// The logo's width in logical pixels; the height follows the texture's proportions.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

impl Widget for Logo<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tint = ui.ir_theme().foreground;
        let size = egui::vec2(self.width, self.width / self.texture.aspect_ratio());
        ui.add(Image::new((self.texture.id(), size)).tint(tint))
    }
}

#[cfg(test)]
mod tests {
    use egui::{Color32, ColorImage, TextureOptions};

    use super::*;
    use crate::components::testing::Harness;

    fn texture(h: &Harness, width: usize, height: usize) -> TextureHandle {
        let image = ColorImage::new([width, height], Color32::WHITE);
        h.ctx
            .load_texture("logo-test", image, TextureOptions::LINEAR)
    }

    #[test]
    fn the_height_follows_the_texture_proportions() {
        let mut h = Harness::new();
        let texture = texture(&h, 200, 50);
        let mut size = egui::Vec2::ZERO;
        h.idle(&mut |ui| size = ui.add(Logo::new(&texture).width(80.0)).rect.size());
        assert!(
            (size.x - 80.0).abs() < 0.5 && (size.y - 20.0).abs() < 0.5,
            "{size:?}"
        );
    }

    #[test]
    fn the_default_width_is_small() {
        let mut h = Harness::new();
        let texture = texture(&h, 10, 10);
        let mut size = egui::Vec2::ZERO;
        h.idle(&mut |ui| size = ui.add(Logo::new(&texture)).rect.size());
        assert!((size.x - 24.0).abs() < 0.5, "{size:?}");
    }
}
