//! Drawing helpers shared by components.

use egui::{Color32, CornerRadius, Rect, Stroke, StrokeKind, Ui};

use crate::theme::{Theme, RING_WIDTH};

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
    fn dim_only_changes_disabled_colors() {
        let c = Color32::from_rgb(200, 100, 50);
        assert_eq!(dim(c, true), c);
        assert_ne!(dim(c, false), c);
    }
}
