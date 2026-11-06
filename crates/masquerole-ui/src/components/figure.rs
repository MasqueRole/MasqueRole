//! A picture shown whole, such as a screenshot.

use egui::{Image, Response, Sense, Stroke, StrokeKind, TextureHandle, Ui, Vec2, Widget};

use crate::theme::{ThemeExt, BORDER_WIDTH};

/// A texture drawn at a given width with its own proportions, in a rounded, bordered frame.
/// Unlike [`Picture`](super::Picture) nothing is cropped.
#[must_use = "add the figure with `ui.add(...)`"]
pub struct Figure<'a> {
    texture: &'a TextureHandle,
    width: f32,
}

impl<'a> Figure<'a> {
    /// A figure as wide as the space it is given, until [`Figure::width`] says otherwise.
    pub fn new(texture: &'a TextureHandle) -> Self {
        Self {
            texture,
            width: f32::INFINITY,
        }
    }

    /// The width in logical pixels. The figure never grows beyond the space available.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

impl Widget for Figure<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let width = self.width.min(ui.available_width()).max(1.0);
        let size = Vec2::new(width, width / self.texture.aspect_ratio());
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        let radius = t.corner_lg();
        Image::new((self.texture.id(), size))
            .corner_radius(radius)
            .paint_at(ui, rect);
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(BORDER_WIDTH, t.border),
            StrokeKind::Inside,
        );
        response
    }
}

#[cfg(test)]
mod tests {
    use egui::{Color32, ColorImage, TextureOptions};

    use super::*;
    use crate::components::testing::Harness;

    fn texture(h: &Harness, width: usize, height: usize) -> TextureHandle {
        h.ctx.load_texture(
            "figure-test",
            ColorImage::filled([width, height], Color32::WHITE),
            TextureOptions::LINEAR,
        )
    }

    #[test]
    fn the_height_follows_the_proportions() {
        let mut h = Harness::new();
        let texture = texture(&h, 400, 200);
        let mut size = Vec2::ZERO;
        h.idle(&mut |ui| size = ui.add(Figure::new(&texture).width(300.0)).rect.size());
        assert!(
            (size.x - 300.0).abs() < 0.5 && (size.y - 150.0).abs() < 0.5,
            "{size:?}"
        );
    }

    #[test]
    fn it_never_grows_beyond_the_available_width() {
        let mut h = Harness::new();
        let texture = texture(&h, 400, 200);
        let (mut small, mut default) = (0.0, 0.0);
        h.idle(&mut |ui| {
            ui.scope(|ui| {
                ui.set_max_width(100.0);
                small = ui.add(Figure::new(&texture).width(900.0)).rect.width();
            });
            default = ui.add(Figure::new(&texture)).rect.width();
        });
        assert!(small <= 100.5, "{small}");
        assert!(default > 400.0, "fills the page by default: {default}");
    }
}
