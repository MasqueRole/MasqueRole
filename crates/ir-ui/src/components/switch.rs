//! On/off switch.

use egui::{
    Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Response, Sense, Ui, Widget, WidgetInfo,
    WidgetType,
};

use super::common::{dim, focus_ring};
use crate::theme::{spacing, Theme, ThemeExt};

const TRACK_WIDTH: f32 = 36.0;
const TRACK_HEIGHT: f32 = 20.0;
/// Gap between the thumb and the track edge.
const THUMB_INSET: f32 = 2.0;

/// A switch with an optional label. Space or Enter toggles it when focused.
///
/// Check `.changed()` on the returned response to react to a toggle.
pub struct Switch<'a> {
    on: &'a mut bool,
    label: String,
    enabled: bool,
}

impl<'a> Switch<'a> {
    /// A switch bound to `on`.
    pub fn new(on: &'a mut bool) -> Self {
        Self {
            on,
            label: String::new(),
            enabled: true,
        }
    }

    /// A label shown to the right of the switch.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Enable or disable the switch.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for Switch<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t: Theme = ui.ir_theme();
        let enabled = self.enabled && ui.is_enabled();
        let galley = (!self.label.is_empty()).then(|| {
            ui.painter().layout_no_wrap(
                self.label.clone(),
                FontId::proportional(t.text_sm),
                Color32::PLACEHOLDER,
            )
        });

        let label_width = galley.as_ref().map_or(0.0, |g| spacing::SM + g.size().x);
        let height = TRACK_HEIGHT.max(galley.as_ref().map_or(0.0, |g| g.size().y));
        let size = egui::vec2(TRACK_WIDTH + label_width, height);
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, mut response) = ui.allocate_exact_size(size, sense);
        if response.clicked() {
            *self.on = !*self.on;
            response.mark_changed();
        }
        let on = *self.on;
        response
            .widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, on, &self.label));

        // 0.0 is off, 1.0 is on; eased between toggles.
        let progress = ui.ctx().animate_bool_responsive(response.id, on);
        let track = Rect::from_min_size(
            Pos2::new(rect.left(), rect.center().y - TRACK_HEIGHT / 2.0),
            egui::vec2(TRACK_WIDTH, TRACK_HEIGHT),
        );
        let pill = CornerRadius::same(u8::MAX);
        let track_color = Theme::mix(t.input, t.primary, progress);
        let thumb_radius = TRACK_HEIGHT / 2.0 - THUMB_INSET;
        let travel = TRACK_WIDTH - TRACK_HEIGHT;
        let thumb_center = Pos2::new(
            track.left() + TRACK_HEIGHT / 2.0 + travel * progress,
            track.center().y,
        );

        let painter = ui.painter();
        painter.rect_filled(track, pill, dim(track_color, enabled));
        painter.circle_filled(thumb_center, thumb_radius, dim(t.background, enabled));
        if let Some(galley) = galley {
            let pos = Pos2::new(
                track.right() + spacing::SM,
                rect.center().y - galley.size().y / 2.0,
            );
            painter.galley(pos, galley, dim(t.foreground, enabled));
        }
        if enabled && response.has_focus() {
            focus_ring(ui, track, pill, &t);
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

    fn show(h: &mut Harness, on: &mut bool, enabled: bool) -> Response {
        let mut response = None;
        h.idle(&mut |ui: &mut Ui| {
            response = Some(ui.add(Switch::new(on).label("Narrator").enabled(enabled)));
        });
        response.unwrap()
    }

    #[test]
    fn clicking_toggles_and_reports_a_change() {
        let mut h = Harness::new();
        let mut on = false;
        let rect = show(&mut h, &mut on, true).rect;

        let mut changed = false;
        h.click(
            rect.left_center() + egui::vec2(10.0, 0.0),
            &mut |ui: &mut Ui| {
                changed |= ui.add(Switch::new(&mut on).label("Narrator")).changed();
            },
        );
        assert!(on && changed);
        h.click(
            rect.left_center() + egui::vec2(10.0, 0.0),
            &mut |ui: &mut Ui| {
                ui.add(Switch::new(&mut on).label("Narrator"));
            },
        );
        assert!(!on);
    }

    #[test]
    fn a_disabled_switch_does_not_toggle() {
        let mut h = Harness::new();
        let mut on = false;
        let rect = show(&mut h, &mut on, false).rect;
        h.click(
            rect.left_center() + egui::vec2(10.0, 0.0),
            &mut |ui: &mut Ui| {
                ui.add(Switch::new(&mut on).enabled(false));
            },
        );
        assert!(!on);
    }

    #[test]
    fn enter_toggles_a_focused_switch() {
        let mut h = Harness::new();
        let mut on = false;
        let id = show(&mut h, &mut on, true).id;
        h.ctx.memory_mut(|m| m.request_focus(id));
        h.key(egui::Key::Enter, &mut |ui: &mut Ui| {
            ui.add(Switch::new(&mut on).label("Narrator"));
        });
        assert!(on);
    }

    #[test]
    fn renders_without_a_label() {
        let mut h = Harness::new();
        let mut on = true;
        h.idle(&mut |ui: &mut Ui| {
            ui.add(Switch::new(&mut on));
        });
    }
}
