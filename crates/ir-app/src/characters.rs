//! The character and world editors.

use std::path::PathBuf;

use ir_core::character::{Character, ExampleDialogue};
use ir_core::lorebook::{LoreEntry, Lorebook};
use ir_ui::components::{
    Button, ButtonSize, ButtonVariant, Card, FormField, Input, Label, LabelTone, Textarea,
};
use ir_ui::spacing;

use eframe::egui::Ui;

/// The editable text of a character card.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharacterForm {
    /// The file being edited, or `None` for a new character.
    pub path: Option<PathBuf>,
    pub name: String,
    pub description: String,
    /// One trait per line.
    pub personality: String,
    pub speaking_style: String,
    pub background: String,
    pub greeting: String,
    /// Pairs of what the user says and how the character answers.
    pub examples: Vec<(String, String)>,
    /// Why the last save was refused.
    pub error: Option<String>,
}

impl CharacterForm {
    /// An empty form for a new character.
    pub fn new() -> Self {
        Self::default()
    }

    /// A form editing `character`, saved to `path`.
    pub fn from_character(path: PathBuf, character: &Character) -> Self {
        Self {
            path: Some(path),
            name: character.name.clone(),
            description: character.description.clone(),
            personality: character.personality.join("\n"),
            speaking_style: character.speaking_style.clone(),
            background: character.background.clone(),
            greeting: character.greeting.clone(),
            examples: character
                .example_dialogue
                .iter()
                .map(|e| (e.user.clone(), e.character.clone()))
                .collect(),
            error: None,
        }
    }

    /// The card the form describes. Blank traits and fully blank examples are dropped.
    pub fn to_character(&self) -> Character {
        Character {
            name: self.name.trim().to_string(),
            description: self.description.trim().to_string(),
            personality: self
                .personality
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect(),
            speaking_style: self.speaking_style.trim().to_string(),
            background: self.background.trim().to_string(),
            greeting: self.greeting.trim().to_string(),
            example_dialogue: self
                .examples
                .iter()
                .filter(|(user, character)| !user.trim().is_empty() || !character.trim().is_empty())
                .map(|(user, character)| ExampleDialogue {
                    user: user.trim().to_string(),
                    character: character.trim().to_string(),
                })
                .collect(),
        }
    }

    /// Draw the form.
    pub fn show(&mut self, ui: &mut Ui) {
        ui.add(
            Label::new(if self.path.is_some() {
                "Edit character"
            } else {
                "New character"
            })
            .heading(),
        );
        let name_error = self.error.clone();
        FormField::new("Name")
            .required(true)
            .maybe_error(name_error)
            .show(ui, |ui| {
                ui.add(
                    Input::new(&mut self.name)
                        .placeholder("Mira Valen")
                        .id_salt("char-name"),
                )
            });
        FormField::new("Description").show(ui, |ui| {
            ui.add(
                Textarea::new(&mut self.description)
                    .rows(2)
                    .id_salt("char-description"),
            )
        });
        FormField::new("Personality")
            .help("One trait per line.")
            .show(ui, |ui| {
                ui.add(
                    Textarea::new(&mut self.personality)
                        .rows(3)
                        .id_salt("char-personality"),
                )
            });
        FormField::new("Speaking style").show(ui, |ui| {
            ui.add(Input::new(&mut self.speaking_style).id_salt("char-style"))
        });
        FormField::new("Background").show(ui, |ui| {
            ui.add(
                Textarea::new(&mut self.background)
                    .rows(4)
                    .id_salt("char-background"),
            )
        });
        FormField::new("Greeting")
            .help("The first message of a new session.")
            .show(ui, |ui| {
                ui.add(
                    Textarea::new(&mut self.greeting)
                        .rows(2)
                        .id_salt("char-greeting"),
                )
            });

        ui.add(Label::new("Example dialogue").tone(LabelTone::Default));
        let mut remove = None;
        for (i, (user, character)) in self.examples.iter_mut().enumerate() {
            Card::new()
                .content(|ui| {
                    FormField::new("The user says").show(ui, |ui| {
                        ui.add(Input::new(user).id_salt(("example-user", i)))
                    });
                    FormField::new("The character answers").show(ui, |ui| {
                        ui.add(Input::new(character).id_salt(("example-character", i)))
                    });
                    if ui
                        .add(
                            Button::new("Remove")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm),
                        )
                        .clicked()
                    {
                        remove = Some(i);
                    }
                })
                .show(ui);
        }
        if let Some(i) = remove {
            self.examples.remove(i);
        }
        if ui
            .add(Button::new("Add example").variant(ButtonVariant::Outline))
            .clicked()
        {
            self.examples.push((String::new(), String::new()));
        }
        ui.add_space(spacing::LG);
    }
}

