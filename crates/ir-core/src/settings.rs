//! Settings (`settings.toml`) and API key storage.
//!
//! Settings live in the platform data directory. The API key never does: it is kept in
//! the OS keychain per provider, with the `IR_API_KEY` environment variable as a
//! development fallback.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Name of the settings file inside the data directory.
pub const SETTINGS_FILE: &str = "settings.toml";

/// Environment variable consulted when no key is stored in the keychain.
pub const API_KEY_ENV: &str = "IR_API_KEY";

/// Environment variable that moves all app data (settings, sessions, library) to another
/// folder, for a portable install or a clean profile.
pub const DATA_DIR_ENV: &str = "IR_DATA_DIR";

const KEYRING_SERVICE: &str = "InfiniteRoleplay";

/// Errors from loading, saving, or reading secrets.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// The platform has no usable data directory.
    #[error("could not determine the platform data directory")]
    NoDataDir,
    /// Reading or writing a file failed.
    #[error("settings file error at {path}: {source}")]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The settings file is not valid TOML or has wrong types.
    #[error("invalid settings file {path}: {source}")]
    Parse {
        /// The file involved.
        path: PathBuf,
        /// The underlying error.
        source: toml::de::Error,
    },
    /// The settings could not be turned into TOML.
    #[error("could not serialize settings: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// The OS keychain reported an error.
    #[error("keychain error: {0}")]
    Keychain(String),
}

/// All user settings. Every field has a default, so partial files load fine.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Provider connection and generation parameters.
    pub api: ApiSettings,
    /// Context window budgeting.
    pub context: ContextSettings,
    /// Memory layers.
    pub memory: MemorySettings,
    /// Writing style.
    pub style: StyleSettings,
    /// Story tree behavior.
    pub tree: TreeSettings,
    /// Troubleshooting options.
    pub debug: DebugSettings,
    /// Optional cap on token usage.
    pub limits: LimitSettings,
    /// Things the app remembers about the user's interface.
    pub ui: UiSettings,
}

/// What the app remembers about the interface.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    /// The first-run tutorial has been shown (or skipped).
    pub tutorial_seen: bool,
    /// Turn off movement and scaling in the interface. Fades and color changes stay.
    pub reduce_motion: bool,
}

/// An optional cap on how many tokens one story may use.
///
/// Usage is estimated (about one token per four characters) for everything sent to and
/// received from the provider, including summaries and fact extraction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitSettings {
    /// Stop generating once a story reaches [`LimitSettings::token_limit`].
    pub token_limit_enabled: bool,
    /// Estimated tokens one story may use while the limit is on.
    pub token_limit: u32,
}

impl Default for LimitSettings {
    fn default() -> Self {
        Self {
            token_limit_enabled: false,
            token_limit: 200_000,
        }
    }
}

impl LimitSettings {
    /// Whether a story that has used `used` tokens may not generate any more.
    pub fn reached(&self, used: i64) -> bool {
        self.token_limit_enabled && used >= i64::from(self.token_limit)
    }
}

/// Troubleshooting options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugSettings {
    /// Write a log file in the data directory. Takes effect at the next start. API keys
    /// are never written to it.
    pub logging: bool,
}

/// Provider connection and generation parameters. The API key is not stored here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiSettings {
    /// A provider preset name, or `openai-compatible` for any compatible endpoint.
    pub provider: String,
    /// Endpoint of the provider.
    pub base_url: String,
    /// Model name, as given by the provider.
    pub model: String,
    /// Sampling temperature.
    pub temperature: f32,
    /// Maximum tokens per response.
    pub max_tokens: u32,
    /// Stream responses token by token.
    pub stream: bool,
}

impl Default for ApiSettings {
    fn default() -> Self {
        Self {
            provider: "openai-compatible".into(),
            base_url: "https://api.example.com/v1".into(),
            model: "your-model-name".into(),
            temperature: 0.9,
            max_tokens: 800,
            stream: true,
        }
    }
}

/// Context window budgeting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextSettings {
    /// Upper limit for the context sent to the API.
    pub max_context_tokens: u32,
    /// Number of messages kept verbatim.
    pub recent_messages: u32,
    /// Older messages are condensed beyond this point.
    pub summarize_after: u32,
}

