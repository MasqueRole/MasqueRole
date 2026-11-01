//! Prompt assembly within a token budget.
//!
//! [`build`] turns the system prompt, characters, world, active lore, facts, summary, and
//! conversation into the message list sent to the provider, trimming to fit
//! `max_context_tokens`.
//!
//! Token counts are estimates from a script-aware heuristic, not a real tokenizer, so
//! leave some headroom in `max_context_tokens`.

use crate::character::Character;
use crate::client::{ChatMessage, Role};
use crate::db::{Node, NodeKind};
use crate::lorebook::LoreEntry;
use crate::settings::ContextSettings;

/// Extra tokens counted per message for role markers and framing.
pub const MESSAGE_OVERHEAD_TOKENS: usize = 4;

/// The newest messages that are never trimmed: the latest exchange.
pub const MIN_KEPT_MESSAGES: usize = 2;

/// Tokens the reply primes: every request ends with the start of the assistant's turn.
pub const REPLY_PRIMING_TOKENS: usize = 3;

/// What a letter or digit adds to the token count of the word it belongs to.
fn char_weight(c: char) -> f32 {
    match c {
        c if c.is_ascii_alphabetic() => 0.22,
        c if c.is_ascii_digit() => 0.34,
        _ => 0.5,
    }
}

/// Han, kana, and hangul: tokenizers spend about one token on each character.
fn is_wide_script(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x2E80..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF
        | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF)
}

/// Estimate the tokens in `text`.
///
/// This is still an estimate, but it follows how real tokenizers behave instead of
/// dividing by a constant: a run of ASCII letters costs about one token per five characters
/// (at least one), digits about one per three, accented and non-Latin letters about one per
/// two, every CJK character one, and each punctuation mark or symbol one. Spaces are free
/// because they merge into the next word; a line break costs one.
pub fn estimate_tokens(text: &str) -> usize {
    fn flush(run: &mut f32, total: &mut usize) {
        if *run > 0.0 {
            *total += (run.ceil() as usize).max(1);
            *run = 0.0;
        }
    }
    let mut total = 0usize;
    let mut run = 0.0f32;
    for c in text.chars() {
        if c.is_alphanumeric() && !is_wide_script(c) {
            run += char_weight(c);
            continue;
        }
        flush(&mut run, &mut total);
        if c == '\n' || is_wide_script(c) || !c.is_whitespace() {
            total += 1;
        }
    }
    flush(&mut run, &mut total);
    total
}

/// Estimate the tokens a message costs, including framing.
pub fn message_tokens(message: &ChatMessage) -> usize {
    estimate_tokens(&message.content) + MESSAGE_OVERHEAD_TOKENS
}

/// Convert a story-tree node into a chat message. Character and narrator lines are
/// prefixed with who is speaking, so the model can tell a group scene apart.
pub fn node_message(node: &Node) -> ChatMessage {
    match node.kind {
        NodeKind::User => ChatMessage::user(&node.content),
        NodeKind::Character => {
            let who = node.speaker.as_deref().unwrap_or("Character");
            ChatMessage::assistant(format!("{who}: {}", node.content))
        }
        NodeKind::Narrator => ChatMessage::assistant(format!("Narrator: {}", node.content)),
        NodeKind::System => ChatMessage::system(&node.content),
    }
}

/// Everything that can go into a prompt.
#[derive(Clone, Copy, Debug)]
pub struct ContextInput<'a> {
    /// The base instructions.
    pub system_prompt: &'a str,
    /// The characters in the scene.
    pub characters: &'a [Character],
    /// The world's general setting, always relevant. May be empty.
    pub world_setting: &'a str,
    /// Lorebook entries triggered by recent messages, most relevant first.
    pub lore: &'a [&'a LoreEntry],
    /// Known facts, oldest first.
    pub facts: &'a [String],
    /// The rolling summary of events older than `messages`.
    pub summary: Option<&'a str>,
    /// The conversation, oldest first.
    pub messages: &'a [ChatMessage],
}

