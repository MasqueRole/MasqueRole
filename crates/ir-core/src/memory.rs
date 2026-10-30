//! Long-term memory: the rolling summary and the fact store.
//!
//! Each task is split into a pure `plan_*` step that decides what to ask the model, an
//! async call that needs only the provider, and storing the result with [`crate::db::Db`].
//! Nothing here holds a database handle across an `await`, so callers can run the model
//! call on any task.
//!
//! A summary is stored at the *last node it covers*. The summary in effect at a node is the
//! nearest one on its path, so it covers exactly the path up to its own node, and rewinding
//! to before it simply selects an older one.

use crate::client::{ChatMessage, GenerationParams, LlmError, LlmProvider};
use crate::context::{node_message, transcript_label};
use crate::db::{Node, NodeId, Summary};
use crate::settings::Settings;

/// Most messages folded into one summary call, so a long imported story is summarized in
/// steps instead of one oversized request.
pub const MAX_SUMMARY_BATCH: usize = 60;

/// Most facts accepted from one extraction call.
pub const MAX_FACTS_PER_CALL: usize = 8;

/// Longest stored fact, in characters.
pub const MAX_FACT_CHARS: usize = 200;

const SUMMARY_PARAMS: GenerationParams = GenerationParams {
    temperature: 0.3,
    max_tokens: 600,
};

const FACT_PARAMS: GenerationParams = GenerationParams {
    temperature: 0.2,
    max_tokens: 300,
};

const SUMMARY_INSTRUCTIONS: &str = "You maintain the running summary of a roleplay story. \
Update the current summary with the new events. Keep names, relationships, places, items, \
promises, and unresolved threads. Write concisely, in past tense and third person. \
Reply with the updated summary only.";

const FACT_INSTRUCTIONS: &str = "You extract durable facts from a roleplay for long-term \
memory. List only NEW facts worth remembering later: names, relationships, places, items, \
promises, decisions, and traits that were revealed. Write one fact per line, starting with \
\"- \", each a short standalone sentence that names who or what it is about. Do not repeat \
known facts. If there is nothing new, reply with NONE.";

/// A request to extend the summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryJob {
    /// The last node the new summary will cover. Store the summary at this node.
    pub through_node: NodeId,
    /// How many messages this job folds in.
    pub message_count: usize,
    prompt: Vec<ChatMessage>,
}

impl SummaryJob {
    /// Estimated tokens of the prompt this job sends.
    pub fn estimated_tokens(&self) -> usize {
        self.prompt.iter().map(crate::context::message_tokens).sum()
    }
}

/// A request to extract new facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactJob {
    /// The newest node covered. Store the facts at this node.
    pub node_id: NodeId,
    prompt: Vec<ChatMessage>,
}

impl FactJob {
    /// Estimated tokens of the prompt this job sends.
    pub fn estimated_tokens(&self) -> usize {
        self.prompt.iter().map(crate::context::message_tokens).sum()
    }
}

/// How many nodes of `path` (root first) the summary in effect already covers.
pub fn summarized_count(path: &[Node], summary: Option<&Summary>) -> usize {
    summary
        .and_then(|s| path.iter().position(|n| n.id == s.node_id))
        .map_or(0, |index| index + 1)
}

/// Decide whether the path needs a new summary.
///
/// `path` is the story from the root to the current node and `summary` the one in effect
/// there (see [`crate::db::Db::summary_at`]). Once more than `summarize_after` messages are
/// unsummarized, everything except the newest `recent_messages` is folded in. Returns
/// `None` when memory is off or nothing needs doing yet.
pub fn plan_summary(
    path: &[Node],
    summary: Option<&Summary>,
    settings: &Settings,
) -> Option<SummaryJob> {
    if !settings.memory.enabled {
        return None;
    }
    let start = summarized_count(path, summary);
    let unsummarized = path.len().saturating_sub(start);
    if unsummarized <= settings.context.summarize_after as usize {
        return None;
    }
    let end = path
        .len()
        .saturating_sub(settings.context.recent_messages as usize)
        .min(start + MAX_SUMMARY_BATCH);
    if end <= start {
        return None;
    }
    let chunk = &path[start..end];

    let previous = summary.map_or("(none yet)", |s| s.text.as_str());
    let user = format!(
        "Current summary:\n{previous}\n\nNew events:\n{}",
        transcript(chunk)
    );
    Some(SummaryJob {
        through_node: chunk[chunk.len() - 1].id,
        message_count: chunk.len(),
        prompt: vec![
            ChatMessage::system(SUMMARY_INSTRUCTIONS),
            ChatMessage::user(user),
        ],
    })
}

