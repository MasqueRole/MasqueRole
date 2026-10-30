//! Design system for InfiniteRoleplay: theme tokens and reusable egui components.
//!
//! Call [`Theme::apply`] once at startup, then build screens from [`components`].

pub mod components;
pub mod fonts;
pub mod theme;

pub use theme::{spacing, Theme, ThemeExt, ThemeMode, BORDER_WIDTH, RING_WIDTH};
