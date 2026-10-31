//! The window buttons of an app that draws its own title bar: close, minimize, full screen.

use egui::Ui;

use super::button::{Button, ButtonSize, ButtonVariant};
use super::icon::IconKind;
use super::tooltip::TooltipExt;
use crate::theme::spacing;

/// What the user asked of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowAction {
    /// Minimize to the task bar.
    Minimize,
    /// Enter or leave full screen.
    ToggleFullscreen,
    /// Close the window.
    Close,
}

/// Three icon buttons in a row. The app applies the returned [`WindowAction`] to the window.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowControls {
    fullscreen: bool,
}

impl WindowControls {
    /// The controls. Pass whether the window is full screen now, so the last button shows
    /// the right icon.
    pub fn new(fullscreen: bool) -> Self {
        Self { fullscreen }
    }

    /// Draw the buttons left to right: close, minimize, full screen. Returns the action of
    /// the button that was clicked this frame.
    pub fn show(self, ui: &mut Ui) -> Option<WindowAction> {
        let mut action = None;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = spacing::XS;
            ui.horizontal(|ui| {
                let (fullscreen_icon, fullscreen_tip) = if self.fullscreen {
                    (IconKind::ExitFullscreen, "Leave full screen")
                } else {
                    (IconKind::Fullscreen, "Full screen")
                };
                let buttons = [
                    (IconKind::Close, "Close", WindowAction::Close),
                    (IconKind::Minimize, "Minimize", WindowAction::Minimize),
                    (
                        fullscreen_icon,
                        fullscreen_tip,
                        WindowAction::ToggleFullscreen,
                    ),
                ];
                for (icon, tip, this) in buttons {
                    let button = Button::new("")
                        .icon(icon)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Icon);
                    if ui.add(button).ir_tooltip(tip).clicked() {
                        action = Some(this);
                    }
                }
            });
        });
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;
    use egui::{Pos2, Rect};

    /// Click the `index`-th of the three buttons and return what was reported.
    fn click_nth(index: usize, fullscreen: bool) -> Option<WindowAction> {
        let mut h = Harness::new();
        let mut total = Rect::NOTHING;
        h.idle(&mut |ui| {
            total = ui
                .scope(|ui| WindowControls::new(fullscreen).show(ui))
                .response
                .rect;
        });
        let third = total.width() / 3.0;
        let pos = Pos2::new(total.min.x + third * (index as f32 + 0.5), total.center().y);
        let mut result = None;
        h.click(pos, &mut |ui| {
            if let Some(action) = WindowControls::new(fullscreen).show(ui) {
                result = Some(action);
            }
        });
        result
    }

    #[test]
    fn each_button_reports_its_action() {
        assert_eq!(click_nth(0, false), Some(WindowAction::Close));
        assert_eq!(click_nth(1, false), Some(WindowAction::Minimize));
        assert_eq!(click_nth(2, false), Some(WindowAction::ToggleFullscreen));
        assert_eq!(click_nth(2, true), Some(WindowAction::ToggleFullscreen));
    }

    #[test]
    fn nothing_is_reported_without_a_click() {
        let mut h = Harness::new();
        let mut seen = Some(WindowAction::Close);
        h.idle(&mut |ui| seen = WindowControls::new(false).show(ui));
        assert_eq!(seen, None);
    }
}
