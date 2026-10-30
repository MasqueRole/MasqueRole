//! A floating navigation island.

use std::hash::Hash;

use egui::{Context, CornerRadius, Frame, Id, Margin, Shadow, Stroke, TopBottomPanel, Ui};

use crate::theme::{spacing, ThemeExt, BORDER_WIDTH};

/// More than half of any bar's height, so the corners always become a full pill.
const PILL_RADIUS: u8 = u8::MAX;

/// Height of the row inside the island: a tab strip. Fixing it up front lets items that
/// are shorter than the strip, such as a logo, center on it.
const ROW_HEIGHT: f32 = 36.0;

/// Blur and offset of the shadow under the island.
const SHADOW_BLUR: u8 = 16;
const SHADOW_OFFSET: i8 = 4;

/// A rounded bar that floats at the top of the window, centered, over the page background.
///
/// It reserves a strip across the top (add it before the sidebar and central panels, like
/// [`Panel::top`](super::Panel::top)) and draws the island in the middle of that strip,
/// sized to its content.
#[derive(Clone, Copy, Debug)]
pub struct Island {
    id: Id,
}

impl Island {
    /// An island. `id_salt` must be unique among panels.
    pub fn new(id_salt: impl Hash) -> Self {
        Self {
            id: Id::new(id_salt),
        }
    }

    /// Draw the island with `add` as its content, laid out left to right.
    pub fn show<R>(self, ctx: &Context, add: impl FnOnce(&mut Ui) -> R) -> R {
        let t = ctx.ir_theme();
        let strip = Frame::new()
            .fill(t.background)
            .inner_margin(Margin::symmetric(spacing::LG as i8, spacing::SM as i8));
        let island = Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(BORDER_WIDTH, t.border))
            .corner_radius(CornerRadius::same(PILL_RADIUS))
            .inner_margin(Margin::symmetric(spacing::MD as i8, spacing::XS as i8))
            .shadow(Shadow {
                offset: [0, SHADOW_OFFSET],
                blur: SHADOW_BLUR,
                spread: 0,
                color: t.scrim,
            });
        // A frame cannot be centered before its size is known, so the width is remembered
        // from the previous frame. Only the very first frame is off-center.
        let width_id = self.id.with("island-width");
        let known: f32 = ctx.data(|d| d.get_temp(width_id)).unwrap_or(0.0);
        TopBottomPanel::top(self.id)
            .frame(strip)
            .show_separator_line(false)
            .show(ctx, |ui| {
                let out = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(((ui.available_width() - known) / 2.0).max(0.0));
                        island.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.set_min_height(ROW_HEIGHT);
                                add(ui)
                            })
                            .inner
                        })
                    })
                    .inner;
                let width = out.response.rect.width();
                if (width - known).abs() > 0.5 {
                    ctx.data_mut(|d| d.insert_temp(width_id, width));
                    ctx.request_repaint();
                }
                out.inner
            })
            .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn the_island_is_centered_and_sized_to_its_content() {
        let mut h = Harness::new();
        let mut rect = egui::Rect::NOTHING;
        for _ in 0..2 {
            h.ctx_frame(vec![], &mut |ctx| {
                Island::new("nav").show(ctx, |ui| {
                    rect = ui
                        .allocate_exact_size(egui::vec2(200.0, 24.0), egui::Sense::hover())
                        .0;
                });
            });
        }
        let center = (rect.min.x + rect.max.x) / 2.0;
        assert!((center - 400.0).abs() < 2.0, "centered in 800 px: {rect:?}");
        assert!(rect.width() < 300.0, "{rect:?}");
    }

    #[test]
    fn the_island_returns_what_its_content_returns() {
        let mut h = Harness::new();
        let mut out = 0;
        h.ctx_frame(vec![], &mut |ctx| {
            out = Island::new("nav").show(ctx, |_| 7);
        });
        assert_eq!(out, 7);
    }

    #[test]
    fn the_island_reserves_space_at_the_top() {
        let mut h = Harness::new();
        let mut top = 0.0;
        h.ctx_frame(vec![], &mut |ctx| {
            Island::new("nav").show(ctx, |ui| ui.label("x"));
            top = ctx.available_rect().min.y;
        });
        assert!(top > 24.0, "the page starts below the island: {top}");
    }
}