/// A finished prompt and what it cost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltContext {
    /// The messages to send: one system message, then the conversation.
    pub messages: Vec<ChatMessage>,
    /// Estimated tokens in `messages`.
    pub tokens: usize,
    /// Conversation messages left out, oldest first (including those beyond
    /// `recent_messages`).
    pub omitted_messages: usize,
    /// Facts left out, oldest first.
    pub omitted_facts: usize,
    /// Lore entries left out, last first.
    pub omitted_lore: usize,
    /// Whether the summary was left out.
    pub summary_omitted: bool,
    /// True when the parts that are never trimmed already exceed the budget.
    pub over_budget: bool,
}

/// Build the prompt, trimming to `settings.max_context_tokens`.
///
/// Always kept: the system prompt, characters, world setting, and the newest
/// [`MIN_KEPT_MESSAGES`] messages. Beyond that, in order of priority, each group takes
/// what fits:
///
/// 1. conversation messages, newest first, up to `recent_messages`, always a contiguous
///    run ending at the newest message;
/// 2. the summary, all or nothing;
/// 3. facts, newest first, stopping at the first that does not fit;
/// 4. lore entries, in the given order, stopping at the first that does not fit.
pub fn build(input: &ContextInput<'_>, settings: &ContextSettings) -> BuiltContext {
    let budget = settings.max_context_tokens as usize;
    let total = input.messages.len();
    let window = (settings.recent_messages as usize)
        .max(MIN_KEPT_MESSAGES)
        .min(total);
    let window_start = total - window;
    let base = Base {
        prompt: input.system_prompt,
        characters: input.characters,
        setting: input.world_setting,
    };

    // Conversation: the newest messages are mandatory, older ones only while they fit.
    let first_mandatory = total.saturating_sub(MIN_KEPT_MESSAGES);
    let mut first = first_mandatory;
    let mut message_cost: usize = input.messages[first..].iter().map(message_tokens).sum();
    let fixed_cost = system_tokens(&render_system(&base, None, &[], &[]));
    while first > window_start {
        let cost = message_tokens(&input.messages[first - 1]);
        if fixed_cost + message_cost + cost > budget {
            break;
        }
        message_cost += cost;
        first -= 1;
    }

    let total_cost = |summary: Option<&str>, facts: &[&str], lore: &[&str]| {
        system_tokens(&render_system(&base, summary, facts, lore)) + message_cost
    };

    let summary = input
        .summary
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| total_cost(Some(s), &[], &[]) <= budget);
    let summary_omitted = input.summary.is_some_and(|s| !s.trim().is_empty()) && summary.is_none();

    let mut facts: Vec<&str> = Vec::new(); // newest first while building
    for fact in input.facts.iter().rev() {
        facts.push(fact);
        let oldest_first: Vec<&str> = facts.iter().rev().copied().collect();
        if total_cost(summary, &oldest_first, &[]) > budget {
            facts.pop();
            break;
        }
    }
    facts.reverse();

    let mut lore: Vec<&str> = Vec::new();
    for entry in input.lore {
        lore.push(&entry.content);
        if total_cost(summary, &facts, &lore) > budget {
            lore.pop();
            break;
        }
    }

    let system = render_system(&base, summary, &facts, &lore);
    let mut messages = Vec::with_capacity(total - first + 1);
    if !system.is_empty() {
        messages.push(ChatMessage::system(system));
    }
    messages.extend_from_slice(&input.messages[first..]);
    let tokens = messages.iter().map(message_tokens).sum();

    BuiltContext {
        messages,
        tokens,
        omitted_messages: first,
        omitted_facts: input.facts.len() - facts.len(),
        omitted_lore: input.lore.len() - lore.len(),
        summary_omitted,
        over_budget: tokens > budget,
    }
}

/// The parts of the system message that are never trimmed.
struct Base<'a> {
    prompt: &'a str,
    characters: &'a [Character],
    setting: &'a str,
}

fn system_tokens(system: &str) -> usize {
    if system.is_empty() {
        0
    } else {
        estimate_tokens(system) + MESSAGE_OVERHEAD_TOKENS
    }
}

