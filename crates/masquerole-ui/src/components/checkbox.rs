//! Checkbox.

use egui::{
    Color32, CursorIcon, FontId, Pos2, Rect, Response, Role, Sense, Shape, Stroke, StrokeKind, Ui,
    Widget, WidgetInfo,
};

use super::common::{dim, focus_ring};
use crate::theme::{spacing, ThemeExt, BORDER_WIDTH, RING_WIDTH};

/// Side length of the box.
const BOX_SIZE: f32 = 16.0;

/// A checkbox with an optional label. Space or Enter toggles it when focused.
///
/// Check `.changed()` on the returned response to react to a toggle.
pub struct Checkbox<'a> {
    checked: &'a mut bool,
    label: String,
    enabled: bool,
}

impl<'a> Checkbox<'a> {
    /// A checkbox bound to `checked`, with a label.
    pub fn new(checked: &'a mut bool, label: impl Into<String>) -> Self {
        Self {
            checked,
            label: label.into(),
            enabled: true,
        }
    }

    /// Enable or disable the checkbox.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for Checkbox<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let enabled = self.enabled && ui.is_enabled();
        let galley = (!self.label.is_empty()).then(|| {
            ui.painter().layout_no_wrap(
                self.label.clone(),
                FontId::proportional(t.text_sm),
                Color32::PLACEHOLDER,
            )
        });

        let label_size = galley.as_ref().map_or(egui::Vec2::ZERO, |g| {
            egui::vec2(spacing::SM + g.size().x, g.size().y)
        });
        let size = egui::vec2(BOX_SIZE + label_size.x, BOX_SIZE.max(label_size.y));
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, mut response) = ui.allocate_exact_size(size, sense);
        if response.clicked() {
            *self.checked = !*self.checked;
            response.mark_changed();
        }
        let checked = *self.checked;
        response
            .widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, checked, &self.label));

        let box_rect = Rect::from_min_size(
            Pos2::new(rect.left(), rect.center().y - BOX_SIZE / 2.0),
            egui::vec2(BOX_SIZE, BOX_SIZE),
        );
        let hovered = enabled && response.hovered();
        let (fill, border) = if checked {
            (t.primary, t.primary)
        } else if hovered {
            (t.background, t.muted_foreground)
        } else {
            (t.background, t.input)
        };
        let painter = ui.painter();
        painter.rect(
            box_rect,
            t.corner_sm(),
            dim(fill, enabled),
            Stroke::new(BORDER_WIDTH, dim(border, enabled)),
            StrokeKind::Inside,
        );
        if checked {
            let at = |x: f32, y: f32| box_rect.min + egui::vec2(x, y) * BOX_SIZE;
            painter.add(Shape::line(
                vec![at(0.24, 0.52), at(0.44, 0.72), at(0.78, 0.30)],
                Stroke::new(RING_WIDTH, dim(t.primary_foreground, enabled)),
            ));
        }
        if let Some(galley) = galley {
            let pos = Pos2::new(
                box_rect.right() + spacing::SM,
                rect.center().y - galley.size().y / 2.0,
            );
            painter.galley(pos, galley, dim(t.foreground, enabled));
        }
        if enabled && response.has_focus() {
            focus_ring(ui, box_rect, t.corner_sm(), &t);
        }

        if enabled {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    /// Draw the checkbox and return its response.
    fn show(h: &mut Harness, checked: &mut bool, enabled: bool) -> Response {
        let mut response = None;
        h.idle(&mut |ui: &mut Ui| {
            response = Some(ui.add(Checkbox::new(checked, "Stream").enabled(enabled)));
        });
        response.unwrap()
    }

    #[test]
    fn clicking_toggles_and_reports_a_change() {
        let mut h = Harness::new();
        let mut checked = false;
        let rect = show(&mut h, &mut checked, true).rect;

        let mut changed = false;
        h.click(rect.center(), &mut |ui: &mut Ui| {
            changed |= ui.add(Checkbox::new(&mut checked, "Stream")).changed();
        });
        assert!(checked && changed);

        h.click(rect.center(), &mut |ui: &mut Ui| {
            ui.add(Checkbox::new(&mut checked, "Stream"));
        });
        assert!(!checked);
    }

    #[test]
    fn clicking_the_label_toggles_too() {
        let mut h = Harness::new();
        let mut checked = false;
        let rect = show(&mut h, &mut checked, true).rect;
        h.click(
            rect.right_center() - egui::vec2(4.0, 0.0),
            &mut |ui: &mut Ui| {
                ui.add(Checkbox::new(&mut checked, "Stream"));
            },
        );
        assert!(checked);
    }

    #[test]
    fn a_disabled_checkbox_does_not_toggle() {
        let mut h = Harness::new();
        let mut checked = false;
        let rect = show(&mut h, &mut checked, false).rect;
        h.click(rect.center(), &mut |ui: &mut Ui| {
            ui.add(Checkbox::new(&mut checked, "Stream").enabled(false));
        });
        assert!(!checked);
    }

    #[test]
    fn space_toggles_a_focused_checkbox() {
        let mut h = Harness::new();
        let mut checked = false;
        let id = show(&mut h, &mut checked, true).id;
        h.ctx.memory_mut(|m| m.request_focus(id));
        h.key(egui::Key::Space, &mut |ui: &mut Ui| {
            ui.add(Checkbox::new(&mut checked, "Stream"));
        });
        assert!(checked);
    }

    #[test]
    fn works_without_a_label() {
        let mut h = Harness::new();
        let mut checked = true;
        h.idle(&mut |ui: &mut Ui| {
            ui.add(Checkbox::new(&mut checked, ""));
        });
    }
}
