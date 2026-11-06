//! Button with variants and sizes.

use egui::text::LayoutJob;
use egui::{Color32, CursorIcon, FontId, Pos2, Response, Sense, Stroke, StrokeKind, Ui, Widget};

use super::common::transition;
use super::icon::{self, IconKind};
use crate::theme::{spacing, ThemeExt, BORDER_WIDTH, RING_WIDTH};

/// Visual style of a [`Button`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Solid primary color.
    #[default]
    Default,
    /// Subdued solid color.
    Secondary,
    /// Transparent with a border.
    Outline,
    /// Transparent until hovered.
    Ghost,
    /// Solid destructive color.
    Destructive,
    /// Looks like a link.
    Link,
}

/// Size of a [`Button`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    /// Compact.
    Sm,
    /// Standard.
    #[default]
    Md,
    /// Large.
    Lg,
    /// Square, for a single glyph.
    Icon,
    /// A smaller square, for a glyph beside a label.
    IconSm,
}

/// A clickable button. Keyboard accessible: Tab to focus, Enter or Space to press.
#[derive(Clone, Debug)]
pub struct Button {
    text: String,
    variant: ButtonVariant,
    size: ButtonSize,
    enabled: bool,
    fill_width: bool,
    icon: Option<IconKind>,
    tint: Option<egui::Color32>,
}

