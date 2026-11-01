//! Character cards.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cards::{self, require_text, Card, CardError, LoadError};

/// One example exchange that shows how the character talks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExampleDialogue {
    /// What the user says.
    pub user: String,
    /// How the character answers.
    pub character: String,
}

/// A character card. Only `name` is required.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Character {
    /// The character's name.
    pub name: String,
    /// A short description.
    #[serde(default)]
    pub description: String,
    /// Personality traits.
    #[serde(default)]
    pub personality: Vec<String>,
    /// How the character speaks.
    #[serde(default)]
    pub speaking_style: String,
    /// Backstory.
    #[serde(default)]
    pub background: String,
    /// What the character says first.
    #[serde(default)]
    pub greeting: String,
    /// Example exchanges.
    #[serde(default)]
    pub example_dialogue: Vec<ExampleDialogue>,
    /// File name of the character's picture in the library's images folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

impl Card for Character {
    fn validate(&self) -> Result<(), CardError> {
        require_text("character name", &self.name)?;
        for (i, example) in self.example_dialogue.iter().enumerate() {
            let n = i + 1;
            require_text(&format!("example_dialogue #{n} user line"), &example.user)?;
            require_text(
                &format!("example_dialogue #{n} character line"),
                &example.character,
            )?;
        }
        Ok(())
    }
}

impl Character {
    /// Parse and validate a character from YAML text.
    pub fn from_yaml(yaml: &str) -> Result<Self, CardError> {
        cards::parse(yaml)
    }

    /// Load a character from a YAML file.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        cards::load(path)
    }

    /// Load every character file in a directory. See [`cards::load_dir`].
    pub fn load_dir(dir: &Path) -> Result<Vec<Result<Self, LoadError>>, LoadError> {
        cards::load_dir(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const README_CARD: &str = r#"
name: "Mira Valen"
description: "A cartographer and reluctant adventurer."
personality:
  - curious
  - sarcastic
  - fiercely loyal to friends
speaking_style: "Dry, short sentences, the occasional cartography term."
background: |
  Mira grew up in the harbor towns of the north and has
  been mapping uncharted coastlines for ten years.
greeting: "Ah, another one who got lost?"
example_dialogue:
  - user: "Where to next?"
    character: "Wherever the map runs out. That's where it gets interesting."
"#;

    #[test]
    fn the_image_is_optional_and_only_written_when_set() {
        let plain = Character::from_yaml("name: Bo").unwrap();
        assert_eq!(plain.image, None);
        assert!(!serde_yaml_ng::to_string(&plain).unwrap().contains("image"));

        let pictured = Character::from_yaml(
            "name: Bo
image: bo.png",
        )
        .unwrap();
        assert_eq!(pictured.image.as_deref(), Some("bo.png"));
        let again = Character::from_yaml(&serde_yaml_ng::to_string(&pictured).unwrap()).unwrap();
        assert_eq!(again, pictured);
    }

    #[test]
    fn parses_the_readme_example() {
        let c = Character::from_yaml(README_CARD).unwrap();
        assert_eq!(c.name, "Mira Valen");
        assert_eq!(
            c.personality,
            ["curious", "sarcastic", "fiercely loyal to friends"]
        );
        assert!(c.background.starts_with("Mira grew up"));
        assert!(c.background.contains("ten years."));
        assert_eq!(c.greeting, "Ah, another one who got lost?");
        assert_eq!(c.example_dialogue.len(), 1);
        assert_eq!(c.example_dialogue[0].user, "Where to next?");
    }

    #[test]
    fn only_the_name_is_required() {
        let c = Character::from_yaml("name: Bo").unwrap();
        assert_eq!(c.name, "Bo");
        assert!(c.personality.is_empty() && c.example_dialogue.is_empty());
        assert_eq!(c.greeting, "");
    }

    #[test]
    fn missing_name_is_a_parse_error() {
        assert!(matches!(
            Character::from_yaml("description: nameless"),
            Err(CardError::Parse(_))
        ));
    }

    #[test]
    fn blank_name_is_invalid() {
        let err = Character::from_yaml("name: '   '").unwrap_err();
        assert!(matches!(err, CardError::Invalid(_)));
        assert!(err.to_string().contains("name"));
    }

    #[test]
    fn unknown_fields_are_rejected_so_typos_surface() {
        let err = Character::from_yaml("name: Bo\npersonalty: [kind]").unwrap_err();
        assert!(matches!(err, CardError::Parse(_)));
        assert!(err.to_string().contains("personalty"));
    }

    #[test]
    fn wrong_types_are_rejected() {
        assert!(Character::from_yaml("name: Bo\npersonality: curious").is_err());
        assert!(Character::from_yaml("name: [a, b]").is_err());
    }

    #[test]
    fn example_dialogue_needs_both_lines() {
        let err =
            Character::from_yaml("name: Bo\nexample_dialogue:\n  - user: hi\n    character: ''\n")
                .unwrap_err();
        assert!(err.to_string().contains("example_dialogue #1"));
        // A missing line is caught by the structure itself.
        assert!(Character::from_yaml("name: Bo\nexample_dialogue:\n  - user: hi\n").is_err());
    }

    #[test]
    fn malformed_yaml_is_a_parse_error() {
        assert!(matches!(
            Character::from_yaml("name: [unclosed"),
            Err(CardError::Parse(_))
        ));
    }

    #[test]
    fn round_trips_through_yaml() {
        let c = Character::from_yaml(README_CARD).unwrap();
        let text = serde_yaml_ng::to_string(&c).unwrap();
        assert_eq!(Character::from_yaml(&text).unwrap(), c);
    }

    #[test]
    fn load_reports_the_file_in_errors() {
        let dir = std::env::temp_dir().join(format!("masquerole-core-char-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("b-good.yaml"), "name: Good").unwrap();
        std::fs::write(dir.join("a-bad.yml"), "name: ''").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a card").unwrap();

        let loaded = Character::load_dir(&dir).unwrap();
        assert_eq!(loaded.len(), 2, "only .yaml and .yml files are read");
        let err = loaded[0].as_ref().unwrap_err();
        assert!(matches!(err, LoadError::Card { .. }));
        assert!(err.to_string().contains("a-bad.yml"));
        assert_eq!(loaded[1].as_ref().unwrap().name, "Good");

        assert!(matches!(
            Character::load(&dir.join("missing.yaml")),
            Err(LoadError::Io { .. })
        ));
        assert!(matches!(
            Character::load_dir(&dir.join("nope")),
            Err(LoadError::Io { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