/// The editable text of a world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldForm {
    /// The file being edited, or `None` for a new world.
    pub path: Option<PathBuf>,
    pub name: String,
    pub setting: String,
    /// Pairs of comma-separated keywords and the text they trigger.
    pub entries: Vec<(String, String)>,
    /// Why the last save was refused.
    pub error: Option<String>,
}

impl WorldForm {
    /// An empty form for a new world.
    pub fn new() -> Self {
        Self::default()
    }

    /// A form editing `world`, saved to `path`.
    pub fn from_lorebook(path: PathBuf, world: &Lorebook) -> Self {
        Self {
            path: Some(path),
            name: world.name.clone(),
            setting: world.setting.clone(),
            entries: world
                .entries
                .iter()
                .map(|e| (e.keys.join(", "), e.content.clone()))
                .collect(),
            error: None,
        }
    }

    /// The lorebook the form describes. Entries with neither keys nor text are dropped.
    pub fn to_lorebook(&self) -> Lorebook {
        Lorebook {
            name: self.name.trim().to_string(),
            setting: self.setting.trim().to_string(),
            entries: self
                .entries
                .iter()
                .filter(|(keys, content)| !keys.trim().is_empty() || !content.trim().is_empty())
                .map(|(keys, content)| LoreEntry {
                    keys: keys
                        .split(',')
                        .map(str::trim)
                        .filter(|k| !k.is_empty())
                        .map(String::from)
                        .collect(),
                    content: content.trim().to_string(),
                })
                .collect(),
        }
    }

    /// Draw the form.
    pub fn show(&mut self, ui: &mut Ui) {
        ui.add(
            Label::new(if self.path.is_some() {
                "Edit world"
            } else {
                "New world"
            })
            .heading(),
        );
        let name_error = self.error.clone();
        FormField::new("Name")
            .required(true)
            .maybe_error(name_error)
            .show(ui, |ui| {
                ui.add(
                    Input::new(&mut self.name)
                        .placeholder("The Misty Coast")
                        .id_salt("world-name"),
                )
            });
        FormField::new("Setting")
            .help("Always part of the prompt.")
            .show(ui, |ui| {
                ui.add(
                    Textarea::new(&mut self.setting)
                        .rows(3)
                        .id_salt("world-setting"),
                )
            });

        ui.add(Label::new("Lore entries"));
        ui.add(
            Label::new("An entry is added to the prompt only when one of its keywords appears in the recent messages.")
                .small()
                .muted(),
        );
        let mut remove = None;
        for (i, (keys, content)) in self.entries.iter_mut().enumerate() {
            Card::new()
                .content(|ui| {
                    FormField::new("Keywords")
                        .help("Separated by commas.")
                        .show(ui, |ui| ui.add(Input::new(keys).id_salt(("lore-keys", i))));
                    FormField::new("Text").show(ui, |ui| {
                        ui.add(Textarea::new(content).rows(2).id_salt(("lore-content", i)))
                    });
                    if ui
                        .add(
                            Button::new("Remove")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm),
                        )
                        .clicked()
                    {
                        remove = Some(i);
                    }
                })
                .show(ui);
        }
        if let Some(i) = remove {
            self.entries.remove(i);
        }
        if ui
            .add(Button::new("Add entry").variant(ButtonVariant::Outline))
            .clicked()
        {
            self.entries.push((String::new(), String::new()));
        }
        ui.add_space(spacing::LG);
    }
}

