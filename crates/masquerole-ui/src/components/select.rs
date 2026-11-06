//! Drop-down select.

use egui::AsIdSalt;

use egui::text::LayoutJob;
use egui::{
    Align2, Color32, ComboBox, FontId, Id, Image, Rect, Response, Sense, Stroke, TextFormat,
    TextureHandle, Ui, Vec2, Widget,
};

use crate::theme::{ThemeExt, BORDER_WIDTH};

/// A drop-down for choosing one of several options, bound to the chosen index.
///
/// Check `.changed()` on the returned response to react to a new choice. Tab to focus it,
/// press Enter or Space to open the list, and Tab or click to pick an option.
pub struct Select<'a> {
    id_salt: Id,
    selected: &'a mut usize,
    options: Vec<String>,
    marks: Vec<SelectMark>,
    placeholder: String,
    width: Option<f32>,
    enabled: bool,
}

/// The small mark before an option.
#[derive(Clone)]
pub enum SelectMark {
    /// No mark.
    None,
    /// One or two letters on a rounded square of the given color.
    Letters(String, Color32),
    /// A white-on-transparent picture, drawn in the text color.
    Picture(TextureHandle),
}

impl std::fmt::Debug for SelectMark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("None"),
            Self::Letters(letters, _) => write!(f, "Letters({letters})"),
            Self::Picture(_) => f.write_str("Picture"),
        }
    }
}

impl SelectMark {
    /// Letters on a colored square.
    pub fn letters(letters: impl Into<String>, color: Color32) -> Self {
        Self::Letters(letters.into(), color)
    }
}

/// Side of the small colored mark before an option.
const MARK_SIZE: f32 = 18.0;

/// Space between a mark and its text.
const MARK_GAP: f32 = 8.0;

impl<'a> Select<'a> {
    /// A select with these options. `id_salt` keeps its open state apart from other selects.
    pub fn new(
        id_salt: impl AsIdSalt,
        selected: &'a mut usize,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            id_salt: Id::unique(id_salt),
            selected,
            options: options.into_iter().map(Into::into).collect(),
            marks: Vec::new(),
            placeholder: "Select\u{2026}".into(),
            width: None,
            enabled: true,
        }
    }

    /// A small mark before each option and before the chosen one in the closed select, one per
    /// option in order. Options without a mark leave it out.
    pub fn marks(mut self, marks: impl IntoIterator<Item = SelectMark>) -> Self {
        self.marks = marks.into_iter().collect();
        self
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
        let marks = &self.marks;
        let mark_of = |i: usize| marks.get(i).filter(|m| !matches!(m, SelectMark::None));
        let closed_mark = mark_of(before).cloned();
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
            let mut shown = LayoutJob::default();
            shown.append(
                &text,
                if closed_mark.is_some() {
                    MARK_SIZE + MARK_GAP
                } else {
                    0.0
                },
                TextFormat {
                    font_id: FontId::proportional(t.text_base),
                    color: t.foreground,
                    ..Default::default()
                },
            );
            let combo = ComboBox::from_id_salt(self.id_salt)
                .selected_text(shown)
                .width(width)
                .show_ui(ui, |ui| {
                    for (i, option) in options.iter().enumerate() {
                        match mark_of(i) {
                            Some(mark) => {
                                if option_row(ui, mark, option, *selected == i).clicked() {
                                    *selected = i;
                                }
                            }
                            None => {
                                ui.selectable_value(selected, i, option);
                            }
                        }
                    }
                });
            if let Some(mark) = &closed_mark {
                // The mark sits in the space left at the start of the closed select's text.
                let rect = combo.response.rect;
                let at = egui::pos2(
                    rect.left() + ui.spacing().button_padding.x,
                    rect.center().y - MARK_SIZE / 2.0,
                );
                draw_mark(ui, Rect::from_min_size(at, Vec2::splat(MARK_SIZE)), mark);
            }
            combo
        });
        let mut response = inner.inner.response;
        if *selected != before {
            response.mark_changed();
        }
        response
    }
}

