//! Modal dialog.

use std::hash::Hash;

use egui::{Align, Context, Frame, Id, Layout, Margin, Modal, RichText, Stroke, Ui};

use crate::theme::{spacing, ThemeExt, BORDER_WIDTH};

/// Default width of the dialog body, in logical pixels.
const DIALOG_WIDTH: f32 = 420.0;

/// A modal dialog over a dimmed backdrop. It closes on Escape or a click outside it.
///
/// Draw it every frame while `open` is true; put the dialog's buttons in the content with
/// [`Dialog::actions`].
#[derive(Clone, Debug)]
pub struct Dialog {
    id: Id,
    title: String,
    description: Option<String>,
    width: f32,
}

impl Dialog {
    /// A dialog with this title. `id_salt` must be unique among dialogs.
    pub fn new(id_salt: impl Hash, title: impl Into<String>) -> Self {
        Self {
            id: Id::new(id_salt),
            title: title.into(),
            description: None,
            width: DIALOG_WIDTH,
        }
    }

    /// The width of the dialog body, in logical pixels. For dialogs holding a form.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Explanatory text below the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Show the dialog if `*open` is true, and clear `*open` when the user dismisses it
    /// with Escape or a click outside. Returns what `content` returned, or `None` when
    /// closed.
    pub fn show<R>(
        self,
        ctx: &Context,
        open: &mut bool,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> Option<R> {
        if !*open {
            return None;
        }
        let t = ctx.ir_theme();
        let frame = Frame::new()
            .fill(t.popover)
            .stroke(Stroke::new(BORDER_WIDTH, t.border))
            .corner_radius(t.corner_lg())
            .inner_margin(Margin::same(spacing::XL as i8));
        let modal = Modal::new(self.id)
            .frame(frame)
            .backdrop_color(t.scrim)
            .show(ctx, |ui| {
                ui.set_width(self.width);
                ui.spacing_mut().item_spacing.y = spacing::SM;
                ui.label(
                    RichText::new(&self.title)
                        .size(t.text_lg)
                        .strong()
                        .color(t.foreground),
                );
                if let Some(description) = &self.description {
                    ui.label(
                        RichText::new(description)
                            .size(t.text_sm)
                            .color(t.muted_foreground),
                    );
                }
                ui.add_space(spacing::SM);
                content(ui)
            });
        if modal.should_close() {
            *open = false;
        }
        Some(modal.inner)
    }

    /// A right-aligned row for the dialog's buttons. Buttons are added right to left, so
    /// add the primary action first.
    pub fn actions(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
        ui.add_space(spacing::SM);
        ui.with_layout(Layout::right_to_left(Align::Center), add);
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Key, Modifiers};

    use super::*;
    use crate::components::testing::Harness;

    fn escape(pressed: bool) -> Event {
        Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn nothing_is_drawn_while_closed() {
        let mut h = Harness::new();
        let mut open = false;
        let mut ran = 0;
        let mut result = Some(0);
        h.ctx_frame(vec![], &mut |ctx| {
            result = Dialog::new("d", "Title").show(ctx, &mut open, |_| {
                ran += 1;
                1
            });
        });
        assert_eq!((result, ran), (None, 0));
    }

    #[test]
    fn content_runs_while_open_and_returns_its_value() {
        let mut h = Harness::new();
        let mut open = true;
        let mut value = None;
        h.ctx_frame(vec![], &mut |ctx| {
            value =
                Dialog::new("d", "Title")
                    .description("Are you sure?")
                    .show(ctx, &mut open, |ui| {
                        Dialog::actions(ui, |ui| {
                            ui.label("buttons");
                        });
                        7
                    });
        });
        assert_eq!(value, Some(7));
        assert!(open, "nothing dismissed it");
    }

    #[test]
    fn escape_closes_the_dialog() {
        let mut h = Harness::new();
        let mut open = true;
        let draw = |h: &mut Harness, open: &mut bool, events: Vec<Event>| {
            h.ctx_frame(events, &mut |ctx| {
                Dialog::new("d", "Title").show(ctx, open, |ui| {
                    ui.label("body");
                });
            });
        };
        draw(&mut h, &mut open, vec![]);
        assert!(open);
        draw(&mut h, &mut open, vec![escape(true)]);
        draw(&mut h, &mut open, vec![escape(false)]);
        assert!(!open);
    }
}
