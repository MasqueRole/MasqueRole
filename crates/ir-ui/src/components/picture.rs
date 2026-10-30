//! A square picture, or an icon in its place.

use egui::{Image, Rect, Response, Sense, Stroke, StrokeKind, TextureHandle, Ui, Vec2, Widget};

use super::icon::{self, IconKind};
use crate::theme::{ThemeExt, BORDER_WIDTH};

/// A square, rounded picture. The picture is cropped from the middle to fill the square.
/// Without one, a muted square with `fallback` is drawn, so layouts do not jump.
#[must_use = "add the picture with `ui.add(...)`"]
pub struct Picture<'a> {
    texture: Option<&'a TextureHandle>,
    fallback: IconKind,
    size: f32,
}

impl<'a> Picture<'a> {
    /// A picture 96 logical pixels wide; `fallback` is shown when there is no texture.
    pub fn new(texture: Option<&'a TextureHandle>, fallback: IconKind) -> Self {
        Self {
            texture,
            fallback,
            size: 96.0,
        }
    }

    /// The width and height in logical pixels.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

/// The part of a texture, in 0..1 coordinates, that fills a square when cropped from the
/// middle.
fn center_crop(aspect_ratio: f32) -> Rect {
    let (u, v) = if aspect_ratio >= 1.0 {
        (1.0 / aspect_ratio, 1.0)
    } else {
        (1.0, aspect_ratio)
    };
    Rect::from_min_max(
        egui::pos2((1.0 - u) / 2.0, (1.0 - v) / 2.0),
        egui::pos2((1.0 + u) / 2.0, (1.0 + v) / 2.0),
    )
}

impl Widget for Picture<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.size), Sense::hover());
        let radius = t.corner_lg();
        match self.texture {
            Some(texture) => {
                Image::new((texture.id(), rect.size()))
                    .uv(center_crop(texture.aspect_ratio()))
                    .corner_radius(radius)
                    .paint_at(ui, rect);
                ui.painter().rect_stroke(
                    rect,
                    radius,
                    Stroke::new(BORDER_WIDTH, t.border),
                    StrokeKind::Inside,
                );
            }
            None => {
                ui.painter().rect(
                    rect,
                    radius,
                    t.muted,
                    Stroke::new(BORDER_WIDTH, t.border),
                    StrokeKind::Inside,
                );
                let glyph = Rect::from_center_size(rect.center(), Vec2::splat(self.size / 3.0));
                icon::paint(ui.painter(), glyph, t.muted_foreground, self.fallback);
            }
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use egui::{Color32, ColorImage, TextureOptions};

    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn it_is_square_with_or_without_a_texture() {
        let mut h = Harness::new();
        let texture = h.ctx.load_texture(
            "picture-test",
            ColorImage::new([40, 10], Color32::WHITE),
            TextureOptions::LINEAR,
        );
        let mut sizes = Vec::new();
        h.idle(&mut |ui| {
            sizes.clear();
            sizes.push(
                ui.add(Picture::new(Some(&texture), IconKind::User).size(64.0))
                    .rect
                    .size(),
            );
            sizes.push(
                ui.add(Picture::new(None, IconKind::Globe).size(64.0))
                    .rect
                    .size(),
            );
        });
        assert!(sizes
            .iter()
            .all(|s| (s.x - 64.0).abs() < 0.01 && (s.y - 64.0).abs() < 0.01));
    }

    #[test]
    fn wide_and_tall_pictures_are_cropped_from_the_middle() {
        let wide = center_crop(2.0);
        assert_eq!((wide.min.x, wide.max.x), (0.25, 0.75));
        assert_eq!((wide.min.y, wide.max.y), (0.0, 1.0));
        let tall = center_crop(0.5);
        assert_eq!((tall.min.x, tall.max.x), (0.0, 1.0));
        assert_eq!((tall.min.y, tall.max.y), (0.25, 0.75));
        assert_eq!(
            center_crop(1.0),
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
        );
    }
}
