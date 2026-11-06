//! Asking the model to write or improve one text of a character card.
//!
//! [`plan`] builds the prompt from the whole card, so the new text fits the rest of the
//! character. [`improve`] makes the call and tidies the reply.

use crate::character::Character;
use crate::client::{ChatMessage, GenerationParams, LlmError, LlmProvider};

/// A text of a character card the model can write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    /// The personality traits, one per line.
    Personality,
    /// How the character speaks.
    SpeakingStyle,
    /// The backstory.
    Background,
    /// What the character says first.
    Greeting,
}

impl Field {
    /// Every field, in the order of the card.
    pub const ALL: [Field; 4] = [
        Field::Personality,
        Field::SpeakingStyle,
        Field::Background,
        Field::Greeting,
    ];

    /// The name of the field, as shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            Field::Personality => "personality",
            Field::SpeakingStyle => "speaking style",
            Field::Background => "background",
            Field::Greeting => "greeting",
        }
    }

    /// What the model should write, and in which shape.
    fn task(self) -> &'static str {
        match self {
            Field::Personality => {
                "the personality: four to eight short traits, one per line, each a few words. \
                 Do not number them and do not add bullets"
            }
            Field::SpeakingStyle => {
                "the speaking style: one or two sentences about how the character talks \
                 (tone, vocabulary, habits)"
            }
            Field::Background => {
                "the background: a short backstory of one or two paragraphs in third person"
            }
            Field::Greeting => {
                "the greeting: the first message of the character to the player, written in \
                 the character's own voice, two to four sentences"
            }
        }
    }

    fn params(self) -> GenerationParams {
        GenerationParams {
            temperature: 0.8,
            max_tokens: 600,
        }
    }
}

const INSTRUCTIONS: &str = "You help write character cards for a roleplay. You are given a \
character card and the name of one text on it. Write that text. If it already has content, \
improve it: keep what the author clearly meant, fix awkward wording, and make it more vivid \
and consistent with the rest of the card. If it is empty, write it from the rest of the \
card. Reply with the text only: no title, no quotes, no explanation.";

/// The text of `field` on `character`, as the model sees it.
fn current(field: Field, character: &Character) -> String {
    match field {
        Field::Personality => character.personality.join("\n"),
        Field::SpeakingStyle => character.speaking_style.clone(),
        Field::Background => character.background.clone(),
        Field::Greeting => character.greeting.clone(),
    }
}

/// The messages to send to the model to write `field` of `character`.
pub fn plan(field: Field, character: &Character) -> Vec<ChatMessage> {
    let mut card = format!("Name: {}\n", character.name.trim());
    let mut line = |label: &str, text: &str| {
        let text = text.trim();
        if !text.is_empty() {
            card.push_str(&format!("{label}: {text}\n"));
        }
    };
    line("Description", &character.description);
    line("Personality", &character.personality.join(", "));
    line("Speaking style", &character.speaking_style);
    line("Background", &character.background);
    line("Greeting", &character.greeting);
    let own = current(field, character);
    let state = if own.trim().is_empty() {
        "It is empty: write it.".to_string()
    } else {
        format!("Its current text is:\n{}", own.trim())
    };
    let user = format!("Character card:\n{card}\nWrite {}.\n{state}", field.task());
    vec![ChatMessage::system(INSTRUCTIONS), ChatMessage::user(user)]
}

/// Tidy a reply: drop code fences, a leading label, wrapping quotes, and list bullets of the
/// personality.
fn clean(field: Field, reply: &str) -> String {
    let mut text = reply.trim();
    if let Some(inner) = text.strip_prefix("```") {
        let inner = inner.split_once('\n').map_or("", |(_, rest)| rest);
        text = inner.trim_end().trim_end_matches("```").trim();
    }
    // A leading "Personality:" or similar.
    if let Some((head, rest)) = text.split_once(':') {
        if head.trim().eq_ignore_ascii_case(field.label()) && !head.contains('\n') {
            text = rest.trim();
        }
    }
    for (open, close) in [('"', '"'), ('\u{201c}', '\u{201d}')] {
        if let Some(inner) = text
            .strip_prefix(open)
            .and_then(|rest| rest.strip_suffix(close))
        {
            // Quotes inside mean the outer ones are part of the text.
            if !inner.contains(open) && !inner.contains(close) {
                text = inner.trim();
                break;
            }
        }
    }
    if field == Field::Personality {
        return text
            .lines()
            .map(|l| {
                l.trim()
                    .trim_start_matches(['-', '*', '\u{2022}'])
                    .trim_start()
            })
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
    }
    text.to_string()
}

