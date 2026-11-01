//! The text sent to the model: the system prompt, and who a response is attributed to.

use masquerole_core::character::Character;
use masquerole_core::settings::Settings;
use masquerole_core::tree::Turn;

const BASE_PROMPT: &str = "You are the game master of an interactive roleplay. \
Continue the story from the player's last message. Stay in the world and in character, keep \
events consistent with what is already established, and never speak or act for the player. \
Write vivid but concise replies, and leave the player room to respond.";

const NARRATOR_PROMPT: &str = "Narrate in the third person, describing scenes, actions, and \
dialogue as prose.";

const SPEAKER_PROMPT: &str = "You only know what appears below: what you saw and heard, and the \
facts you know. The thoughts, secrets, and background of the other characters are hidden from \
you, and you must never use knowledge your character could not have. Reply only as this \
character, without a name label, and never speak or act for the player or for anyone else. \
Write vividly but concisely, and leave the player room to respond.";

const GROUP_PROMPT: &str = "Several characters are present. Write the scene as prose and \
attribute each line of dialogue to the character speaking it by name.";

/// The system prompt for the current settings and characters.
pub fn system_prompt(settings: &Settings, characters: &[Character]) -> String {
    let mut prompt = BASE_PROMPT.to_string();
    if settings.style.narrator_mode {
        prompt.push_str("\n\n");
        prompt.push_str(NARRATOR_PROMPT);
    }
    if characters.len() > 1 {
        prompt.push_str("\n\n");
        prompt.push_str(GROUP_PROMPT);
    }
    let profile = settings.style.profile.trim();
    if !profile.is_empty() && profile != "default" {
        prompt.push_str(&format!("\n\nWrite in this style: {profile}."));
    }
    prompt
}

/// The system prompt for one character answering in a scene with several. It names who is
/// speaking and what they may know; their card and the rest of the context follow it.
pub fn speaker_prompt(settings: &Settings, speaker: &str) -> String {
    let mut prompt = format!(
        "You are {speaker}, a character in an interactive roleplay. Answer in character, \
staying in the world and keeping events consistent with what is established.\n\n{SPEAKER_PROMPT}"
    );
    let profile = settings.style.profile.trim();
    if !profile.is_empty() && profile != "default" {
        prompt.push_str(&format!("\n\nWrite in this style: {profile}."));
    }
    prompt
}

/// A model's reply without a leading speaker label such as `Mira Valen:` or `**Mira:**`.
pub fn clean_reply(text: &str, speaker: &str) -> String {
    let text = text.trim();
    let lower = text.to_lowercase();
    let name = speaker.to_lowercase();
    for label in [
        format!("{name}:"),
        format!("**{name}:**"),
        format!("[{name}]:"),
    ] {
        if lower.starts_with(&label) {
            return text[label.len()..].trim_start().to_string();
        }
    }
    text.to_string()
}

/// Who a response is attributed to: the character when exactly one is in the scene and
/// narrator mode is off, otherwise `None` for the narrator.
pub fn speaker(settings: &Settings, characters: &[String]) -> Option<String> {
    match characters {
        [only] if !settings.style.narrator_mode => Some(only.clone()),
        _ => None,
    }
}

/// The story turn for a finished model response.
pub fn response_turn(settings: &Settings, characters: &[String], text: &str) -> Turn {
    match speaker(settings, characters) {
        Some(name) => Turn::character(name, text),
        None => Turn::narrator(text),
    }
}

#[cfg(test)]
mod tests {
    use masquerole_core::db::NodeKind;

    use super::*;

    fn card(name: &str) -> Character {
        Character {
            name: name.into(),
            ..Character::default()
        }
    }

    #[test]
    fn the_base_prompt_always_comes_first() {
        let prompt = system_prompt(&Settings::default(), &[]);
        assert!(prompt.starts_with("You are the game master"));
        assert!(!prompt.contains("third person"));
        assert!(!prompt.contains("style"));
    }

    #[test]
    fn narrator_mode_group_scenes_and_styles_add_instructions() {
        let mut settings = Settings::default();
        settings.style.narrator_mode = true;
        settings.style.profile = "noir".into();
        let prompt = system_prompt(&settings, &[card("A"), card("B")]);
        assert!(prompt.contains("third person"));
        assert!(prompt.contains("Several characters"));
        assert!(prompt.contains("Write in this style: noir."));

        settings.style.profile = "  default ".into();
        assert!(!system_prompt(&settings, &[]).contains("style"));
    }

    #[test]
    fn one_character_speaks_unless_narrator_mode_or_a_group() {
        let mut settings = Settings::default();
        let one = ["Mira".to_string()];
        let two = ["Mira".to_string(), "Orrin".to_string()];
        assert_eq!(speaker(&settings, &one).as_deref(), Some("Mira"));
        assert_eq!(speaker(&settings, &two), None);
        assert_eq!(speaker(&settings, &[]), None);
        settings.style.narrator_mode = true;
        assert_eq!(speaker(&settings, &one), None);
    }

    #[test]
    fn a_speaker_prompt_names_the_speaker_and_hides_everyone_elses_inner_life() {
        let mut settings = Settings::default();
        let prompt = speaker_prompt(&settings, "Mira Valen");
        assert!(prompt.starts_with("You are Mira Valen"));
        assert!(prompt.contains("only know what appears below"));
        assert!(!prompt.contains("style"));
        settings.style.profile = "noir".into();
        assert!(speaker_prompt(&settings, "Mira Valen").contains("Write in this style: noir."));
    }

    #[test]
    fn a_leading_name_label_is_removed_from_a_reply() {
        assert_eq!(clean_reply("Mira Valen: Hello.", "Mira Valen"), "Hello.");
        assert_eq!(
            clean_reply("  **mira valen:** Hello.", "Mira Valen"),
            "Hello."
        );
        assert_eq!(clean_reply("[Mira Valen]: Hello.", "Mira Valen"), "Hello.");
        assert_eq!(
            clean_reply("Hello. Mira Valen: me.", "Mira Valen"),
            "Hello. Mira Valen: me."
        );
        assert_eq!(clean_reply("Orrin: Hello.", "Mira Valen"), "Orrin: Hello.");
    }

    #[test]
    fn response_turns_match_the_speaker() {
        let settings = Settings::default();
        let turn = response_turn(&settings, &["Mira".to_string()], "Hello.");
        assert_eq!(
            (turn.kind, turn.speaker.as_deref()),
            (NodeKind::Character, Some("Mira"))
        );
        let turn = response_turn(&settings, &[], "Fog.");
        assert_eq!((turn.kind, turn.speaker), (NodeKind::Narrator, None));
        assert_eq!(turn.content, "Fog.");
    }
}
