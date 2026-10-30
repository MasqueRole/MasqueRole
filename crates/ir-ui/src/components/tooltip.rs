//! Tooltip: a short hint shown while hovering a widget.

use egui::Response;

use super::label::Label;

/// A short text hint shown when the pointer rests on a widget, or the widget has keyboard
/// focus.
#[derive(Clone, Debug)]
pub struct Tooltip {
    text: String,
}

impl Tooltip {
    /// A tooltip with this text.
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    /// Attach the tooltip to a widget's response and return the response.
    pub fn show(self, response: Response) -> Response {
        response.on_hover_ui(|ui| {
            ui.add(Label::new(self.text).small());
        })
    }
}

/// `response.ir_tooltip("Delete")` as a shorthand for [`Tooltip`].
pub trait TooltipExt {
    /// Show a tooltip with `text` while this widget is hovered.
    fn ir_tooltip(self, text: impl Into<String>) -> Self;
}

impl TooltipExt for Response {
    fn ir_tooltip(self, text: impl Into<String>) -> Self {
        Tooltip::new(text).show(self)
    }
}

#[cfg(test)]
mod tests {
    use egui::popup::was_tooltip_open_last_frame;
    use egui::{Sense, Ui};

    use super::*;
    use crate::components::testing::Harness;

    fn draw(ui: &mut Ui, rect: &mut egui::Rect) -> egui::Id {
        let (r, response) = ui.allocate_exact_size(egui::vec2(80.0, 32.0), Sense::hover());
        *rect = r;
        let id = response.id;
        response.ir_tooltip("Delete this node");
        id
    }

    #[test]
    fn appears_while_hovering_and_not_otherwise() {
        let mut h = Harness::new();
        let (mut rect, mut id) = (egui::Rect::NOTHING, egui::Id::NULL);
        for _ in 0..12 {
            h.idle(&mut |ui: &mut Ui| id = draw(ui, &mut rect));
        }
        assert!(
            !was_tooltip_open_last_frame(&h.ctx, id),
            "no tooltip without hovering"
        );

        let pos = rect.center();
        h.frame(vec![egui::Event::PointerMoved(pos)], &mut |ui: &mut Ui| {
            id = draw(ui, &mut rect)
        });
        for _ in 0..12 {
            h.idle(&mut |ui: &mut Ui| id = draw(ui, &mut rect));
        }
        assert!(was_tooltip_open_last_frame(&h.ctx, id));
    }

    #[test]
    fn the_tooltip_builder_returns_the_same_widget() {
        let mut h = Harness::new();
        h.idle(&mut |ui: &mut Ui| {
            let r = ui.allocate_response(egui::vec2(10.0, 10.0), Sense::hover());
            let id = r.id;
            assert_eq!(Tooltip::new("x").show(r).id, id);
        });
    }
}
