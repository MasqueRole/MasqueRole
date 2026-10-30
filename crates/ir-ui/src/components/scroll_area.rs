//! Scroll area.

use std::hash::Hash;

use egui::scroll_area::ScrollAreaOutput;
use egui::Ui;

/// A scrolling region. Fills the space it is given by default, so a list does not jump in
/// size as its content changes.
#[must_use = "call `.show(ui, ...)` to draw the scroll area"]
pub struct ScrollArea {
    inner: egui::ScrollArea,
    auto_shrink: bool,
}

impl ScrollArea {
    fn from(inner: egui::ScrollArea) -> Self {
        Self {
            inner,
            auto_shrink: false,
        }
    }

    /// Scroll vertically.
    pub fn vertical() -> Self {
        Self::from(egui::ScrollArea::vertical())
    }

    /// Scroll horizontally.
    pub fn horizontal() -> Self {
        Self::from(egui::ScrollArea::horizontal())
    }

    /// Scroll in both directions.
    pub fn both() -> Self {
        Self::from(egui::ScrollArea::both())
    }

    /// Never grow taller than `height`.
    pub fn max_height(mut self, height: f32) -> Self {
        self.inner = self.inner.max_height(height);
        self
    }

    /// Keep the scroll position at the end as content grows, until the user scrolls up.
    /// Used by the chat view to follow streaming text.
    pub fn stick_to_bottom(mut self, stick: bool) -> Self {
        self.inner = self.inner.stick_to_bottom(stick);
        self
    }

    /// Shrink to fit short content instead of filling the available space.
    pub fn auto_shrink(mut self, shrink: bool) -> Self {
        self.auto_shrink = shrink;
        self
    }

    /// Give the area a stable identity, so its scroll position survives layout changes.
    pub fn id_salt(mut self, id_salt: impl Hash) -> Self {
        self.inner = self.inner.id_salt(id_salt);
        self
    }

    /// Draw the area with `add` as its content.
    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> ScrollAreaOutput<R> {
        self.inner.auto_shrink([self.auto_shrink; 2]).show(ui, add)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    fn tall_content(ui: &mut Ui) {
        ui.allocate_space(egui::vec2(50.0, 2000.0));
    }

    #[test]
    fn returns_the_content_value_and_size() {
        let mut h = Harness::new();
        let mut out = None;
        h.idle(&mut |ui: &mut Ui| {
            out = Some(ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                tall_content(ui);
                5
            }));
        });
        let out = out.unwrap();
        assert_eq!(out.inner, 5);
        assert!(out.content_size.y >= 2000.0);
        assert!(out.inner_rect.height() <= 100.5);
    }

    #[test]
    fn stick_to_bottom_follows_growing_content() {
        let mut h = Harness::new();
        let mut offset = 0.0;
        for _ in 0..3 {
            h.idle(&mut |ui: &mut Ui| {
                offset = ScrollArea::vertical()
                    .id_salt("chat")
                    .max_height(100.0)
                    .stick_to_bottom(true)
                    .show(ui, tall_content)
                    .state
                    .offset
                    .y;
            });
        }
        assert!(offset > 1500.0, "scrolled to the end: {offset}");
    }

    #[test]
    fn without_sticking_the_view_stays_at_the_top() {
        let mut h = Harness::new();
        let mut offset = 1.0;
        for _ in 0..3 {
            h.idle(&mut |ui: &mut Ui| {
                offset = ScrollArea::vertical()
                    .max_height(100.0)
                    .show(ui, tall_content)
                    .state
                    .offset
                    .y;
            });
        }
        assert_eq!(offset, 0.0);
    }

    #[test]
    fn auto_shrink_fits_short_content() {
        let mut h = Harness::new();
        // Measure the space the area takes from the layout: how far down a widget drawn right
        // after it lands. egui sizes a shrinking area from the previous frame's content, so
        // draw twice.
        let (mut shrunk, mut filled) = (0.0, 0.0);
        for _ in 0..2 {
            h.idle(&mut |ui: &mut Ui| {
                let top = ui.cursor().top();
                ScrollArea::vertical()
                    .id_salt("shrinks")
                    .auto_shrink(true)
                    .show(ui, |ui| ui.label("short"));
                shrunk = ui.label("after").rect.top() - top;
            });
        }
        for _ in 0..2 {
            h.idle(&mut |ui: &mut Ui| {
                let top = ui.cursor().top();
                ScrollArea::vertical()
                    .id_salt("fills")
                    .show(ui, |ui| ui.label("short"));
                filled = ui.label("after").rect.top() - top;
            });
        }
        assert!(shrunk < 100.0, "short content takes little room: {shrunk}");
        assert!(
            filled > 400.0,
            "a filling area takes the whole space: {filled}"
        );
    }

    #[test]
    fn horizontal_and_both_render() {
        let mut h = Harness::new();
        h.idle(&mut |ui: &mut Ui| {
            ScrollArea::horizontal().show(ui, |ui| ui.label("wide"));
            ScrollArea::both().show(ui, |ui| ui.label("big"));
        });
    }
}