/// Ask the model for the updated summary text.
pub async fn summarize(provider: &dyn LlmProvider, job: &SummaryJob) -> Result<String, LlmError> {
    let text = provider
        .complete(job.prompt.clone(), SUMMARY_PARAMS)
        .await?
        .trim()
        .to_string();
    if text.is_empty() {
        return Err(LlmError::BadResponse(
            "the model returned an empty summary".into(),
        ));
    }
    Ok(text)
}

/// Decide whether to extract facts from `new_nodes`, the nodes added since the last
/// extraction. `known_facts` are those already stored at this point in the story. Returns
/// `None` when fact extraction is off or there is nothing new.
pub fn plan_fact_extraction(
    new_nodes: &[Node],
    known_facts: &[String],
    settings: &Settings,
) -> Option<FactJob> {
    if !settings.memory.enabled || !settings.memory.fact_extraction {
        return None;
    }
    let last = new_nodes.last()?;
    let known = if known_facts.is_empty() {
        "(none)".to_string()
    } else {
        known_facts
            .iter()
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let user = format!(
        "Known facts:\n{known}\n\nNew messages:\n{}",
        transcript(new_nodes)
    );
    Some(FactJob {
        node_id: last.id,
        prompt: vec![
            ChatMessage::system(FACT_INSTRUCTIONS),
            ChatMessage::user(user),
        ],
    })
}

/// Ask the model for new facts, dropping any that repeat `known_facts`.
pub async fn extract_facts(
    provider: &dyn LlmProvider,
    job: &FactJob,
    known_facts: &[String],
) -> Result<Vec<String>, LlmError> {
    let output = provider.complete(job.prompt.clone(), FACT_PARAMS).await?;
    Ok(parse_facts(&output, known_facts))
}

/// Read a model's fact list: one fact per line, optionally bulleted or numbered, or `NONE`.
/// Drops blanks, repeats of `known` and of earlier lines (ignoring case and punctuation),
/// truncates long facts, and keeps at most [`MAX_FACTS_PER_CALL`].
pub fn parse_facts(output: &str, known: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = known.iter().map(|f| normalize(f)).collect();
    let mut facts = Vec::new();
    for line in output.lines() {
        let fact = strip_list_marker(line.trim()).trim();
        if fact.is_empty() || normalize(fact) == "none" {
            continue;
        }
        let fact: String = fact.chars().take(MAX_FACT_CHARS).collect();
        let key = normalize(&fact);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        facts.push(fact);
        if facts.len() == MAX_FACTS_PER_CALL {
            break;
        }
    }
    facts
}

fn strip_list_marker(line: &str) -> &str {
    for bullet in ["- ", "* ", "\u{2022} "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return rest;
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &line[digits..];
        for sep in [". ", ") "] {
            if let Some(rest) = rest.strip_prefix(sep) {
                return rest;
            }
        }
    }
    line
}

/// Lowercase, collapse whitespace, and drop trailing punctuation, for comparing facts.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(|c: char| c.is_ascii_punctuation())
        .to_lowercase()
}

