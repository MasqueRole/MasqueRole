//! A picture (square, round, or a wide banner), or an icon in its place.

use egui::{Image, Rect, Response, Sense, Stroke, StrokeKind, TextureHandle, Ui, Vec2, Widget};

use super::icon::{self, IconKind};
use crate::theme::{ThemeExt, BORDER_WIDTH};

/// The shape a [`Picture`] is cropped to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Square,
    Circle,
    Banner,
}

/// A rounded picture: a square, a circle for an avatar, or a banner as wide as the space
/// available. The picture is cropped from the middle to fill the shape. Without one, a muted
/// shape with `fallback` is drawn, so layouts do not jump.
#[must_use = "add the picture with `ui.add(...)`"]
pub struct Picture<'a> {
    texture: Option<&'a TextureHandle>,
    fallback: IconKind,
    size: f32,
    shape: Shape,
}

impl<'a> Picture<'a> {
    /// A picture 96 logical pixels wide; `fallback` is shown when there is no texture.
    pub fn new(texture: Option<&'a TextureHandle>, fallback: IconKind) -> Self {
        Self {
            texture,
            fallback,
            size: 96.0,
            shape: Shape::Square,
        }
    }

    /// A banner: as wide as the space available and `height` logical pixels high, with the
    /// picture cropped from the middle.
    pub fn banner(texture: Option<&'a TextureHandle>, fallback: IconKind, height: f32) -> Self {
        Self {
            texture,
            fallback,
            size: height,
            shape: Shape::Banner,
        }
    }

    /// The width and height in logical pixels. For a banner, the height.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Draw a circle instead of a rounded square, for an avatar.
    pub fn circle(mut self) -> Self {
        if self.shape == Shape::Square {
            self.shape = Shape::Circle;
        }
        self
    }
}

/// The part of a texture, in 0..1 coordinates, that fills a square when cropped from the
/// middle.
#[cfg(test)]
fn center_crop(aspect_ratio: f32) -> Rect {
    crop_to(aspect_ratio, 1.0)
}

/// The part of a texture with `aspect_ratio` (width over height), in 0..1 coordinates, that
/// fills a shape with `target` aspect ratio when cropped from the middle.
fn crop_to(aspect_ratio: f32, target: f32) -> Rect {
    let relative = aspect_ratio / target.max(f32::EPSILON);
    let (u, v) = if relative >= 1.0 {
        (1.0 / relative, 1.0)
    } else {
        (1.0, relative)
    };
    Rect::from_min_max(
        egui::pos2((1.0 - u) / 2.0, (1.0 - v) / 2.0),
        egui::pos2((1.0 + u) / 2.0, (1.0 + v) / 2.0),
    )
}

impl Widget for Picture<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let size = match self.shape {
            Shape::Banner => Vec2::new(ui.available_width(), self.size),
            _ => Vec2::splat(self.size),
        };
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        let radius = match self.shape {
            Shape::Circle => egui::CornerRadius::same((self.size / 2.0).round().min(255.0) as u8),
            _ => t.corner_lg(),
        };
        match self.texture {
            Some(texture) => {
                Image::new((texture.id(), rect.size()))
                    .uv(crop_to(
                        texture.aspect_ratio(),
                        rect.width() / rect.height().max(f32::EPSILON),
                    ))
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
                let glyph = Rect::from_center_size(
                    rect.center(),
                    Vec2::splat(self.size.min(rect.width()) / 3.0),
                );
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

    #[test]
    fn a_banner_fills_the_width_and_a_circle_stays_square() {
        let mut h = Harness::new();
        let mut sizes = Vec::new();
        h.idle(&mut |ui| {
            sizes.clear();
            let width = ui.available_width();
            sizes.push((
                ui.add(Picture::banner(None, IconKind::Globe, 120.0))
                    .rect
                    .size(),
                width,
            ));
            sizes.push((
                ui.add(Picture::new(None, IconKind::User).size(32.0).circle())
                    .rect
                    .size(),
                32.0,
            ));
        });
        assert!((sizes[0].0.x - sizes[0].1).abs() < 0.5 && (sizes[0].0.y - 120.0).abs() < 0.01);
        assert_eq!(sizes[1].0, Vec2::splat(32.0));
    }

    #[test]
    fn a_banner_crops_a_square_picture_to_a_wide_strip() {
        // A square picture shown 4 times as wide as high keeps the middle quarter, full width.
        let crop = crop_to(1.0, 4.0);
        assert_eq!((crop.min.x, crop.max.x), (0.0, 1.0));
        assert!((crop.min.y - 0.375).abs() < 1e-6 && (crop.max.y - 0.625).abs() < 1e-6);
        // A picture that is already that wide is shown whole.
        assert_eq!(
            crop_to(4.0, 4.0),
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
        );
    }
}