impl Button {
    /// A default-variant, medium-size button.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            variant: ButtonVariant::default(),
            size: ButtonSize::default(),
            enabled: true,
            fill_width: false,
            icon: None,
            tint: None,
        }
    }

    /// Draw the icon and text in this color instead of the variant's, in every state. For a
    /// ghost button that should only look dangerous in its glyph.
    pub fn tint(mut self, color: egui::Color32) -> Self {
        self.tint = Some(color);
        self
    }

    /// Show an icon before the text. With an empty text and [`ButtonSize::Icon`], the
    /// button is just the icon.
    pub fn icon(mut self, icon: IconKind) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Stretch across the available width with the text left-aligned and cut with an
    /// ellipsis if too long. Meant for list rows, such as the sidebar.
    pub fn fill_width(mut self, fill: bool) -> Self {
        self.fill_width = fill;
        self
    }

    /// Set the visual variant.
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set the size.
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Enable or disable the button.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for Button {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let enabled = self.enabled && ui.is_enabled();
        let is_link = self.variant == ButtonVariant::Link;

        let (font_size, pad_x, height) = match self.size {
            ButtonSize::Sm => (t.text_xs, spacing::LG, 28.0),
            ButtonSize::Md => (t.text_sm, spacing::XL, 36.0),
            ButtonSize::Lg => (t.text_base, spacing::XL, 44.0),
            ButtonSize::Icon => (t.text_base, 0.0, 36.0),
            ButtonSize::IconSm => (t.text_sm, 0.0, 28.0),
        };
        let available = ui.available_width();
        let has_text = !self.text.is_empty();
        // The icon is a little larger than the text beside it.
        let icon_px = if is_link {
            0.0
        } else {
            self.icon.map_or(0.0, |_| font_size + 2.0)
        };
        let gap = if icon_px > 0.0 && has_text {
            spacing::SM
        } else {
            0.0
        };
        let wrap_width = if self.fill_width {
            (available - 2.0 * pad_x - icon_px - gap).max(0.0)
        } else {
            f32::INFINITY
        };
        let mut job = LayoutJob::simple(
            self.text.clone(),
            FontId::proportional(font_size),
            Color32::PLACEHOLDER,
            wrap_width,
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.painter().layout_job(job);

        let desired = if is_link {
            galley.size()
        } else if self.fill_width {
            egui::vec2(available, height)
        } else if matches!(self.size, ButtonSize::Icon | ButtonSize::IconSm) {
            egui::vec2(height, height)
        } else {
            egui::vec2(galley.size().x + icon_px + gap + 2.0 * pad_x, height)
        };
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(desired, sense);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &self.text));

        let hovered = enabled && response.hovered();
        let pressed = enabled && response.is_pointer_button_down_on();
        // A press gives a little, like a real button. Links are text and stay put.
        let rect = if is_link {
            rect
        } else {
            let scale = crate::motion::press_scale(ui.ctx(), response.id.with("press"), pressed);
            rect.scale_from_center(scale)
        };
        let (mut bg, mut fg, mut stroke) = match self.variant {
            ButtonVariant::Default => (
                shade(t.primary, t.background, hovered, pressed),
                t.primary_foreground,
                Stroke::NONE,
            ),
            ButtonVariant::Secondary => (
                shade(t.secondary, t.foreground, hovered, pressed),
                t.secondary_foreground,
                Stroke::NONE,
            ),
            ButtonVariant::Outline => (
                if hovered || pressed {
                    shade(t.accent, t.foreground, false, pressed)
                } else {
                    t.background
                },
                if hovered || pressed {
                    t.accent_foreground
                } else {
                    t.foreground
                },
                Stroke::new(BORDER_WIDTH, t.input),
            ),
            ButtonVariant::Ghost => (
                if hovered || pressed {
                    shade(t.accent, t.foreground, false, pressed)
                } else {
                    Color32::TRANSPARENT
                },
                if hovered || pressed {
                    t.accent_foreground
                } else {
                    t.foreground
                },
                Stroke::NONE,
            ),
            ButtonVariant::Destructive => (
                shade(t.destructive, t.background, hovered, pressed),
                t.destructive_foreground,
                Stroke::NONE,
            ),
            ButtonVariant::Link => (Color32::TRANSPARENT, t.primary, Stroke::NONE),
        };
        if let Some(color) = self.tint {
            fg = color;
        }
        if !enabled {
            bg = bg.gamma_multiply(0.5);
            fg = fg.gamma_multiply(0.5);
            stroke.color = stroke.color.gamma_multiply(0.5);
        }
        // Hover and click fade between the colors instead of jumping.
        bg = transition(ui, response.id.with("bg"), bg);
        fg = transition(ui, response.id.with("fg"), fg);
        stroke.color = transition(ui, response.id.with("stroke"), stroke.color);

        // Buttons are pills, and square icon buttons are round.
        let radius = egui::CornerRadius::same(u8::MAX);
        let painter = ui.painter();
        painter.rect(rect, radius, bg, stroke, StrokeKind::Inside);
        let content_width = icon_px + gap + galley.size().x;
        let start_x = if self.fill_width {
            rect.left() + pad_x
        } else {
            rect.center().x - content_width / 2.0
        };
        if let (Some(kind), true) = (self.icon, icon_px > 0.0) {
            let icon_rect = egui::Rect::from_center_size(
                Pos2::new(start_x + icon_px / 2.0, rect.center().y),
                egui::Vec2::splat(icon_px),
            );
            icon::paint(painter, icon_rect, fg, kind);
        }
        let text_pos = Pos2::new(
            start_x + icon_px + gap,
            rect.center().y - galley.size().y / 2.0,
        );
        painter.galley(text_pos, galley.clone(), fg);
        if is_link && hovered {
            let y = text_pos.y + galley.size().y;
            painter.hline(
                text_pos.x..=text_pos.x + galley.size().x,
                y,
                Stroke::new(BORDER_WIDTH, fg),
            );
        }
        if enabled && response.has_focus() {
            painter.rect_stroke(
                rect,
                radius,
                Stroke::new(RING_WIDTH, t.ring),
                StrokeKind::Outside,
            );
        }

        if enabled {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

/// Blend a base color toward `toward` for hover (10%) and pressed (20%) states.
fn shade(base: Color32, toward: Color32, hovered: bool, pressed: bool) -> Color32 {
    let amount = if pressed {
        0.2
    } else if hovered {
        0.1
    } else {
        0.0
    };
    base.lerp_to_gamma(toward, amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_sets_fields() {
        let b = Button::new("Go")
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Lg)
            .enabled(false);
        assert_eq!(b.variant, ButtonVariant::Outline);
        assert_eq!(b.size, ButtonSize::Lg);
        assert!(!b.enabled);
    }

    #[test]
    fn an_icon_makes_the_button_wider_and_an_icon_only_button_stays_square() {
        let ctx = egui::Context::default();
        crate::Theme::apply(&ctx);
        let (mut plain, mut with_icon, mut icon_only) = (0.0, 0.0, egui::Vec2::ZERO);
        ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                plain = ui.add(Button::new("New")).rect.width();
                with_icon = ui.add(Button::new("New").icon(IconKind::Plus)).rect.width();
                icon_only = ui
                    .add(
                        Button::new("")
                            .icon(IconKind::Settings)
                            .size(ButtonSize::Icon),
                    )
                    .rect
                    .size();
            });
        })
        .drop_without_applying_deltas();
        assert!(with_icon > plain + 10.0, "{plain} vs {with_icon}");
        assert_eq!(icon_only.x, icon_only.y);
    }

    #[test]
    fn shade_changes_only_on_interaction() {
        let (a, b) = (Color32::BLACK, Color32::WHITE);
        assert_eq!(shade(a, b, false, false), a);
        assert_ne!(shade(a, b, true, false), a);
        assert_ne!(shade(a, b, true, true), shade(a, b, true, false));
    }

    #[test]
    fn every_variant_and_size_renders() {
        let ctx = egui::Context::default();
        crate::Theme::apply(&ctx);
        ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                for variant in [
                    ButtonVariant::Default,
                    ButtonVariant::Secondary,
                    ButtonVariant::Outline,
                    ButtonVariant::Ghost,
                    ButtonVariant::Destructive,
                    ButtonVariant::Link,
                ] {
                    for size in [
                        ButtonSize::Sm,
                        ButtonSize::Md,
                        ButtonSize::Lg,
                        ButtonSize::Icon,
                    ] {
                        ui.add(Button::new("x").variant(variant).size(size));
                        ui.add(Button::new("x").variant(variant).size(size).enabled(false));
                    }
                }
            });
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn a_press_shrinks_the_button_a_little_and_reduced_motion_does_not() {
        let size_while_pressed = |reduced: bool| {
            let ctx = egui::Context::default();
            crate::Theme::apply(&ctx);
            crate::motion::set_reduced(&ctx, reduced);
            let mut rect = egui::Rect::NOTHING;
            let mut painted = Vec::new();
            for frame in 0..4 {
                let events = if frame == 0 {
                    vec![]
                } else {
                    vec![
                        egui::Event::PointerMoved(rect.center()),
                        egui::Event::PointerButton {
                            pos: rect.center(),
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                };
                let input = egui::RawInput {
                    time: Some(f64::from(frame) * 0.05),
                    events,
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        rect = ui.add(Button::new("Press me")).rect;
                    });
                });
                output.textures_delta.clear();
                painted.push(format!("{:?}", output.shapes));
            }
            painted
        };
        // The pressed frames paint something different from the resting one, unless motion
        // is reduced, where the button only changes color.
        let normal = size_while_pressed(false);
        assert_ne!(normal[0], normal[3]);
        let reduced = size_while_pressed(true);
        assert_ne!(reduced[0], reduced[3], "the color still changes");
    }
}
