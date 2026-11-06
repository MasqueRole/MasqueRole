//! Chip input: type keywords, and each one turns into a bean you can delete.

use egui::AsIdSalt;

use egui::text::LayoutJob;
use egui::{
    Color32, CursorIcon, FontId, Id, Key, Modifiers, Rect, Response, RichText, Sense, TextEdit, Ui,
    Vec2, Widget,
};

use super::common::transition;
use super::icon::{self, IconKind};
use super::input::text_field;
use crate::theme::{spacing, ThemeExt};

/// Height of a chip.
const CHIP_HEIGHT: f32 = 24.0;

/// Width of the delete button inside a chip.
const DELETE_WIDTH: f32 = 20.0;

/// Size of the delete icon.
const DELETE_ICON: f32 = 10.0;

/// The widest a chip gets before its text is cut with an ellipsis.
const CHIP_MAX_WIDTH: f32 = 220.0;

/// The narrowest the text field after the chips gets, so there is always room to type.
const MIN_TYPING_WIDTH: f32 = 96.0;

/// Add `word` as a chip unless it is blank or already there (ignoring case). Returns whether
/// a chip was added.
pub fn push_chip(chips: &mut Vec<String>, word: &str) -> bool {
    let word = word.trim();
    if word.is_empty() || chips.iter().any(|c| c.eq_ignore_ascii_case(word)) {
        return false;
    }
    chips.push(word.to_string());
    true
}

/// Turn everything before the last comma in `pending` into chips, and keep the rest as the
/// text still being typed. Pasting `a, b, c` gives two chips and leaves `c` pending. Returns
/// whether `pending` had a comma.
pub fn absorb_commas(chips: &mut Vec<String>, pending: &mut String) -> bool {
    if !pending.contains(',') {
        return false;
    }
    let all = std::mem::take(pending);
    let mut parts: Vec<&str> = all.split(',').collect();
    let rest = parts.pop().unwrap_or_default();
    for part in parts {
        push_chip(chips, part);
    }
    *pending = rest.trim_start().to_string();
    true
}

/// Turn the text still being typed into a chip, for Enter and for saving.
pub fn commit_pending(chips: &mut Vec<String>, pending: &mut String) {
    push_chip(chips, pending);
    pending.clear();
}

/// A field of keyword chips followed by a text field. A comma or Enter turns the typed text
/// into a chip, the cross on a chip removes it, and Backspace in the empty field removes the
/// last one.
///
/// Call [`commit_pending`] before reading the chips when the user is done, so a keyword they
/// did not finish with a comma is not lost.
pub struct ChipInput<'a> {
    chips: &'a mut Vec<String>,
    pending: &'a mut String,
    placeholder: String,
    id_salt: Id,
}

impl<'a> ChipInput<'a> {
    /// A chip input editing `chips`, with `pending` as the text being typed.
    pub fn new(chips: &'a mut Vec<String>, pending: &'a mut String) -> Self {
        Self {
            chips,
            pending,
            placeholder: String::new(),
            id_salt: Id::unique("chip-input"),
        }
    }

    /// Hint text shown while there are no chips and nothing is typed.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Give the field a stable identity.
    pub fn id_salt(mut self, id_salt: impl AsIdSalt) -> Self {
        self.id_salt = Id::unique(id_salt);
        self
    }
}

