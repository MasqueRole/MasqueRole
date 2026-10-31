//! Chat bubble: one message in the conversation.

use egui::{
    Align, Color32, Frame, Layout, Margin, Response, RichText, Sense, Stroke, TextStyle,
    TextWrapMode, Ui, Widget, WidgetText,
};

use crate::theme::{spacing, Theme, ThemeExt, BORDER_WIDTH};

/// The widest a bubble grows, as a fraction of the space available.
const MAX_WIDTH_FRACTION: f32 = 0.8;

/// Shown after streaming text, like a text cursor. Plain ASCII, because the bundled fonts
/// have no block-element glyphs (they draw as an empty box).
const STREAMING_CURSOR: &str = "|";

/// Who a [`ChatBubble`] speaks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BubbleKind {
    /// The player: right-aligned, in the primary color.
    User,
    /// A character: left-aligned, with the speaker's name above.
    Character,
    /// The narrator: centered, outlined, in italics.
    Narrator,
}

/// A chat message. Text wraps and can be selected.
#[derive(Clone, Debug)]
pub struct ChatBubble {
    kind: BubbleKind,
    speaker: Option<String>,
    text: String,
    streaming: bool,
    tag: Option<String>,
}

impl ChatBubble {
    fn new(kind: BubbleKind, speaker: Option<String>, text: impl Into<String>) -> Self {
        Self {
            kind,
            speaker,
            text: text.into(),
            streaming: false,
            tag: None,
        }
    }

    /// A message from the player.
    pub fn user(text: impl Into<String>) -> Self {
        Self::new(BubbleKind::User, None, text)
    }

    /// A message from a named character.
    pub fn character(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(BubbleKind::Character, Some(name.into()), text)
    }

    /// A narrator message.
    pub fn narrator(text: impl Into<String>) -> Self {
        Self::new(BubbleKind::Narrator, None, text)
    }

    /// Show a cursor after the text while the response is still arriving.
    pub fn streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    /// A small tag above the bubble, such as `#12`, that names the message so it can be
    /// reached with a command.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Which kind of bubble this is.
    pub fn kind(&self) -> BubbleKind {
        self.kind
    }

    /// The fill, text color, and border for a bubble kind.
    fn colors(kind: BubbleKind, t: &Theme) -> (Color32, Color32, Stroke) {
        match kind {
            BubbleKind::User => (t.primary, t.primary_foreground, Stroke::NONE),
            BubbleKind::Character => (t.secondary, t.secondary_foreground, Stroke::NONE),
            BubbleKind::Narrator => (
                Color32::TRANSPARENT,
                t.muted_foreground,
                Stroke::new(BORDER_WIDTH, t.border),
            ),
        }
    }
}

impl Widget for ChatBubble {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let (fill, text_color, stroke) = Self::colors(self.kind, &t);
        let max_width = ui.available_width() * MAX_WIDTH_FRACTION;
        let align = match self.kind {
            BubbleKind::User => Align::Max,
            BubbleKind::Character => Align::Min,
            BubbleKind::Narrator => Align::Center,
        };

        let mut text = self.text;
        if self.streaming {
            text.push_str(STREAMING_CURSOR);
        }
        let mut rich = RichText::new(text).size(t.text_base).color(text_color);
        if self.kind == BubbleKind::Narrator {
            rich = rich.italics();
        }

        // Measure the wrapped text first and give the bubble exactly that width. This keeps
        // the size independent of the alignment of the layout the bubble is drawn in.
        let wrap_width = (max_width - 2.0 * spacing::MD).max(0.0);
        let galley = WidgetText::from(rich.clone()).into_galley(
            ui,
            Some(TextWrapMode::Wrap),
            wrap_width,
            TextStyle::Body,
        );
        let content_width = (galley.size().x.ceil() + 1.0).min(wrap_width);

        ui.with_layout(Layout::top_down(align), |ui| {
            ui.spacing_mut().item_spacing.y = spacing::XS;
            let heading = match (&self.speaker, &self.tag) {
                (Some(name), Some(tag)) => Some(format!("{name}  {tag}")),
                (Some(name), None) => Some(name.clone()),
                (None, Some(tag)) => Some(tag.clone()),
                (None, None) => None,
            };
            if let Some(heading) = heading {
                ui.label(
                    RichText::new(heading)
                        .size(t.text_xs)
                        .color(t.muted_foreground),
                );
            }
            let frame = Frame::new()
                .fill(fill)
                .stroke(stroke)
                .corner_radius(t.corner_lg())
                .inner_margin(Margin::symmetric(spacing::MD as i8, spacing::SM as i8))
                .show(ui, |ui| {
                    ui.set_width(content_width);
                    ui.add(egui::Label::new(rich).wrap());
                })
                .response;
            // On top of the text, so a click anywhere on the bubble picks it. Dragging still
            // reaches the text underneath, so it can be selected.
            ui.interact(frame.rect, frame.id.with("pick"), Sense::click())
        })
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;
    use crate::ThemeMode;

    const LONG: &str = "The fog rolled in over the harbor, thick as wool, and the lamp \
        of the Karrow lighthouse swung through it in slow, patient arcs while the keeper \
        counted the boats that had not come home.";

    fn bubble_rect(h: &mut Harness, bubble: &ChatBubble) -> egui::Rect {
        let mut rect = egui::Rect::NOTHING;
        h.idle(&mut |ui: &mut Ui| {
            ui.set_width(600.0);
            rect = ui.add(bubble.clone()).rect;
        });
        rect
    }

