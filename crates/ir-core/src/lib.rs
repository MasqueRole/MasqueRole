//! Core logic for InfiniteRoleplay: provider client, memory, story tree, storage, settings.
//!
//! This crate must not depend on any UI crate.

pub mod settings;

pub use settings::{Settings, SettingsError};

/// Application name shown in window titles and data directories.
pub const APP_NAME: &str = "InfiniteRoleplay";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_set() {
        assert_eq!(APP_NAME, "InfiniteRoleplay");
    }
}
