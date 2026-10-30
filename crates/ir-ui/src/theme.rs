//! Design tokens: colors, radii, spacing, and typography, plus the light and dark palettes.
//!
//! Call [`Theme::apply`] once at startup. Components then read the active palette
//! through [`ThemeExt::ir_theme`].

use egui::{Color32, Context, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Ui, Visuals};

/// Width of borders and separators, in logical pixels.
///
/// Typed constants, not bare literals: `Stroke::new` is generic over its width, and newer
/// Rust versions reject an untyped float literal there.
pub const BORDER_WIDTH: f32 = 1.0;

/// Width of the focus ring and selection outline, in logical pixels.
pub const RING_WIDTH: f32 = 2.0;

/// The spacing scale in logical pixels. Use only these values.
pub mod spacing {
    /// 4 px.
    pub const XS: f32 = 4.0;
    /// 8 px.
    pub const SM: f32 = 8.0;
    /// 12 px.
    pub const MD: f32 = 12.0;
    /// 16 px.
    pub const LG: f32 = 16.0;
    /// 24 px.
    pub const XL: f32 = 24.0;
    /// 32 px.
    pub const XXL: f32 = 32.0;
}

/// Which palette the app uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    /// Follow the operating system (the default).
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// A complete set of design tokens for one palette.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// True for the dark palette.
    pub is_dark: bool,

    /// Page background.
    pub background: Color32,
    /// Default text color.
    pub foreground: Color32,
    /// Card surface.
    pub card: Color32,
    /// Text on cards.
    pub card_foreground: Color32,
    /// Popover and window surface.
    pub popover: Color32,
    /// Main action color.
    pub primary: Color32,
    /// Text on `primary`.
    pub primary_foreground: Color32,
    /// Secondary surface.
    pub secondary: Color32,
    /// Text on `secondary`.
    pub secondary_foreground: Color32,
    /// Subdued surface.
    pub muted: Color32,
    /// Subdued text.
    pub muted_foreground: Color32,
    /// Hover and selection surface.
    pub accent: Color32,
    /// Text on `accent`.
    pub accent_foreground: Color32,
    /// Dangerous action color.
    pub destructive: Color32,
    /// Text on `destructive`.
    pub destructive_foreground: Color32,
    /// Borders and separators.
    pub border: Color32,
    /// Input borders.
    pub input: Color32,
    /// Focus ring.
    pub ring: Color32,

    /// Story tree: the node you are on.
    pub tree_current: Color32,
    /// Story tree: nodes on the path you took.
    pub tree_explored: Color32,
    /// Story tree: alternative branches.
    pub tree_open: Color32,

    /// Small corner radius.
    pub radius_sm: u8,
    /// Medium corner radius.
    pub radius_md: u8,
    /// Large corner radius.
    pub radius_lg: u8,

    /// Extra-small text size.
    pub text_xs: f32,
    /// Small text size.
    pub text_sm: f32,
    /// Base text size.
    pub text_base: f32,
    /// Large text size.
    pub text_lg: f32,
    /// Extra-large text size.
    pub text_xl: f32,
    /// Double extra-large text size.
    pub text_2xl: f32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

impl Theme {
    /// The light palette.
    pub fn light() -> Self {
        Self {
            is_dark: false,
            background: rgb(0xffffff),
            foreground: rgb(0x09090b),
            card: rgb(0xffffff),
            card_foreground: rgb(0x09090b),
            popover: rgb(0xffffff),
            primary: rgb(0x18181b),
            primary_foreground: rgb(0xfafafa),
            secondary: rgb(0xf4f4f5),
            secondary_foreground: rgb(0x18181b),
            muted: rgb(0xf4f4f5),
            muted_foreground: rgb(0x71717a),
            accent: rgb(0xf4f4f5),
            accent_foreground: rgb(0x18181b),
            destructive: rgb(0xdc2626),
            destructive_foreground: rgb(0xfafafa),
            border: rgb(0xe4e4e7),
            input: rgb(0xe4e4e7),
            ring: rgb(0xa1a1aa),
            tree_current: rgb(0xf5c542),
            tree_explored: rgb(0x8fd694),
            tree_open: rgb(0xd4d4d8),
            ..Self::scale()
        }
    }

    /// The dark palette.
    pub fn dark() -> Self {
        Self {
            is_dark: true,
            background: rgb(0x09090b),
            foreground: rgb(0xfafafa),
            card: rgb(0x0f0f12),
            card_foreground: rgb(0xfafafa),
            popover: rgb(0x18181b),
            primary: rgb(0xfafafa),
            primary_foreground: rgb(0x18181b),
            secondary: rgb(0x27272a),
            secondary_foreground: rgb(0xfafafa),
            muted: rgb(0x27272a),
            muted_foreground: rgb(0xa1a1aa),
            accent: rgb(0x27272a),
            accent_foreground: rgb(0xfafafa),
            destructive: rgb(0x991b1b),
            destructive_foreground: rgb(0xfafafa),
            border: rgb(0x27272a),
            input: rgb(0x3f3f46),
            ring: rgb(0xd4d4d8),
            tree_current: rgb(0xf5c542),
            tree_explored: rgb(0x4f9e5a),
            tree_open: rgb(0x52525b),
            ..Self::scale()
        }
    }