    /// The height a bubble takes in the layout, including the line above it.
    fn taken_height(h: &mut Harness, bubble: &ChatBubble) -> f32 {
        let mut height = 0.0;
        h.idle(&mut |ui: &mut Ui| {
            let before = ui.cursor().min.y;
            ui.add(bubble.clone());
            height = ui.cursor().min.y - before;
        });
        height
    }

    #[test]
    fn a_tag_adds_a_line_above_even_without_a_speaker() {
        let mut h = Harness::new();
        let plain = taken_height(&mut h, &ChatBubble::user("Hi."));
        let tagged = taken_height(&mut h, &ChatBubble::user("Hi.").tag("#12"));
        assert!(tagged > plain);
        let named = taken_height(&mut h, &ChatBubble::character("Mira", "Hi."));
        let both = taken_height(&mut h, &ChatBubble::character("Mira", "Hi.").tag("#12"));
        assert!(
            (both - named).abs() < 0.5,
            "the tag shares the speaker's line"
        );
    }

    #[test]
    fn clicking_a_bubble_reports_a_click() {
        let mut h = Harness::new();
        let mut rect = egui::Rect::NOTHING;
        h.idle(&mut |ui| rect = ui.add(ChatBubble::narrator("Fog.")).rect);
        let mut clicked = false;
        h.click(rect.center(), &mut |ui| {
            clicked |= ui.add(ChatBubble::narrator("Fog.")).clicked();
        });
        assert!(clicked);
    }

    #[test]
    fn constructors_set_kind_and_speaker() {
        assert_eq!(ChatBubble::user("hi").kind(), BubbleKind::User);
        let c = ChatBubble::character("Mira", "hello");
        assert_eq!(
            (c.kind(), c.speaker.as_deref()),
            (BubbleKind::Character, Some("Mira"))
        );
        assert_eq!(ChatBubble::narrator("fog").kind(), BubbleKind::Narrator);
        assert!(ChatBubble::user("x").streaming(true).streaming);
    }

    #[test]
    fn bubbles_are_capped_to_a_share_of_the_width_and_wrap() {
        let mut h = Harness::new();
        let (mut rect, mut available) = (egui::Rect::NOTHING, 0.0);
        h.idle(&mut |ui: &mut Ui| {
            available = ui.available_width();
            rect = ui.add(ChatBubble::user(LONG)).rect;
        });
        assert!(
            rect.width() <= available * MAX_WIDTH_FRACTION + 0.5,
            "{} of {available}",
            rect.width()
        );
        assert!(
            rect.height() > 40.0,
            "long text wraps to several lines: {}",
            rect.height()
        );
    }

    #[test]
    fn short_messages_shrink_to_fit() {
        let mut h = Harness::new();
        let short = bubble_rect(&mut h, &ChatBubble::user("Hi."));
        let long = bubble_rect(&mut h, &ChatBubble::user(LONG));
        assert!(short.width() < long.width() / 2.0);
    }

    #[test]
    fn every_kind_of_bubble_shrinks_to_fit_its_text() {
        let mut h = Harness::new();
        let makers: [fn(&str) -> ChatBubble; 3] = [
            |text| ChatBubble::user(text),
            |text| ChatBubble::narrator(text),
            |text| ChatBubble::character("Mira", text),
        ];
        for make in makers {
            let short = bubble_rect(&mut h, &make("Hi."));
            let long = bubble_rect(&mut h, &make(LONG));
            assert!(
                short.width() < long.width() / 2.0,
                "{:?}: {} vs {}",
                make("x").kind(),
                short.width(),
                long.width()
            );
        }
    }

    #[test]
    fn users_sit_right_and_characters_sit_left() {
        let mut h = Harness::new();
        let user = bubble_rect(&mut h, &ChatBubble::user("Hi."));
        let character = bubble_rect(&mut h, &ChatBubble::character("Mira", "Hi."));
        let narrator = bubble_rect(&mut h, &ChatBubble::narrator("Hi."));
        assert!(user.center().x > character.center().x);
        assert!(narrator.center().x > character.center().x);
        assert!(narrator.center().x < user.center().x);
    }

    #[test]
    fn streaming_adds_room_for_the_cursor() {
        let mut h = Harness::new();
        let plain = bubble_rect(&mut h, &ChatBubble::character("Mira", "Hello"));
        let streaming = bubble_rect(
            &mut h,
            &ChatBubble::character("Mira", "Hello").streaming(true),
        );
        assert!(streaming.width() > plain.width());
    }

    #[test]
    fn each_kind_has_distinct_readable_colors_in_both_themes() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let t = match mode {
                ThemeMode::Dark => Theme::dark(),
                _ => Theme::light(),
            };
            let user = ChatBubble::colors(BubbleKind::User, &t);
            let character = ChatBubble::colors(BubbleKind::Character, &t);
            assert_ne!(user.0, character.0, "user and character look different");
            for kind in [BubbleKind::User, BubbleKind::Character] {
                let (fill, text, _) = ChatBubble::colors(kind, &t);
                let gap = (crate::components::common::luminance(fill)
                    - crate::components::common::luminance(text))
                .abs();
                assert!(
                    gap > 0.4,
                    "{kind:?} text must contrast with its fill: {gap}"
                );
            }
        }
    }

    #[test]
    fn empty_text_renders() {
        let mut h = Harness::new();
        bubble_rect(&mut h, &ChatBubble::user(""));
    }
}
