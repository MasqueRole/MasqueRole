//! Badge: a small pill for status and tags.

use egui::text::LayoutJob;
use egui::{Color32, FontId, Response, Sense, Stroke, StrokeKind, Ui, Widget};

use crate::theme::{spacing, ThemeExt, BORDER_WIDTH};

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
    max_width: Option<f32>,
}

impl Badge {
    /// A default-variant badge.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            variant: BadgeVariant::default(),
            max_width: None,
        }
    }

    /// Keep the badge at most this wide: a longer text is cut with an ellipsis, and the whole
    /// text shows in a tooltip.
    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = Some(width);
        self
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
        let wrap = self
            .max_width
            .map_or(f32::INFINITY, |w| (w - 2.0 * spacing::SM).max(0.0));
        let mut job = LayoutJob::simple(
            self.text.clone(),
            FontId::proportional(t.text_xs),
            Color32::PLACEHOLDER,
            wrap,
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('\u{2026}');
        let galley = ui.painter().layout_job(job);
        let cut = galley.elided;
        let size = galley.size() + egui::vec2(2.0 * spacing::SM, 2.0 * spacing::XS);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Label, ui.is_enabled(), &self.text)
        });

        let (bg, fg, stroke) = match self.variant {
            BadgeVariant::Default => (t.primary, t.primary_foreground, Stroke::NONE),
            BadgeVariant::Secondary => (t.secondary, t.secondary_foreground, Stroke::NONE),
            BadgeVariant::Outline => (
                Color32::TRANSPARENT,
                t.foreground,
                Stroke::new(BORDER_WIDTH, t.border),
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
        if cut {
            response.on_hover_text(&self.text)
        } else {
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_max_width_cuts_a_long_text_and_leaves_short_ones_alone() {
        let ctx = egui::Context::default();
        crate::Theme::apply(&ctx);
        let mut widths = Vec::new();
        ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let long = "The Very Long Name Of A World That Goes On And On Forever";
                widths.push(ui.add(Badge::new(long).max_width(120.0)).rect.width());
                widths.push(ui.add(Badge::new("Jo").max_width(120.0)).rect.width());
                widths.push(ui.add(Badge::new(long)).rect.width());
            });
        })
        .drop_without_applying_deltas();
        assert!(widths[0] <= 121.0, "{widths:?}");
        assert!(widths[1] < 60.0, "{widths:?}");
        assert!(
            widths[2] > 200.0,
            "without a limit the badge keeps its width"
        );
    }
}
