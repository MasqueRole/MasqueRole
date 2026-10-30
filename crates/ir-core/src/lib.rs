//! Core logic for InfiniteRoleplay: provider client, memory, story tree, storage, settings.
//!
//! This crate must not depend on any UI crate.

pub mod cards;
pub mod character;
pub mod client;
pub mod context;
pub mod db;
pub mod lorebook;
pub mod memory;
pub mod settings;

pub use client::{LlmError, LlmProvider};
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
