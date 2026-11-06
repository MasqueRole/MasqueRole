//! Chat bubble: one message in the conversation.

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Color32, FontId, Frame, Layout, Margin, Response, RichText, Sense, Stroke, TextStyle,
    TextWrapMode, Ui, Widget, WidgetText,
};

use super::icon::IconKind;
use super::picture::Picture;
use crate::theme::{spacing, Theme, ThemeExt, BORDER_WIDTH};

/// Diameter of the avatar beside a character's message.
const AVATAR_SIZE: f32 = 32.0;

/// The widest a bubble grows, as a fraction of the space available.
const MAX_WIDTH_FRACTION: f32 = 0.8;

/// Shown in a bubble that is waiting for its first words. Plain ASCII dots, because the
/// bundled fonts do not have every symbol.
const THINKING: &str = "Thinking...";

/// How many letters per second the shimmer travels.
const SHIMMER_SPEED: f64 = 9.0;

/// How many letters wide the bright part of the shimmer is on each side of its center.
const SHIMMER_REACH: f64 = 3.5;

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

/// The avatar of a character's message: a picture, or an icon while there is none.
#[derive(Clone)]
struct Avatar {
    picture: Option<egui::TextureHandle>,
    fallback: IconKind,
}

impl std::fmt::Debug for Avatar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Avatar")
            .field("has_picture", &self.picture.is_some())
            .field("fallback", &self.fallback)
            .finish()
    }
}

/// A chat message. Text wraps and can be selected.
#[derive(Clone, Debug)]
pub struct ChatBubble {
    kind: BubbleKind,
    speaker: Option<String>,
    text: String,
    streaming: bool,
    tag: Option<String>,
    avatar: Option<Avatar>,
}

impl ChatBubble {
    fn new(kind: BubbleKind, speaker: Option<String>, text: impl Into<String>) -> Self {
        Self {
            kind,
            speaker,
            text: text.into(),
            streaming: false,
            tag: None,
            avatar: None,
        }
    }

