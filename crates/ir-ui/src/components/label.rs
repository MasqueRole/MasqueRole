//! Text label.

use egui::{Response, RichText, Ui, Widget};

use crate::theme::ThemeExt;

/// Color role of a [`Label`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelTone {
    /// Regular text.
    #[default]
    Default,
    /// Subdued text, for help and descriptions.
    Muted,
    /// Error text.
    Destructive,
}

/// Themed text.
#[derive(Clone, Debug)]
pub struct Label {
    text: String,
    tone: LabelTone,
    small: bool,
    heading: bool,
    monospace: bool,
    max_width: Option<f32>,
}

impl Label {
    /// Regular body text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: LabelTone::default(),
            small: false,
            heading: false,
            monospace: false,
            max_width: None,
        }
    }

    /// Set the color role.
    pub fn tone(mut self, tone: LabelTone) -> Self {
        self.tone = tone;
        self
    }

    /// Subdued text.
    pub fn muted(self) -> Self {
        self.tone(LabelTone::Muted)
    }

    /// Smaller text.
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    /// Heading-size, strong text.
    pub fn heading(mut self) -> Self {
        self.heading = true;
        self
    }

    /// Keep the text on one line at most this wide, cut with an ellipsis. Hovering a cut
    /// text shows all of it.
    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = Some(width.max(0.0));
        self
    }

    /// Fixed-width text, for aligned output such as the story tree.
    pub fn monospace(mut self) -> Self {
        self.monospace = true;
        self
    }
}

impl Widget for Label {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let color = match self.tone {
            LabelTone::Default => t.foreground,
            LabelTone::Muted => t.muted_foreground,
            LabelTone::Destructive => t.destructive,
        };
        let size = if self.heading {
            t.text_xl
        } else if self.small {
            t.text_sm
        } else {
            t.text_base
        };
        let full = self.text.clone();
        let mut text = RichText::new(self.text).color(color).size(size);
        if self.heading {
            text = text.strong();
        }
        if self.monospace {
            text = text.monospace().size(t.text_sm);
        }
        let Some(max) = self.max_width else {
            return ui.add(egui::Label::new(text));
        };
        let width = max.min(ui.available_width());
        let natural = ui
            .painter()
            .layout_no_wrap(full.clone(), egui::FontId::proportional(size), color)
            .size()
            .x;
        let response = ui
            .scope(|ui| {
                ui.set_max_width(width);
                ui.add(egui::Label::new(text).truncate())
            })
            .inner;
        if natural > width {
            response.on_hover_text(full)
        } else {
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_max_width_keeps_a_long_title_on_one_line() {
        let ctx = egui::Context::default();
        crate::Theme::apply(&ctx);
        let mut sizes = Vec::new();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let long = "Aleksandra Wi\u{15b}niewska-Kowalczyk and the lighthouse at the end of the world";
                sizes.push(ui.add(Label::new(long).heading().max_width(200.0)).rect.size());
                sizes.push(ui.add(Label::new("Jo").heading().max_width(200.0)).rect.size());
            });
        });
        assert!(sizes[0].x <= 201.0, "{sizes:?}");
        assert!(
            (sizes[0].y - sizes[1].y).abs() < 1.0,
            "one line either way: {sizes:?}"
        );
    }
}
