//! A label, a control, and help or error text, stacked.

use egui::{InnerResponse, RichText, Ui};

use crate::theme::{spacing, ThemeExt};

/// Wraps any control with a label above it and help or error text below.
///
/// An error replaces the help text.
#[derive(Clone, Debug)]
pub struct FormField {
    label: String,
    help: Option<String>,
    error: Option<String>,
    required: bool,
}

impl FormField {
    /// A field with this label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            help: None,
            error: None,
            required: false,
        }
    }

    /// Explanatory text below the control.
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// An error message below the control, in the destructive color.
    pub fn error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Set the error from an `Option`, convenient for validation results.
    pub fn maybe_error(mut self, error: Option<String>) -> Self {
        self.error = error;
        self
    }

    /// Mark the field as required with an asterisk.
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    /// Draw the label, the control from `add`, and the help or error text.
    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let t = ui.ir_theme();
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = spacing::XS;
            let mut label = self.label.clone();
            if self.required {
                label.push_str(" *");
            }
            ui.label(RichText::new(label).size(t.text_sm).color(t.foreground));
            let result = add(ui);
            match (&self.error, &self.help) {
                (Some(error), _) => {
                    ui.label(RichText::new(error).size(t.text_sm).color(t.destructive));
                }
                (None, Some(help)) => {
                    ui.label(
                        RichText::new(help)
                            .size(t.text_sm)
                            .color(t.muted_foreground),
                    );
                }
                (None, None) => {}
            }
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn the_control_closure_runs_once_and_its_value_is_returned() {
        let mut h = Harness::new();
        let mut result = None;
        h.idle(&mut |ui: &mut Ui| {
            result = Some(FormField::new("Model").help("Name").show(ui, |_| 42).inner);
        });
        assert_eq!(result, Some(42));
    }

    #[test]
    fn builders_set_fields() {
        let f = FormField::new("Key").help("h").error("e").required(true);
        assert_eq!(
            (f.help.as_deref(), f.error.as_deref(), f.required),
            (Some("h"), Some("e"), true)
        );
        let none = FormField::new("Key").error("e").maybe_error(None);
        assert_eq!(none.error, None);
    }

    #[test]
    fn renders_each_combination() {
        let mut h = Harness::new();
        for field in [
            FormField::new("A"),
            FormField::new("B").help("help"),
            FormField::new("C").error("bad"),
            FormField::new("D").help("help").error("bad").required(true),
        ] {
            let mut calls = 0;
            h.idle(&mut |ui: &mut Ui| {
                field.clone().show(ui, |_| calls += 1);
            });
            assert_eq!(calls, 1, "the control is drawn exactly once");
        }
    }
}