    /// Show a round avatar beside a character's message: `picture`, or `fallback` while there
    /// is no picture. Other kinds of message have no avatar.
    pub fn avatar(mut self, picture: Option<egui::TextureHandle>, fallback: IconKind) -> Self {
        self.avatar = Some(Avatar { picture, fallback });
        self
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

    /// Mark the response as still arriving. While no text has arrived the bubble says
    /// "Thinking..." with a shimmer moving across the letters.
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
    fn ui(mut self, ui: &mut Ui) -> Response {
        match self.avatar.take() {
            Some(Avatar { picture, fallback }) if self.kind == BubbleKind::Character => {
                ui.horizontal_top(|ui| {
                    ui.add(
                        Picture::new(picture.as_ref(), fallback)
                            .size(AVATAR_SIZE)
                            .circle(),
                    );
                    self.body(ui)
                })
                .inner
            }
            _ => self.body(ui),
        }
    }
}

/// The text of a bubble with a bright band of letters travelling across it. Letters in the
/// band have the full `color` and the others a dimmer one, so the text itself shimmers and the
/// bubble stays as it is. Without `time` (reduced motion) the text is still and dim.
fn shimmer_job(
    text: &str,
    color: Color32,
    font_id: FontId,
    italics: bool,
    time: Option<f64>,
) -> LayoutJob {
    let dim = color.gamma_multiply(0.45);
    let count = text.chars().count() as f64;
    // The band starts before the first letter and leaves after the last one, so the text
    // rests dim for a moment between passes.
    let period = count + 2.0 * SHIMMER_REACH + 3.0;
    let center = time.map(|t| (t * SHIMMER_SPEED) % period - SHIMMER_REACH);
    let mut job = LayoutJob::default();
    for (index, letter) in text.chars().enumerate() {
        let strength = center.map_or(0.0, |c| {
            let near = 1.0 - ((index as f64 - c).abs() / SHIMMER_REACH).min(1.0);
            near * near * (3.0 - 2.0 * near)
        });
        job.append(
            &letter.to_string(),
            0.0,
            TextFormat {
                font_id: font_id.clone(),
                color: dim.lerp_to_gamma(color, strength as f32),
                italics,
                ..TextFormat::default()
            },
        );
    }
    job
}

impl ChatBubble {
    /// Draw the bubble with its heading, without the avatar.
    fn body(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let (fill, text_color, stroke) = Self::colors(self.kind, &t);
        let max_width = ui.available_width() * MAX_WIDTH_FRACTION;
        let align = match self.kind {
            BubbleKind::User => Align::Max,
            BubbleKind::Character => Align::Min,
            BubbleKind::Narrator => Align::Center,
        };

        let thinking = self.streaming && self.text.trim().is_empty();
        let text = if thinking {
            THINKING.to_string()
        } else {
            self.text
        };
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
                    if thinking {
                        let reduced = crate::motion::reduced(ui.ctx());
                        let time = ui.input(|i| i.time);
                        if !reduced {
                            ui.ctx().request_repaint();
                        }
                        let job = shimmer_job(
                            THINKING,
                            text_color,
                            FontId::proportional(t.text_base),
                            self.kind == BubbleKind::Narrator,
                            (!reduced).then_some(time),
                        );
                        ui.add(egui::Label::new(job).wrap());
                    } else {
                        ui.add(egui::Label::new(rich).wrap());
                    }
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
    fn streaming_text_has_no_cursor() {
        let mut h = Harness::new();
        let plain = bubble_rect(&mut h, &ChatBubble::character("Mira", "Hello"));
        let streaming = bubble_rect(
            &mut h,
            &ChatBubble::character("Mira", "Hello").streaming(true),
        );
        assert_eq!(streaming.width(), plain.width());
    }

    #[test]
    fn a_bubble_without_words_yet_says_thinking() {
        let mut h = Harness::new();
        let thinking = bubble_rect(&mut h, &ChatBubble::character("Mira", "").streaming(true));
        let said = bubble_rect(&mut h, &ChatBubble::character("Mira", THINKING));
        assert_eq!(thinking.width(), said.width());
        assert!(thinking.width() > 40.0);
        // Spaces alone are not words.
        let blank = bubble_rect(&mut h, &ChatBubble::narrator("  ").streaming(true));
        let narrated = bubble_rect(&mut h, &ChatBubble::narrator(THINKING));
        assert_eq!(blank.width(), narrated.width());
    }

    #[test]
    fn the_shimmer_moves_across_the_letters_and_rests_when_motion_is_reduced() {
        let font = FontId::proportional(14.0);
        let colors = |time: Option<f64>| -> Vec<Color32> {
            // Letters of one color are merged into one section, so count per letter.
            let job = shimmer_job(THINKING, Color32::WHITE, font.clone(), false, time);
            job.sections
                .iter()
                .flat_map(|s| {
                    let letters = job.text[s.byte_range.start.0..s.byte_range.end.0]
                        .chars()
                        .count();
                    std::iter::repeat_n(s.format.color, letters)
                })
                .collect()
        };
        let early = colors(Some(0.5));
        let later = colors(Some(1.0));
        assert_eq!(early.len(), THINKING.chars().count());
        assert_ne!(early, later, "the bright band moved");
        let brightest = |c: &[Color32]| (0..c.len()).max_by_key(|&i| c[i].r()).unwrap();
        assert!(brightest(&later) > brightest(&early), "to the right");
        // Never brighter than the text color, never invisible.
        assert!(early.iter().all(|c| c.a() > 80));
        // Reduced motion: every letter the same dim color.
        let still = colors(None);
        assert!(still.iter().all(|c| *c == still[0]));
        assert!(still[0].r() < 255);
        // The text is the same at every moment.
        let text = |time| shimmer_job(THINKING, Color32::WHITE, font.clone(), false, time).text;
        assert_eq!(text(Some(2.0)), THINKING);
        assert_eq!(text(None), THINKING);
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

    #[test]
    fn an_unbreakable_link_stays_inside_the_bubble() {
        let mut h = Harness::new();
        let link = format!(
            "https://example.com/{}",
            "very-long-path-segment/".repeat(12)
        );
        let mut rect = egui::Rect::NOTHING;
        let mut avail = 0.0;
        h.idle(&mut |ui: &mut Ui| {
            avail = ui.available_width();
            rect = ui.add(ChatBubble::user(&link)).rect;
        });
        assert!(rect.width() <= avail + 1.0, "{rect:?} in {avail}");
    }

    #[test]
    fn an_emoji_only_message_and_an_empty_one_still_get_a_bubble() {
        let mut h = Harness::new();
        let mut sizes = Vec::new();
        h.idle(&mut |ui: &mut Ui| {
            sizes.clear();
            for text in [
                "\u{1f525}\u{1f525}\u{1f525}",
                "",
                "\u{627}\u{644}\u{633}\u{644}\u{627}\u{645}",
            ] {
                sizes.push(ui.add(ChatBubble::character("Mira", text)).rect.size());
            }
        });
        assert!(sizes.iter().all(|s| s.x > 0.0 && s.y > 0.0), "{sizes:?}");
    }

    #[test]
    fn a_character_with_an_avatar_is_drawn_beside_it_and_others_ignore_it() {
        let mut h = Harness::new();
        let (mut plain, mut with, mut user) = (
            egui::Rect::NOTHING,
            egui::Rect::NOTHING,
            egui::Rect::NOTHING,
        );
        h.idle(&mut |ui: &mut Ui| {
            ui.set_width(600.0);
            plain = ui.add(ChatBubble::character("Mira", "Hello there.")).rect;
            with = ui
                .add(ChatBubble::character("Mira", "Hello there.").avatar(None, IconKind::User))
                .rect;
            user = ui
                .add(ChatBubble::user("Hi.").avatar(None, IconKind::User))
                .rect;
        });
        // The returned rectangle is the bubble's, which the avatar pushes to the right.
        assert!(
            with.min.x - plain.min.x >= AVATAR_SIZE,
            "{with:?} vs {plain:?}"
        );
        assert!(
            user.width() < 100.0,
            "a player's message has no avatar: {user:?}"
        );
    }
}