    /// Radii and type scale, shared by both palettes. Colors are placeholders.
    fn scale() -> Self {
        Self {
            is_dark: false,
            background: Color32::PLACEHOLDER,
            foreground: Color32::PLACEHOLDER,
            card: Color32::PLACEHOLDER,
            card_foreground: Color32::PLACEHOLDER,
            popover: Color32::PLACEHOLDER,
            primary: Color32::PLACEHOLDER,
            primary_foreground: Color32::PLACEHOLDER,
            secondary: Color32::PLACEHOLDER,
            secondary_foreground: Color32::PLACEHOLDER,
            muted: Color32::PLACEHOLDER,
            muted_foreground: Color32::PLACEHOLDER,
            accent: Color32::PLACEHOLDER,
            accent_foreground: Color32::PLACEHOLDER,
            destructive: Color32::PLACEHOLDER,
            destructive_foreground: Color32::PLACEHOLDER,
            border: Color32::PLACEHOLDER,
            input: Color32::PLACEHOLDER,
            ring: Color32::PLACEHOLDER,
            tree_current: Color32::PLACEHOLDER,
            tree_explored: Color32::PLACEHOLDER,
            tree_open: Color32::PLACEHOLDER,
            radius_sm: 4,
            radius_md: 6,
            radius_lg: 8,
            text_xs: 11.0,
            text_sm: 13.0,
            text_base: 14.0,
            text_lg: 16.0,
            text_xl: 20.0,
            text_2xl: 24.0,
        }
    }

    /// Install fonts, styles, and visuals for both palettes, and follow the OS theme.
    ///
    /// Call once at startup. Switch palettes later with [`Theme::set_mode`].
    pub fn apply(ctx: &Context) {
        crate::fonts::install(ctx);
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let palette = Self::for_egui(theme);
            ctx.set_style_of(theme, palette.style(theme));
        }
        Self::set_mode(ctx, ThemeMode::System);
    }

    /// Switch between light, dark, and the OS preference at runtime.
    pub fn set_mode(ctx: &Context, mode: ThemeMode) {
        ctx.set_theme(match mode {
            ThemeMode::System => egui::ThemePreference::System,
            ThemeMode::Light => egui::ThemePreference::Light,
            ThemeMode::Dark => egui::ThemePreference::Dark,
        });
    }

    /// The palette for an egui theme.
    pub fn for_egui(theme: egui::Theme) -> Self {
        match theme {
            egui::Theme::Light => Self::light(),
            egui::Theme::Dark => Self::dark(),
        }
    }

    /// Corner radius for `radius_sm`.
    pub fn corner_sm(&self) -> CornerRadius {
        CornerRadius::same(self.radius_sm)
    }

    /// Corner radius for `radius_md`.
    pub fn corner_md(&self) -> CornerRadius {
        CornerRadius::same(self.radius_md)
    }

    /// Corner radius for `radius_lg`.
    pub fn corner_lg(&self) -> CornerRadius {
        CornerRadius::same(self.radius_lg)
    }

    /// Blend `from` toward `to` by `t` (0.0 to 1.0), used for hover and pressed states.
    pub fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
        from.lerp_to_gamma(to, t)
    }

    fn style(&self, theme: egui::Theme) -> egui::Style {
        let mut style = egui::Style {
            visuals: self.visuals(theme),
            ..Default::default()
        };

        style.spacing.item_spacing = egui::vec2(spacing::SM, spacing::SM);
        style.spacing.window_margin = egui::Margin::same(spacing::LG as i8);
        style.spacing.button_padding = egui::vec2(spacing::LG, spacing::SM);
        style.spacing.menu_margin = egui::Margin::same(spacing::SM as i8);
        style.spacing.indent = spacing::LG;
        style.spacing.interact_size.y = 32.0;

        let proportional = |size| FontId::new(size, FontFamily::Proportional);
        style.text_styles = [
            (TextStyle::Small, proportional(self.text_xs)),
            (TextStyle::Body, proportional(self.text_base)),
            (TextStyle::Button, proportional(self.text_sm)),
            (TextStyle::Heading, proportional(self.text_xl)),
            (
                TextStyle::Monospace,
                FontId::new(self.text_sm, FontFamily::Monospace),
            ),
        ]
        .into();
        style
    }

    fn visuals(&self, theme: egui::Theme) -> Visuals {
        let mut v = match theme {
            egui::Theme::Light => Visuals::light(),
            egui::Theme::Dark => Visuals::dark(),
        };

        v.panel_fill = self.background;
        v.window_fill = self.popover;
        v.extreme_bg_color = self.background;
        v.faint_bg_color = self.muted;
        v.code_bg_color = self.muted;
        v.window_stroke = Stroke::new(BORDER_WIDTH, self.border);
        v.window_corner_radius = self.corner_lg();
        v.menu_corner_radius = self.corner_md();
        v.hyperlink_color = self.foreground;
        v.selection.bg_fill = Self::mix(self.background, self.ring, 0.45);
        v.selection.stroke = Stroke::new(RING_WIDTH, self.ring);

        let radius = self.corner_md();
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = self.card;
        w.noninteractive.weak_bg_fill = self.card;
        w.noninteractive.bg_stroke = Stroke::new(BORDER_WIDTH, self.border);
        w.noninteractive.fg_stroke = Stroke::new(BORDER_WIDTH, self.foreground);

        w.inactive.bg_fill = self.secondary;
        w.inactive.weak_bg_fill = self.secondary;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(BORDER_WIDTH, self.secondary_foreground);

        w.hovered.bg_fill = self.accent;
        w.hovered.weak_bg_fill = self.accent;
        w.hovered.bg_stroke = Stroke::new(BORDER_WIDTH, self.border);
        w.hovered.fg_stroke = Stroke::new(BORDER_WIDTH, self.accent_foreground);

        w.active.bg_fill = Self::mix(self.accent, self.foreground, 0.08);
        w.active.weak_bg_fill = w.active.bg_fill;
        w.active.bg_stroke = Stroke::new(BORDER_WIDTH, self.ring);
        w.active.fg_stroke = Stroke::new(BORDER_WIDTH, self.accent_foreground);

        w.open.bg_fill = self.accent;
        w.open.weak_bg_fill = self.accent;
        w.open.bg_stroke = Stroke::new(BORDER_WIDTH, self.border);
        w.open.fg_stroke = Stroke::new(BORDER_WIDTH, self.accent_foreground);

        for state in [
            &mut w.noninteractive,
            &mut w.inactive,
            &mut w.hovered,
            &mut w.active,
            &mut w.open,
        ] {
            state.corner_radius = radius;
            state.expansion = 0.0;
        }
        v
    }
}

