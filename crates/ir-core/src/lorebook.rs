//! Lorebooks: world knowledge that is injected only when keywords come up.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cards::{self, require_text, Card, CardError, LoadError};

/// One piece of world knowledge and the keywords that trigger it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoreEntry {
    /// Words or phrases that trigger the entry.
    pub keys: Vec<String>,
    /// The text added to the prompt when triggered.
    pub content: String,
}

impl LoreEntry {
    /// A stable identifier for the entry: its first key. Stored in a story-tree node's
    /// `active_lore` so a restored node knows which entries were active.
    pub fn id(&self) -> &str {
        self.keys.first().map_or("", String::as_str)
    }

    /// Whether any key appears in `text`. Matching ignores case and respects word
    /// boundaries, so `keeper` matches "The Keeper's lamp" but not "beekeeper".
    pub fn matches(&self, text: &str) -> bool {
        let text = text.to_lowercase();
        self.keys
            .iter()
            .any(|key| contains_word(&text, &key.trim().to_lowercase()))
    }
}

/// A lorebook: a setting plus keyword-triggered entries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lorebook {
    /// The world's name.
    pub name: String,
    /// A general description, always relevant.
    #[serde(default)]
    pub setting: String,
    /// Keyword-triggered entries.
    #[serde(default)]
    pub entries: Vec<LoreEntry>,
}

impl Card for Lorebook {
    fn validate(&self) -> Result<(), CardError> {
        require_text("world name", &self.name)?;
        for (i, entry) in self.entries.iter().enumerate() {
            let n = i + 1;
            if entry.keys.iter().all(|k| k.trim().is_empty()) {
                return Err(CardError::Invalid(format!(
                    "entry #{n} needs at least one non-empty key"
                )));
            }
            if entry.keys.iter().any(|k| k.trim().is_empty()) {
                return Err(CardError::Invalid(format!(
                    "entry #{n} has an empty key, which would match everything"
                )));
            }
            require_text(&format!("entry #{n} content"), &entry.content)?;
        }
        Ok(())
    }
}

impl Lorebook {
    /// Parse and validate a lorebook from YAML text.
    pub fn from_yaml(yaml: &str) -> Result<Self, CardError> {
        cards::parse(yaml)
    }

    /// Load a lorebook from a YAML file.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        cards::load(path)
    }

    /// Load every lorebook file in a directory. See [`cards::load_dir`].
    pub fn load_dir(dir: &Path) -> Result<Vec<Result<Self, LoadError>>, LoadError> {
        cards::load_dir(dir)
    }

    /// The entries triggered by any of `recent_messages`, in lorebook order, each once.
    pub fn triggered<S: AsRef<str>>(&self, recent_messages: &[S]) -> Vec<&LoreEntry> {
        self.entries
            .iter()
            .filter(|entry| recent_messages.iter().any(|m| entry.matches(m.as_ref())))
            .collect()
    }
}

