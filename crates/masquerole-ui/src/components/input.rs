//! Single-line text input.

use egui::{Frame, Id, Margin, Response, RichText, Stroke, TextEdit, Ui, Widget};

use super::common::focus_ring;
use crate::theme::{spacing, ThemeExt, BORDER_WIDTH};

/// A single-line text field bound to a `String`.
///
/// Check `.changed()` on the returned response to react to edits.
pub struct Input<'a> {
    text: &'a mut String,
    placeholder: String,
    password: bool,
    enabled: bool,
    id_salt: Option<Id>,
}

impl<'a> Input<'a> {
    /// An input editing `text`.
    pub fn new(text: &'a mut String) -> Self {
        Self {
            text,
            placeholder: String::new(),
            password: false,
            enabled: true,
            id_salt: None,
        }
    }

    /// Hint text shown while the field is empty.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Hide the characters, for secrets.
    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }

    /// Enable or disable editing.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Give the field a stable identity, needed when several fields share a position
    /// over time.
    pub fn id_salt(mut self, id_salt: impl egui::AsIdSalt) -> Self {
        self.id_salt = Some(Id::unique(id_salt));
        self
    }
}

impl Widget for Input<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let hint = RichText::new(self.placeholder).color(t.muted_foreground);
        let (text, password, id_salt) = (self.text, self.password, self.id_salt);
        text_field(ui, self.enabled, |ui| {
            let mut edit = TextEdit::singleline(text)
                .interactive(ui.is_enabled())
                .frame(egui::Frame::NONE)
                .password(password)
                .hint_text(hint)
                .desired_width(ui.available_width());
            if let Some(id) = id_salt {
                edit = edit.id_salt(id);
            }
            ui.add(edit)
        })
    }
}

/// Draw the bordered frame shared by [`Input`] and `Textarea`, with `add` placing the
/// text edit inside it. Returns the text edit's response.
pub(crate) fn text_field(
    ui: &mut Ui,
    enabled: bool,
    add: impl FnOnce(&mut Ui) -> Response,
) -> Response {
    let t = ui.ir_theme();
    let frame = Frame::new()
        .fill(t.muted)
        .corner_radius(t.corner_md())
        .inner_margin(Margin::symmetric(spacing::LG as i8, spacing::SM as i8 + 1))
        .stroke(Stroke::new(BORDER_WIDTH, t.muted));

    let mut prepared = frame.begin(ui);
    // The frame's padding sets the height. egui 0.37 also stretches a text edit to the
    // minimum interactive height, which would add the padding on top of it.
    prepared.content_ui.spacing_mut().interact_size.y = 0.0;
    if !enabled {
        prepared.content_ui.disable();
        prepared.frame.fill = t.card;
    }
    let response = add(&mut prepared.content_ui);
    if response.has_focus() {
        prepared.frame.stroke = Stroke::new(BORDER_WIDTH, t.ring);
    }
    let outer = prepared.end(ui);
    if response.has_focus() {
        focus_ring(ui, outer.rect, t.corner_md(), &t);
    }
    response
}

#[cfg(test)]
mod tests {
    use egui::{Event, Key};

    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn typing_edits_the_bound_text() {
        let mut h = Harness::new();
        let mut text = String::from("ab");
        let mut id = None;
        let mut ui_fn = |ui: &mut Ui| {
            let r = ui.add(Input::new(&mut text).placeholder("name"));
            id = Some(r.id);
        };
        h.idle(&mut ui_fn);
        let id = id.unwrap();
        h.ctx.memory_mut(|m| m.request_focus(id));
        h.idle(&mut |ui: &mut Ui| {
            ui.add(Input::new(&mut text));
        });
        h.frame(vec![Event::Text("cd".into())], &mut |ui: &mut Ui| {
            ui.add(Input::new(&mut text));
        });
        h.key(Key::Backspace, &mut |ui: &mut Ui| {
            ui.add(Input::new(&mut text));
        });
        assert_eq!(text, "abc");
    }

    #[test]
    fn disabled_input_ignores_typing() {
        let mut h = Harness::new();
        let mut text = String::new();
        let mut id = None;
        h.idle(&mut |ui: &mut Ui| {
            id = Some(ui.add(Input::new(&mut text).enabled(false)).id);
        });
        h.ctx.memory_mut(|m| m.request_focus(id.unwrap()));
        h.frame(vec![Event::Text("x".into())], &mut |ui: &mut Ui| {
            ui.add(Input::new(&mut text).enabled(false));
        });
        assert!(text.is_empty());
    }

    #[test]
    fn builders_render() {
        let mut h = Harness::new();
        let mut text = String::new();
        h.idle(&mut |ui: &mut Ui| {
            ui.add(
                Input::new(&mut text)
                    .placeholder("hint")
                    .password(true)
                    .id_salt("k"),
            );
        });
    }
}