/// Access the active [`Theme`] from anything that knows the current egui theme.
pub trait ThemeExt {
    /// The palette matching the currently active light or dark mode.
    fn ir_theme(&self) -> Theme;
}

impl ThemeExt for Context {
    fn ir_theme(&self) -> Theme {
        Theme::for_egui(self.theme())
    }
}

impl ThemeExt for Ui {
    fn ir_theme(&self) -> Theme {
        self.ctx().ir_theme()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_differ_and_share_scale() {
        let (light, dark) = (Theme::light(), Theme::dark());
        assert!(!light.is_dark && dark.is_dark);
        assert_ne!(light.background, dark.background);
        assert_eq!(light.radius_lg, dark.radius_lg);
        assert_eq!(light.text_base, dark.text_base);
    }

    #[test]
    fn no_placeholder_colors_remain() {
        for t in [Theme::light(), Theme::dark()] {
            for c in [
                t.background,
                t.foreground,
                t.card,
                t.card_foreground,
                t.popover,
                t.primary,
                t.primary_foreground,
                t.secondary,
                t.secondary_foreground,
                t.muted,
                t.muted_foreground,
                t.accent,
                t.accent_foreground,
                t.destructive,
                t.destructive_foreground,
                t.border,
                t.input,
                t.ring,
                t.tree_current,
                t.tree_explored,
                t.tree_open,
            ] {
                assert_ne!(c, Color32::PLACEHOLDER);
            }
        }
    }

    #[test]
    fn type_scale_is_increasing() {
        let t = Theme::light();
        let sizes = [
            t.text_xs,
            t.text_sm,
            t.text_base,
            t.text_lg,
            t.text_xl,
            t.text_2xl,
        ];
        assert!(sizes.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn apply_styles_both_themes() {
        let ctx = Context::default();
        Theme::apply(&ctx);
        assert_eq!(
            ctx.style_of(egui::Theme::Dark).visuals.panel_fill,
            Theme::dark().background
        );
        assert_eq!(
            ctx.style_of(egui::Theme::Light).visuals.panel_fill,
            Theme::light().background
        );
    }

    #[test]
    fn set_mode_switches_active_palette() {
        let ctx = Context::default();
        Theme::apply(&ctx);
        Theme::set_mode(&ctx, ThemeMode::Dark);
        let _ = ctx.run(Default::default(), |_| {});
        assert!(ctx.ir_theme().is_dark);
        Theme::set_mode(&ctx, ThemeMode::Light);
        let _ = ctx.run(Default::default(), |_| {});
        assert!(!ctx.ir_theme().is_dark);
    }
}
