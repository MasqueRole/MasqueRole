//! Multi-line text input.

use egui::{Id, Margin, Response, RichText, ScrollArea, TextEdit, Ui, Widget};

use super::input::text_field;
use crate::theme::{spacing, ThemeExt};

/// A multi-line text field bound to a `String`.
pub struct Textarea<'a> {
    text: &'a mut String,
    placeholder: String,
    rows: usize,
    enabled: bool,
    id_salt: Option<Id>,
    bare: bool,
    max_height: Option<f32>,
}

impl<'a> Textarea<'a> {
    /// A textarea editing `text`, four rows tall.
    pub fn new(text: &'a mut String) -> Self {
        Self {
            text,
            placeholder: String::new(),
            rows: 4,
            enabled: true,
            id_salt: None,
            bare: false,
            max_height: None,
        }
    }

    /// Hint text shown while the field is empty.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Minimum height in text rows.
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows.max(1);
        self
    }

    /// Enable or disable editing.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Draw no background and no border, only padding, so the field blends into whatever it
    /// sits on.
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    /// The most height the field takes; beyond it the text scrolls. Only for a
    /// [`bare`](Self::bare) field.
    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = Some(height);
        self
    }

    /// Give the field a stable identity.
    pub fn id_salt(mut self, id_salt: impl egui::AsIdSalt) -> Self {
        self.id_salt = Some(Id::unique(id_salt));
        self
    }
}

impl Widget for Textarea<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let hint = RichText::new(self.placeholder).color(t.muted_foreground);
        let (text, rows, id_salt) = (self.text, self.rows, self.id_salt);
        if self.bare {
            let mut edit = TextEdit::multiline(text)
                .interactive(ui.is_enabled())
                .frame(egui::Frame::NONE)
                .margin(Margin::symmetric(spacing::SM as i8, spacing::SM as i8))
                .hint_text(hint)
                .desired_rows(rows)
                .desired_width(ui.available_width());
            if let Some(id) = id_salt {
                edit = edit.id_salt(id);
            }
            let Some(max_height) = self.max_height else {
                return ui.add(edit);
            };
            return ScrollArea::vertical()
                .max_height(max_height)
                .auto_shrink([false, true])
                .show(ui, |ui| ui.add(edit))
                .inner;
        }
        text_field(ui, self.enabled, |ui| {
            let mut edit = TextEdit::multiline(text)
                .interactive(ui.is_enabled())
                .frame(egui::Frame::NONE)
                .hint_text(hint)
                .desired_rows(rows)
                .desired_width(ui.available_width());
            if let Some(id) = id_salt {
                edit = edit.id_salt(id);
            }
            ui.add(edit)
        })
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Key};

    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn enter_adds_a_new_line() {
        let mut h = Harness::new();
        let mut text = String::from("one");
        let mut id = None;
        h.idle(&mut |ui: &mut Ui| {
            id = Some(ui.add(Textarea::new(&mut text)).id);
        });
        h.ctx.memory_mut(|m| m.request_focus(id.unwrap()));
        h.idle(&mut |ui: &mut Ui| {
            ui.add(Textarea::new(&mut text));
        });
        h.key(Key::Enter, &mut |ui: &mut Ui| {
            ui.add(Textarea::new(&mut text));
        });
        h.frame(vec![Event::Text("two".into())], &mut |ui: &mut Ui| {
            ui.add(Textarea::new(&mut text));
        });
        assert_eq!(text, "one\ntwo");
    }

    #[test]
    fn a_bare_field_edits_and_scrolls_past_its_maximum_height() {
        let mut h = Harness::new();
        let mut text = "line
"
        .repeat(40);
        let mut rect = egui::Rect::NOTHING;
        h.idle(&mut |ui: &mut Ui| {
            rect = ui
                .add(Textarea::new(&mut text).bare().rows(1).max_height(120.0))
                .rect;
        });
        assert!(rect.height() > 120.0, "the edit itself is taller: {rect:?}");
        let mut outer = egui::Rect::NOTHING;
        h.idle(&mut |ui: &mut Ui| {
            outer = ui
                .scope(|ui| {
                    ui.add(Textarea::new(&mut text).bare().rows(1).max_height(120.0));
                })
                .response
                .rect;
        });
        assert!(outer.height() <= 121.0, "{outer:?}");
    }

    #[test]
    fn rows_never_drop_below_one() {
        let mut text = String::new();
        assert_eq!(Textarea::new(&mut text).rows(0).rows, 1);
    }

    #[test]
    fn renders_disabled_with_placeholder() {
        let mut h = Harness::new();
        let mut text = String::new();
        h.idle(&mut |ui: &mut Ui| {
            ui.add(
                Textarea::new(&mut text)
                    .placeholder("notes")
                    .enabled(false)
                    .rows(2),
            );
        });
    }
}
