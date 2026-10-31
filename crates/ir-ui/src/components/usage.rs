//! A donut chart for how much of a budget is used, with a bar chart in its hover card.

use egui::{Color32, Pos2, Response, Sense, Shape, Stroke, Ui, Vec2, Widget};

use crate::theme::{spacing, ThemeExt};

/// Side of the donut.
const DONUT_SIZE: f32 = 22.0;

/// Ring thickness of the donut.
const RING: f32 = 3.5;

/// Width of the bar in the hover card.
const BAR_WIDTH: f32 = 200.0;

/// Height of the bar in the hover card.
const BAR_HEIGHT: f32 = 10.0;

/// Segments of a full circle.
const SEGMENTS: usize = 48;

/// A small donut showing `fraction` of something used. Hovering it shows a card with a title,
/// a bar chart of the same fraction, and any text lines given.
///
/// Without a fraction (no limit is set) the ring is empty and the card only shows the lines.
#[must_use = "add the donut with `ui.add(...)`"]
#[derive(Clone, Debug)]
pub struct UsageDonut {
    fraction: Option<f32>,
    title: String,
    lines: Vec<String>,
}

impl UsageDonut {
    /// A donut for `fraction` (0 to 1; more counts as full) or `None` if there is no limit.
    pub fn new(fraction: Option<f32>) -> Self {
        Self {
            fraction: fraction.map(|f| f.clamp(0.0, 1.0)),
            title: String::new(),
            lines: Vec::new(),
        }
    }

    /// The heading of the hover card.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// A line of text in the hover card, below the bar.
    pub fn line(mut self, line: impl Into<String>) -> Self {
        self.lines.push(line.into());
        self
    }

    /// The fraction as shown, in whole percent.
    pub fn percent(&self) -> Option<u32> {
        self.fraction.map(|f| (f * 100.0).round() as u32)
    }
}

/// Points of an arc around `center`, starting at the top and going clockwise.
fn arc(center: Pos2, radius: f32, fraction: f32) -> Vec<Pos2> {
    let steps = ((SEGMENTS as f32 * fraction).ceil() as usize).max(2);
    (0..=steps)
        .map(|i| {
            let angle = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * fraction * i as f32 / steps as f32;
            center + Vec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect()
}

/// The color of the filled part: destructive once everything is used.
fn fill_color(t: &crate::Theme, fraction: f32) -> Color32 {
    if fraction >= 1.0 {
        t.destructive
    } else {
        t.primary
    }
}

impl Widget for UsageDonut {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(DONUT_SIZE), Sense::hover());
        let center = rect.center();
        let radius = (DONUT_SIZE - RING) / 2.0;
        let painter = ui.painter();
        painter.circle_stroke(center, radius, Stroke::new(RING, t.muted));
        if let Some(fraction) = self.fraction.filter(|f| *f > 0.0) {
            painter.add(Shape::line(
                arc(center, radius, fraction),
                Stroke::new(RING, fill_color(&t, fraction)),
            ));
        }
        let percent = self.percent();
        response.on_hover_ui(|ui| {
            ui.set_max_width(BAR_WIDTH + 2.0 * spacing::SM);
            ui.spacing_mut().item_spacing.y = spacing::SM;
            if !self.title.is_empty() {
                ui.label(egui::RichText::new(&self.title).strong());
            }
            if let (Some(fraction), Some(percent)) = (self.fraction, percent) {
                let (bar, _) =
                    ui.allocate_exact_size(Vec2::new(BAR_WIDTH, BAR_HEIGHT), Sense::hover());
                let corner = t.corner_sm();
                ui.painter().rect_filled(bar, corner, t.muted);
                if fraction > 0.0 {
                    let mut filled = bar;
                    filled.set_width((bar.width() * fraction).max(BAR_HEIGHT.min(bar.width())));
                    ui.painter()
                        .rect_filled(filled, corner, fill_color(&t, fraction));
                }
                ui.label(format!("{percent}% of the limit used"));
            }
            for line in &self.lines {
                ui.label(egui::RichText::new(line).color(t.muted_foreground));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn fractions_are_clamped_and_reported_in_whole_percent() {
        assert_eq!(UsageDonut::new(Some(0.374)).percent(), Some(37));
        assert_eq!(UsageDonut::new(Some(4.0)).percent(), Some(100));
        assert_eq!(UsageDonut::new(Some(-1.0)).percent(), Some(0));
        assert_eq!(UsageDonut::new(None).percent(), None);
    }

    #[test]
    fn the_arc_starts_at_the_top_and_a_full_one_closes_the_ring() {
        let center = Pos2::new(10.0, 10.0);
        let quarter = arc(center, 5.0, 0.25);
        assert!((quarter[0] - Pos2::new(10.0, 5.0)).length() < 0.01);
        let end = quarter[quarter.len() - 1];
        assert!((end - Pos2::new(15.0, 10.0)).length() < 0.01, "{end:?}");
        let full = arc(center, 5.0, 1.0);
        assert!((full[0] - full[full.len() - 1]).length() < 0.01);
    }

    #[test]
    fn the_donut_has_a_fixed_square_size() {
        let mut h = Harness::new();
        let mut sizes = Vec::new();
        h.idle(&mut |ui| {
            sizes = [None, Some(0.0), Some(0.5), Some(1.0)]
                .map(|f| ui.add(UsageDonut::new(f)).rect.size())
                .to_vec();
        });
        assert!(sizes.iter().all(|s| *s == Vec2::splat(DONUT_SIZE)));
    }

    #[test]
    fn full_is_drawn_in_the_warning_color() {
        let t = crate::Theme::light();
        assert_eq!(fill_color(&t, 0.99), t.primary);
        assert_eq!(fill_color(&t, 1.0), t.destructive);
    }
}
