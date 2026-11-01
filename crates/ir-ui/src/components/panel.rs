//! Panels: the themed regions of an application window.

use std::hash::Hash;

use egui::{
    CentralPanel, Context, CornerRadius, Frame, Id, InnerResponse, Margin, Rect, SidePanel, Stroke,
    StrokeKind, TopBottomPanel, Ui, Vec2,
};

use crate::theme::{spacing, ThemeExt, BORDER_WIDTH};

#[derive(Clone, Copy, Debug)]
enum Kind {
    Side(f32),
    Rail(f32),
    Top,
    Bottom,
    Central,
    Content,
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

    /// A narrow column on the left for icon-only navigation, `width` logical pixels wide
    /// (see [`RAIL_WIDTH`](super::rail::RAIL_WIDTH)). Add it before the sidebar.
    pub fn rail(id_salt: impl Hash, width: f32) -> Self {
        Self {
            kind: Kind::Rail(width),
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

    /// The content area on top of the [`surface`](Self::surface): transparent, with the page
    /// margin. Use it instead of [`central`](Self::central) when the window has a surface.
    pub fn content() -> Self {
        Self {
            kind: Kind::Content,
            id: Id::NULL,
        }
    }

    /// Paint the content surface: the rounded, bordered sheet that the page lives on, over all
    /// the space the sidebar and the top bar leave. The sidebar and top bar share the page
    /// background, so only the sheet has a border and a rounded corner, at the top left.
    ///
    /// Call it after the side and top panels and before the bottom and [`content`](Self::content)
    /// panels, which are transparent and draw on it.
    pub fn surface(ctx: &Context) {
        let t = ctx.ir_theme();
        let radius = t.radius_lg;
        let corner = CornerRadius {
            nw: radius,
            ..CornerRadius::ZERO
        };
        let painter = ctx.layer_painter(egui::LayerId::background());
        let rect = ctx.available_rect();
        // The rounded corner leaves the window behind it showing, which is black without this.
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::splat(f32::from(radius))),
            CornerRadius::ZERO,
            t.background,
        );
        painter.rect(
            rect,
            corner,
            t.card,
            Stroke::new(BORDER_WIDTH, t.border),
            StrokeKind::Inside,
        );
    }

    /// Draw the panel with `add` as its content.
    pub fn show<R>(self, ctx: &Context, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let t = ctx.ir_theme();
        let bar = |margin: Margin| Frame::new().fill(t.background).inner_margin(margin);
        let clear = |margin: Margin| Frame::new().inner_margin(margin);
        let page = Margin::same(spacing::LG as i8);
        match self.kind {
            Kind::Side(width) => SidePanel::left(self.id)
                .exact_width(width)
                .resizable(false)
                .frame(bar(page))
                .show_separator_line(false)
                .show(ctx, add),
            Kind::Rail(width) => SidePanel::left(self.id)
                .exact_width(width)
                .resizable(false)
                .frame(bar(Margin::symmetric(spacing::SM as i8, spacing::SM as i8)))
                .show_separator_line(false)
                .show(ctx, add),
            Kind::Top => TopBottomPanel::top(self.id)
                .frame(bar(Margin::symmetric(spacing::LG as i8, spacing::SM as i8)))
                .show_separator_line(false)
                .show(ctx, add),
            Kind::Bottom => TopBottomPanel::bottom(self.id)
                .frame(clear(page))
                .show_separator_line(false)
                .show(ctx, add),
            Kind::Content => CentralPanel::default().frame(clear(page)).show(ctx, add),
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
