//! The character and world editors.

use std::path::PathBuf;

use masquerole_core::character::{Character, ExampleDialogue};
use masquerole_core::lorebook::{LoreEntry, Lorebook};
use masquerole_ui::components::{
    commit_pending, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, ChipInput, Dialog,
    FormField, IconKind, Input, Label, LabelTone, Picture, Separator, Textarea, TooltipExt,
};
use masquerole_ui::spacing;

use eframe::egui::{Align, Layout, TextureHandle, Ui};

/// How many keywords a summary row shows before it says how many more there are.
const ROW_KEYWORDS: usize = 3;

/// The widest a keyword chip in a summary row gets.
const ROW_KEYWORD_WIDTH: f32 = 120.0;

/// Width of the editing overlay.
const OVERLAY_WIDTH: f32 = 480.0;

/// What a summary row asked for.
#[derive(Default)]
struct RowAction {
    edit: bool,
    delete: bool,
}

/// One line of a list of entries: badges for `keywords`, then `text` cut to fit, with the
/// buttons to edit and delete it. A click anywhere on the text edits it, too.
fn summary_row(ui: &mut Ui, keywords: &[String], title: &str, text: &str) -> RowAction {
    let mut action = RowAction::default();
    ui.horizontal(|ui| {
        ui.set_min_height(32.0);
        for key in keywords.iter().take(ROW_KEYWORDS) {
            ui.add(
                Badge::new(key)
                    .variant(BadgeVariant::Secondary)
                    .max_width(ROW_KEYWORD_WIDTH),
            );
        }
        if keywords.len() > ROW_KEYWORDS {
            ui.add(
                Badge::new(format!("+{}", keywords.len() - ROW_KEYWORDS))
                    .variant(BadgeVariant::Outline),
            );
        }
        if keywords.is_empty() && title.is_empty() && text.is_empty() {
            ui.add(Label::new("Empty").small().muted());
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let button = |icon| {
                Button::new("")
                    .icon(icon)
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Icon)
            };
            action.delete |= ui
                .add(button(IconKind::Trash))
                .ir_tooltip("Delete")
                .clicked();
            action.edit |= ui.add(button(IconKind::Pen)).ir_tooltip("Edit").clicked();
            let shown = if title.is_empty() { text } else { title };
            let width = ui.available_width();
            let label = ui.add(Label::new(shown).small().muted().max_width(width));
            action.edit |= label.interact(eframe::egui::Sense::click()).clicked();
        });
    });
    ui.add(Separator::horizontal());
    action
}

/// The picture row at the top of an editor: the current picture (or a placeholder) with
/// buttons to choose or remove it. Returns whether the user asked to choose a picture; the
/// caller opens the file dialog. Removing clears `image` right here.
fn picture_row(
    ui: &mut Ui,
    image: &mut Option<String>,
    texture: Option<&TextureHandle>,
    fallback: IconKind,
) -> bool {
    let mut choose = false;
    ui.horizontal(|ui| {
        ui.add(Picture::new(texture, fallback));
        ui.vertical(|ui| {
            ui.add(Label::new("Picture").tone(LabelTone::Default));
            ui.add(
                Label::new("Shown in the editor. PNG, JPEG, or WebP.")
                    .small()
                    .muted(),
            );
            ui.horizontal(|ui| {
                let text = if image.is_some() {
                    "Change"
                } else {
                    "Add picture"
                };
                choose = ui
                    .add(
                        Button::new(text)
                            .icon(IconKind::Image)
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm),
                    )
                    .clicked();
                if image.is_some()
                    && ui
                        .add(
                            Button::new("Remove")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm),
                        )
                        .clicked()
                {
                    *image = None;
                }
            });
        });
    });
    ui.add_space(spacing::SM);
    choose
}

/// The editable text of a character card.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharacterForm {
    /// The file being edited, or `None` for a new character.
    pub path: Option<PathBuf>,
    pub name: String,
    /// Other names the character answers to, separated by commas.
    pub aliases: String,
    pub description: String,
    /// One trait per line.
    pub personality: String,
    pub speaking_style: String,
    pub background: String,
    pub greeting: String,
    /// Pairs of what the user says and how the character answers.
    pub examples: Vec<(String, String)>,
    /// The example open in the overlay, if any.
    pub editing_example: Option<ExampleDraft>,
    /// File name of the picture in the library, if any.
    pub image: Option<String>,
    /// Why the last save was refused.
    pub error: Option<String>,
}