fn render_system(base: &Base<'_>, summary: Option<&str>, facts: &[&str], lore: &[&str]) -> String {
    let mut sections: Vec<String> = Vec::new();
    if !base.prompt.trim().is_empty() {
        sections.push(base.prompt.trim().to_string());
    }
    if !base.characters.is_empty() {
        let cards: Vec<String> = base.characters.iter().map(render_character).collect();
        sections.push(format!("## Characters\n\n{}", cards.join("\n\n")));
    }
    if !base.setting.trim().is_empty() {
        sections.push(format!("## World\n\n{}", base.setting.trim()));
    }
    if !lore.is_empty() {
        sections.push(format!("## Relevant lore\n\n{}", bullets(lore)));
    }
    if !facts.is_empty() {
        sections.push(format!("## Known facts\n\n{}", bullets(facts)));
    }
    if let Some(summary) = summary {
        sections.push(format!("## Story so far\n\n{summary}"));
    }
    sections.join("\n\n")
}

fn bullets(items: &[&str]) -> String {
    items
        .iter()
        .map(|item| format!("- {}", item.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_character(c: &Character) -> String {
    let mut out = format!("### {}", c.name.trim());
    let mut field = |label: &str, value: &str| {
        if !value.trim().is_empty() {
            out.push_str(&format!("\n{label}: {}", value.trim()));
        }
    };
    field("Description", &c.description);
    field("Personality", &c.personality.join(", "));
    field("Speaking style", &c.speaking_style);
    field("Background", &c.background);
    if !c.example_dialogue.is_empty() {
        out.push_str("\nExample dialogue:");
        for example in &c.example_dialogue {
            out.push_str(&format!(
                "\nUser: {}\n{}: {}",
                example.user.trim(),
                c.name.trim(),
                example.character.trim()
            ));
        }
    }
    out
}

/// The role label used when a message is shown in a transcript for summarizing.
pub(crate) fn transcript_label(role: Role) -> &'static str {
    match role {
        Role::System => "Scene",
        Role::User => "Player",
        Role::Assistant => "Story",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::ExampleDialogue;
    use crate::db::{NodeId, SessionId};

    fn settings(max_context_tokens: u32, recent_messages: u32) -> ContextSettings {
        ContextSettings {
            max_context_tokens,
            recent_messages,
            summarize_after: 30,
        }
    }

    fn mira() -> Character {
        Character {
            name: "Mira".into(),
            description: "A cartographer.".into(),
            personality: vec!["curious".into(), "sarcastic".into()],
            speaking_style: "Dry.".into(),
            background: "Maps coasts.".into(),
            greeting: "Hello".into(),
            example_dialogue: vec![ExampleDialogue {
                user: "Where to?".into(),
                character: "Past the map.".into(),
            }],
            image: None,
        }
    }

    fn conversation(n: usize) -> Vec<ChatMessage> {
        (0..n)
            .map(|i| {
                if i % 2 == 0 {
                    ChatMessage::user(format!("message {i:02} {}", "x".repeat(40)))
                } else {
                    ChatMessage::assistant(format!("message {i:02} {}", "y".repeat(40)))
                }
            })
            .collect()
    }

    fn lore(n: usize) -> Vec<LoreEntry> {
        (0..n)
            .map(|i| LoreEntry {
                keys: vec![format!("key{i}")],
                content: format!("lore entry {i} {}", "z".repeat(60)),
            })
            .collect()
    }

    fn facts(n: usize) -> Vec<String> {
        (0..n)
            .map(|i| format!("fact {i:02} {}", "f".repeat(40)))
            .collect()
    }

    /// Parts of a scenario; each test enables the groups it needs.
    struct Scenario {
        characters: Vec<Character>,
        lore: Vec<LoreEntry>,
        facts: Vec<String>,
        summary: Option<String>,
        messages: Vec<ChatMessage>,
    }

    impl Scenario {
        fn full() -> Self {
            Self {
                characters: vec![mira()],
                lore: lore(3),
                facts: facts(4),
                summary: Some(format!("The story began. {}", "s".repeat(80))),
                messages: conversation(10),
            }
        }

        fn build(&self, settings: &ContextSettings) -> BuiltContext {
            let lore: Vec<&LoreEntry> = self.lore.iter().collect();
            build(
                &ContextInput {
                    system_prompt: "You are the game master.",
                    characters: &self.characters,
                    world_setting: "A foggy coast.",
                    lore: &lore,
                    facts: &self.facts,
                    summary: self.summary.as_deref(),
                    messages: &self.messages,
                },
                settings,
            )
        }
    }

    const HUGE: u32 = 1_000_000;

    #[test]
    fn token_estimate() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        // Spaces merge into the next word; words and punctuation count separately.
        assert_eq!(estimate_tokens("a b c"), 3);
        assert_eq!(estimate_tokens("Hi, there!"), 5);
        assert_eq!(estimate_tokens("12345"), 2);
        assert_eq!(estimate_tokens("a\nb"), 3);
        // Counts characters, not bytes: accented letters cost half a token each.
        assert_eq!(estimate_tokens("\u{e9}\u{e9}\u{e9}\u{e9}"), 2);
        // CJK text costs a token per character, far more than a quarter.
        assert_eq!(estimate_tokens("\u{4f60}\u{597d}\u{4e16}\u{754c}"), 4);
        assert_eq!(
            message_tokens(&ChatMessage::user("abcd")),
            1 + MESSAGE_OVERHEAD_TOKENS
        );
    }

    #[test]
    fn everything_fits_with_a_big_budget() {
        let s = Scenario::full();
        let built = s.build(&settings(HUGE, 20));
        assert!(!built.over_budget && !built.summary_omitted);
        assert_eq!(
            (
                built.omitted_messages,
                built.omitted_facts,
                built.omitted_lore
            ),
            (0, 0, 0)
        );
        assert_eq!(built.messages.len(), 1 + s.messages.len());
        assert_eq!(&built.messages[1..], &s.messages[..]);

        let system = &built.messages[0];
        assert_eq!(system.role, Role::System);
        let text = &system.content;
        let positions: Vec<usize> = [
            "You are the game master.",
            "## Characters",
            "### Mira",
            "## World",
            "## Relevant lore",
            "## Known facts",
            "## Story so far",
        ]
        .iter()
        .map(|part| text.find(part).unwrap_or_else(|| panic!("missing {part}")))
        .collect();
        assert!(
            positions.windows(2).all(|w| w[0] < w[1]),
            "sections out of order"
        );
        assert!(text.contains("Personality: curious, sarcastic"));
        assert!(text.contains("User: Where to?\nMira: Past the map."));
    }

    #[test]
    fn empty_sections_are_left_out() {
        let built = build(
            &ContextInput {
                system_prompt: "Be brief.",
                characters: &[],
                world_setting: "  ",
                lore: &[],
                facts: &[],
                summary: Some("  "),
                messages: &[ChatMessage::user("hi")],
            },
            &settings(HUGE, 20),
        );
        assert_eq!(built.messages[0].content, "Be brief.");
        assert!(!built.summary_omitted, "a blank summary is not an omission");
    }

    #[test]
    fn no_system_message_when_there_is_nothing_to_say() {
        let built = build(
            &ContextInput {
                system_prompt: "",
                characters: &[],
                world_setting: "",
                lore: &[],
                facts: &[],
                summary: None,
                messages: &[ChatMessage::user("hi")],
            },
            &settings(HUGE, 20),
        );
        assert_eq!(built.messages, [ChatMessage::user("hi")]);
    }

    #[test]
    fn recent_messages_limits_the_window() {
        let s = Scenario::full();
        let built = s.build(&settings(HUGE, 4));
        assert_eq!(built.omitted_messages, 6);
        assert_eq!(&built.messages[1..], &s.messages[6..]);
    }

    #[test]
    fn the_latest_exchange_is_kept_even_if_recent_messages_is_tiny() {
        let s = Scenario::full();
        let built = s.build(&settings(HUGE, 0));
        assert_eq!(&built.messages[1..], &s.messages[8..]);
    }

    #[test]
    fn newest_messages_survive_any_budget() {
        let s = Scenario::full();
        let full = s.build(&settings(HUGE, 20)).tokens as u32;
        for budget in (0..=full + 5).rev() {
            let built = s.build(&settings(budget, 20));
            let n = built.messages.len();
            // The latest exchange is always last, in order.
            assert_eq!(
                &built.messages[n - 2..],
                &s.messages[8..],
                "budget {budget}"
            );
            // Messages are a contiguous newest run.
            let kept = n - 1;
            assert_eq!(&built.messages[1..], &s.messages[s.messages.len() - kept..]);
            assert_eq!(built.omitted_messages, s.messages.len() - kept);
            // Within budget unless only the mandatory parts remain.
            assert!(
                built.tokens <= budget as usize || built.over_budget,
                "budget {budget}"
            );
            if built.over_budget {
                assert_eq!(kept, MIN_KEPT_MESSAGES, "budget {budget}");
                assert!(built.summary_omitted);
                assert_eq!((built.omitted_facts, built.omitted_lore), (4, 3));
            }
        }
    }

    #[test]
    fn a_zero_budget_still_sends_the_latest_exchange() {
        let s = Scenario::full();
        let built = s.build(&settings(0, 20));
        assert!(built.over_budget);
        assert_eq!(&built.messages[1..], &s.messages[8..]);
    }

    #[test]
    fn single_message_conversations_work() {
        let mut s = Scenario::full();
        s.messages.truncate(1);
        let built = s.build(&settings(0, 20));
        assert_eq!(built.messages.last(), Some(&s.messages[0]));
        s.messages.clear();
        let built = s.build(&settings(HUGE, 20));
        assert_eq!(built.messages.len(), 1, "just the system message");
    }

    #[test]
    fn lore_is_dropped_before_anything_else() {
        let s = Scenario::full();
        let full = s.build(&settings(HUGE, 20)).tokens as u32;
        let built = s.build(&settings(full - 1, 20));
        assert!(built.omitted_lore >= 1);
        assert_eq!(built.omitted_facts, 0);
        assert!(!built.summary_omitted);
        assert_eq!(built.omitted_messages, 0);
    }

    #[test]
    fn lore_keeps_its_order_and_drops_from_the_end() {
        let s = Scenario::full();
        let full = s.build(&settings(HUGE, 20)).tokens as u32;
        let built = s.build(&settings(full - 1, 20));
        let text = &built.messages[0].content;
        assert!(text.contains("lore entry 0"));
        assert!(!text.contains("lore entry 2"));
    }

    #[test]
    fn facts_are_dropped_before_the_summary_and_oldest_go_first() {
        let mut s = Scenario::full();
        s.lore.clear();
        let full = s.build(&settings(HUGE, 20)).tokens as u32;
        let built = s.build(&settings(full - 1, 20));
        assert!(built.omitted_facts >= 1);
        assert!(!built.summary_omitted);
        assert_eq!(built.omitted_messages, 0);
        let text = &built.messages[0].content;
        assert!(!text.contains("fact 00"), "oldest fact goes first");
        assert!(text.contains("fact 03"), "newest fact stays");
    }

    #[test]
    fn the_summary_is_dropped_before_messages() {
        let mut s = Scenario::full();
        s.lore.clear();
        s.facts.clear();
        let full = s.build(&settings(HUGE, 20)).tokens as u32;
        let built = s.build(&settings(full - 1, 20));
        assert!(built.summary_omitted);
        assert!(!built.messages[0].content.contains("Story so far"));
        assert_eq!(built.omitted_messages, 0);
    }

    #[test]
    fn oldest_messages_go_first() {
        let mut s = Scenario::full();
        s.lore.clear();
        s.facts.clear();
        s.summary = None;
        let full = s.build(&settings(HUGE, 20)).tokens as u32;
        let built = s.build(&settings(full - 1, 20));
        assert_eq!(built.omitted_messages, 1);
        assert_eq!(&built.messages[1..], &s.messages[1..]);
    }

    #[test]
    fn node_messages_name_their_speaker() {
        let node = |kind, speaker: Option<&str>| Node {
            id: NodeId(1),
            session_id: SessionId(1),
            parent_id: None,
            kind,
            speaker: speaker.map(Into::into),
            content: "Hello.".into(),
            label: None,
            bookmark: None,
            active_lore: vec![],
            created_at: 0,
        };
        assert_eq!(
            node_message(&node(NodeKind::User, None)),
            ChatMessage::user("Hello.")
        );
        assert_eq!(
            node_message(&node(NodeKind::Character, Some("Mira"))),
            ChatMessage::assistant("Mira: Hello.")
        );
        assert_eq!(
            node_message(&node(NodeKind::Character, None)),
            ChatMessage::assistant("Character: Hello.")
        );
        assert_eq!(
            node_message(&node(NodeKind::Narrator, None)),
            ChatMessage::assistant("Narrator: Hello.")
        );
        assert_eq!(
            node_message(&node(NodeKind::System, None)),
            ChatMessage::system("Hello.")
        );
    }
}
