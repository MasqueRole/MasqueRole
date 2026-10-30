//! Font loading.
//!
//! The UI uses egui's bundled fonts: Ubuntu-Light for text and Hack for monospace,
//! both compiled into the binary (no files to ship). To use other fonts, add them to
//! the `FontDefinitions` below with `include_bytes!` and put them first in the family list.

use egui::{Context, FontDefinitions};

/// Load the UI and monospace fonts into egui. Called by [`crate::Theme::apply`].
pub fn install(ctx: &Context) {
    ctx.set_fonts(FontDefinitions::default());
}
