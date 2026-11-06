//! Drawing helpers shared by components.

use egui::{Color32, CornerRadius, Id, Rect, Stroke, StrokeKind, Ui};

use crate::theme::{Theme, RING_WIDTH};

/// Seconds a control takes to change color when it is hovered, pressed, or released.
pub(crate) const TRANSITION: f32 = 0.12;

/// The color a control should be drawn in now, moving toward `target` over [`TRANSITION`]
/// seconds instead of jumping. `id` must be stable and unique per animated color. The first
/// frame draws `target` at once.
pub(crate) fn transition(ui: &Ui, id: Id, target: Color32) -> Color32 {
    let [r, g, b, a] = target.to_array();
    let channel = |i: u8, value: u8| {
        ui.ctx()
            .animate_value_with_time(id.with(i), f32::from(value), TRANSITION)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    // The channels are premultiplied, so blending them blends the visible colors.
    Color32::from_rgba_premultiplied(channel(0, r), channel(1, g), channel(2, b), channel(3, a))
}

/// Fade a color for a disabled control.
pub(crate) fn dim(color: Color32, enabled: bool) -> Color32 {
    if enabled {
        color
    } else {
        color.gamma_multiply(0.5)
    }
}

/// Draw the focus ring just outside `rect`.
pub(crate) fn focus_ring(ui: &Ui, rect: Rect, radius: CornerRadius, theme: &Theme) {
    ui.painter().rect_stroke(
        rect,
        radius,
        Stroke::new(RING_WIDTH, theme.ring),
        StrokeKind::Outside,
    );
}

/// Approximate perceived brightness, 0.0 (black) to 1.0 (white). Used to check contrast in tests.
#[cfg(test)]
pub(crate) fn luminance(c: Color32) -> f32 {
    (0.2126 * f32::from(c.r()) + 0.7152 * f32::from(c.g()) + 0.0722 * f32::from(c.b())) / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_color_moves_toward_its_target_over_several_frames() {
        let ctx = egui::Context::default();
        let (from, to) = (Color32::BLACK, Color32::WHITE);
        let mut seen = Vec::new();
        for (frame, target) in [from, to, to, to, to].into_iter().enumerate() {
            let input = egui::RawInput {
                time: Some(frame as f64 * 0.05),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    seen.push(transition(ui, Id::unique("t"), target));
                });
            })
            .drop_without_applying_deltas();
        }
        // The first frame draws the target at once; the change then takes several frames.
        assert_eq!(seen[0], from);
        assert!(seen[1].r() < 255 && seen[1].r() >= seen[0].r(), "{seen:?}");
        assert!(seen[2].r() > seen[1].r(), "{seen:?}");
        assert_eq!(seen[4], to);
    }

    #[test]
    fn dim_only_changes_disabled_colors() {
        let c = Color32::from_rgb(200, 100, 50);
        assert_eq!(dim(c, true), c);
        assert_ne!(dim(c, false), c);
    }
}
