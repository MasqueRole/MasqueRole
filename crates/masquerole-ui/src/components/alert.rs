//! Alert: a boxed message that needs the reader's attention.

use egui::{Frame, Margin, Response, RichText, Stroke, Ui, Widget};

use crate::components::icon::{Icon, IconKind};
use crate::theme::{spacing, Theme, ThemeExt, BORDER_WIDTH};

/// A tinted box with a warning icon, a title, and a text. It spans the available width.
#[derive(Clone, Debug)]
pub struct Alert {
    title: String,
    text: String,
}

impl Alert {
    /// An alert with a short `title` and the `text` that explains it.
    pub fn new(title: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            text: text.into(),
        }
    }
}

impl Widget for Alert {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let frame = Frame::new()
            .fill(Theme::mix(t.card, t.destructive, 0.12))
            .stroke(Stroke::new(
                BORDER_WIDTH,
                Theme::mix(t.border, t.destructive, 0.6),
            ))
            .corner_radius(t.corner_md())
            .inner_margin(Margin::same(spacing::LG as i8));
        let response = frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.add(Icon::new(IconKind::Warning).color(t.destructive));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = spacing::XS;
                        ui.label(
                            RichText::new(&self.title)
                                .size(t.text_base)
                                .strong()
                                .color(t.foreground),
                        );
                        ui.label(
                            RichText::new(&self.text)
                                .size(t.text_sm)
                                .color(t.muted_foreground),
                        );
                    });
                });
            })
            .response;
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &self.title));
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn it_spans_the_width_and_wraps_long_text() {
        let mut h = Harness::new();
        let mut rect = None;
        h.idle(&mut |ui| {
            let width = ui.available_width();
            let r = ui.add(Alert::new("Careful", "word ".repeat(200)));
            assert!((r.rect.width() - width).abs() < 1.0);
            rect = Some(r.rect);
        });
        assert!(rect.unwrap().height() > 60.0);
    }
}
