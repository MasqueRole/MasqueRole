//! Drop-down select.

use std::hash::Hash;

use egui::{ComboBox, Id, Response, Stroke, Ui, Widget};

use crate::theme::{ThemeExt, BORDER_WIDTH};

/// A drop-down for choosing one of several options, bound to the chosen index.
///
/// Check `.changed()` on the returned response to react to a new choice. Tab to focus it,
/// press Enter or Space to open the list, and Tab or click to pick an option.
pub struct Select<'a> {
    id_salt: Id,
    selected: &'a mut usize,
    options: Vec<String>,
    placeholder: String,
    width: Option<f32>,
    enabled: bool,
}

impl<'a> Select<'a> {
    /// A select with these options. `id_salt` keeps its open state apart from other selects.
    pub fn new(
        id_salt: impl Hash,
        selected: &'a mut usize,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            id_salt: Id::new(id_salt),
            selected,
            options: options.into_iter().map(Into::into).collect(),
            placeholder: "Select\u{2026}".into(),
            width: None,
            enabled: true,
        }
    }

    /// Text shown when the index points at no option.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Width of the closed select, in logical pixels.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Enable or disable the select.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for Select<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let before = *self.selected;
        let text = self
            .options
            .get(before)
            .unwrap_or(&self.placeholder)
            .clone();

        let selected = self.selected;
        let options = &self.options;
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let inner = ui.add_enabled_ui(self.enabled, |ui| {
            // An outlined look: the closed select on the page background, a border like inputs.
            let widgets = &mut ui.visuals_mut().widgets;
            for state in [&mut widgets.inactive, &mut widgets.open] {
                state.bg_fill = t.background;
                state.weak_bg_fill = t.background;
                state.bg_stroke = Stroke::new(BORDER_WIDTH, t.input);
                state.fg_stroke = Stroke::new(BORDER_WIDTH, t.foreground);
            }
            ComboBox::from_id_salt(self.id_salt)
                .selected_text(text)
                .width(width)
                .show_ui(ui, |ui| {
                    for (i, option) in options.iter().enumerate() {
                        ui.selectable_value(selected, i, option);
                    }
                })
        });
        let mut response = inner.inner.response;
        if *selected != before {
            response.mark_changed();
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    const OPTIONS: [&str; 3] = ["OpenAI", "xAI", "Ollama"];

    #[test]
    fn renders_closed_with_the_selected_option() {
        let mut h = Harness::new();
        let mut selected = 1;
        h.idle(&mut |ui: &mut Ui| {
            ui.add(Select::new("provider", &mut selected, OPTIONS).width(200.0));
        });
        assert_eq!(selected, 1);
    }

    #[test]
    fn an_out_of_range_index_shows_the_placeholder_and_is_left_alone() {
        let mut h = Harness::new();
        let mut selected = 9;
        h.idle(&mut |ui: &mut Ui| {
            ui.add(Select::new("provider", &mut selected, OPTIONS).placeholder("Pick one"));
        });
        assert_eq!(selected, 9);
    }

    #[test]
    fn clicking_opens_the_list_and_choosing_an_option_selects_it() {
        let mut h = Harness::new();
        let mut selected = 0;
        let mut closed = egui::Rect::NOTHING;
        h.idle(&mut |ui: &mut Ui| {
            closed = ui
                .add(Select::new("provider", &mut selected, OPTIONS).width(200.0))
                .rect;
        });

        // Open the popup.
        h.click(closed.center(), &mut |ui: &mut Ui| {
            ui.add(Select::new("provider", &mut selected, OPTIONS).width(200.0));
        });
        h.idle(&mut |ui: &mut Ui| {
            ui.add(Select::new("provider", &mut selected, OPTIONS).width(200.0));
        });

        // The options are listed below the closed select, one row each, inside the menu
        // margin. Click the middle of the third row.
        let style = h.ctx.style();
        let row = style.spacing.interact_size.y;
        let pitch = row + style.spacing.item_spacing.y;
        let third_center = style.spacing.menu_margin.top as f32 + 2.0 * pitch + row / 2.0;
        let third = egui::Pos2::new(closed.center().x, closed.bottom() + third_center);
        let mut changed = false;
        h.click(third, &mut |ui: &mut Ui| {
            changed |= ui
                .add(Select::new("provider", &mut selected, OPTIONS).width(200.0))
                .changed();
        });
        assert_eq!(selected, 2);
        assert!(changed);
    }

    #[test]
    fn a_disabled_select_does_not_open() {
        let mut h = Harness::new();
        let mut selected = 0;
        let mut closed = egui::Rect::NOTHING;
        h.idle(&mut |ui: &mut Ui| {
            closed = ui
                .add(Select::new("provider", &mut selected, OPTIONS).enabled(false))
                .rect;
        });
        h.click(closed.center(), &mut |ui: &mut Ui| {
            ui.add(Select::new("provider", &mut selected, OPTIONS).enabled(false));
        });
        assert!(!h.ctx.memory(|m| m.any_popup_open()));
    }
}
