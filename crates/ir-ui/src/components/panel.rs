//! Panels: the themed regions of an application window.

use std::hash::Hash;

use egui::{
    CentralPanel, Context, Frame, Id, InnerResponse, Margin, SidePanel, TopBottomPanel, Ui,
};

use crate::theme::{spacing, ThemeExt};

#[derive(Clone, Copy, Debug)]
enum Kind {
    Side(f32),
    Top,
    Bottom,
    Central,
}

/// A region of the window: a fixed-width sidebar, a top or bottom bar, or the central area.
///
/// Add panels in this order within a frame: side, top, bottom, then central, so the central
/// area gets the space that is left.
#[derive(Clone, Copy, Debug)]
pub struct Panel {
    kind: Kind,
    id: Id,
}

impl Panel {
    /// A sidebar on the left, `width` logical pixels wide. `id_salt` must be unique.
    pub fn side(id_salt: impl Hash, width: f32) -> Self {
        Self {
            kind: Kind::Side(width),
            id: Id::new(id_salt),
        }
    }

    /// A bar across the top.
    pub fn top(id_salt: impl Hash) -> Self {
        Self {
            kind: Kind::Top,
            id: Id::new(id_salt),
        }
    }

    /// A bar across the bottom.
    pub fn bottom(id_salt: impl Hash) -> Self {
        Self {
            kind: Kind::Bottom,
            id: Id::new(id_salt),
        }
    }

    /// The area left over after the other panels.
    pub fn central() -> Self {
        Self {
            kind: Kind::Central,
            id: Id::NULL,
        }
    }

    /// Draw the panel with `add` as its content.
    pub fn show<R>(self, ctx: &Context, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let t = ctx.ir_theme();
        let bar = |margin: Margin| Frame::new().fill(t.card).inner_margin(margin);
        let page = Margin::same(spacing::LG as i8);
        match self.kind {
            Kind::Side(width) => SidePanel::left(self.id)
                .exact_width(width)
                .resizable(false)
                .frame(bar(page))
                .show(ctx, add),
            Kind::Top => TopBottomPanel::top(self.id)
                .frame(bar(Margin::symmetric(spacing::LG as i8, spacing::SM as i8)))
                .show(ctx, add),
            Kind::Bottom => TopBottomPanel::bottom(self.id)
                .frame(bar(page))
                .show(ctx, add),
            Kind::Central => CentralPanel::default()
                .frame(Frame::new().fill(t.background).inner_margin(page))
                .show(ctx, add),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn all_panels_draw_and_return_their_content() {
        let mut h = Harness::new();
        let mut results = Vec::new();
        h.ctx_frame(vec![], &mut |ctx| {
            results.clear();
            results.push(Panel::side("side", 200.0).show(ctx, |_| 1).inner);
            results.push(Panel::top("top").show(ctx, |_| 2).inner);
            results.push(Panel::bottom("bottom").show(ctx, |_| 3).inner);
            results.push(Panel::central().show(ctx, |_| 4).inner);
        });
        assert_eq!(results, [1, 2, 3, 4]);
    }

    #[test]
    fn the_sidebar_has_the_requested_width_and_the_rest_goes_to_the_center() {
        let mut h = Harness::new();
        let (mut side, mut center) = (0.0, 0.0);
        h.ctx_frame(vec![], &mut |ctx| {
            side = Panel::side("side", 240.0)
                .show(ctx, |ui| ui.min_rect().width())
                .response
                .rect
                .width();
            center = Panel::central().show(ctx, |_| ()).response.rect.width();
        });
        assert!((side - 240.0).abs() < 1.0, "{side}");
        assert!((center - (800.0 - side)).abs() < 1.0, "{center}");
    }

    #[test]
    fn the_top_and_bottom_bars_leave_the_middle() {
        let mut h = Harness::new();
        let mut rects = Vec::new();
        h.ctx_frame(vec![], &mut |ctx| {
            rects.clear();
            for panel in [Panel::top("top"), Panel::bottom("bottom"), Panel::central()] {
                rects.push(panel.show(ctx, |ui| ui.label("content")).response.rect);
            }
        });
        let (top, bottom, center) = (rects[0], rects[1], rects[2]);
        assert!(
            top.bottom() <= center.top() + 0.5,
            "{top:?} above {center:?}"
        );
        assert!(
            center.bottom() <= bottom.top() + 0.5,
            "{center:?} above {bottom:?}"
        );
        assert!(
            center.height() > 300.0,
            "most of the window is left: {center:?}"
        );
    }
}
