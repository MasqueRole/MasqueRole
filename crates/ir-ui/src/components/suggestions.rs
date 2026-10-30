//! A floating list of suggestions above a text field, such as matching commands.

use std::hash::Hash;

use egui::{
    Align2, Area, Color32, Context, FontId, Frame, Id, Margin, Order, Pos2, Rect, Sense, Shadow,
    Stroke, Vec2,
};

use super::icon::{self, IconKind};
use crate::theme::{spacing, ThemeExt, BORDER_WIDTH};

const ROW_HEIGHT: f32 = 36.0;
const ICON_SIZE: f32 = 16.0;
const MAX_WIDTH: f32 = 520.0;

/// Width reserved for the title before the description starts, as a share of the row.
const TITLE_SHARE: f32 = 0.42;

/// Blur and offset of the shadow under the list.
const SHADOW_BLUR: u8 = 12;
const SHADOW_OFFSET: i8 = 2;

/// One entry: an icon, a short title, and a muted description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    /// The icon before the title.
    pub icon: IconKind,
    /// What is suggested, such as a command and its arguments.
    pub title: String,
    /// What choosing it does.
    pub description: String,
}

/// What [`Suggestions::show`] reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SuggestionsOutput {
    /// The index of the entry that was clicked this frame.
    pub picked: Option<usize>,
    /// Whether the pointer is over the list. A text field loses focus when its list is
    /// pressed, so callers keep the list open while this is true.
    pub hovered: bool,
}

/// A list that floats just above `anchor`, usually the text field it belongs to, like a
/// tooltip. Draw it every frame while it should be visible.
#[must_use = "call `.show(ctx)` to draw the list"]
pub struct Suggestions<'a> {
    id: Id,
    items: &'a [Suggestion],
    anchor: Rect,
}

impl<'a> Suggestions<'a> {
    /// A list of `items` above `anchor`. `id_salt` must be unique among areas.
    pub fn new(id_salt: impl Hash, anchor: Rect, items: &'a [Suggestion]) -> Self {
        Self {
            id: Id::new(id_salt),
            items,
            anchor,
        }
    }

    /// Draw the list. Does nothing when there are no items.
    pub fn show(self, ctx: &Context) -> SuggestionsOutput {
        if self.items.is_empty() {
            return SuggestionsOutput::default();
        }
        let t = ctx.ir_theme();
        let width = self.anchor.width().min(MAX_WIDTH);
        let frame = Frame::new()
            .fill(t.popover)
            .stroke(Stroke::new(BORDER_WIDTH, t.border))
            .corner_radius(t.corner_lg())
            .inner_margin(Margin::same(spacing::XS as i8))
            .shadow(Shadow {
                offset: [0, SHADOW_OFFSET],
                blur: SHADOW_BLUR,
                spread: 0,
                color: t.scrim,
            });
        let mut picked = None;
        let area = Area::new(self.id)
            .order(Order::Foreground)
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(self.anchor.left_top() - Vec2::new(0.0, spacing::XS))
            .show(ctx, |ui| {
                frame.show(ui, |ui| {
                    ui.set_width(width - 2.0 * spacing::XS);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (index, item) in self.items.iter().enumerate() {
                        if row(ui, item) {
                            picked = Some(index);
                        }
                    }
                });
            });
        let hovered = ctx
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|pos| area.response.rect.contains(pos));
        SuggestionsOutput { picked, hovered }
    }
}

/// Draw one row. Returns whether it was clicked.
fn row(ui: &mut egui::Ui, item: &Suggestion) -> bool {
    let t = ui.ir_theme();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &item.title));
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, t.corner_md(), t.accent);
    }
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + spacing::SM + ICON_SIZE / 2.0, rect.center().y),
        Vec2::splat(ICON_SIZE),
    );
    icon::paint(ui.painter(), icon_rect, t.foreground, item.icon);

    let text_x = icon_rect.right() + spacing::SM;
    let title = ui.painter().layout_no_wrap(
        item.title.clone(),
        FontId::proportional(t.text_sm),
        Color32::PLACEHOLDER,
    );
    ui.painter().galley(
        Pos2::new(text_x, rect.center().y - title.size().y / 2.0),
        title.clone(),
        t.foreground,
    );
    let description_x =
        (text_x + title.size().x + spacing::MD).max(rect.left() + rect.width() * TITLE_SHARE);
    let mut job = egui::text::LayoutJob::simple(
        item.description.clone(),
        FontId::proportional(t.text_xs),
        Color32::PLACEHOLDER,
        (rect.right() - description_x - spacing::SM).max(0.0),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let description = ui.painter().layout_job(job);
    ui.painter().galley(
        Pos2::new(description_x, rect.center().y - description.size().y / 2.0),
        description,
        t.muted_foreground,
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    fn items() -> Vec<Suggestion> {
        vec![
            Suggestion {
                icon: IconKind::Branch,
                title: "/branch [label]".into(),
                description: "Fork the story here".into(),
            },
            Suggestion {
                icon: IconKind::List,
                title: "/branches".into(),
                description: "List every branch".into(),
            },
        ]
    }

    fn anchor() -> Rect {
        Rect::from_min_size(Pos2::new(40.0, 400.0), Vec2::new(500.0, 60.0))
    }

    #[test]
    fn nothing_is_drawn_without_items() {
        let mut h = Harness::new();
        let mut out = SuggestionsOutput {
            picked: Some(0),
            hovered: true,
        };
        h.ctx_frame(vec![], &mut |ctx| {
            out = Suggestions::new("s", anchor(), &[]).show(ctx);
        });
        assert_eq!(out, SuggestionsOutput::default());
    }

    #[test]
    fn the_list_floats_above_the_anchor() {
        let mut h = Harness::new();
        let list = items();
        for _ in 0..2 {
            h.ctx_frame(vec![], &mut |ctx| {
                Suggestions::new("s", anchor(), &list).show(ctx);
            });
        }
        let id = Id::new("s");
        let rect = h.ctx.memory(|m| m.area_rect(id)).expect("the area exists");
        assert!(
            rect.bottom() <= anchor().top(),
            "{rect:?} above {:?}",
            anchor()
        );
        assert!(rect.height() >= 2.0 * ROW_HEIGHT);
    }

    #[test]
    fn clicking_a_row_picks_it_and_hovering_is_reported() {
        let mut h = Harness::new();
        let list = items();
        let mut out = SuggestionsOutput::default();
        for _ in 0..2 {
            h.ctx_frame(vec![], &mut |ctx| {
                out = Suggestions::new("s", anchor(), &list).show(ctx);
            });
        }
        let rect = h.ctx.memory(|m| m.area_rect(Id::new("s"))).unwrap();
        // The last row sits at the bottom of the list.
        let target = Pos2::new(
            rect.center().x,
            rect.bottom() - ROW_HEIGHT / 2.0 - spacing::XS,
        );

        h.ctx_frame(vec![egui::Event::PointerMoved(target)], &mut |ctx| {
            out = Suggestions::new("s", anchor(), &list).show(ctx);
        });
        assert!(out.hovered && out.picked.is_none());

        let press = |pressed| egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut picked = None;
        for event in [press(true), press(false)] {
            h.ctx_frame(vec![event], &mut |ctx| {
                out = Suggestions::new("s", anchor(), &list).show(ctx);
                picked = picked.or(out.picked);
            });
        }
        assert_eq!(picked, Some(1));
    }
}
