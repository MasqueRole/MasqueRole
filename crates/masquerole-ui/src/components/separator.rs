//! Separator line.

use egui::{Response, Sense, Stroke, Ui, Widget};

use crate::theme::{ThemeExt, BORDER_WIDTH};

/// A 1 px line in the border color.
#[derive(Clone, Copy, Debug)]
pub struct Separator {
    vertical: bool,
}

impl Separator {
    /// A line across the available width.
    pub fn horizontal() -> Self {
        Self { vertical: false }
    }

    /// A line down the available height.
    pub fn vertical() -> Self {
        Self { vertical: true }
    }
}

impl Widget for Separator {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let size = if self.vertical {
            egui::vec2(1.0, ui.available_height())
        } else {
            egui::vec2(ui.available_width(), 1.0)
        };
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        let stroke = Stroke::new(BORDER_WIDTH, t.border);
        if self.vertical {
            ui.painter().vline(rect.center().x, rect.y_range(), stroke);
        } else {
            ui.painter().hline(rect.x_range(), rect.center().y, stroke);
        }
        response
    }
}