#[cfg(test)]
mod tests {
    use ir_core::cards::Card as _;

    use super::*;

    const MIRA: &str = r#"
name: "Mira Valen"
description: "A cartographer."
personality: [curious, sarcastic]
speaking_style: "Dry."
background: "Maps coasts."
greeting: "Ah, another one who got lost?"
example_dialogue:
  - user: "Where to?"
    character: "Past the map."
"#;

    #[test]
    fn a_character_survives_a_trip_through_the_form() {
        let card = Character::from_yaml(MIRA).unwrap();
        let form = CharacterForm::from_character(PathBuf::from("mira.yaml"), &card);
        assert_eq!(form.personality, "curious\nsarcastic");
        assert_eq!(
            form.path.as_deref(),
            Some(std::path::Path::new("mira.yaml"))
        );
        assert_eq!(form.to_character(), card);
    }

    #[test]
    fn form_text_is_trimmed_and_blank_rows_are_dropped() {
        let form = CharacterForm {
            name: "  Bo  ".into(),
            personality: "kind\n\n  \n  brave  \n".into(),
            examples: vec![
                ("  hi ".into(), " hello ".into()),
                (String::new(), "  ".into()),
                ("only user".into(), String::new()),
            ],
            ..CharacterForm::new()
        };
        let card = form.to_character();
        assert_eq!(card.name, "Bo");
        assert_eq!(card.personality, ["kind", "brave"]);
        // The blank row is gone; a half-filled one is kept so validation can explain it.
        assert_eq!(card.example_dialogue.len(), 2);
        assert_eq!(card.example_dialogue[0].user, "hi");
    }

    #[test]
    fn an_empty_form_is_not_a_valid_character_and_a_filled_one_is() {
        assert!(CharacterForm::new().to_character().validate().is_err());
        let ok = CharacterForm {
            name: "Bo".into(),
            ..CharacterForm::new()
        };
        assert!(ok.to_character().validate().is_ok());
        let half = CharacterForm {
            name: "Bo".into(),
            examples: vec![("only user".into(), String::new())],
            ..CharacterForm::new()
        };
        assert!(half
            .to_character()
            .validate()
            .unwrap_err()
            .to_string()
            .contains("example_dialogue #1"));
    }

    #[test]
    fn a_world_survives_a_trip_through_the_form() {
        let world = Lorebook::from_yaml(
            "name: Coast\nsetting: Foggy.\nentries:\n  - keys: [lighthouse, keeper]\n    content: Old.\n",
        )
        .unwrap();
        let form = WorldForm::from_lorebook(PathBuf::from("coast.yaml"), &world);
        assert_eq!(
            form.entries,
            [("lighthouse, keeper".to_string(), "Old.".to_string())]
        );
        assert_eq!(form.to_lorebook(), world);
    }

    #[test]
    fn keywords_are_split_on_commas_and_blank_entries_dropped() {
        let form = WorldForm {
            name: "W".into(),
            entries: vec![
                (" a ,b,, c ".into(), " text ".into()),
                ("  ".into(), "".into()),
            ],
            ..WorldForm::new()
        };
        let world = form.to_lorebook();
        assert_eq!(world.entries.len(), 1);
        assert_eq!(world.entries[0].keys, ["a", "b", "c"]);
        assert_eq!(world.entries[0].content, "text");
        assert!(world.validate().is_ok());
    }

    #[test]
    fn an_entry_without_keywords_fails_validation() {
        let form = WorldForm {
            name: "W".into(),
            entries: vec![(String::new(), "text without keys".into())],
            ..WorldForm::new()
        };
        assert!(form.to_lorebook().validate().is_err());
        assert!(
            WorldForm::new().to_lorebook().validate().is_err(),
            "a world needs a name"
        );
    }
}
