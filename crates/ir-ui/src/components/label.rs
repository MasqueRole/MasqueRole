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
        let mut text = RichText::new(self.text).color(color).size(size);
        if self.heading {
            text = text.strong();
        }
        if self.monospace {
            text = text.monospace().size(t.text_sm);
        }
        ui.add(egui::Label::new(text))
    }
}