/// Whether `needle` occurs in `haystack` with no letter or digit touching either side.
/// Both must already be lowercase.
fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    haystack.match_indices(needle).any(|(start, found)| {
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + found.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const README_BOOK: &str = r#"
name: "The Misty Coast"
setting: "A coastal region where constant fog makes navigation nearly impossible."
entries:
  - keys: ["lighthouse", "keeper"]
    content: "The Karrow lighthouse has been guarded by an order for centuries."
  - keys: ["fogglass"]
    content: "Fogglass is a rare mineral that briefly clears fog."
"#;

    fn book() -> Lorebook {
        Lorebook::from_yaml(README_BOOK).unwrap()
    }

    fn ids<'a>(entries: &[&'a LoreEntry]) -> Vec<&'a str> {
        entries.iter().map(|e| e.id()).collect()
    }

    #[test]
    fn parses_the_readme_example() {
        let b = book();
        assert_eq!(b.name, "The Misty Coast");
        assert!(b.setting.contains("fog"));
        assert_eq!(b.entries.len(), 2);
        assert_eq!(b.entries[0].keys, ["lighthouse", "keeper"]);
        assert_eq!(b.entries[0].id(), "lighthouse");
    }

    #[test]
    fn setting_and_entries_are_optional() {
        let b = Lorebook::from_yaml("name: Bare").unwrap();
        assert!(b.entries.is_empty() && b.setting.is_empty());
    }

    #[test]
    fn validation_errors_name_the_entry() {
        let no_keys = Lorebook::from_yaml("name: W\nentries:\n  - keys: []\n    content: x\n");
        assert!(no_keys.unwrap_err().to_string().contains("entry #1"));

        let blank_key =
            Lorebook::from_yaml("name: W\nentries:\n  - keys: [ok, '  ']\n    content: x\n");
        assert!(blank_key.unwrap_err().to_string().contains("empty key"));

        let no_content = Lorebook::from_yaml(
            "name: W\nentries:\n  - keys: [a]\n    content: x\n  - keys: [b]\n    content: ''\n",
        );
        assert!(no_content
            .unwrap_err()
            .to_string()
            .contains("entry #2 content"));

        assert!(matches!(
            Lorebook::from_yaml("name: ''"),
            Err(CardError::Invalid(_))
        ));
    }

    #[test]
    fn structural_errors_are_parse_errors() {
        assert!(matches!(
            Lorebook::from_yaml("setting: x"),
            Err(CardError::Parse(_))
        ));
        assert!(matches!(
            Lorebook::from_yaml("name: W\nentries:\n  - keys: [a]\n"),
            Err(CardError::Parse(_))
        ));
        assert!(matches!(
            Lorebook::from_yaml("name: W\nentrys: []"),
            Err(CardError::Parse(_))
        ));
    }

    #[test]
    fn matching_ignores_case() {
        let b = book();
        assert_eq!(
            ids(&b.triggered(&["We climbed the LIGHTHOUSE at dusk."])),
            ["lighthouse"]
        );
        assert_eq!(ids(&b.triggered(&["Fogglass glints."])), ["fogglass"]);
    }

    #[test]
    fn any_key_triggers_the_entry() {
        let b = book();
        assert_eq!(ids(&b.triggered(&["Ask the keeper."])), ["lighthouse"]);
    }

    #[test]
    fn matching_respects_word_boundaries() {
        let b = book();
        assert!(b.triggered(&["A beekeeper waves."]).is_empty());
        assert!(b.triggered(&["lighthouses everywhere"]).is_empty());
        assert!(b.triggered(&["fogglass2"]).is_empty());
        // Punctuation and possessives are boundaries.
        assert_eq!(ids(&b.triggered(&["The keeper's lamp"])), ["lighthouse"]);
        assert_eq!(
            ids(&b.triggered(&["\"Lighthouse!\" she cried."])),
            ["lighthouse"]
        );
        assert_eq!(ids(&b.triggered(&["(fogglass)"])), ["fogglass"]);
    }

    #[test]
    fn matching_looks_across_all_recent_messages() {
        let b = book();
        let hits = b.triggered(&["Nothing here.", "Then the fogglass.", "And the keeper."]);
        // Lorebook order, not message order, and each entry once.
        assert_eq!(ids(&hits), ["lighthouse", "fogglass"]);
        let twice = b.triggered(&["keeper", "lighthouse", "keeper"]);
        assert_eq!(twice.len(), 1);
    }

    #[test]
    fn no_messages_or_no_match_triggers_nothing() {
        let b = book();
        assert!(b.triggered::<&str>(&[]).is_empty());
        assert!(b.triggered(&["Nothing relevant."]).is_empty());
        assert!(b.triggered(&[String::new()]).is_empty());
    }

    #[test]
    fn multi_word_and_unicode_keys() {
        let b = Lorebook::from_yaml(
            "name: W\nentries:\n  - keys: ['old harbor', 'Café']\n    content: x\n",
        )
        .unwrap();
        assert_eq!(
            b.triggered(&["Down at the Old   Harbor"]).len(),
            0,
            "spacing must match"
        );
        assert_eq!(b.triggered(&["Down at the old harbor."]).len(), 1);
        assert_eq!(b.triggered(&["Meet at the CAFÉ."]).len(), 1);
        assert!(b.triggered(&["Cafés are nice"]).is_empty());
    }

    #[test]
    fn works_with_owned_strings() {
        let b = book();
        let messages = vec![String::from("the lighthouse")];
        assert_eq!(b.triggered(&messages).len(), 1);
    }
}
