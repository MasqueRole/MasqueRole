//! The application's top bar: a left, a centered, and a right group.

use egui::AsIdSalt;

use egui::{Align, Frame, Id, Layout, Margin, Pos2, Rect, Sense, Ui, UiBuilder, Vec2};

use crate::theme::{spacing, ThemeExt};

/// Height of the row the groups sit in; the height of a pill-style tab strip.
const ROW_HEIGHT: f32 = 32.0;

/// What the user did to the empty part of the bar, so an app with its own title bar can move
/// the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BarResponse {
    /// A drag started on the bar itself, not on a widget in it.
    pub drag_started: bool,
    /// The bar itself was double-clicked.
    pub double_clicked: bool,
}

/// A bar across the top of the window in the page background color, with one group at the
/// left edge, one centered, and one at the right edge.
///
/// Add it before the sidebar and central panels, like [`Panel::top`](super::Panel::top).
#[derive(Clone, Copy, Debug)]
pub struct TopBar {
    id: Id,
}

impl TopBar {
    /// A top bar. `id_salt` must be unique among panels.
    pub fn new(id_salt: impl AsIdSalt) -> Self {
        Self {
            id: Id::unique(id_salt),
        }
    }

    /// Draw the bar. `left` and `center` are laid out left to right; `right` is laid out
    /// right to left, so its first widget is the rightmost.
    pub fn show(
        self,
        ui: &mut Ui,
        left: impl FnOnce(&mut Ui),
        center: impl FnOnce(&mut Ui),
        right: impl FnOnce(&mut Ui),
    ) -> BarResponse {
        let ctx = ui.ctx().clone();
        let t = ctx.ir_theme();
        let frame = Frame::new()
            .fill(t.background)
            .inner_margin(Margin::symmetric(spacing::LG as i8, spacing::SM as i8));
        // A group cannot be centered before its width is known, so the width is remembered
        // from the previous frame. Only the very first frame is off-center.
        let width_id = self.id.with("center-width");
        let known: f32 = ctx.data(|d| d.get_temp(width_id)).unwrap_or(0.0);
        let mut bar = BarResponse::default();
        egui::Panel::top(self.id)
            .frame(frame)
            .show_separator_line(false)
            .show(ui, |ui| {
                let (row, background) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), ROW_HEIGHT),
                    Sense::click_and_drag(),
                );
                // Widgets added below sit on top of this, so only the gaps report.
                bar = BarResponse {
                    drag_started: background.drag_started(),
                    double_clicked: background.double_clicked(),
                };
                let left_to_right = Layout::left_to_right(Align::Center);
                ui.scope_builder(UiBuilder::new().max_rect(row).layout(left_to_right), left);
                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(row)
                        .layout(Layout::right_to_left(Align::Center)),
                    right,
                );
                let centered = Rect::from_min_size(
                    Pos2::new(row.center().x - known / 2.0, row.min.y),
                    row.size(),
                );
                let used = ui
                    .scope_builder(
                        UiBuilder::new().max_rect(centered).layout(left_to_right),
                        center,
                    )
                    .response
                    .rect
                    .width();
                if (used - known).abs() > 0.5 {
                    ctx.data_mut(|d| d.insert_temp(width_id, used));
                    ctx.request_repaint();
                }
            });
        bar
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    fn draw(ui: &mut Ui, left: &mut Rect, center: &mut Rect, right: &mut Rect) {
        let size = Vec2::new(80.0, 24.0);
        let place = |ui: &mut Ui, out: &mut Rect| {
            *out = ui.allocate_exact_size(size, Sense::hover()).0;
        };
        TopBar::new("bar").show(
            ui,
            |ui| place(ui, left),
            |ui| place(ui, center),
            |ui| place(ui, right),
        );
    }

    #[test]
    fn the_groups_sit_left_center_and_right() {
        let mut h = Harness::new();
        let (mut l, mut c, mut r) = (Rect::NOTHING, Rect::NOTHING, Rect::NOTHING);
        for _ in 0..2 {
            h.root_frame(vec![], &mut |ui| draw(ui, &mut l, &mut c, &mut r));
        }
        assert!(l.min.x < 40.0, "left group at the left edge: {l:?}");
        assert!(r.max.x > 760.0, "right group at the right edge: {r:?}");
        assert!(
            (c.center().x - 400.0).abs() < 2.0,
            "centered in 800 px: {c:?}"
        );
    }

    #[test]
    fn dragging_the_empty_bar_is_reported() {
        use egui::{Event, PointerButton};
        let mut h = Harness::new();
        let mut left = Rect::NOTHING;
        let mut dragged = false;
        let mut run = |h: &mut Harness, events: Vec<Event>| {
            h.root_frame(events, &mut |root| {
                let size = Vec2::new(80.0, 24.0);
                let bar = TopBar::new("bar").show(
                    root,
                    |ui| left = ui.allocate_exact_size(size, Sense::click()).0,
                    |_| {},
                    |_| {},
                );
                dragged |= bar.drag_started;
            });
        };
        run(&mut h, vec![]);
        run(&mut h, vec![]);
        let gap = Pos2::new(300.0, 24.0);
        let button = |pressed| Event::PointerButton {
            pos: gap,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run(&mut h, vec![Event::PointerMoved(gap)]);
        run(&mut h, vec![button(true)]);
        run(
            &mut h,
            vec![Event::PointerMoved(gap + Vec2::new(30.0, 0.0))],
        );
        assert!(dragged, "a drag on empty space moves the window");
    }

    #[test]
    fn the_bar_reserves_space_at_the_top() {
        let mut h = Harness::new();
        let mut top = 0.0;
        h.root_frame(vec![], &mut |root| {
            TopBar::new("bar").show(root, |_| {}, |_| {}, |_| {});
            top = root.available_rect_before_wrap().min.y;
        });
        assert!(top >= ROW_HEIGHT, "the page starts below the bar: {top}");
    }
}
