//! Slider for a number in a range.

use std::ops::RangeInclusive;

use egui::{
    Color32, CornerRadius, CursorIcon, EventFilter, FontId, Key, Modifiers, Pos2, Rect, Response,
    Sense, Stroke, Ui, Widget, WidgetInfo,
};

use super::common::dim;
use crate::theme::{spacing, ThemeExt, BORDER_WIDTH, RING_WIDTH};

const THUMB_SIZE: f32 = 16.0;
const TRACK_HEIGHT: f32 = 6.0;
/// Shortest track worth drawing.
const MIN_TRACK_WIDTH: f32 = 4.0 * THUMB_SIZE;

/// A horizontal slider bound to an `f64`.
///
/// Drag or click the track, or focus it and use Left/Right (one step), PageUp/PageDown
/// (ten steps), and Home/End. Check `.changed()` on the response to react.
pub struct Slider<'a> {
    value: &'a mut f64,
    range: RangeInclusive<f64>,
    step: Option<f64>,
    decimals: usize,
    show_value: bool,
    enabled: bool,
}

impl<'a> Slider<'a> {
    /// A slider editing `value` within `range`.
    pub fn new(value: &'a mut f64, range: RangeInclusive<f64>) -> Self {
        Self {
            value,
            range,
            step: None,
            decimals: 0,
            show_value: true,
            enabled: true,
        }
    }

    /// Snap values to multiples of `step` from the start of the range.
    pub fn step(mut self, step: f64) -> Self {
        self.step = (step > 0.0).then_some(step);
        self
    }

    /// Digits after the decimal point in the shown value.
    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

    /// Show or hide the numeric value to the right of the track.
    pub fn show_value(mut self, show: bool) -> Self {
        self.show_value = show;
        self
    }

    /// Enable or disable the slider.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Clamp to the range and snap to the step.
    fn snap(&self, value: f64) -> f64 {
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let snapped = match self.step {
            Some(step) => lo + ((value - lo) / step).round() * step,
            None => value,
        };
        snapped.clamp(lo, hi)
    }

    /// How far one keyboard step moves the value.
    fn key_step(&self) -> f64 {
        self.step
            .unwrap_or((*self.range.end() - *self.range.start()) / 100.0)
    }
}