/// Draw one chip. Returns whether its delete button was clicked.
fn chip(ui: &mut Ui, text: &str, id: Id) -> bool {
    let t = ui.ir_theme();
    let mut job = LayoutJob::simple(
        text.to_string(),
        FontId::proportional(t.text_sm),
        Color32::PLACEHOLDER,
        CHIP_MAX_WIDTH - DELETE_WIDTH - spacing::SM,
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('\u{2026}');
    let galley = ui.painter().layout_job(job);
    let width = spacing::SM + galley.size().x + DELETE_WIDTH;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, CHIP_HEIGHT), Sense::hover());
    let delete_rect = Rect::from_min_size(
        egui::pos2(rect.right() - DELETE_WIDTH, rect.top()),
        Vec2::new(DELETE_WIDTH, CHIP_HEIGHT),
    );
    let delete = ui.interact(delete_rect, id.with("delete"), Sense::click());
    let pill = egui::CornerRadius::same(u8::MAX);
    ui.painter().rect_filled(rect, pill, t.secondary);
    let fade = transition(
        ui,
        id.with("hover"),
        if delete.hovered() {
            t.destructive
        } else {
            Color32::TRANSPARENT
        },
    );
    ui.painter().rect_filled(delete_rect, pill, fade);
    ui.painter().galley(
        egui::pos2(
            rect.left() + spacing::SM,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        t.secondary_foreground,
    );
    icon::paint(
        ui.painter(),
        Rect::from_center_size(delete_rect.center(), Vec2::splat(DELETE_ICON)),
        if delete.hovered() {
            t.destructive_foreground
        } else {
            t.muted_foreground
        },
        IconKind::Close,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text));
    delete.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

impl Widget for ChipInput<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let Self {
            chips,
            pending,
            placeholder,
            id_salt,
        } = self;
        let edit_id = ui.make_persistent_id(id_salt);
        let hint = RichText::new(placeholder).color(t.muted_foreground);
        text_field(ui, true, |ui| {
            let mut response = None;
            let mut remove = None;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(spacing::XS, spacing::XS);
                for (i, text) in chips.iter().enumerate() {
                    if chip(ui, text, edit_id.with(("chip", i))) {
                        remove = Some(i);
                    }
                }

                // Keys that act on the chips are taken before the text field sees them.
                let focused = ui.memory(|m| m.has_focus(edit_id));
                if focused {
                    let enter = !pending.trim().is_empty()
                        && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
                    if enter {
                        commit_pending(chips, pending);
                    }
                    let backspace = pending.is_empty()
                        && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace));
                    if backspace {
                        chips.pop();
                    }
                }
                // What is left of the row, so the text field sits right after the chips; with
                // too little left it starts a new row.
                let room = ui.max_rect().right() - ui.cursor().min.x - spacing::SM;
                let hint = if chips.is_empty() {
                    hint.clone()
                } else {
                    RichText::new("")
                };
                let edit = TextEdit::singleline(pending)
                    .id(edit_id)
                    .frame(egui::Frame::NONE)
                    .hint_text(hint)
                    .desired_width(room.max(MIN_TYPING_WIDTH));
                let r = ui.add(edit);
                if r.changed() {
                    absorb_commas(chips, pending);
                }
                response = Some(r);
            });
            if let Some(i) = remove {
                chips.remove(i);
            }
            response.expect("the text field is always drawn")
        })
    }
}

#[cfg(test)]
mod tests {
    use egui::Event;

    use super::*;
    use crate::components::testing::Harness;

