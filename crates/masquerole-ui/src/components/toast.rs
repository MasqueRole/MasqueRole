//! Toast notifications.

use std::time::Duration;

use egui::{
    Align2, Area, Color32, Context, FontId, Frame, Id, Margin, Order, Rect, RichText, Sense,
    Shadow, Stroke, Vec2,
};

use super::button::{Button, ButtonSize, ButtonVariant};
use super::icon::{self, IconKind};
use crate::motion;
use crate::theme::{spacing, ThemeExt, BORDER_WIDTH};

/// Width of a toast, in logical pixels.
const TOAST_WIDTH: f32 = 320.0;

/// Diameter of the round badge behind the icon.
const BADGE_SIZE: f32 = 32.0;

/// Size of the icon in the badge.
const ICON_SIZE: f32 = 16.0;

/// Width reserved for the close button.
const CLOSE_SIZE: f32 = 32.0;

/// How far a toast travels while it enters and leaves, in logical pixels.
const SLIDE: f32 = 24.0;

/// How long a normal toast stays, in seconds.
const DEFAULT_SECONDS: f64 = 4.0;

/// How long an error toast stays, in seconds. Errors need more time to be read.
const DESTRUCTIVE_SECONDS: f64 = 8.0;

/// Visual style of a [`Toast`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastKind {
    /// A neutral message.
    #[default]
    Default,
    /// An error or warning.
    Destructive,
}

/// One notification.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    title: String,
    description: Option<String>,
    kind: ToastKind,
    icon: Option<IconKind>,
    seconds: f64,
}

impl Toast {
    /// A neutral toast with this title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            kind: ToastKind::Default,
            icon: None,
            seconds: DEFAULT_SECONDS,
        }
    }

    /// An error toast with this title. It stays longer than a neutral one.
    pub fn destructive(title: impl Into<String>) -> Self {
        Self {
            kind: ToastKind::Destructive,
            seconds: DESTRUCTIVE_SECONDS,
            ..Self::new(title)
        }
    }

    /// Detail text below the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// The icon that says what happened, such as a copy or save icon. Without one a neutral
    /// toast shows an info icon and an error toast a warning icon.
    pub fn icon(mut self, icon: IconKind) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The icon that is shown.
    pub fn icon_kind(&self) -> IconKind {
        self.icon.unwrap_or(match self.kind {
            ToastKind::Default => IconKind::Info,
            ToastKind::Destructive => IconKind::Warning,
        })
    }

    /// How long the toast stays, in seconds.
    pub fn seconds(mut self, seconds: f64) -> Self {
        self.seconds = seconds.max(0.0);
        self
    }

    /// The visual style.
    pub fn kind(&self) -> ToastKind {
        self.kind
    }
}

struct Entry {
    id: u64,
    toast: Toast,
    /// When the toast first appeared, in egui time. `None` until it is first shown.
    shown_at: Option<f64>,
    /// When the toast started to leave, in egui time. It is drawn until the exit is over.
    leaving_at: Option<f64>,
}

impl Entry {
    fn expires_at(&self) -> Option<f64> {
        self.shown_at.map(|at| at + self.toast.seconds)
    }

    /// Whether the toast has not started to leave.
    fn is_live(&self) -> bool {
        self.leaving_at.is_none()
    }
}

/// The queue of toasts. Keep one in the app state, `push` toasts from anywhere that can
/// reach it, and call [`Toasts::show`] once per frame.
#[derive(Default)]
pub struct Toasts {
    entries: Vec<Entry>,
    next_id: u64,
}

impl Toasts {
    /// An empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a toast.
    pub fn push(&mut self, toast: Toast) {
        self.entries.push(Entry {
            id: self.next_id,
            toast,
            shown_at: None,
            leaving_at: None,
        });
        self.next_id += 1;
    }

    /// How many toasts are queued. A toast that is on its way out does not count.
    pub fn len(&self) -> usize {
        self.entries.iter().filter(|e| e.is_live()).count()
    }