/// Render nodes as `Label: text` lines for a prompt.
fn transcript(nodes: &[Node]) -> String {
    nodes
        .iter()
        .map(|node| {
            let message = node_message(node);
            match node.speaker.as_deref() {
                // Character and narrator lines already carry their speaker.
                Some(_) => message.content,
                None if message.content.starts_with("Narrator: ") => message.content,
                None => format!("{}: {}", transcript_label(message.role), message.content),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures_util::future::BoxFuture;

    use super::*;
    use crate::client::TextStream;
    use crate::db::{NewNode, NodeKind, SessionId, SummaryId};

    fn node(id: i64, kind: NodeKind, speaker: Option<&str>, content: &str) -> Node {
        Node {
            id: NodeId(id),
            session_id: SessionId(1),
            parent_id: (id > 1).then_some(NodeId(id - 1)),
            kind,
            speaker: speaker.map(Into::into),
            content: content.into(),
            label: None,
            bookmark: None,
            active_lore: vec![],
            created_at: 0,
        }
    }

    /// A path of `n` alternating user/character nodes with ids 1..=n.
    fn path(n: i64) -> Vec<Node> {
        (1..=n)
            .map(|i| {
                if i % 2 == 1 {
                    node(i, NodeKind::User, None, &format!("player line {i}"))
                } else {
                    node(
                        i,
                        NodeKind::Character,
                        Some("Mira"),
                        &format!("mira line {i}"),
                    )
                }
            })
            .collect()
    }

    fn summary_at(node: i64, text: &str) -> Summary {
        Summary {
            id: SummaryId(1),
            session_id: SessionId(1),
            node_id: NodeId(node),
            text: text.into(),
            created_at: 0,
        }
    }

    fn settings(summarize_after: u32, recent_messages: u32) -> Settings {
        let mut s = Settings::default();
        s.context.summarize_after = summarize_after;
        s.context.recent_messages = recent_messages;
        s
    }

    /// A provider that returns a canned reply and records what it was asked.
    struct Mock {
        reply: Result<String, LlmError>,
        seen: Mutex<Vec<Vec<ChatMessage>>>,
    }

    impl Mock {
        fn replying(text: &str) -> Self {
            Self {
                reply: Ok(text.into()),
                seen: Mutex::new(Vec::new()),
            }
        }
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
            Box::pin(async move { reply })
        }
    }

    #[test]
    fn summarized_count_follows_the_summary_node() {
        let p = path(10);
        assert_eq!(summarized_count(&p, None), 0);
        assert_eq!(summarized_count(&p, Some(&summary_at(4, "s"))), 4);
        // A summary from another branch is not on this path: treat as none.
        assert_eq!(summarized_count(&p, Some(&summary_at(99, "s"))), 0);
    }

    #[test]
    fn no_summary_until_past_the_threshold() {
        let s = settings(30, 20);
        assert!(plan_summary(&path(30), None, &s).is_none());
        let job = plan_summary(&path(31), None, &s).unwrap();
        // Everything but the newest 20 is folded in: nodes 1..=11.
        assert_eq!(job.message_count, 11);
        assert_eq!(job.through_node, NodeId(11));
    }

    #[test]
    fn summaries_extend_from_where_the_last_one_ended() {
        let s = settings(30, 20);
        let p = path(50);
        let prev = summary_at(11, "Earlier events.");

        // 39 unsummarized: not yet.
        assert!(plan_summary(&p[..50], Some(&prev), &settings(40, 20)).is_none());
        // 39 > 30: fold nodes 12..=30, keeping the newest 20.
        let job = plan_summary(&p, Some(&prev), &s).unwrap();
        assert_eq!(job.through_node, NodeId(30));
        assert_eq!(job.message_count, 19);
        let user = &job.prompt[1].content;
        assert!(user.starts_with("Current summary:\nEarlier events."));
        assert!(user.contains("mira line 12") && user.contains("mira line 30"));
        assert!(!user.contains("line 11") && !user.contains("line 31"));
    }

    #[test]
    fn right_after_summarizing_nothing_is_due() {
        let s = settings(30, 20);
        let after = summary_at(11, "s");
        assert!(plan_summary(&path(31), Some(&after), &s).is_none());
        // It takes another 10 messages to cross the threshold again.
        assert!(plan_summary(&path(40), Some(&after), &s).is_none());
        assert!(plan_summary(&path(42), Some(&after), &s).is_some());
    }

    #[test]
    fn memory_off_means_no_jobs() {
        let mut s = settings(30, 20);
        s.memory.enabled = false;
        assert!(plan_summary(&path(100), None, &s).is_none());
        assert!(plan_fact_extraction(&path(3), &[], &s).is_none());
    }

    #[test]
    fn recent_messages_larger_than_the_threshold_summarizes_nothing() {
        // Nothing is older than the verbatim window, so there is nothing to fold in.
        assert!(plan_summary(&path(40), None, &settings(30, 50)).is_none());
    }

    #[test]
    fn long_histories_are_summarized_in_batches() {
        let s = settings(30, 20);
        let p = path(300);
        let job = plan_summary(&p, None, &s).unwrap();
        assert_eq!(job.message_count, MAX_SUMMARY_BATCH);
        assert_eq!(job.through_node, NodeId(MAX_SUMMARY_BATCH as i64));
        // Following up from that summary continues where it left off.
        let next = plan_summary(&p, Some(&summary_at(60, "s")), &s).unwrap();
        assert_eq!(next.through_node, NodeId(120));
    }

    #[test]
    fn transcripts_label_every_kind_of_line() {
        let nodes = [
            node(1, NodeKind::System, None, "The scene opens."),
            node(2, NodeKind::User, None, "I knock."),
            node(3, NodeKind::Character, Some("Orrin"), "Who's there?"),
            node(4, NodeKind::Narrator, None, "Fog rolls in."),
        ];
        assert_eq!(
            transcript(&nodes),
            "Scene: The scene opens.\nPlayer: I knock.\nOrrin: Who's there?\nNarrator: Fog rolls in."
        );
    }

    #[tokio::test]
    async fn summarize_sends_the_job_and_returns_trimmed_text() {
        let job = plan_summary(&path(31), None, &settings(30, 20)).unwrap();
        let mock = Mock::replying("  The party met Mira.\n");
        assert_eq!(summarize(&mock, &job).await.unwrap(), "The party met Mira.");

        let seen = mock.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0][0].content, SUMMARY_INSTRUCTIONS);
        assert!(seen[0][1].content.contains("Current summary:\n(none yet)"));
    }

    #[tokio::test]
    async fn an_empty_summary_is_an_error() {
        let job = plan_summary(&path(31), None, &settings(30, 20)).unwrap();
        let err = summarize(&Mock::replying("  \n"), &job).await.unwrap_err();
        assert!(matches!(err, LlmError::BadResponse(_)));
    }

    #[tokio::test]
    async fn provider_errors_pass_through() {
        let job = plan_summary(&path(31), None, &settings(30, 20)).unwrap();
        let mock = Mock {
            reply: Err(LlmError::RateLimit {
                retry_after_secs: Some(5),
            }),
            seen: Mutex::new(Vec::new()),
        };
        assert_eq!(
            summarize(&mock, &job).await,
            Err(LlmError::RateLimit {
                retry_after_secs: Some(5)
            })
        );
    }

    #[test]
    fn fact_jobs_cover_the_new_nodes_and_list_known_facts() {
        let new = path(4)[2..].to_vec();
        let known = vec!["Mira is a cartographer.".to_string()];
        let job = plan_fact_extraction(&new, &known, &Settings::default()).unwrap();
        assert_eq!(job.node_id, NodeId(4));
        let user = &job.prompt[1].content;
        assert!(user.contains("- Mira is a cartographer."));
        assert!(user.contains("player line 3") && user.contains("Mira: mira line 4"));
        assert!(!user.contains("line 2"));

        let none = plan_fact_extraction(&new, &[], &Settings::default()).unwrap();
        assert!(none.prompt[1].content.contains("Known facts:\n(none)"));
    }

    #[test]
    fn fact_extraction_respects_its_switch_and_needs_new_nodes() {
        let mut s = Settings::default();
        assert!(plan_fact_extraction(&[], &[], &s).is_none());
        s.memory.fact_extraction = false;
        assert!(plan_fact_extraction(&path(3), &[], &s).is_none());
        // Summaries still work with fact extraction off.
        assert!(plan_summary(&path(100), None, &s).is_some());
    }

    #[test]
    fn parses_bulleted_numbered_and_plain_lines() {
        let out =
            "- Mira owes Orrin a favor.\n* The lamp is cracked.\n\u{2022} Fogglass clears fog.\n\
                   1. Orrin trained under Hale.\n2) The reef is named Teeth.\nPlain line.\n\n   \n";
        assert_eq!(
            parse_facts(out, &[]),
            [
                "Mira owes Orrin a favor.",
                "The lamp is cracked.",
                "Fogglass clears fog.",
                "Orrin trained under Hale.",
                "The reef is named Teeth.",
                "Plain line.",
            ]
        );
    }

    #[test]
    fn none_means_no_facts() {
        assert!(parse_facts("NONE", &[]).is_empty());
        assert!(parse_facts("none.", &[]).is_empty());
        assert!(parse_facts("- None", &[]).is_empty());
        assert!(parse_facts("", &[]).is_empty());
        // A real fact that merely starts with the word is kept.
        assert_eq!(parse_facts("None of the crew trust Mira.", &[]).len(), 1);
    }

    #[test]
    fn known_and_repeated_facts_are_dropped() {
        let known = vec!["Mira is a cartographer.".to_string()];
        let out = "- mira is a  cartographer\n- Orrin keeps the lamp.\n- ORRIN keeps the lamp!\n";
        assert_eq!(parse_facts(out, &known), ["Orrin keeps the lamp."]);
    }

    #[test]
    fn facts_are_capped_in_length_and_number() {
        let long = format!("- {}", "a".repeat(500));
        assert_eq!(parse_facts(&long, &[])[0].chars().count(), MAX_FACT_CHARS);

        let many: String = (0..20).map(|i| format!("- fact number {i}\n")).collect();
        let facts = parse_facts(&many, &[]);
        assert_eq!(facts.len(), MAX_FACTS_PER_CALL);
        assert_eq!(facts[0], "fact number 0");
    }

    #[tokio::test]
    async fn extract_facts_filters_what_the_model_returns() {
        let new = path(2);
        let job = plan_fact_extraction(&new, &[], &Settings::default()).unwrap();
        let mock = Mock::replying("- Mira met the player.\n- Mira met the player.\nNONE");
        let facts = extract_facts(&mock, &job, &[]).await.unwrap();
        assert_eq!(facts, ["Mira met the player."]);
    }

    /// The pieces work end to end against a real database: summarize, store at the covered
    /// node, and see the right summary and facts after rewinding.
    #[tokio::test]
    async fn end_to_end_with_the_database() {
        use crate::db::Db;

        let db = Db::open_in_memory().unwrap();
        let session = db.create_session("s", None).unwrap().id;
        let mut parent = None;
        let mut ids = Vec::new();
        for i in 1..=40 {
            let node = db
                .add_node(&NewNode {
                    session_id: session,
                    parent_id: parent,
                    kind: if i % 2 == 1 {
                        NodeKind::User
                    } else {
                        NodeKind::Narrator
                    },
                    speaker: None,
                    content: format!("line {i}"),
                    label: None,
                    active_lore: vec![],
                })
                .unwrap();
            parent = Some(node.id);
            ids.push(node.id);
        }
        let settings = settings(30, 20);

        let tip = ids[39];
        let path = db.path_to_root(tip).unwrap();
        let job = plan_summary(&path, db.summary_at(tip).unwrap().as_ref(), &settings).unwrap();
        let text = summarize(&Mock::replying("Summary one."), &job)
            .await
            .unwrap();
        db.add_summary(job.through_node, &text).unwrap();
        assert_eq!(job.through_node, ids[19]);

        // At the tip the new summary applies; nothing more is due yet.
        let current = db.summary_at(tip).unwrap();
        assert_eq!(current.as_ref().unwrap().text, "Summary one.");
        assert!(plan_summary(&path, current.as_ref(), &settings).is_none());
        // Rewinding to before the summarized range hides it again.
        assert_eq!(db.summary_at(ids[5]).unwrap(), None);

        // Facts learned at the tip are unknown after rewinding.
        let job = plan_fact_extraction(&path[38..], &[], &settings).unwrap();
        let facts = extract_facts(&Mock::replying("- A secret door exists."), &job, &[])
            .await
            .unwrap();
        for fact in &facts {
            db.add_fact(job.node_id, fact).unwrap();
        }
        assert_eq!(db.facts_at(tip).unwrap().len(), 1);
        assert!(db.facts_at(ids[10]).unwrap().is_empty());
    }
}
