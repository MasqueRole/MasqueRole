//! A bar that floats above the content at the bottom of the window.

use egui::AsIdSalt;

use egui::{Align2, Area, Context, Frame, Id, Margin, Order, Rect, Shadow, Stroke, Ui};

use crate::theme::{spacing, ThemeExt, BORDER_WIDTH};

/// The widest the bar gets; on a wide window it stays centered at this width.
const MAX_WIDTH: f32 = 760.0;

/// How opaque the bar is. The content that scrolls under it shows through the rest.
const OPACITY: f32 = 0.82;

/// A rounded, translucent island at the bottom center of `bounds`, drawn above the content
/// so that the content can scroll underneath it.
///
/// The bar grows with its content. Reserve its [`height`](Self::height) at the end of the
/// content, or the last lines will stay hidden under the bar.
#[derive(Clone, Copy, Debug)]
pub struct FloatingBar {
    id: Id,
}

impl FloatingBar {
    /// A bar. `id_salt` must be unique.
    pub fn new(id_salt: impl AsIdSalt) -> Self {
        Self {
            id: Id::unique(id_salt),
        }
    }

    /// The space the bar covers at the bottom of the content as of the last frame, including
    /// the gap below it: add this much space at the end of the content. Zero before the first
    /// frame.
    pub fn height(self, ctx: &Context) -> f32 {
        ctx.data(|d| d.get_temp::<f32>(self.id.with("height")))
            .map_or(0.0, |h| h + spacing::MD)
    }

    /// Draw the bar inside `bounds`, with `add` as its content. Returns the rectangle of the
    /// bar.
    pub fn show(self, ctx: &Context, bounds: Rect, add: impl FnOnce(&mut Ui)) -> Rect {
        let t = ctx.ir_theme();
        let width = bounds.width().min(MAX_WIDTH);
        let frame = Frame::new()
            .fill(t.card.gamma_multiply(OPACITY))
            .stroke(Stroke::new(BORDER_WIDTH, t.border))
            .corner_radius(t.corner_lg())
            .inner_margin(Margin::same(spacing::SM as i8))
            .shadow(Shadow {
                offset: [0, 4],
                blur: 16,
                spread: 0,
                color: egui::Color32::from_black_alpha(if t.is_dark { 90 } else { 28 }),
            });
        let inner_width = width - 2.0 * (spacing::SM + BORDER_WIDTH);
        let response = Area::new(self.id)
            .order(Order::Middle)
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(bounds.center_bottom())
            .show(ctx, |ui| {
                frame.show(ui, |ui| {
                    ui.set_width(inner_width.max(0.0));
                    add(ui);
                });
            })
            .response;
        ctx.data_mut(|d| d.insert_temp(self.id.with("height"), response.rect.height()));
        response.rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn the_bar_sits_at_the_bottom_center_of_its_bounds() {
        let mut h = Harness::new();
        let bounds = Rect::from_min_max(egui::pos2(100.0, 50.0), egui::pos2(700.0, 500.0));
        let mut bar = Rect::NOTHING;
        h.ctx_frame(vec![], &mut |ctx| {
            bar = FloatingBar::new("bar").show(ctx, bounds, |ui| {
                ui.label("hello");
            });
        });
        assert!((bar.center().x - bounds.center().x).abs() < 1.0, "{bar:?}");
        assert!((bar.bottom() - bounds.bottom()).abs() < 1.0, "{bar:?}");
        assert!(bar.width() <= bounds.width() + 0.5, "{bar:?}");
    }

    #[test]
    fn a_wide_window_keeps_the_bar_at_its_maximum_width() {
        let mut h = Harness::new();
        let bounds = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(3000.0, 500.0));
        let mut bar = Rect::NOTHING;
        h.ctx_frame(vec![], &mut |ctx| {
            bar = FloatingBar::new("bar").show(ctx, bounds, |_| {});
        });
        assert!((bar.width() - MAX_WIDTH).abs() < 1.0, "{bar:?}");
    }

    #[test]
    fn the_height_to_reserve_is_known_after_the_first_frame() {
        let mut h = Harness::new();
        let bounds = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(600.0, 500.0));
        let bar = FloatingBar::new("bar");
        let mut before = f32::NAN;
        let mut after = f32::NAN;
        h.ctx_frame(vec![], &mut |ctx| {
            before = bar.height(ctx);
            bar.show(ctx, bounds, |ui| {
                ui.label("hello");
            });
        });
        h.ctx_frame(vec![], &mut |ctx| after = bar.height(ctx));
        assert_eq!(before, 0.0);
        assert!(after > spacing::MD, "{after}");
    }
}