impl Default for ContextSettings {
    fn default() -> Self {
        Self {
            max_context_tokens: 24_000,
            recent_messages: 20,
            summarize_after: 30,
        }
    }
}

/// Memory layers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemorySettings {
    /// Use summaries and facts at all.
    pub enabled: bool,
    /// Automatically remember important facts.
    pub fact_extraction: bool,
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            fact_extraction: true,
        }
    }
}

/// Writing style.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StyleSettings {
    /// Name of the active style profile.
    pub profile: String,
    /// Let a narrator describe scenes.
    pub narrator_mode: bool,
}

impl Default for StyleSettings {
    fn default() -> Self {
        Self {
            profile: "default".into(),
            narrator_mode: false,
        }
    }
}

/// How story-tree nodes get their labels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeLabel {
    /// A short AI-generated label.
    #[default]
    Auto,
    /// Ask every time.
    Manual,
}

/// Story tree behavior.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TreeSettings {
    /// Create a node after every turn.
    pub auto_checkpoint: bool,
    /// How nodes are labeled.
    pub node_label: NodeLabel,
    /// Show the tree after branching.
    pub show_tree_on_branch: bool,
}

impl Default for TreeSettings {
    fn default() -> Self {
        Self {
            auto_checkpoint: true,
            node_label: NodeLabel::Auto,
            show_tree_on_branch: true,
        }
    }
}

impl Settings {
    /// The platform data directory for the app, or the folder named by `IR_DATA_DIR`.
    pub fn data_dir() -> Result<PathBuf, SettingsError> {
        Self::data_dir_with(std::env::var_os(DATA_DIR_ENV))
    }

    /// [`Settings::data_dir`] with the override passed in. An empty override counts as unset.
    fn data_dir_with(over: Option<std::ffi::OsString>) -> Result<PathBuf, SettingsError> {
        if let Some(dir) = over.filter(|d| !d.is_empty()) {
            return Ok(PathBuf::from(dir));
        }
        ProjectDirs::from("", "RoleplayHarness", crate::APP_NAME)
            .map(|dirs| dirs.data_dir().to_path_buf())
            .ok_or(SettingsError::NoDataDir)
    }

    /// Where `settings.toml` lives on this platform.
    pub fn default_path() -> Result<PathBuf, SettingsError> {
        Ok(Self::data_dir()?.join(SETTINGS_FILE))
    }

    /// Load from the default location. A missing file yields the defaults.
    pub fn load() -> Result<Self, SettingsError> {
        Self::load_from(&Self::default_path()?)
    }

    /// Save to the default location, creating the directory if needed.
    pub fn save(&self) -> Result<(), SettingsError> {
        self.save_to(&Self::default_path()?)
    }

    /// Load from `path`. A missing file yields the defaults.
    pub fn load_from(path: &Path) -> Result<Self, SettingsError> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|source| SettingsError::Parse {
                path: path.to_path_buf(),
                source,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(SettingsError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Save to `path`, creating parent directories if needed.
    pub fn save_to(&self, path: &Path) -> Result<(), SettingsError> {
        let io_err = |source| SettingsError::Io {
            path: path.to_path_buf(),
            source,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io_err)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?).map_err(io_err)
    }
}

/// Somewhere to keep one secret per provider.
pub trait SecretStore {
    /// The stored secret for `provider`, if any.
    fn get(&self, provider: &str) -> Result<Option<String>, SettingsError>;
    /// Store or replace the secret for `provider`.
    fn set(&self, provider: &str, secret: &str) -> Result<(), SettingsError>;
    /// Remove the secret for `provider`. Removing a missing secret is not an error.
    fn delete(&self, provider: &str) -> Result<(), SettingsError>;
}

/// The OS keychain (Windows Credential Manager, macOS Keychain, Secret Service).
#[derive(Clone, Copy, Debug, Default)]
pub struct KeychainStore;

impl KeychainStore {
    fn entry(provider: &str) -> Result<keyring::Entry, SettingsError> {
        keyring::Entry::new(KEYRING_SERVICE, provider).map_err(keychain_err)
    }
}

fn keychain_err(e: keyring::Error) -> SettingsError {
    SettingsError::Keychain(e.to_string())
}

impl SecretStore for KeychainStore {
    fn get(&self, provider: &str) -> Result<Option<String>, SettingsError> {
        match Self::entry(provider)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keychain_err(e)),
        }
    }

    fn set(&self, provider: &str, secret: &str) -> Result<(), SettingsError> {
        Self::entry(provider)?
            .set_password(secret)
            .map_err(keychain_err)
    }

    fn delete(&self, provider: &str) -> Result<(), SettingsError> {
        match Self::entry(provider)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keychain_err(e)),
        }
    }
}

