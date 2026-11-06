//! Semantic memory: find the facts and older messages that fit what is being said now.
//!
//! Vectors are kept in the database ([`Db::put_embedding`]) and compared in memory. A text is
//! embedded the first time it is needed, so stories that existed before semantic memory
//! was switched on catch up by themselves.

use std::collections::HashSet;

use crate::db::{Db, DbError, Fact, Node, NodeKind, SessionId};
use crate::embed::{EmbedError, Embedder};
use crate::vector::nearest;

/// Most facts brought forward for one message.
pub const MAX_RECALLED_FACTS: usize = 5;

/// Most older messages brought back for one message.
pub const MAX_RECALLED_MESSAGES: usize = 3;

/// How alike a fact must be to the message to count as relevant.
pub const MIN_FACT_SCORE: f32 = 0.25;

/// How alike an older message must be to the message to be brought back.
pub const MIN_MESSAGE_SCORE: f32 = 0.35;

/// Longest recalled message, in characters. Longer ones are cut with an ellipsis.
pub const MAX_RECALLED_CHARS: usize = 300;

const KIND_FACT: &str = "fact";
const KIND_NODE: &str = "node";

/// What can go wrong while recalling.
#[derive(Debug, thiserror::Error)]
pub enum RecallError {
    /// The database failed.
    #[error(transparent)]
    Db(#[from] DbError),
    /// The embedding model failed.
    #[error(transparent)]
    Embed(#[from] EmbedError),
}

/// What fits the message, least relevant first, so the best entries are the last to be dropped
/// when the prompt is trimmed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recall {
    /// Texts of the facts that fit.
    pub facts: Vec<String>,
    /// Older messages that fit, as `Speaker: text`.
    pub messages: Vec<String>,
}

/// Find what fits `query` among `facts` and the `older` messages (those that no longer fit in
/// the conversation window). Missing vectors are made on the way.
pub fn recall(
    db: &Db,
    embedder: &dyn Embedder,
    session: SessionId,
    query: &str,
    facts: &[Fact],
    older: &[&Node],
) -> Result<Recall, RecallError> {
    if query.trim().is_empty() || (facts.is_empty() && older.is_empty()) {
        return Ok(Recall::default());
    }
    let model = embedder.model();
    let spoken: Vec<&Node> = older
        .iter()
        .copied()
        .filter(|n| n.kind != NodeKind::System && !n.content.trim().is_empty())
        .collect();

    index(
        db,
        embedder,
        session,
        KIND_FACT,
        facts.iter().map(|f| (f.id.0, f.text.as_str())),
    )?;
    index(
        db,
        embedder,
        session,
        KIND_NODE,
        spoken.iter().map(|n| (n.id.0, n.content.as_str())),
    )?;

    let query = embedder
        .embed(&[query.to_string()])?
        .into_iter()
        .next()
        .unwrap_or_default();

    let wanted = |kind: &str, ids: HashSet<i64>| -> Result<Vec<(i64, Vec<f32>)>, DbError> {
        Ok(db
            .embeddings(session, kind, model)?
            .into_iter()
            .filter(|(id, _)| ids.contains(id))
            .collect())
    };
    let fact_vectors = wanted(KIND_FACT, facts.iter().map(|f| f.id.0).collect())?;
    let node_vectors = wanted(KIND_NODE, spoken.iter().map(|n| n.id.0).collect())?;

    let mut hits = nearest(&query, &fact_vectors, MAX_RECALLED_FACTS, MIN_FACT_SCORE);
    hits.reverse();
    let fact_text = |id: i64| facts.iter().find(|f| f.id.0 == id).map(|f| f.text.clone());
    let facts_out = hits.iter().filter_map(|(id, _)| fact_text(*id)).collect();

    let mut hits = nearest(
        &query,
        &node_vectors,
        MAX_RECALLED_MESSAGES,
        MIN_MESSAGE_SCORE,
    );
    hits.reverse();
    let messages = hits
        .iter()
        .filter_map(|(id, _)| spoken.iter().find(|n| n.id.0 == *id))
        .map(|n| render_message(n))
        .collect();

    Ok(Recall {
        facts: facts_out,
        messages,
    })
}

/// Embed and store the texts that have no vector from this model yet.
fn index<'a>(
    db: &Db,
    embedder: &dyn Embedder,
    session: SessionId,
    kind: &str,
    items: impl Iterator<Item = (i64, &'a str)>,
) -> Result<(), RecallError> {
    let model = embedder.model();
    let known: HashSet<i64> = db
        .embeddings(session, kind, model)?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let missing: Vec<(i64, &str)> = items.filter(|(id, _)| !known.contains(id)).collect();
    if missing.is_empty() {
        return Ok(());
    }
    let texts: Vec<String> = missing.iter().map(|(_, t)| (*t).to_string()).collect();
    let vectors = embedder.embed(&texts)?;
    for ((id, _), vector) in missing.iter().zip(&vectors) {
        db.put_embedding(session, kind, *id, model, vector)?;
    }
    Ok(())
}

fn render_message(node: &Node) -> String {
    let who = match node.kind {
        NodeKind::User => "Player",
        NodeKind::Narrator => "Narrator",
        _ => node.speaker.as_deref().unwrap_or("Character"),
    };
    let text = node.content.trim();
    if text.chars().count() > MAX_RECALLED_CHARS {
        let cut: String = text.chars().take(MAX_RECALLED_CHARS).collect();
        format!("{who}: {}...", cut.trim_end())
    } else {
        format!("{who}: {text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewNode;
    use crate::embed::HashEmbedder;

    fn story() -> (Db, SessionId, Vec<Node>) {
        let db = Db::open_in_memory().unwrap();
        let session = db.create_session("Test", None).unwrap().id;
        let mut parent = None;
        let mut nodes = Vec::new();
        for (kind, speaker, text) in [
            (
                NodeKind::User,
                None,
                "I ask where the silver key is hidden.",
            ),
            (
                NodeKind::Character,
                Some("Mira"),
                "The silver key is under the lighthouse stairs.",
            ),
            (NodeKind::User, None, "We eat bread and cheese by the fire."),
            (NodeKind::System, None, "Silver key scene note."),
        ] {
            let node = db
                .add_node(&NewNode {
                    session_id: session,
                    parent_id: parent,
                    kind,
                    speaker: speaker.map(Into::into),
                    content: text.into(),
                    label: None,
                    active_lore: Vec::new(),
                })
                .unwrap();
            parent = Some(node.id);
            nodes.push(node);
        }
        (db, session, nodes)
    }

    #[test]
    fn the_fitting_facts_and_old_messages_come_back_and_the_rest_stay_out() {
        let (db, session, nodes) = story();
        let leaf = nodes.last().unwrap().id;
        let keep = db
            .add_fact(leaf, "Mira hid the silver key in the lighthouse.")
            .unwrap();
        let other = db
            .add_fact(leaf, "Dragons sleep beneath the northern mountain.")
            .unwrap();
        let older: Vec<&Node> = nodes.iter().collect();
        let out = recall(
            &db,
            &HashEmbedder,
            session,
            "Who knows where the silver key is?",
            &[keep.clone(), other],
            &older,
        )
        .unwrap();
        assert_eq!(out.facts, [keep.text]);
        assert!(out.messages.iter().all(|m| !m.contains("bread")));
        assert!(out
            .messages
            .iter()
            .any(|m| m.starts_with("Mira: The silver key")));
        assert!(
            out.messages.iter().all(|m| !m.contains("scene note")),
            "system notes are never recalled"
        );
    }

    #[test]
    fn vectors_are_made_once_and_kept() {
        let (db, session, nodes) = story();
        let older: Vec<&Node> = nodes.iter().collect();
        recall(&db, &HashEmbedder, session, "silver key", &[], &older).unwrap();
        let stored = db.embeddings(session, KIND_NODE, "hash-256").unwrap();
        assert_eq!(stored.len(), 3, "the system note is skipped");
        recall(&db, &HashEmbedder, session, "bread", &[], &older).unwrap();
        assert_eq!(
            db.embeddings(session, KIND_NODE, "hash-256").unwrap(),
            stored
        );
        // Vectors of another model are not used.
        assert!(db
            .embeddings(session, KIND_NODE, "other")
            .unwrap()
            .is_empty());
        db.clear_embeddings(session).unwrap();
        assert!(db
            .embeddings(session, KIND_NODE, "hash-256")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn nothing_is_recalled_for_an_empty_question_or_an_empty_memory() {
        let (db, session, nodes) = story();
        let older: Vec<&Node> = nodes.iter().collect();
        assert_eq!(
            recall(&db, &HashEmbedder, session, "  ", &[], &older).unwrap(),
            Recall::default()
        );
        assert_eq!(
            recall(&db, &HashEmbedder, session, "silver key", &[], &[]).unwrap(),
            Recall::default()
        );
    }

    #[test]
    fn long_messages_are_cut() {
        let (db, session, _) = story();
        let long = db
            .add_node(&NewNode {
                session_id: session,
                parent_id: None,
                kind: NodeKind::Narrator,
                speaker: None,
                content: "silver key ".repeat(60),
                label: None,
                active_lore: Vec::new(),
            })
            .unwrap();
        let out = recall(&db, &HashEmbedder, session, "silver key", &[], &[&long]).unwrap();
        let text = &out.messages[0];
        assert!(text.starts_with("Narrator: silver key silver"));
        assert!(text.ends_with("..."));
        assert!(text.chars().count() < MAX_RECALLED_CHARS + 20);
    }
}
