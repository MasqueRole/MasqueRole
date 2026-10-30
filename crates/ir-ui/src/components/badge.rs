//! Badge: a small pill for status and tags.

use egui::{Color32, FontId, Response, Sense, Stroke, StrokeKind, Ui, Widget};

use crate::theme::{spacing, ThemeExt};

/// Visual style of a [`Badge`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    /// Solid primary color.
    #[default]
    Default,
    /// Subdued solid color.
    Secondary,
    /// Transparent with a border.
    Outline,
    /// Solid destructive color.
    Destructive,
}

/// A small non-interactive label pill.
#[derive(Clone, Debug)]
pub struct Badge {
    text: String,
    variant: BadgeVariant,
}

impl Badge {
    /// A default-variant badge.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            variant: BadgeVariant::default(),
        }
    }

    /// Set the visual variant.
    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Widget for Badge {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let galley = ui.painter().layout_no_wrap(
            self.text.clone(),
            FontId::proportional(t.text_xs),
            Color32::PLACEHOLDER,
        );
        let size = galley.size() + egui::vec2(2.0 * spacing::SM, 2.0 * spacing::XS);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), &self.text)
        });

        let (bg, fg, stroke) = match self.variant {
            BadgeVariant::Default => (t.primary, t.primary_foreground, Stroke::NONE),
            BadgeVariant::Secondary => (t.secondary, t.secondary_foreground, Stroke::NONE),
            BadgeVariant::Outline => (
                Color32::TRANSPARENT,
                t.foreground,
                Stroke::new(1.0, t.border),
            ),
            BadgeVariant::Destructive => (t.destructive, t.destructive_foreground, Stroke::NONE),
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            egui::CornerRadius::same(u8::MAX),
            bg,
            stroke,
            StrokeKind::Inside,
        );
        painter.galley(rect.center() - galley.size() / 2.0, galley, fg);
        response
    }
}
