//! A row in a list, with a menu behind a three-dots button.

use egui::AsIdSalt;

use egui::text::LayoutJob;
use egui::{Color32, CornerRadius, CursorIcon, FontId, Id, Pos2, Rect, Sense, Ui, Vec2};

use super::common::transition;
use super::icon::{self, IconKind};
use super::popover::Popover;
use crate::theme::{spacing, ThemeExt};

const HEIGHT: f32 = 36.0;
const DOTS_SIZE: f32 = 28.0;
const ICON_SIZE: f32 = 16.0;

/// Width of the menu that opens from the dots.
const MENU_WIDTH: f32 = 160.0;

/// What [`ListItem::show`] reports.
#[derive(Debug)]
pub struct ListItemOutput<R> {
    /// The row itself was clicked.
    pub clicked: bool,
    /// What the menu returned this frame, or `None` while it is closed.
    pub menu: Option<R>,
}

/// A full-width row: a label, and at its right edge, inside the row, a three-dots button
/// that opens a menu. The dots show while the row is hovered, selected, or its menu is open.
#[must_use = "call `.show(ui, ...)` to draw the row"]
pub struct ListItem<'a> {
    label: &'a str,
    selected: bool,
    menu: bool,
    id: Id,
}

impl<'a> ListItem<'a> {
    /// A row with this label. `id_salt` must be unique among the rows of a list.
    pub fn new(id_salt: impl AsIdSalt, label: &'a str) -> Self {
        Self {
            label,
            selected: false,
            menu: true,
            id: Id::unique(id_salt),
        }
    }

    /// Whether this is the row being viewed or edited.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Show the three-dots menu (the default). A row without it only reacts to clicks.
    pub fn menu(mut self, menu: bool) -> Self {
        self.menu = menu;
        self
    }

    /// Draw the row. `menu` fills the menu opened by the dots; add [`MenuItem`]s to it.
    ///
    /// [`MenuItem`]: super::MenuItem
    pub fn show<R>(self, ui: &mut Ui, menu: impl FnOnce(&mut Ui) -> R) -> ListItemOutput<R> {
        let t = ui.ir_theme();
        let (rect, row) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), HEIGHT), Sense::click());
        row.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, self.label));

        let dots_rect = Rect::from_center_size(
            Pos2::new(
                rect.right() - spacing::XS - DOTS_SIZE / 2.0,
                rect.center().y,
            ),
            Vec2::splat(DOTS_SIZE),
        );
        // Added after the row, so it is on top and takes the clicks inside its rect.
        let dots = ui.interact(dots_rect, self.id.with("dots"), Sense::click());
        let popover = Popover::new(self.id.with("menu")).width(MENU_WIDTH);
        let menu_open = self.menu && popover.is_open(ui);

        let hovered = row.hovered() || (self.menu && dots.hovered());
        let pill = CornerRadius::same(u8::MAX);
        let row_fill = if self.selected {
            t.secondary
        } else if hovered || menu_open {
            t.muted
        } else {
            Color32::TRANSPARENT
        };
        let row_fill = transition(ui, self.id.with("fill"), row_fill);
        ui.painter().rect_filled(rect, t.corner_lg(), row_fill);

        let show_dots = self.menu && (hovered || self.selected || menu_open);
        let label_right = if show_dots {
            dots_rect.left() - spacing::XS
        } else {
            rect.right() - spacing::MD
        };
        // One row, cut at the edge of the label area instead of wrapping.
        let mut job = LayoutJob::simple(
            self.label.to_string(),
            FontId::proportional(t.text_sm),
            Color32::PLACEHOLDER,
            (label_right - rect.left() - spacing::MD).max(0.0),
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.painter().layout_job(job);
        if galley.elided {
            row.clone().on_hover_text(self.label);
        }
        let text_color = if self.selected {
            t.secondary_foreground
        } else {
            t.foreground
        };
        ui.painter().galley(
            Pos2::new(
                rect.left() + spacing::MD,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            text_color,
        );

        if show_dots {
            let dots_fill = if dots.hovered() || menu_open {
                t.accent
            } else {
                Color32::TRANSPARENT
            };
            let dots_fill = transition(ui, self.id.with("dots-fill"), dots_fill);
            ui.painter().rect_filled(dots_rect, pill, dots_fill);
            icon::paint(
                ui.painter(),
                Rect::from_center_size(dots_rect.center(), Vec2::splat(ICON_SIZE)),
                t.foreground,
                IconKind::More,
            );
        }
        if (self.menu && dots.hovered()) || row.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }

        let menu = if self.menu {
            popover.show(ui, &dots, menu)
        } else {
            None
        };
        ListItemOutput {
            clicked: row.clicked() && !(self.menu && dots.clicked()),
            menu,
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::Pos2;

    use super::*;
    use crate::components::testing::Harness;
    use crate::components::MenuItem;

    /// Draw one row and report `(clicked, delete chosen, menu open)`.
    fn draw(ui: &mut Ui, rect: &mut Rect) -> (bool, bool, bool) {
        let mut delete = false;
        let out = ListItem::new("row", "Mira Valen").show(ui, |ui| {
            delete = ui
                .add(
                    MenuItem::new("Delete")
                        .icon(IconKind::Trash)
                        .destructive(true),
                )
                .clicked();
        });
        *rect = Rect::from_min_size(ui.min_rect().min, Vec2::new(ui.available_width(), HEIGHT));
        (out.clicked, delete, out.menu.is_some())
    }

    #[test]
    fn clicking_the_row_selects_it_without_opening_the_menu() {
        let mut h = Harness::new();
        let mut rect = Rect::NOTHING;
        h.idle(&mut |ui| {
            draw(ui, &mut rect);
        });
        let target = Pos2::new(rect.left() + 20.0, rect.center().y);
        let (mut clicked, mut open) = (false, false);
        h.click(target, &mut |ui| {
            let (c, _, o) = draw(ui, &mut rect);
            clicked |= c;
            open |= o;
        });
        assert!(clicked);
        assert!(!open);
    }

    #[test]
    fn the_dots_open_a_menu_and_choosing_delete_reports_it() {
        let mut h = Harness::new();
        let mut rect = Rect::NOTHING;
        h.idle(&mut |ui| {
            draw(ui, &mut rect);
        });
        let dots = Pos2::new(
            rect.right() - spacing::XS - DOTS_SIZE / 2.0,
            rect.center().y,
        );
        let (mut row_clicked, mut open) = (false, false);
        h.click(dots, &mut |ui| {
            let (c, _, _) = draw(ui, &mut rect);
            row_clicked |= c;
        });
        h.idle(&mut |ui| {
            let (_, _, o) = draw(ui, &mut rect);
            open = o;
        });
        assert!(open, "the dots open the menu");
        assert!(!row_clicked, "the dots do not count as clicking the row");
    }

    #[test]
    fn a_long_label_does_not_break_the_row_height() {
        let mut h = Harness::new();
        let mut height = 0.0;
        h.idle(&mut |ui| {
            let before = ui.cursor().top();
            ListItem::new(
                "long",
                "A story whose name is far too long to fit in a narrow sidebar column",
            )
            .show(ui, |_| {});
            height = ui.cursor().top() - before - ui.spacing().item_spacing.y;
        });
        assert!((height - HEIGHT).abs() < 1.0, "{height}");
    }
}
