//! Core logic for MasqueRole: provider client, memory, story tree, storage, settings.
//!
//! This crate must not depend on any UI crate.

pub mod cards;
pub mod character;
pub mod client;
pub mod commands;
pub mod context;
pub mod db;
pub mod exchange;
pub mod lorebook;
pub mod memory;
pub mod redact;
pub mod scene;
pub mod settings;
pub mod tree;
pub mod update;

pub use client::{LlmError, LlmProvider};
pub use settings::{Settings, SettingsError};

/// Application name shown in window titles and data directories.
pub const APP_NAME: &str = "MasqueRole";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_set() {
        assert_eq!(APP_NAME, "MasqueRole");
    }
}