/// One option of the open list: the mark and the text on a button as wide as the list, so the
/// whole row can be clicked.
fn option_row(ui: &mut Ui, mark: &SelectMark, text: &str, selected: bool) -> Response {
    let height = ui.spacing().interact_size.y;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, text));
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, selected);
        if selected || response.hovered() || response.has_focus() {
            ui.painter().rect(
                rect,
                visuals.corner_radius,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
        }
        let pad = ui.spacing().button_padding.x;
        let mark_rect = Rect::from_min_size(
            egui::pos2(rect.left() + pad, rect.center().y - MARK_SIZE / 2.0),
            Vec2::splat(MARK_SIZE),
        );
        draw_mark(ui, mark_rect, mark);
        ui.painter().text(
            egui::pos2(mark_rect.right() + MARK_GAP, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(ui.ir_theme().text_base),
            visuals.text_color(),
        );
    }
    response
}

/// Draw `mark` in `rect`.
fn draw_mark(ui: &Ui, rect: Rect, mark: &SelectMark) {
    let t = ui.ir_theme();
    match mark {
        SelectMark::None => {}
        SelectMark::Letters(letters, color) => {
            ui.painter().rect_filled(rect, t.corner_sm(), *color);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                letters,
                FontId::proportional(t.text_xs),
                Color32::WHITE,
            );
        }
        SelectMark::Picture(texture) => {
            Image::new((texture.id(), rect.size()))
                .tint(t.foreground)
                .paint_at(ui, rect);
        }
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
    fn options_can_carry_a_colored_mark() {
        let mut h = Harness::new();
        let mut selected = 1;
        let marks = [
            SelectMark::letters("O", Color32::from_rgb(16, 163, 127)),
            SelectMark::letters("X", Color32::GRAY),
            SelectMark::None,
        ];
        let mut closed = egui::Rect::NOTHING;
        h.idle(&mut |ui: &mut Ui| {
            closed = ui
                .add(
                    Select::new("provider", &mut selected, OPTIONS)
                        .marks(marks.clone())
                        .width(200.0),
                )
                .rect;
        });
        h.click(closed.center(), &mut |ui: &mut Ui| {
            ui.add(Select::new("provider", &mut selected, OPTIONS).marks(marks.clone()));
        });
        h.idle(&mut |ui: &mut Ui| {
            ui.add(Select::new("provider", &mut selected, OPTIONS).marks(marks.clone()));
        });
        assert_eq!(selected, 1);
    }

    #[test]
    fn the_whole_row_with_a_mark_is_clickable_not_only_its_text() {
        let mut h = Harness::new();
        let mut selected = 0;
        let marks = [
            SelectMark::letters("O", Color32::GRAY),
            SelectMark::letters("X", Color32::GRAY),
            SelectMark::letters("L", Color32::GRAY),
        ];
        let mut closed = egui::Rect::NOTHING;
        let draw = |ui: &mut Ui, selected: &mut usize| {
            ui.add(
                Select::new("provider", selected, OPTIONS)
                    .marks(marks.clone())
                    .width(200.0),
            )
            .rect
        };
        h.idle(&mut |ui: &mut Ui| closed = draw(ui, &mut selected));
        h.click(closed.center(), &mut |ui: &mut Ui| {
            draw(ui, &mut selected);
        });
        h.idle(&mut |ui: &mut Ui| {
            draw(ui, &mut selected);
        });
        let style = h.ctx.global_style();
        let row = style.spacing.interact_size.y;
        let pitch = row + style.spacing.item_spacing.y;
        let y = closed.bottom() + style.spacing.menu_margin.top as f32 + 2.0 * pitch + row / 2.0;
        // The far right of the row, where there is neither mark nor text.
        let blank = egui::Pos2::new(closed.right() - 30.0, y);
        h.click(blank, &mut |ui: &mut Ui| {
            draw(ui, &mut selected);
        });
        assert_eq!(selected, 2);
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
        let style = h.ctx.global_style();
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
        assert!(!egui::Popup::is_any_open(&h.ctx));
    }
}
