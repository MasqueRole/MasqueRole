//! Multi-line text input.

use egui::{Id, Response, RichText, TextEdit, Ui, Widget};

use super::input::text_field;
use crate::theme::ThemeExt;

/// A multi-line text field bound to a `String`.
pub struct Textarea<'a> {
    text: &'a mut String,
    placeholder: String,
    rows: usize,
    enabled: bool,
    id_salt: Option<Id>,
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

    /// Give the field a stable identity.
    pub fn id_salt(mut self, id_salt: impl std::hash::Hash) -> Self {
        self.id_salt = Some(Id::new(id_salt));
        self
    }
}

impl Widget for Textarea<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let hint = RichText::new(self.placeholder).color(t.muted_foreground);
        let (text, rows, id_salt) = (self.text, self.rows, self.id_salt);
        text_field(ui, self.enabled, |ui| {
            let mut edit = TextEdit::multiline(text)
                .frame(false)
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