/// An example dialogue being edited in the overlay.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExampleDraft {
    /// The example being changed, or `None` for a new one.
    pub index: Option<usize>,
    pub user: String,
    pub character: String,
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
            aliases: character.aliases.join(", "),
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
            editing_example: None,
            image: character.image.clone(),
            error: None,
        }
    }

    /// Open the overlay for example `index`, or for a new example.
    pub fn edit_example(&mut self, index: Option<usize>) {
        let (user, character) = index
            .and_then(|i| self.examples.get(i).cloned())
            .unwrap_or_default();
        self.editing_example = Some(ExampleDraft {
            index,
            user,
            character,
        });
    }

    /// Close the overlay. With `save` the draft replaces its example, or is added; a blank
    /// draft is not added.
    pub fn finish_example(&mut self, save: bool) {
        let Some(draft) = self.editing_example.take() else {
            return;
        };
        if !save {
            return;
        }
        let pair = (draft.user, draft.character);
        match draft.index.filter(|i| *i < self.examples.len()) {
            Some(i) => self.examples[i] = pair,
            None if !pair.0.trim().is_empty() || !pair.1.trim().is_empty() => {
                self.examples.push(pair);
            }
            None => {}
        }
    }

    /// The card the form describes. Blank traits and fully blank examples are dropped.
    pub fn to_character(&self) -> Character {
        Character {
            name: self.name.trim().to_string(),
            aliases: self
                .aliases
                .split(',')
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(String::from)
                .collect(),
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
            image: self.image.clone(),
        }
    }

    /// Draw the form. Returns whether the user asked to choose a picture.
    pub fn show(&mut self, ui: &mut Ui, picture: Option<&TextureHandle>) -> bool {
        ui.add(
            Label::new(if self.path.is_some() {
                "Edit character"
            } else {
                "New character"
            })
            .heading(),
        );
        let choose = picture_row(ui, &mut self.image, picture, IconKind::User);
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
        FormField::new("Nicknames")
            .help("Other names that call this character in a scene with several, separated by commas.")
            .show(ui, |ui| {
                ui.add(
                    Input::new(&mut self.aliases)
                        .placeholder("Cartographer, Mira")
                        .id_salt("char-aliases"),
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
        ui.add(
            Label::new("Short exchanges that show how the character talks.")
                .small()
                .muted(),
        );
        let mut open = None;
        let mut remove = None;
        for (i, (user, character)) in self.examples.iter().enumerate() {
            let row = summary_row(ui, &[], user, character);
            if row.edit {
                open = Some(Some(i));
            }
            if row.delete {
                remove = Some(i);
            }
        }
        if let Some(i) = remove {
            self.examples.remove(i);
        }
        if ui
            .add(
                Button::new("Add example")
                    .icon(IconKind::Plus)
                    .variant(ButtonVariant::Outline),
            )
            .clicked()
        {
            open = Some(None);
        }
        if let Some(index) = open {
            self.edit_example(index);
        }
        self.example_overlay(ui);
        ui.add_space(spacing::LG);
        choose
    }
}

impl CharacterForm {
    /// The overlay for the example being edited, if one is open.
    fn example_overlay(&mut self, ui: &mut Ui) {
        let Some(draft) = self.editing_example.as_mut() else {
            return;
        };
        let title = if draft.index.is_some() {
            "Edit example"
        } else {
            "New example"
        };
        let (mut open, mut save, mut cancel) = (true, false, false);
        Dialog::new("example-overlay", title)
            .width(OVERLAY_WIDTH)
            .show(ui.ctx(), &mut open, |ui| {
                FormField::new("The user says").show(ui, |ui| {
                    ui.add(
                        Textarea::new(&mut draft.user)
                            .rows(2)
                            .id_salt("example-user"),
                    )
                });
                FormField::new("The character answers").show(ui, |ui| {
                    ui.add(
                        Textarea::new(&mut draft.character)
                            .rows(4)
                            .id_salt("example-character"),
                    )
                });
                Dialog::actions(ui, |ui| {
                    save = ui.add(Button::new("Save")).clicked();
                    if ui
                        .add(Button::new("Cancel").variant(ButtonVariant::Outline))
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        open &= !cancel;
        if save {
            self.finish_example(true);
        } else if !open {
            self.finish_example(false);
        }
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
    /// For each entry, the characters who know it (empty: everyone). The editor does not
    /// show this, but keeps what a world file says so saving does not lose it.
    pub known_by: Vec<Vec<String>>,
    /// The entry open in the overlay, if any.
    pub editing_entry: Option<EntryDraft>,
    /// File name of the picture in the library, if any.
    pub image: Option<String>,
    /// Why the last save was refused.
    pub error: Option<String>,
}

/// A lore entry being edited in the overlay.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntryDraft {
    /// The entry being changed, or `None` for a new one.
    pub index: Option<usize>,
    /// The keywords typed so far, each one a chip.
    pub keywords: Vec<String>,
    /// The keyword being typed.
    pub pending: String,
    pub text: String,
}

/// The keywords of an entry, from the comma-separated form it is stored in.
fn split_keywords(keys: &str) -> Vec<String> {
    keys.split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(String::from)
        .collect()
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
            known_by: world.entries.iter().map(|e| e.known_by.clone()).collect(),
            editing_entry: None,
            image: world.image.clone(),
            error: None,
        }
    }

    /// Open the overlay for entry `index`, or for a new entry.
    pub fn edit_entry(&mut self, index: Option<usize>) {
        let (keys, text) = index
            .and_then(|i| self.entries.get(i).cloned())
            .unwrap_or_default();
        self.editing_entry = Some(EntryDraft {
            index,
            keywords: split_keywords(&keys),
            pending: String::new(),
            text,
        });
    }

    /// Close the overlay. With `save` the draft replaces its entry, or is added; a draft
    /// with no keywords and no text is not added. A keyword typed without a comma at the end
    /// still counts.
    pub fn finish_entry(&mut self, save: bool) {
        let Some(mut draft) = self.editing_entry.take() else {
            return;
        };
        if !save {
            return;
        }
        commit_pending(&mut draft.keywords, &mut draft.pending);
        let pair = (draft.keywords.join(", "), draft.text);
        self.known_by.resize(self.entries.len(), Vec::new());
        match draft.index.filter(|i| *i < self.entries.len()) {
            Some(i) => self.entries[i] = pair,
            None if !pair.0.is_empty() || !pair.1.trim().is_empty() => {
                self.entries.push(pair);
                self.known_by.push(Vec::new());
            }
            None => {}
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
                .enumerate()
                .filter(|(_, (keys, content))| {
                    !keys.trim().is_empty() || !content.trim().is_empty()
                })
                .map(|(i, (keys, content))| LoreEntry {
                    known_by: self.known_by.get(i).cloned().unwrap_or_default(),
                    keys: keys
                        .split(',')
                        .map(str::trim)
                        .filter(|k| !k.is_empty())
                        .map(String::from)
                        .collect(),
                    content: content.trim().to_string(),
                })
                .collect(),
            image: self.image.clone(),
        }
    }

    /// Draw the form. Returns whether the user asked to choose a picture.
    pub fn show(&mut self, ui: &mut Ui, picture: Option<&TextureHandle>) -> bool {
        ui.add(
            Label::new(if self.path.is_some() {
                "Edit world"
            } else {
                "New world"
            })
            .heading(),
        );
        let choose = picture_row(ui, &mut self.image, picture, IconKind::Globe);
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
        let mut open = None;
        let mut remove = None;
        for (i, (keys, content)) in self.entries.iter().enumerate() {
            let row = summary_row(ui, &split_keywords(keys), "", content.trim());
            if row.edit {
                open = Some(Some(i));
            }
            if row.delete {
                remove = Some(i);
            }
        }
        if let Some(i) = remove {
            self.entries.remove(i);
            if i < self.known_by.len() {
                self.known_by.remove(i);
            }
        }
        if ui
            .add(
                Button::new("Add entry")
                    .icon(IconKind::Plus)
                    .variant(ButtonVariant::Outline),
            )
            .clicked()
        {
            open = Some(None);
        }
        if let Some(index) = open {
            self.edit_entry(index);
        }
        self.entry_overlay(ui);
        ui.add_space(spacing::LG);
        choose
    }
}

impl WorldForm {
    /// The overlay for the lore entry being edited, if one is open.
    fn entry_overlay(&mut self, ui: &mut Ui) {
        let Some(draft) = self.editing_entry.as_mut() else {
            return;
        };
        let title = if draft.index.is_some() {
            "Edit lore entry"
        } else {
            "New lore entry"
        };
        let (mut open, mut save, mut delete, mut cancel) = (true, false, false, false);
        let existing = draft.index.is_some();
        Dialog::new("lore-overlay", title)
            .width(OVERLAY_WIDTH)
            .description("The entry joins the prompt when one of its keywords appears in the recent messages.")
            .show(ui.ctx(), &mut open, |ui| {
                FormField::new("Keywords")
                    .help("Type a keyword and press comma or Enter. Click a bean to delete it.")
                    .show(ui, |ui| {
                        ui.add(
                            ChipInput::new(&mut draft.keywords, &mut draft.pending)
                                .placeholder("lighthouse, keeper")
                                .id_salt("lore-keywords"),
                        )
                    });
                FormField::new("Lore").show(ui, |ui| {
                    ui.add(
                        Textarea::new(&mut draft.text)
                            .rows(6)
                            .id_salt("lore-text"),
                    )
                });
                Dialog::actions(ui, |ui| {
                    save = ui.add(Button::new("Save")).clicked();
                    if ui
                        .add(Button::new("Cancel").variant(ButtonVariant::Outline))
                        .clicked()
                    {
                        cancel = true;
                    }
                    if existing
                        && ui
                            .add(Button::new("Delete").variant(ButtonVariant::Ghost))
                            .clicked()
                    {
                        delete = true;
                    }
                });
            });
        open &= !cancel;
        if delete {
            if let Some(i) = self.editing_entry.take().and_then(|d| d.index) {
                if i < self.entries.len() {
                    self.entries.remove(i);
                    if i < self.known_by.len() {
                        self.known_by.remove(i);
                    }
                }
            }
        } else if save {
            self.finish_entry(true);
        } else if !open {
            self.finish_entry(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use masquerole_core::cards::Card as _;

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

    #[test]
    fn a_new_lore_entry_is_added_with_its_chips_and_a_half_typed_keyword() {
        let mut form = WorldForm::new();
        form.edit_entry(None);
        let draft = form.editing_entry.as_mut().unwrap();
        draft.keywords = vec!["lighthouse".into(), "keeper".into()];
        draft.pending = "fogglass".into();
        draft.text = "  An old order.  ".into();
        form.finish_entry(true);
        assert!(form.editing_entry.is_none());
        assert_eq!(
            form.entries,
            [(
                "lighthouse, keeper, fogglass".to_string(),
                "  An old order.  ".to_string()
            )]
        );
        let world = form.to_lorebook();
        assert_eq!(world.entries[0].keys, ["lighthouse", "keeper", "fogglass"]);
        assert_eq!(world.entries[0].content, "An old order.");
    }

    #[test]
    fn editing_an_entry_starts_from_its_keywords_and_replaces_it() {
        let mut form = WorldForm::new();
        form.entries = vec![("a, b".into(), "one".into()), ("c".into(), "two".into())];
        form.edit_entry(Some(0));
        let draft = form.editing_entry.as_mut().unwrap();
        assert_eq!(draft.keywords, ["a", "b"]);
        draft.keywords.remove(0);
        draft.text = "changed".into();
        form.finish_entry(true);
        assert_eq!(form.entries[0], ("b".to_string(), "changed".to_string()));
        assert_eq!(form.entries.len(), 2);
    }

    #[test]
    fn cancelling_or_saving_an_empty_entry_changes_nothing() {
        let mut form = WorldForm::new();
        form.edit_entry(None);
        form.editing_entry.as_mut().unwrap().text = "never saved".into();
        form.finish_entry(false);
        assert!(form.entries.is_empty() && form.editing_entry.is_none());

        form.edit_entry(None);
        form.finish_entry(true);
        assert!(form.entries.is_empty(), "a blank draft is not added");
    }

    #[test]
    fn example_dialogue_is_edited_in_a_draft_too() {
        let mut form = CharacterForm::new();
        form.edit_example(None);
        let draft = form.editing_example.as_mut().unwrap();
        draft.user = "Hello?".into();
        draft.character = "Who goes there?".into();
        form.finish_example(true);
        assert_eq!(form.examples.len(), 1);

        form.edit_example(Some(0));
        assert_eq!(form.editing_example.as_ref().unwrap().user, "Hello?");
        form.editing_example.as_mut().unwrap().character = "Hm.".into();
        form.finish_example(true);
        assert_eq!(form.examples[0].1, "Hm.");

        form.edit_example(None);
        form.finish_example(true);
        assert_eq!(form.examples.len(), 1, "a blank draft is not added");
        form.edit_example(Some(0));
        form.finish_example(false);
        assert_eq!(form.examples[0].1, "Hm.");
    }

    #[test]
    fn the_editors_draw_with_the_overlays_open_and_with_worst_case_entries() {
        use eframe::egui;
        let ctx = egui::Context::default();
        masquerole_ui::Theme::apply(&ctx);
        let long = "keyword ".repeat(30);
        let mut world = WorldForm::new();
        world.entries = vec![
            (format!("{long}, a, b, c, d"), "text ".repeat(200)),
            (String::new(), String::new()),
        ];
        let mut character = CharacterForm::new();
        character.examples = vec![("hi ".repeat(100), "answer ".repeat(100))];
        for open in [false, true] {
            if open {
                world.edit_entry(Some(0));
                character.edit_example(Some(0));
            }
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    world.show(ui, None);
                    character.show(ui, None);
                });
            });
        }
    }
}
