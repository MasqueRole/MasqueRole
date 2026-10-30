//! Shared loading for YAML cards: characters and lorebooks.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

/// Why a card's contents were rejected.
#[derive(Debug, thiserror::Error)]
pub enum CardError {
    /// Not valid YAML, an unknown or missing field, or a wrong type.
    #[error("invalid YAML: {0}")]
    Parse(#[from] serde_yaml_ng::Error),
    /// Valid YAML that breaks a rule, such as an empty name.
    #[error("{0}")]
    Invalid(String),
}

/// Why a card file could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// Reading the file or directory failed.
    #[error("could not read {path}: {source}")]
    Io {
        /// The file or directory involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The file was read but its contents were rejected.
    #[error("{path}: {source}")]
    Card {
        /// The file involved.
        path: PathBuf,
        /// What was wrong with it.
        source: CardError,
    },
}

/// A YAML card with rules beyond its shape.
pub trait Card: DeserializeOwned {
    /// Check the rules that the YAML structure cannot express.
    fn validate(&self) -> Result<(), CardError>;
}

/// Parse and validate a card from YAML text.
pub fn parse<T: Card>(yaml: &str) -> Result<T, CardError> {
    let card: T = serde_yaml_ng::from_str(yaml)?;
    card.validate()?;
    Ok(card)
}

/// Load and validate a card from a file.
pub fn load<T: Card>(path: &Path) -> Result<T, LoadError> {
    let text = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&text).map_err(|source| LoadError::Card {
        path: path.to_path_buf(),
        source,
    })
}

/// Load every `.yaml` and `.yml` file in a directory, sorted by file name. A bad file is
/// reported in its own slot and does not stop the others from loading.
pub fn load_dir<T: Card>(dir: &Path) -> Result<Vec<Result<T, LoadError>>, LoadError> {
    let io_err = |source| LoadError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(io_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_err)?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|p| {
            p.is_file()
                && p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml")
                })
        })
        .collect();
    paths.sort();
    Ok(paths.iter().map(|p| load(p)).collect())
}

/// Require that `value` has visible text.
pub(crate) fn require_text(what: &str, value: &str) -> Result<(), CardError> {
    if value.trim().is_empty() {
        Err(CardError::Invalid(format!("{what} must not be empty")))
    } else {
        Ok(())
    }
}
