//! Popover: a floating panel opened from a trigger.

use egui::AsIdSalt;

use egui::{Id, Popup, PopupCloseBehavior, Response, Ui};

/// Default minimum width of the panel, in logical pixels.
const MIN_WIDTH: f32 = 240.0;

/// A panel that opens below a trigger widget when it is clicked, and closes on a click
/// outside it or on Escape.
///
/// Draw it every frame, right after the trigger:
///
/// ```text
/// let trigger = ui.add(Button::new("Options"));
/// Popover::new("options").show(ui, &trigger, |ui| { /* content */ });
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Popover {
    id: Id,
    min_width: f32,
}

impl Popover {
    /// A popover identified by `id_salt`, unique within the current UI.
    pub fn new(id_salt: impl AsIdSalt) -> Self {
        Self {
            id: Id::unique(id_salt),
            min_width: MIN_WIDTH,
        }
    }

    /// Minimum width of the panel, in logical pixels.
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    /// Toggle on a click of `trigger` and draw the panel while open. Returns what `content`
    /// returned, or `None` while closed.
    pub fn show<R>(
        self,
        ui: &Ui,
        trigger: &Response,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> Option<R> {
        let popup_id = ui.make_persistent_id(self.id);
        if trigger.clicked() {
            Popup::toggle_id(ui.ctx(), popup_id);
        }
        Popup::from_response(trigger)
            .id(popup_id)
            .open_memory(None)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(self.min_width);
                content(ui)
            })
            .map(|shown| shown.inner)
    }

    /// Whether the panel is currently open.
    pub fn is_open(self, ui: &Ui) -> bool {
        Popup::is_id_open(ui.ctx(), ui.make_persistent_id(self.id))
    }
}

#[cfg(test)]
mod tests {
    use egui::{Pos2, Sense};

    use super::*;
    use crate::components::testing::Harness;

    /// A trigger area plus the popover, counting how often the content ran.
    fn draw(ui: &mut Ui, ran: &mut u32, trigger_rect: &mut egui::Rect) -> bool {
        let (rect, trigger) = ui.allocate_exact_size(egui::vec2(100.0, 32.0), Sense::click());
        *trigger_rect = rect;
        let popover = Popover::new("menu");
        popover.show(ui, &trigger, |ui| {
            *ran += 1;
            ui.label("content");
        });
        popover.is_open(ui)
    }

    #[test]
    fn clicking_the_trigger_opens_and_clicking_outside_closes() {
        let mut h = Harness::new();
        let (mut ran, mut rect, mut open) = (0, egui::Rect::NOTHING, false);
        h.idle(&mut |ui: &mut Ui| open = draw(ui, &mut ran, &mut rect));
        assert!(!open);
        assert_eq!(ran, 0, "closed popovers draw nothing");

        h.click(rect.center(), &mut |ui: &mut Ui| {
            open = draw(ui, &mut ran, &mut rect)
        });
        h.idle(&mut |ui: &mut Ui| open = draw(ui, &mut ran, &mut rect));
        assert!(open);
        assert!(ran > 0);

        h.click(Pos2::new(700.0, 500.0), &mut |ui: &mut Ui| {
            open = draw(ui, &mut ran, &mut rect)
        });
        h.idle(&mut |ui: &mut Ui| open = draw(ui, &mut ran, &mut rect));
        assert!(!open);
    }

    #[test]
    fn clicking_the_trigger_again_closes_it() {
        let mut h = Harness::new();
        let (mut ran, mut rect, mut open) = (0, egui::Rect::NOTHING, false);
        h.idle(&mut |ui: &mut Ui| open = draw(ui, &mut ran, &mut rect));
        for expected in [true, false] {
            h.click(rect.center(), &mut |ui: &mut Ui| {
                open = draw(ui, &mut ran, &mut rect)
            });
            h.idle(&mut |ui: &mut Ui| open = draw(ui, &mut ran, &mut rect));
            assert_eq!(open, expected);
        }
    }
}