/// An in-memory store, for tests and for running without a keychain.
#[derive(Debug, Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, provider: &str) -> Result<Option<String>, SettingsError> {
        Ok(self.0.lock().expect("store lock").get(provider).cloned())
    }

    fn set(&self, provider: &str, secret: &str) -> Result<(), SettingsError> {
        self.0
            .lock()
            .expect("store lock")
            .insert(provider.into(), secret.into());
        Ok(())
    }

    fn delete(&self, provider: &str) -> Result<(), SettingsError> {
        self.0.lock().expect("store lock").remove(provider);
        Ok(())
    }
}

/// Find the API key for `provider`: the store first, then `env_key`. Empty values count
/// as missing.
pub fn resolve_api_key(
    store: &dyn SecretStore,
    provider: &str,
    env_key: Option<String>,
) -> Result<Option<String>, SettingsError> {
    let stored = store.get(provider)?.filter(|k| !k.is_empty());
    Ok(stored.or(env_key.filter(|k| !k.is_empty())))
}

/// Find the API key for `provider`, reading `IR_API_KEY` as the fallback.
pub fn api_key(store: &dyn SecretStore, provider: &str) -> Result<Option<String>, SettingsError> {
    resolve_api_key(store, provider, std::env::var(API_KEY_ENV).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique, empty temp directory per test; removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("ir-core-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_token_limit_only_applies_while_it_is_on() {
        let off = LimitSettings::default();
        assert!(!off.reached(10_000_000), "off means no limit");
        let on = LimitSettings {
            token_limit_enabled: true,
            token_limit: 1_000,
        };
        assert!(!on.reached(999));
        assert!(on.reached(1_000));
        assert!(on.reached(5_000));
    }

    #[test]
    fn the_data_directory_can_be_overridden() {
        let custom = Settings::data_dir_with(Some("D:/portable/ir".into())).unwrap();
        assert_eq!(custom, PathBuf::from("D:/portable/ir"));
        // An empty value is the same as not setting it.
        assert_eq!(
            Settings::data_dir_with(Some(String::new().into())).ok(),
            Settings::data_dir_with(None).ok()
        );
    }

    #[test]
    fn the_tutorial_starts_unseen_and_the_flag_round_trips() {
        let mut s = Settings::default();
        assert!(!s.ui.tutorial_seen);
        s.ui.tutorial_seen = true;
        let text = toml::to_string_pretty(&s).unwrap();
        assert!(toml::from_str::<Settings>(&text).unwrap().ui.tutorial_seen);
        // Files from older versions have no [ui] section.
        assert!(
            !toml::from_str::<Settings>("[api]\nmodel = \"m\"")
                .unwrap()
                .ui
                .tutorial_seen
        );
    }

    #[test]
    fn defaults_match_readme() {
        let s = Settings::default();
        assert_eq!(s.api.provider, "openai-compatible");
        assert_eq!(s.api.temperature, 0.9);
        assert_eq!(s.api.max_tokens, 800);
        assert!(s.api.stream);
        assert_eq!(s.context.max_context_tokens, 24_000);
        assert_eq!(s.context.recent_messages, 20);
        assert_eq!(s.context.summarize_after, 30);
        assert!(s.memory.enabled && s.memory.fact_extraction);
        assert_eq!(s.style.profile, "default");
        assert!(!s.style.narrator_mode);
        assert!(s.tree.auto_checkpoint && s.tree.show_tree_on_branch);
        assert!(!s.debug.logging);
        assert!(!s.ui.reduce_motion);
        assert!(!s.limits.token_limit_enabled);
        assert_eq!(s.limits.token_limit, 200_000);
        assert_eq!(s.tree.node_label, NodeLabel::Auto);
    }

    #[test]
    fn round_trips_through_toml() {
        let mut s = Settings::default();
        s.api.model = "some-model".into();
        s.api.temperature = 0.5;
        s.context.recent_messages = 8;
        s.style.narrator_mode = true;
        s.tree.node_label = NodeLabel::Manual;

        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), s);
    }

    #[test]
    fn readme_example_parses() {
        let text = r#"
            [api]
            provider = "ollama"
            base_url = "http://localhost:11434/v1"
            model = "llama"
            [tree]
            node_label = "manual"
        "#;
        let s: Settings = toml::from_str(text).unwrap();
        assert_eq!(s.api.provider, "ollama");
        assert_eq!(s.tree.node_label, NodeLabel::Manual);
        // Unspecified sections and fields fall back to defaults.
        assert_eq!(s.api.max_tokens, 800);
        assert_eq!(s.context, ContextSettings::default());
    }

    #[test]
    fn save_and_load_file() {
        let dir = TempDir::new("save-load");
        let path = dir.0.join("nested").join(SETTINGS_FILE);
        let mut s = Settings::default();
        s.api.model = "saved".into();
        s.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path).unwrap(), s);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = TempDir::new("missing");
        let loaded = Settings::load_from(&dir.0.join(SETTINGS_FILE)).unwrap();
        assert_eq!(loaded, Settings::default());
    }

    #[test]
    fn invalid_file_is_a_parse_error() {
        let dir = TempDir::new("invalid");
        std::fs::create_dir_all(&dir.0).unwrap();
        let path = dir.0.join(SETTINGS_FILE);
        std::fs::write(&path, "api = [not toml").unwrap();
        assert!(matches!(
            Settings::load_from(&path),
            Err(SettingsError::Parse { .. })
        ));
    }

    #[test]
    fn settings_file_has_no_secrets() {
        let text = toml::to_string_pretty(&Settings::default()).unwrap();
        assert!(!text.to_lowercase().contains("key"));
    }

    #[test]
    fn default_path_is_in_app_data_dir() {
        // Skipped silently on systems without a home directory. Platforms differ: Windows
        // adds a `data` folder below the app name, and Linux lowercases the name. So only
        // check that the name is a component, ignoring case.
        if let Ok(path) = Settings::default_path() {
            assert_eq!(path.file_name().unwrap(), SETTINGS_FILE);
            let app_name = crate::APP_NAME.to_lowercase();
            assert!(path
                .components()
                .any(|c| c.as_os_str().to_string_lossy().to_lowercase() == app_name));
        }
    }

    #[test]
    fn stored_key_wins_over_env() {
        let store = MemoryStore::default();
        store.set("xai", "stored").unwrap();
        let key = resolve_api_key(&store, "xai", Some("from-env".into())).unwrap();
        assert_eq!(key.as_deref(), Some("stored"));
    }

    #[test]
    fn env_is_the_fallback() {
        let store = MemoryStore::default();
        let key = resolve_api_key(&store, "xai", Some("from-env".into())).unwrap();
        assert_eq!(key.as_deref(), Some("from-env"));
        assert_eq!(resolve_api_key(&store, "xai", None).unwrap(), None);
        assert_eq!(
            resolve_api_key(&store, "xai", Some(String::new())).unwrap(),
            None
        );
    }

    #[test]
    fn keys_are_per_provider() {
        let store = MemoryStore::default();
        store.set("a", "key-a").unwrap();
        store.set("b", "key-b").unwrap();
        assert_eq!(store.get("a").unwrap().as_deref(), Some("key-a"));
        store.delete("a").unwrap();
        assert_eq!(store.get("a").unwrap(), None);
        assert_eq!(store.get("b").unwrap().as_deref(), Some("key-b"));
        store.delete("a").unwrap();
    }
}
