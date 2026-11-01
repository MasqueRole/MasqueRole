//! An entry in a menu.

use egui::{Color32, CursorIcon, FontId, Response, Sense, Ui, Vec2, Widget};

use super::common::transition;
use super::icon::{self, IconKind};
use crate::theme::{spacing, ThemeExt};

const HEIGHT: f32 = 32.0;
const ICON_SIZE: f32 = 16.0;

/// A row in a menu opened from a popover. Click it to choose it.
///
/// A destructive item is drawn in the destructive color, for actions such as deleting.
#[must_use = "add the item with `ui.add(...)` and check `.clicked()`"]
pub struct MenuItem {
    text: String,
    icon: Option<IconKind>,
    destructive: bool,
}

impl MenuItem {
    /// An item with this text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            icon: None,
            destructive: false,
        }
    }

    /// An icon before the text.
    pub fn icon(mut self, icon: IconKind) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Draw the item as a destructive action.
    pub fn destructive(mut self, destructive: bool) -> Self {
        self.destructive = destructive;
        self
    }
}

impl Widget for MenuItem {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), HEIGHT), Sense::click());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &self.text));
        let accent = if self.destructive {
            t.destructive
        } else {
            t.foreground
        };
        let fill = if !(response.hovered() || response.has_focus()) {
            Color32::TRANSPARENT
        } else if self.destructive {
            accent.gamma_multiply(0.15)
        } else {
            t.accent
        };
        let fill = transition(ui, response.id.with("fill"), fill);
        ui.painter().rect_filled(rect, t.corner_md(), fill);
        let mut x = rect.left() + spacing::SM;
        if let Some(kind) = self.icon {
            let icon_rect = egui::Rect::from_center_size(
                egui::pos2(x + ICON_SIZE / 2.0, rect.center().y),
                Vec2::splat(ICON_SIZE),
            );
            icon::paint(ui.painter(), icon_rect, accent, kind);
            x += ICON_SIZE + spacing::SM;
        }
        let galley = ui.painter().layout_no_wrap(
            self.text,
            FontId::proportional(t.text_sm),
            Color32::PLACEHOLDER,
        );
        let pos = egui::pos2(x, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(pos, galley, accent);
        if response.clicked() {
            // A chosen item ends the menu it is in.
            ui.memory_mut(|m| m.close_popup());
        }
        response.on_hover_cursor(CursorIcon::PointingHand)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn it_fills_the_width_and_reports_clicks() {
        let mut h = Harness::new();
        let mut rect = egui::Rect::NOTHING;
        h.idle(&mut |ui| {
            rect = ui
                .add(
                    MenuItem::new("Delete")
                        .icon(IconKind::Trash)
                        .destructive(true),
                )
                .rect;
        });
        assert!(rect.width() > 100.0 && (rect.height() - HEIGHT).abs() < 0.5);

        let mut clicked = false;
        h.click(rect.center(), &mut |ui| {
            clicked |= ui
                .add(
                    MenuItem::new("Delete")
                        .icon(IconKind::Trash)
                        .destructive(true),
                )
                .clicked();
        });
        assert!(clicked);
    }
}