    fn chips(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_comma_turns_the_typed_word_into_a_chip() {
        let mut list = Vec::new();
        let mut pending = String::from("lighthouse,");
        assert!(absorb_commas(&mut list, &mut pending));
        assert_eq!(list, chips(&["lighthouse"]));
        assert_eq!(pending, "");
    }

    #[test]
    fn pasting_a_list_makes_several_chips_and_keeps_the_last_word_pending() {
        let mut list = chips(&["old"]);
        let mut pending = String::from(" a ,b,, c ");
        absorb_commas(&mut list, &mut pending);
        assert_eq!(list, chips(&["old", "a", "b"]));
        assert_eq!(pending, "c ");
        assert!(
            !absorb_commas(&mut list, &mut pending),
            "no comma, no change"
        );
    }

    #[test]
    fn blank_and_repeated_words_make_no_chip() {
        let mut list = chips(&["Keeper"]);
        assert!(!push_chip(&mut list, "   "));
        assert!(!push_chip(&mut list, "keeper"));
        assert!(push_chip(&mut list, " fogglass "));
        assert_eq!(list, chips(&["Keeper", "fogglass"]));
    }

    #[test]
    fn committing_takes_what_is_still_being_typed() {
        let mut list = Vec::new();
        let mut pending = String::from("  harbor ");
        commit_pending(&mut list, &mut pending);
        assert_eq!(list, chips(&["harbor"]));
        assert!(pending.is_empty());
        commit_pending(&mut list, &mut pending);
        assert_eq!(list.len(), 1, "nothing typed, nothing added");
    }

    #[test]
    fn typing_a_comma_in_the_field_makes_a_chip() {
        let mut h = Harness::new();
        let (mut list, mut pending) = (Vec::new(), String::new());
        let mut id = Id::NULL;
        h.idle(&mut |ui: &mut Ui| {
            id = ui.add(ChipInput::new(&mut list, &mut pending)).id;
        });
        h.ctx.memory_mut(|m| m.request_focus(id));
        h.idle(&mut |ui: &mut Ui| {
            ui.add(ChipInput::new(&mut list, &mut pending));
        });
        h.frame(vec![Event::Text("fog,".into())], &mut |ui: &mut Ui| {
            ui.add(ChipInput::new(&mut list, &mut pending));
        });
        assert_eq!(list, chips(&["fog"]));
        assert_eq!(pending, "");
    }

    #[test]
    fn enter_commits_and_backspace_in_the_empty_field_removes_the_last_chip() {
        let mut h = Harness::new();
        let (mut list, mut pending) = (chips(&["a", "b"]), String::from("c"));
        let mut id = Id::NULL;
        h.idle(&mut |ui: &mut Ui| {
            id = ui.add(ChipInput::new(&mut list, &mut pending)).id;
        });
        h.ctx.memory_mut(|m| m.request_focus(id));
        h.idle(&mut |ui: &mut Ui| {
            ui.add(ChipInput::new(&mut list, &mut pending));
        });
        h.key(Key::Enter, &mut |ui: &mut Ui| {
            ui.add(ChipInput::new(&mut list, &mut pending));
        });
        assert_eq!(list, chips(&["a", "b", "c"]));
        assert!(pending.is_empty());
        h.key(Key::Backspace, &mut |ui: &mut Ui| {
            ui.add(ChipInput::new(&mut list, &mut pending));
        });
        assert_eq!(list, chips(&["a", "b"]));
    }

    #[test]
    fn clicking_the_cross_of_a_chip_removes_it() {
        let mut h = Harness::new();
        let (mut list, mut pending) = (chips(&["fog", "harbor"]), String::new());
        let mut rect = Rect::NOTHING;
        h.idle(&mut |ui: &mut Ui| {
            rect = ui
                .scope(|ui| ui.add(ChipInput::new(&mut list, &mut pending)))
                .response
                .rect;
        });
        // The cross of the first chip is found by trying points over the field until one
        // removes a chip.
        let mut y = rect.top();
        'scan: while y < rect.bottom() {
            let mut x = rect.left();
            while x < rect.right().min(rect.left() + 200.0) {
                let (l, p) = (&mut list, &mut pending);
                h.click(egui::pos2(x, y), &mut |ui: &mut Ui| {
                    ui.add(ChipInput::new(l, p));
                });
                if list.len() == 1 {
                    break 'scan;
                }
                x += 4.0;
            }
            y += 4.0;
        }
        assert_eq!(list.len(), 1, "a chip was removed");
    }

    #[test]
    fn many_long_chips_wrap_inside_the_field_width() {
        let mut h = Harness::new();
        let mut list: Vec<String> = (0..30).map(|i| format!("keyword number {i}")).collect();
        let mut pending = String::new();
        let (mut rect, mut avail) = (Rect::NOTHING, 0.0);
        h.idle(&mut |ui: &mut Ui| {
            avail = ui.available_width();
            rect = ui
                .scope(|ui| ui.add(ChipInput::new(&mut list, &mut pending)))
                .response
                .rect;
        });
        assert!(rect.width() <= avail + 1.0, "{rect:?} in {avail}");
        assert!(
            rect.height() > 2.0 * CHIP_HEIGHT,
            "the chips wrapped onto rows"
        );
    }
}