/// Ask the model to write or improve `field` of `character`.
pub async fn improve(
    provider: &dyn LlmProvider,
    field: Field,
    character: &Character,
) -> Result<String, LlmError> {
    let reply = provider
        .complete(plan(field, character), field.params())
        .await?;
    let text = clean(field, &reply);
    if text.is_empty() {
        return Err(LlmError::BadResponse(format!(
            "the model returned an empty {}",
            field.label()
        )));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures_util::future::BoxFuture;

    use super::*;
    use crate::client::TextStream;

    struct Mock {
        reply: String,
        seen: Mutex<Vec<Vec<ChatMessage>>>,
    }

    impl LlmProvider for Mock {
        fn stream_chat(
            &self,
            _: Vec<ChatMessage>,
            _: GenerationParams,
        ) -> BoxFuture<'_, Result<TextStream, LlmError>> {
            Box::pin(async { Err(LlmError::BadResponse("not used".into())) })
        }

        fn complete(
            &self,
            messages: Vec<ChatMessage>,
            _: GenerationParams,
        ) -> BoxFuture<'_, Result<String, LlmError>> {
            self.seen.lock().unwrap().push(messages);
            let reply = self.reply.clone();
            Box::pin(async move { Ok(reply) })
        }
    }

    fn mira() -> Character {
        Character {
            name: "Mira Valen".into(),
            description: "A cartographer.".into(),
            personality: vec!["curious".into(), "dry".into()],
            speaking_style: "Dry.".into(),
            ..Character::default()
        }
    }

    fn text_of(message: &ChatMessage) -> &str {
        &message.content
    }

    #[test]
    fn the_prompt_holds_the_card_and_the_current_text() {
        let messages = plan(Field::SpeakingStyle, &mira());
        let user = text_of(&messages[1]);
        assert!(user.contains("Name: Mira Valen"));
        assert!(user.contains("Personality: curious, dry"));
        assert!(user.contains("Its current text is:\nDry."));
        assert!(!user.contains("Background:"), "empty texts are left out");
    }

    #[test]
    fn an_empty_field_is_written_from_the_rest() {
        let messages = plan(Field::Greeting, &mira());
        assert!(text_of(&messages[1]).contains("It is empty: write it."));
        assert!(text_of(&messages[1]).contains("the greeting"));
    }

    #[test]
    fn replies_are_tidied() {
        assert_eq!(
            clean(
                Field::Personality,
                "```\n- curious\n* dry\n\u{2022} brave\n```"
            ),
            "curious\ndry\nbrave"
        );
        assert_eq!(
            clean(Field::Greeting, "Greeting: \"Ah, another one?\""),
            "Ah, another one?"
        );
        assert_eq!(
            clean(Field::Greeting, "\"She said \"hi\" twice.\""),
            "\"She said \"hi\" twice.\"",
            "inner quotes mean the quotes are part of the text"
        );
        assert_eq!(
            clean(Field::Greeting, "\u{201c}Hello there.\u{201d}"),
            "Hello there."
        );
        assert_eq!(
            clean(Field::Background, "  Born by the sea.\n\nShe left. "),
            "Born by the sea.\n\nShe left."
        );
    }

    #[tokio::test]
    async fn improve_asks_once_and_returns_the_tidied_reply() {
        let provider = Mock {
            reply: "- sharp\n- warm".into(),
            seen: Mutex::new(Vec::new()),
        };
        let text = improve(&provider, Field::Personality, &mira())
            .await
            .unwrap();
        assert_eq!(text, "sharp\nwarm");
        assert_eq!(provider.seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_empty_reply_is_an_error() {
        let provider = Mock {
            reply: " \n".into(),
            seen: Mutex::new(Vec::new()),
        };
        let error = improve(&provider, Field::Background, &mira())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("empty background"), "{error}");
    }
}
