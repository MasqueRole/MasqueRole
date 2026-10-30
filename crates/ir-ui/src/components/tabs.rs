//! Tab strip.

use egui::{
    Color32, CursorIcon, EventFilter, FontId, Frame, Key, Margin, Modifiers, Response, Sense, Ui,
    Widget, WidgetInfo, WidgetType,
};

use super::common::focus_ring;
use crate::theme::{spacing, ThemeExt};

const TRIGGER_HEIGHT: f32 = 28.0;

/// A strip of tab triggers bound to the selected index. The caller draws the content of the
/// selected tab below it.
///
/// Tab to a trigger and press Enter or Space to select it, or use Left/Right to move
/// between triggers. Check `.changed()` on the response to react to a new selection.
pub struct Tabs<'a> {
    selected: &'a mut usize,
    labels: Vec<String>,
}

impl<'a> Tabs<'a> {
    /// Tabs with these labels; `selected` is the index of the active one.
    pub fn new(
        selected: &'a mut usize,
        labels: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            selected,
            labels: labels.into_iter().map(Into::into).collect(),
        }
    }
}

impl Widget for Tabs<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        if !self.labels.is_empty() {
            *self.selected = (*self.selected).min(self.labels.len() - 1);
        }
        let before = *self.selected;
        let mut selected = before;
        let mut triggers: Vec<Response> = Vec::with_capacity(self.labels.len());

        let frame = Frame::new()
            .fill(t.muted)
            .corner_radius(t.corner_md())
            .inner_margin(Margin::same(spacing::XS as i8));
        let mut response = frame
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = spacing::XS;
                    for (i, label) in self.labels.iter().enumerate() {
                        let active = i == before;
                        let galley = ui.painter().layout_no_wrap(
                            label.clone(),
                            FontId::proportional(t.text_sm),
                            Color32::PLACEHOLDER,
                        );
                        let size = egui::vec2(galley.size().x + 2.0 * spacing::MD, TRIGGER_HEIGHT);
                        let (rect, r) = ui.allocate_exact_size(size, Sense::click());
                        r.widget_info(|| {
                            WidgetInfo::selected(WidgetType::SelectableLabel, true, active, label)
                        });
                        if r.clicked() {
                            selected = i;
                        }

                        let radius = t.corner_sm();
                        if active {
                            ui.painter().rect_filled(rect, radius, t.background);
                        }
                        let color = if active || r.hovered() {
                            t.foreground
                        } else {
                            t.muted_foreground
                        };
                        let pos = rect.center() - galley.size() / 2.0;
                        ui.painter().galley(pos, galley, color);
                        if r.has_focus() {
                            focus_ring(ui, rect, radius, &t);
                        }
                        triggers.push(r.on_hover_cursor(CursorIcon::PointingHand));
                    }
                });
            })
            .response;

        // Left/Right move between triggers when one has focus.
        if let Some(focused) = triggers.iter().position(|r| r.has_focus()) {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    triggers[focused].id,
                    EventFilter {
                        horizontal_arrows: true,
                        ..Default::default()
                    },
                );
            });
            let pressed = |key| ui.input_mut(|i| i.consume_key(Modifiers::NONE, key));
            let target = if pressed(Key::ArrowRight) {
                (focused + 1).min(triggers.len() - 1)
            } else if pressed(Key::ArrowLeft) {
                focused.saturating_sub(1)
            } else {
                focused
            };
            if target != focused {
                selected = target;
                triggers[target].request_focus();
            }
        }

        if selected != before {
            *self.selected = selected;
            response.mark_changed();
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use egui::Pos2;

    use super::*;
    use crate::components::testing::Harness;

    const LABELS: [&str; 3] = ["Chat", "Memory", "Tree"];

    /// A frame callback that draws the tabs bound to `selected`.
    fn redraw(selected: &mut usize) -> impl FnMut(&mut Ui) + '_ {
        move |ui: &mut Ui| {
            ui.add(Tabs::new(selected, LABELS));
        }
    }

    #[test]
    fn clicking_a_trigger_selects_it() {
        let mut h = Harness::new();
        let mut selected = 0;
        let mut whole = egui::Rect::NOTHING;
        h.idle(&mut |ui: &mut Ui| {
            whole = ui.add(Tabs::new(&mut selected, LABELS)).rect;
        });
        // The strip is three triggers wide; click inside the last third.
        let target = Pos2::new(whole.right() - 20.0, whole.center().y);

        let mut changed = false;
        h.click(target, &mut |ui: &mut Ui| {
            changed |= ui.add(Tabs::new(&mut selected, LABELS)).changed();
        });
        assert_eq!(selected, 2);
        assert!(changed);
    }

    #[test]
    fn the_selection_is_clamped_to_the_labels() {
        let mut h = Harness::new();
        let mut selected = 9;
        h.idle(&mut |ui: &mut Ui| {
            ui.add(Tabs::new(&mut selected, LABELS));
        });
        assert_eq!(selected, 2);
    }

    #[test]
    fn arrow_keys_move_the_selection_and_stop_at_the_ends() {
        let mut h = Harness::new();
        let mut selected = 0;
        h.idle(&mut redraw(&mut selected));
        // Tab moves keyboard focus to the first trigger, as it would for a keyboard user.
        h.key(Key::Tab, &mut redraw(&mut selected));
        assert_eq!(selected, 0);

        for _ in 0..3 {
            h.key(Key::ArrowRight, &mut redraw(&mut selected));
        }
        assert_eq!(selected, 2, "clamped at the last tab");
        h.key(Key::ArrowLeft, &mut redraw(&mut selected));
        assert_eq!(selected, 1);
    }

    #[test]
    fn no_labels_is_harmless() {
        let mut h = Harness::new();
        let mut selected = 0;
        h.idle(&mut |ui: &mut Ui| {
            ui.add(Tabs::new(&mut selected, Vec::<String>::new()));
        });
        assert_eq!(selected, 0);
    }
}