    /// Whether no toasts are queued.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Start the clock on new toasts, start the exit of expired ones, and remove the ones
    /// that finished leaving. `now` is egui time in seconds. Returns when the next toast
    /// expires, if any.
    pub fn tick(&mut self, now: f64) -> Option<f64> {
        for entry in &mut self.entries {
            entry.shown_at.get_or_insert(now);
            if entry.is_live() && entry.expires_at().is_some_and(|at| at <= now) {
                entry.leaving_at = Some(now);
            }
        }
        let gone = f64::from(motion::EXIT_SECONDS) + 0.05;
        self.entries
            .retain(|e| e.leaving_at.is_none_or(|at| now < at + gone));
        self.entries
            .iter()
            .filter(|e| e.is_live())
            .filter_map(Entry::expires_at)
            .min_by(f64::total_cmp)
    }

    /// Start the exit of the toast with this id.
    fn dismiss(&mut self, id: u64, now: f64) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id && e.is_live()) {
            entry.leaving_at = Some(now);
        }
    }

    /// Draw the toasts in the bottom-right corner, newest at the bottom.
    pub fn show(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        if let Some(next) = self.tick(now) {
            ctx.request_repaint_after(Duration::from_secs_f64((next - now).max(0.0)));
        }
        if self.entries.is_empty() {
            return;
        }
        if self.entries.iter().any(|e| !e.is_live()) {
            // Come back to remove the toast once it has left.
            ctx.request_repaint_after(Duration::from_secs_f32(motion::EXIT_SECONDS + 0.06));
        }

        let t = ctx.ir_theme();
        let mut dismissed = Vec::new();
        Area::new(Id::new("masquerole-ui-toasts"))
            .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-spacing::LG, -spacing::LG))
            .order(Order::Foreground)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = spacing::SM;
                for entry in &self.entries {
                    // A toast slides up and fades in, and leaves the way it came, only faster.
                    let key = Id::new(("masquerole-ui-toast", entry.id));
                    let live = entry.is_live();
                    let shown = motion::fade(
                        ctx,
                        key.with("fade"),
                        live,
                        if live {
                            motion::ENTER_SECONDS
                        } else {
                            motion::EXIT_SECONDS
                        },
                    );
                    let slide =
                        (1.0 - motion::eased(
                            ctx,
                            key.with("slide"),
                            live,
                            motion::ENTER_SECONDS,
                            motion::EXIT_SECONDS,
                        )) * SLIDE;
                    let accent = match entry.toast.kind {
                        ToastKind::Default => t.foreground,
                        ToastKind::Destructive => t.destructive,
                    };
                    let slot = ui.available_rect_before_wrap();
                    let mut frame_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(slot.translate(egui::vec2(0.0, slide)))
                            .layout(*ui.layout()),
                    );
                    frame_ui.set_opacity(shown);
                    let response = Frame::new()
                        .fill(t.popover)
                        .stroke(Stroke::new(BORDER_WIDTH, t.border))
                        .corner_radius(t.corner_lg())
                        .inner_margin(Margin::same(spacing::LG as i8))
                        .shadow(Shadow {
                            offset: [0, 4],
                            blur: 16,
                            spread: 0,
                            color: Color32::from_black_alpha(if t.is_dark { 90 } else { 28 }),
                        })
                        .show(&mut frame_ui, |ui| {
                            ui.set_width(TOAST_WIDTH);
                            ui.horizontal(|ui| {
                                // The icon sits in a round badge tinted with its color.
                                let (badge, _) =
                                    ui.allocate_exact_size(Vec2::splat(BADGE_SIZE), Sense::hover());
                                ui.painter().circle_filled(
                                    badge.center(),
                                    BADGE_SIZE / 2.0,
                                    accent.gamma_multiply(0.16),
                                );
                                icon::paint(
                                    ui.painter(),
                                    Rect::from_center_size(badge.center(), Vec2::splat(ICON_SIZE)),
                                    accent,
                                    entry.toast.icon_kind(),
                                );
                                ui.vertical(|ui| {
                                    ui.set_width(
                                        TOAST_WIDTH
                                            - BADGE_SIZE
                                            - CLOSE_SIZE
                                            - 2.0 * ui.spacing().item_spacing.x,
                                    );
                                    ui.spacing_mut().item_spacing.y = spacing::XS;
                                    if entry.toast.description.is_none() {
                                        // One line: center it next to the round badge.
                                        let line = ui.fonts(|f| {
                                            f.row_height(&FontId::proportional(t.text_sm))
                                        });
                                        ui.add_space(((BADGE_SIZE - line) / 2.0).max(0.0));
                                    }
                                    ui.label(
                                        RichText::new(&entry.toast.title)
                                            .size(t.text_sm)
                                            .strong()
                                            .color(t.foreground),
                                    );
                                    if let Some(description) = &entry.toast.description {
                                        ui.label(
                                            RichText::new(description)
                                                .size(t.text_sm)
                                                .color(t.muted_foreground),
                                        );
                                    }
                                });
                                let close = Button::new("")
                                    .icon(IconKind::Close)
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Icon);
                                if ui.add(close).clicked() {
                                    dismissed.push(entry.id);
                                }
                            });
                        })
                        .response;
                    ui.advance_cursor_after_rect(response.rect.translate(egui::vec2(0.0, -slide)));
                }
            });
        for id in dismissed {
            self.dismiss(id, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn toasts_expire_after_their_duration() {
        let mut toasts = Toasts::new();
        toasts.push(Toast::new("Saved"));
        toasts.push(Toast::destructive("Failed"));
        assert_eq!(toasts.len(), 2);

        assert_eq!(
            toasts.tick(10.0),
            Some(14.0),
            "the clock starts at the first tick"
        );
        toasts.tick(13.9);
        assert_eq!(toasts.len(), 2);
        assert_eq!(toasts.tick(14.1), Some(18.0));
        assert_eq!(
            toasts.len(),
            1,
            "the neutral toast is gone, the error stays longer"
        );
        assert_eq!(toasts.tick(18.1), None);
        assert!(toasts.is_empty());
    }

    #[test]
    fn a_toast_pushed_later_gets_its_own_clock() {
        let mut toasts = Toasts::new();
        toasts.push(Toast::new("First").seconds(2.0));
        toasts.tick(0.0);
        toasts.push(Toast::new("Second").seconds(2.0));
        toasts.tick(1.5);
        toasts.tick(2.1);
        assert_eq!(toasts.len(), 1, "only the first expired");
        let live = toasts.entries.iter().find(|e| e.is_live()).unwrap();
        assert_eq!(live.toast.title, "Second");
    }

    #[test]
    fn builders_set_fields() {
        let t = Toast::destructive("No key")
            .description("Add one in Settings")
            .seconds(-3.0);
        assert_eq!(t.kind(), ToastKind::Destructive);
        assert_eq!(t.description.as_deref(), Some("Add one in Settings"));
        assert_eq!(t.seconds, 0.0, "negative durations are clamped");
        assert_eq!(Toast::new("x").kind(), ToastKind::Default);
    }

    #[test]
    fn a_toast_shows_the_icon_of_what_happened_or_one_for_its_kind() {
        assert_eq!(Toast::new("x").icon_kind(), IconKind::Info);
        assert_eq!(Toast::destructive("x").icon_kind(), IconKind::Warning);
        assert_eq!(
            Toast::new("x").icon(IconKind::Save).icon_kind(),
            IconKind::Save
        );
        assert_eq!(
            Toast::destructive("x").icon(IconKind::Trash).icon_kind(),
            IconKind::Trash
        );
    }

    #[test]
    fn showing_draws_and_keeps_unexpired_toasts() {
        let mut h = Harness::new();
        let mut toasts = Toasts::new();
        toasts.push(Toast::new("Hello").description("World"));
        toasts.push(Toast::destructive("Oops"));
        h.ctx_frame(vec![], &mut |ctx| toasts.show(ctx));
        assert_eq!(toasts.len(), 2);
    }

    #[test]
    fn clicking_the_close_button_dismisses_a_toast() {
        let mut h = Harness::new();
        let mut toasts = Toasts::new();
        toasts.push(Toast::new("Dismiss me"));
        h.ctx_frame(vec![], &mut |ctx| toasts.show(ctx));

        // The close button is at the right edge of the toast, in the bottom-right corner.
        let spacing = h.ctx.style().spacing.interact_size.y;
        let pos = egui::pos2(
            800.0 - spacing::LG - spacing::LG - spacing / 2.0,
            600.0 - spacing::LG - spacing::LG - spacing / 2.0,
        );
        h.ctx_frame(vec![egui::Event::PointerMoved(pos)], &mut |ctx| {
            toasts.show(ctx)
        });
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        h.ctx_frame(vec![press(true)], &mut |ctx| toasts.show(ctx));
        h.ctx_frame(vec![press(false)], &mut |ctx| toasts.show(ctx));
        assert!(toasts.is_empty());
    }
}
