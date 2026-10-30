//! Tab strip.

use egui::{
    Color32, CornerRadius, CursorIcon, EventFilter, FontId, Frame, Key, Margin, Modifiers,
    Response, Sense, Ui, Widget, WidgetInfo, WidgetType,
};

use super::common::focus_ring;
use super::icon::{self, IconKind};
use crate::theme::{spacing, ThemeExt};

const TRIGGER_HEIGHT: f32 = 28.0;

/// Trigger height of the pill style, the same as a medium button.
const PILL_HEIGHT: f32 = 32.0;

/// Radius larger than half any trigger's height, so the corners become a full pill.
const PILL_RADIUS: u8 = u8::MAX;

/// Size of a trigger's icon.
const ICON_SIZE: f32 = 16.0;

/// A strip of tab triggers bound to the selected index. The caller draws the content of the
/// selected tab below it.
///
/// Tab to a trigger and press Enter or Space to select it, or use Left/Right to move
/// between triggers. Check `.changed()` on the response to react to a new selection.
pub struct Tabs<'a> {
    selected: &'a mut usize,
    labels: Vec<String>,
    icons: Vec<IconKind>,
    pills: bool,
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
            icons: Vec::new(),
            pills: false,
        }
    }

    /// An icon before each label, in the same order as the labels.
    pub fn icons(mut self, icons: impl IntoIterator<Item = IconKind>) -> Self {
        self.icons = icons.into_iter().collect();
        self
    }

    /// Draw the triggers as separate pills without a background strip: round on hover, and
    /// filled when selected. Meant for navigation.
    pub fn pills(mut self, pills: bool) -> Self {
        self.pills = pills;
        self
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

        let frame = if self.pills {
            Frame::new()
        } else {
            Frame::new()
                .fill(t.muted)
                .corner_radius(t.corner_md())
                .inner_margin(Margin::same(spacing::XS as i8))
        };
        let (height, radius) = if self.pills {
            (PILL_HEIGHT, CornerRadius::same(PILL_RADIUS))
        } else {
            (TRIGGER_HEIGHT, t.corner_sm())
        };
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
                        let icon = self.icons.get(i).copied();
                        let icon_width = icon.map_or(0.0, |_| ICON_SIZE + spacing::SM);
                        let pad = if self.pills { spacing::LG } else { spacing::MD };
                        let size = egui::vec2(galley.size().x + icon_width + 2.0 * pad, height);
                        let (rect, r) = ui.allocate_exact_size(size, Sense::click());
                        r.widget_info(|| {
                            WidgetInfo::selected(WidgetType::SelectableLabel, true, active, label)
                        });
                        if r.clicked() {
                            selected = i;
                        }

                        if self.pills {
                            if active {
                                ui.painter().rect_filled(rect, radius, t.secondary);
                            } else if r.hovered() {
                                ui.painter().rect_filled(rect, radius, t.muted);
                            }
                        } else if active {
                            ui.painter().rect_filled(rect, radius, t.background);
                        }
                        let color = if active || r.hovered() {
                            t.foreground
                        } else {
                            t.muted_foreground
                        };
                        let content_width = icon_width + galley.size().x;
                        let start_x = rect.center().x - content_width / 2.0;
                        if let Some(kind) = icon {
                            let icon_rect = egui::Rect::from_center_size(
                                egui::pos2(start_x + ICON_SIZE / 2.0, rect.center().y),
                                egui::Vec2::splat(ICON_SIZE),
                            );
                            icon::paint(ui.painter(), icon_rect, color, kind);
                        }
                        let pos = egui::pos2(
                            start_x + icon_width,
                            rect.center().y - galley.size().y / 2.0,
                        );
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
    fn pills_have_no_strip_and_icons_widen_the_triggers() {
        let mut h = Harness::new();
        let mut selected = 0;
        let (mut strip, mut pills, mut with_icons) = (
            egui::Rect::NOTHING,
            egui::Rect::NOTHING,
            egui::Rect::NOTHING,
        );
        h.idle(&mut |ui: &mut Ui| {
            strip = ui.add(Tabs::new(&mut selected, LABELS)).rect;
            pills = ui.add(Tabs::new(&mut selected, LABELS).pills(true)).rect;
            with_icons = ui
                .add(Tabs::new(&mut selected, LABELS).pills(true).icons([
                    IconKind::Chat,
                    IconKind::User,
                    IconKind::Branch,
                ]))
                .rect;
        });
        assert!(pills.height() >= PILL_HEIGHT && pills.height() < strip.height() + 8.0);
        assert!(with_icons.width() > pills.width() + 3.0 * ICON_SIZE);
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
