//! The icon rail: a narrow column of icon-only navigation buttons.

use egui::{Color32, Response, Sense, Ui, Vec2, Widget};

use super::common::transition;
use super::icon::{self, IconKind};
use crate::theme::{spacing, ThemeExt, RING_WIDTH};

/// Side of the square button.
const BUTTON_SIZE: f32 = 40.0;

/// Side of the icon inside the button.
const ICON_SIZE: f32 = 20.0;

/// Width a rail panel needs for its buttons and margins. Use it as the width of
/// [`Panel::rail`](super::Panel::rail).
pub const RAIL_WIDTH: f32 = BUTTON_SIZE + 2.0 * spacing::SM;

/// One button of the rail. Put a [`TooltipExt::ir_tooltip`](super::TooltipExt::ir_tooltip)
/// on it to name it, since the rail shows icons only.
#[must_use = "add the button with `ui.add(...)`"]
#[derive(Clone, Copy, Debug)]
pub struct RailButton {
    icon: IconKind,
    selected: bool,
    enabled: bool,
}

impl RailButton {
    /// A button showing `icon`.
    pub fn new(icon: IconKind) -> Self {
        Self {
            icon,
            selected: false,
            enabled: true,
        }
    }

    /// Mark the button as the current destination.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// A disabled button is drawn dimmed and does not react to hovering, but still reports
    /// clicks so a tooltip or a message can explain why it does nothing yet.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for RailButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(BUTTON_SIZE), Sense::click());
        let hovered = response.hovered() && self.enabled;
        let pressed = self.enabled && response.is_pointer_button_down_on();
        let scale = crate::motion::press_scale(ui.ctx(), response.id.with("press"), pressed);
        let rect = rect.scale_from_center(scale);
        let fill = if self.selected {
            t.accent
        } else if hovered {
            t.muted
        } else {
            Color32::TRANSPARENT
        };
        let fill = transition(ui, response.id.with("fill"), fill);
        ui.painter().rect_filled(rect, t.corner_md(), fill);
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                t.corner_md(),
                egui::Stroke::new(RING_WIDTH, t.ring),
                egui::StrokeKind::Inside,
            );
        }
        let color = if !self.enabled {
            t.muted_foreground.gamma_multiply(0.6)
        } else if self.selected || hovered {
            t.foreground
        } else {
            t.muted_foreground
        };
        let color = transition(ui, response.id.with("icon"), color);
        icon::paint(
            ui.painter(),
            egui::Rect::from_center_size(rect.center(), Vec2::splat(ICON_SIZE)),
            color,
            self.icon,
        );
        if self.enabled {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn buttons_are_square_and_fit_the_rail() {
        let mut h = Harness::new();
        let mut size = Vec2::ZERO;
        h.idle(&mut |ui| size = ui.add(RailButton::new(IconKind::Chat)).rect.size());
        assert_eq!(size, Vec2::splat(BUTTON_SIZE));
    }

    #[test]
    fn a_click_is_reported_even_when_disabled() {
        let mut h = Harness::new();
        let mut rect = egui::Rect::NOTHING;
        h.idle(&mut |ui| rect = ui.add(RailButton::new(IconKind::Store).enabled(false)).rect);
        let mut clicked = false;
        h.click(rect.center(), &mut |ui| {
            clicked |= ui
                .add(RailButton::new(IconKind::Store).enabled(false))
                .clicked();
        });
        assert!(clicked);
    }

    #[test]
    fn selecting_a_button_changes_how_it_is_painted() {
        // A fresh context per call: the first frame draws the target color without fading.
        let painted = |selected: bool| {
            let ctx = egui::Context::default();
            crate::Theme::apply(&ctx);
            let output = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(RailButton::new(IconKind::Chat).selected(selected));
                });
            });
            format!("{:?}", output.shapes)
        };
        assert_ne!(painted(true), painted(false));
    }
}
