//! Card with header, content, and footer areas.

use egui::{Frame, Margin, Response, RichText, Stroke, Ui};

use crate::theme::{spacing, ThemeExt, BORDER_WIDTH};

type Area<'a> = Box<dyn FnOnce(&mut Ui) + 'a>;

/// A bordered surface. Fill the areas you need, then call [`Card::show`].
pub struct Card<'a> {
    header: Option<Area<'a>>,
    content: Option<Area<'a>>,
    footer: Option<Area<'a>>,
}

impl Default for Card<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Card<'a> {
    /// An empty card.
    pub fn new() -> Self {
        Self {
            header: None,
            content: None,
            footer: None,
        }
    }

    /// The header area, usually a [`Card::title`] and [`Card::description`].
    pub fn header(mut self, add: impl FnOnce(&mut Ui) + 'a) -> Self {
        self.header = Some(Box::new(add));
        self
    }

    /// The main content area.
    pub fn content(mut self, add: impl FnOnce(&mut Ui) + 'a) -> Self {
        self.content = Some(Box::new(add));
        self
    }

    /// The footer area, usually actions.
    pub fn footer(mut self, add: impl FnOnce(&mut Ui) + 'a) -> Self {
        self.footer = Some(Box::new(add));
        self
    }

    /// A card title for use in the header.
    pub fn title(ui: &mut Ui, text: impl Into<String>) {
        let t = ui.ir_theme();
        ui.label(
            RichText::new(text)
                .size(t.text_lg)
                .strong()
                .color(t.card_foreground),
        );
    }

    /// A subdued description for use in the header.
    pub fn description(ui: &mut Ui, text: impl Into<String>) {
        let t = ui.ir_theme();
        ui.label(
            RichText::new(text)
                .size(t.text_sm)
                .color(t.muted_foreground),
        );
    }

    /// Draw the card across the available width.
    pub fn show(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let frame = Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(BORDER_WIDTH, t.border))
            .corner_radius(t.corner_lg())
            .inner_margin(Margin::same(spacing::XL as i8));
        frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = spacing::SM;
                let mut first = true;
                for area in [self.header, self.content, self.footer]
                    .into_iter()
                    .flatten()
                {
                    if !first {
                        ui.add_space(spacing::SM);
                    }
                    first = false;
                    ui.scope(area);
                }
            })
            .response
    }
}