impl Widget for Slider<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let enabled = self.enabled && ui.is_enabled();
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let span = (hi - lo).max(f64::EPSILON);

        let value_galley = self.show_value.then(|| {
            // Reserve room for the widest value so the track does not jump while dragging.
            let widest = format!("{:.*}", self.decimals, lo)
                .len()
                .max(format!("{:.*}", self.decimals, hi).len());
            ui.painter().layout_no_wrap(
                "0".repeat(widest),
                FontId::proportional(t.text_sm),
                Color32::PLACEHOLDER,
            )
        });

        let mut response = None;
        ui.horizontal(|ui| {
            let reserved = value_galley
                .as_ref()
                .map_or(0.0, |g| spacing::SM + g.size().x);
            let width = (ui.available_width() - reserved).max(MIN_TRACK_WIDTH);
            let sense = if enabled {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            };
            let (rect, mut r) = ui.allocate_exact_size(egui::vec2(width, THUMB_SIZE), sense);

            let half = THUMB_SIZE / 2.0;
            let value_at = |x: f32| {
                let fraction = f64::from(
                    ((x - rect.left() - half) / (rect.width() - THUMB_SIZE)).clamp(0.0, 1.0),
                );
                lo + fraction * span
            };
            let before = *self.value;
            let mut value = self.snap(before);

            if enabled {
                if let Some(pos) = r.interact_pointer_pos() {
                    if r.is_pointer_button_down_on() || r.dragged() {
                        value = self.snap(value_at(pos.x));
                    }
                }
                if r.has_focus() {
                    ui.memory_mut(|m| {
                        m.set_focus_lock_filter(
                            r.id,
                            EventFilter {
                                horizontal_arrows: true,
                                ..Default::default()
                            },
                        );
                    });
                    let step = self.key_step();
                    let pressed = |key| ui.input_mut(|i| i.consume_key(Modifiers::NONE, key));
                    if pressed(Key::ArrowRight) {
                        value = self.snap(value + step);
                    }
                    if pressed(Key::ArrowLeft) {
                        value = self.snap(value - step);
                    }
                    if pressed(Key::PageUp) {
                        value = self.snap(value + 10.0 * step);
                    }
                    if pressed(Key::PageDown) {
                        value = self.snap(value - 10.0 * step);
                    }
                    if pressed(Key::Home) {
                        value = lo;
                    }
                    if pressed(Key::End) {
                        value = hi;
                    }
                }
            }
            if value != before {
                *self.value = value;
                r.mark_changed();
            }
            r.widget_info(|| WidgetInfo::slider(enabled, *self.value, ""));

            let fraction = ((*self.value - lo) / span) as f32;
            let center_y = rect.center().y;
            let thumb_x = rect.left() + half + fraction * (rect.width() - THUMB_SIZE);
            let pill = CornerRadius::same(u8::MAX);
            let track = Rect::from_min_max(
                Pos2::new(rect.left(), center_y - TRACK_HEIGHT / 2.0),
                Pos2::new(rect.right(), center_y + TRACK_HEIGHT / 2.0),
            );
            let filled = Rect::from_min_max(track.min, Pos2::new(thumb_x, track.max.y));

            let painter = ui.painter();
            painter.rect_filled(track, pill, dim(t.input, enabled));
            painter.rect_filled(filled, pill, dim(t.primary, enabled));
            let thumb = Pos2::new(thumb_x, center_y);
            painter.circle(
                thumb,
                half,
                dim(t.background, enabled),
                Stroke::new(BORDER_WIDTH, dim(t.primary, enabled)),
            );
            if enabled && (r.hovered() || r.dragged()) {
                painter.circle_stroke(thumb, half + 1.0, Stroke::new(BORDER_WIDTH, t.ring));
            }
            if enabled && r.has_focus() {
                painter.circle_stroke(thumb, half + RING_WIDTH, Stroke::new(RING_WIDTH, t.ring));
            }

            if let Some(galley) = &value_galley {
                let text = format!("{:.*}", self.decimals, *self.value);
                let text_galley = ui.painter().layout_no_wrap(
                    text,
                    FontId::proportional(t.text_sm),
                    Color32::PLACEHOLDER,
                );
                let (text_rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
                let pos = Pos2::new(
                    text_rect.right() - text_galley.size().x,
                    text_rect.center().y - text_galley.size().y / 2.0,
                );
                ui.painter()
                    .galley(pos, text_galley, dim(t.muted_foreground, enabled));
            }

            response = Some(if enabled {
                r.on_hover_cursor(CursorIcon::PointingHand)
            } else {
                r
            });
        });
        response.expect("the slider track is always drawn")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    fn show(h: &mut Harness, value: &mut f64, enabled: bool) -> Response {
        let mut response = None;
        h.idle(&mut |ui: &mut Ui| {
            ui.set_width(300.0);
            response = Some(
                ui.add(
                    Slider::new(value, 0.0..=2.0)
                        .step(0.1)
                        .decimals(1)
                        .enabled(enabled),
                ),
            );
        });
        response.unwrap()
    }

    fn redraw(value: &mut f64) -> impl FnMut(&mut Ui) + '_ {
        move |ui: &mut Ui| {
            ui.set_width(300.0);
            ui.add(Slider::new(value, 0.0..=2.0).step(0.1).decimals(1));
        }
    }

    #[test]
    fn snapping_clamps_and_rounds_to_the_step() {
        let mut v = 0.0;
        let s = Slider::new(&mut v, 0.0..=2.0).step(0.5);
        assert_eq!(s.snap(0.74), 0.5);
        assert_eq!(s.snap(0.76), 1.0);
        assert_eq!(s.snap(-3.0), 0.0);
        assert_eq!(s.snap(9.0), 2.0);
        // Steps are measured from the start of the range.
        let mut v = 0.0;
        let s = Slider::new(&mut v, 1.0..=3.0).step(0.75);
        assert_eq!(s.snap(1.8), 1.75);
        // A non-positive step means continuous.
        let mut v = 0.0;
        assert_eq!(Slider::new(&mut v, 0.0..=1.0).step(0.0).snap(0.123), 0.123);
    }

    #[test]
    fn clicking_the_track_sets_the_value() {
        let mut h = Harness::new();
        let mut value = 0.0;
        let rect = show(&mut h, &mut value, true).rect;

        h.click(rect.center(), &mut redraw(&mut value));
        assert!(
            (value - 1.0).abs() < 0.11,
            "middle of the track is about half: {value}"
        );

        h.click(
            rect.right_center() - egui::vec2(1.0, 0.0),
            &mut redraw(&mut value),
        );
        assert_eq!(value, 2.0);
        h.click(
            rect.left_center() + egui::vec2(1.0, 0.0),
            &mut redraw(&mut value),
        );
        assert_eq!(value, 0.0);
    }

    #[test]
    fn dragging_follows_the_pointer() {
        let mut h = Harness::new();
        let mut value = 0.0;
        let rect = show(&mut h, &mut value, true).rect;
        let start = rect.left_center() + egui::vec2(8.0, 0.0);
        let end = rect.right_center() - egui::vec2(8.0, 0.0);

        h.frame(
            vec![egui::Event::PointerMoved(start)],
            &mut redraw(&mut value),
        );
        h.frame(
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }],
            &mut redraw(&mut value),
        );
        h.frame(
            vec![egui::Event::PointerMoved(end)],
            &mut redraw(&mut value),
        );
        assert_eq!(value, 2.0);
    }

    #[test]
    fn arrow_keys_step_the_focused_slider() {
        let mut h = Harness::new();
        let mut value = 1.0;
        let id = show(&mut h, &mut value, true).id;
        h.ctx.memory_mut(|m| m.request_focus(id));
        h.idle(&mut redraw(&mut value));

        h.key(Key::ArrowRight, &mut redraw(&mut value));
        assert!((value - 1.1).abs() < 1e-9, "{value}");
        h.key(Key::ArrowLeft, &mut redraw(&mut value));
        h.key(Key::ArrowLeft, &mut redraw(&mut value));
        assert!((value - 0.9).abs() < 1e-9, "{value}");
        h.key(Key::PageUp, &mut redraw(&mut value));
        assert!((value - 1.9).abs() < 1e-9, "{value}");
        h.key(Key::End, &mut redraw(&mut value));
        assert_eq!(value, 2.0);
        h.key(Key::ArrowRight, &mut redraw(&mut value));
        assert_eq!(value, 2.0, "stays within the range");
        h.key(Key::Home, &mut redraw(&mut value));
        assert_eq!(value, 0.0);
    }

    #[test]
    fn a_disabled_slider_ignores_input() {
        let mut h = Harness::new();
        let mut value = 1.0;
        let rect = show(&mut h, &mut value, false).rect;
        h.click(
            rect.right_center() - egui::vec2(1.0, 0.0),
            &mut |ui: &mut Ui| {
                ui.set_width(300.0);
                ui.add(Slider::new(&mut value, 0.0..=2.0).enabled(false));
            },
        );
        assert_eq!(value, 1.0);
    }

    #[test]
    fn out_of_range_values_are_pulled_in_when_drawn() {
        let mut h = Harness::new();
        let mut value = 7.0;
        let id = show(&mut h, &mut value, true).id;
        h.ctx.memory_mut(|m| m.request_focus(id));
        h.key(Key::ArrowLeft, &mut redraw(&mut value));
        assert!(value <= 2.0);
    }

    #[test]
    fn renders_without_the_value() {
        let mut h = Harness::new();
        let mut value = 0.5;
        h.idle(&mut |ui: &mut Ui| {
            ui.add(Slider::new(&mut value, 0.0..=1.0).show_value(false));
        });
    }
}
