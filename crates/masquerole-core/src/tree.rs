//! The story tree: branching, rewinding, bookmarks, pruning, and the `/tree` text view.
//!
//! The data lives in [`crate::db`]; [`StoryTree`] is the session-scoped API on top of it.
//! Every turn is a node whose parent is the previous turn, so going back and writing
//! something new simply adds a sibling. What the story "knows" at a node (facts, summary,
//! active lore) is derived from the path to it, so jumping to a node restores exactly that
//! state and hides everything learned afterwards.

use std::collections::{HashMap, HashSet};

use crate::db::{Db, DbError, Fact, NewNode, Node, NodeId, NodeKind, Session, SessionId, Summary};

/// Longest node label in the text view, in characters.
const MAX_LABEL_CHARS: usize = 40;

/// Errors from tree operations.
#[derive(Debug, thiserror::Error)]
pub enum TreeError {
    /// The database failed.
    #[error(transparent)]
    Db(#[from] DbError),
    /// The story has no current node yet.
    #[error("the story has no current node")]
    NoCurrentNode,
    /// The current node is the root, so there is nothing before it.
    #[error("already at the start of the story")]
    AtRoot,
    /// No node matches what the user typed.
    #[error("no node or bookmark matches {0:?}")]
    NotFound(String),
    /// More than one node matches what the user typed.
    #[error("{target:?} matches several nodes: {}", .candidates.iter().map(|id| format!("#{}", id.0)).collect::<Vec<_>>().join(", "))]
    Ambiguous {
        /// What the user typed.
        target: String,
        /// The nodes it matched.
        candidates: Vec<NodeId>,
    },
}

/// A new turn to add to the story.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    /// Who produced it.
    pub kind: NodeKind,
    /// Who spoke, for character turns.
    pub speaker: Option<String>,
    /// The text.
    pub content: String,
    /// A short label for the tree view.
    pub label: Option<String>,
    /// Identifiers of the lorebook entries active for this turn.
    pub active_lore: Vec<String>,
}

impl Turn {
    fn new(kind: NodeKind, speaker: Option<String>, content: impl Into<String>) -> Self {
        Self {
            kind,
            speaker,
            content: content.into(),
            label: None,
            active_lore: Vec::new(),
        }
    }

    /// A turn written by the player.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(NodeKind::User, None, content)
    }

    /// A turn spoken by a character.
    pub fn character(speaker: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(NodeKind::Character, Some(speaker.into()), content)
    }

    /// A narrator turn.
    pub fn narrator(content: impl Into<String>) -> Self {
        Self::new(NodeKind::Narrator, None, content)
    }

    /// A system turn, such as the opening scene.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(NodeKind::System, None, content)
    }

    /// Set the tree label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the active lore identifiers.
    pub fn with_lore(mut self, lore: Vec<String>) -> Self {
        self.active_lore = lore;
        self
    }
}

/// The story state at a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// The node itself, including its active lore.
    pub node: Node,
    /// The nodes from the root to `node`, inclusive.
    pub path: Vec<Node>,
    /// The facts learned on that path. Facts from later turns or other branches are absent.
    pub facts: Vec<Fact>,
    /// The summary in effect at the node.
    pub summary: Option<Summary>,
}

/// Where the story was forked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchPoint {
    /// The fork node.
    pub node: NodeId,
    /// The bookmark set on it, if a label was given.
    pub label: Option<String>,
    /// How many continuations the node already has. Whatever is written next adds one more.
    pub existing_children: usize,
}

/// One storyline, identified by its latest node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    /// The last node of the storyline.
    pub tip: NodeId,
    /// A name for it: the tip's bookmark or label, else a preview of its text.
    pub label: String,
    /// Number of nodes from the root to the tip.
    pub depth: usize,
    /// The nearest earlier node where the story split, if it ever did.
    pub forked_at: Option<NodeId>,
    /// Whether the current node lies on this storyline.
    pub contains_current: bool,
}

/// The result of pruning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pruned {
    /// How many nodes were removed.
    pub removed: usize,
    /// The current node afterwards. If the current node was removed, this is the pruned
    /// node's parent.
    pub current: Option<NodeId>,
}

/// Story-tree operations for one session.
#[derive(Clone, Copy, Debug)]
pub struct StoryTree<'a> {
    db: &'a Db,
    session: SessionId,
}

impl<'a> StoryTree<'a> {
    /// The tree of `session`.
    pub fn new(db: &'a Db, session: SessionId) -> Self {
        Self { db, session }
    }

    /// The session record.
    pub fn session(&self) -> Result<Session, TreeError> {
        Ok(self.db.session(self.session)?)
    }

    /// The node the story is at, or `None` for an empty story.
    pub fn current(&self) -> Result<Option<Node>, TreeError> {
        match self.session()?.current_node_id {
            Some(id) => Ok(Some(self.db.node(id)?)),
            None => Ok(None),
        }
    }

    /// Add a turn after the current node and move to it. In an empty story it becomes the
    /// root. If the current node already has continuations this starts a new storyline.
    pub fn append(&self, turn: Turn) -> Result<Node, TreeError> {
        let parent = self.session()?.current_node_id;
        let node = self.db.add_node(&NewNode {
            session_id: self.session,
            parent_id: parent,
            kind: turn.kind,
            speaker: turn.speaker,
            content: turn.content,
            label: turn.label,
            active_lore: turn.active_lore,
        })?;
        self.db.set_current_node(self.session, Some(node.id))?;
        Ok(node)
    }

    /// Record a fact learned at the current node. It is known from here on, on this
    /// storyline only.
    pub fn learn_fact(&self, text: &str) -> Result<Fact, TreeError> {
        let current = self.require_current()?;
        Ok(self.db.add_fact(current.id, text)?)
    }

    /// The story state at `node`.
    pub fn snapshot(&self, node: NodeId) -> Result<Snapshot, TreeError> {
        let node = self.node_in_session(node)?;
        Ok(Snapshot {
            path: self.db.path_to_root(node.id)?,
            facts: self.db.facts_at(node.id)?,
            summary: self.db.summary_at(node.id)?,
            node,
        })
    }

    /// Make `node` the current node and return the state restored there.
    pub fn jump(&self, node: NodeId) -> Result<Snapshot, TreeError> {
        let snapshot = self.snapshot(node)?;
        self.db.set_current_node(self.session, Some(node))?;
        Ok(snapshot)
    }

    /// Step back to the parent of the current node (`/undo`).
    pub fn undo(&self) -> Result<Snapshot, TreeError> {
        let current = self.require_current()?;
        let parent = current.parent_id.ok_or(TreeError::AtRoot)?;
        self.jump(parent)
    }

    /// Find the node meant by `target`: `#12`, a bookmark, a label (ignoring case), or a
    /// bare node number. Labels that match several nodes are an error.
    pub fn resolve(&self, target: &str) -> Result<NodeId, TreeError> {
        let target = target.trim();
        let not_found = || TreeError::NotFound(target.to_string());
        if target.is_empty() {
            return Err(not_found());
        }
        if let Some(number) = target.strip_prefix('#') {
            let id = number.parse().map_err(|_| not_found())?;
            return Ok(self.node_in_session(NodeId(id))?.id);
        }
        if let Some(node) = self.db.find_bookmark(self.session, target)? {
            return Ok(node.id);
        }
        let wanted = target.to_lowercase();
        let matches: Vec<NodeId> = self
            .db
            .list_nodes(self.session)?
            .into_iter()
            .filter(|n| {
                n.label
                    .as_deref()
                    .is_some_and(|l| l.to_lowercase() == wanted)
            })
            .map(|n| n.id)
            .collect();
        match matches.as_slice() {
            [] => {}
            [only] => return Ok(*only),
            _ => {
                return Err(TreeError::Ambiguous {
                    target: target.to_string(),
                    candidates: matches,
                })
            }
        }
        match target.parse() {
            Ok(id) => Ok(self.node_in_session(NodeId(id))?.id),
            Err(_) => Err(not_found()),
        }
    }

    /// Jump to the node meant by `target` (`/goto`).
    pub fn goto(&self, target: &str) -> Result<Snapshot, TreeError> {
        let id = self.resolve(target)?;
        self.jump(id)
    }

    /// Fork the story at the current node (`/branch`). The next turn becomes a new
    /// storyline next to any that already continue from here. With a label, the node is
    /// also bookmarked so you can come back to it by name.
    pub fn branch(&self, label: Option<&str>) -> Result<BranchPoint, TreeError> {
        let current = self.require_current()?;
        let label = label.map(str::trim).filter(|l| !l.is_empty());
        if let Some(label) = label {
            self.db.set_bookmark(current.id, Some(label))?;
        }
        Ok(BranchPoint {
            node: current.id,
            label: label.map(Into::into),
            existing_children: self.db.children(current.id)?.len(),
        })
    }

    /// Bookmark the current node (`/mark`). A name already used elsewhere moves here.
    pub fn mark(&self, label: &str) -> Result<NodeId, TreeError> {
        let label = label.trim();
        if label.is_empty() {
            return Err(TreeError::NotFound(String::new()));
        }
        let current = self.require_current()?;
        self.db.set_bookmark(current.id, Some(label))?;
        Ok(current.id)
    }

    /// Remove the bookmark from the current node.
    pub fn unmark(&self) -> Result<NodeId, TreeError> {
        let current = self.require_current()?;
        self.db.set_bookmark(current.id, None)?;
        Ok(current.id)
    }

    /// Delete a node and everything after it (`/prune`). If the current node is among
    /// them, the story moves back to the pruned node's parent.
    pub fn prune(&self, target: NodeId) -> Result<Pruned, TreeError> {
        let node = self.node_in_session(target)?;
        let current = self.session()?.current_node_id;
        let current_removed = match current {
            Some(c) => self.db.path_to_root(c)?.iter().any(|n| n.id == target),
            None => false,
        };
        let removed = self.db.delete_subtree(target)?;
        let current = if current_removed {
            self.db.set_current_node(self.session, node.parent_id)?;
            node.parent_id
        } else {
            current
        };
        Ok(Pruned { removed, current })
    }

    /// All storylines, each identified by its tip, oldest first (`/branches`).
    pub fn branches(&self) -> Result<Vec<Branch>, TreeError> {
        let index = Index::load(self.db, self.session)?;
        let on_current_path = self.current_path_ids()?;
        let current = self.session()?.current_node_id;

        let mut branches = Vec::new();
        for node in index.nodes.values().filter(|n| index.is_leaf(n.id)) {
            let ancestors = index.ancestors(node.id);
            let forked_at = ancestors
                .iter()
                .skip(1)
                .find(|id| index.children_of(**id).len() > 1)
                .copied();
            branches.push(Branch {
                tip: node.id,
                label: node
                    .bookmark
                    .clone()
                    .or_else(|| node.label.clone())
                    .unwrap_or_else(|| preview(node)),
                depth: ancestors.len(),
                forked_at,
                contains_current: current.is_some_and(|c| ancestors.contains(&c))
                    || on_current_path.contains(&node.id),
            });
        }
        branches.sort_by_key(|b| b.tip);
        Ok(branches)
    }

    /// The story tree as text, as printed by `/tree`.
    pub fn render(&self) -> Result<String, TreeError> {
        let index = Index::load(self.db, self.session)?;
        if index.roots.is_empty() {
            return Ok("(the story has not started)".to_string());
        }
        let current = self.session()?.current_node_id;
        let on_path = self.current_path_ids()?;

        // Depth-first with an explicit stack, so very long stories cannot overflow it.
        struct Line {
            id: NodeId,
            prefix: String,
            child_prefix: String,
        }
        let mut stack: Vec<Line> = index
            .roots
            .iter()
            .rev()
            .map(|&id| Line {
                id,
                prefix: String::new(),
                child_prefix: String::new(),
            })
            .collect();

        let mut rows: Vec<(String, NodeId)> = Vec::new();
        while let Some(line) = stack.pop() {
            let node = &index.nodes[&line.id];
            let marker = if Some(line.id) == current {
                '\u{25c6}'
            } else if on_path.contains(&line.id) {
                '\u{25cf}'
            } else {
                '\u{25cb}'
            };
            let mut text = format!("{}{marker} {}", line.prefix, display_label(node));
            if let Some(bookmark) = &node.bookmark {
                text.push_str(&format!(" \u{2605}{bookmark}"));
            }
            if Some(line.id) == current {
                text.push_str("   \u{2190} you are here");
            }
            rows.push((text, line.id));

            let children = index.children_of(line.id);
            for (i, &child) in children.iter().enumerate().rev() {
                let last = i + 1 == children.len();
                stack.push(Line {
                    id: child,
                    prefix: format!(
                        "{}{}",
                        line.child_prefix,
                        if last {
                            "\u{2514}\u{2500}"
                        } else {
                            "\u{251c}\u{2500}"
                        }
                    ),
                    child_prefix: format!(
                        "{}{}",
                        line.child_prefix,
                        if last { "  " } else { "\u{2502} " }
                    ),
                });
            }
        }

        let width = rows
            .iter()
            .map(|(t, _)| t.chars().count())
            .max()
            .unwrap_or(0);
        let mut out = String::new();
        for (text, id) in rows {
            let pad = width - text.chars().count();
            out.push_str(&format!("{text}{}  [#{}]\n", " ".repeat(pad), id.0));
        }
        out.push_str(
            "\n\u{25cf} on current path   \u{25c6} current node   \u{25cb} other branch\n",
        );
        Ok(out)
    }

    fn require_current(&self) -> Result<Node, TreeError> {
        self.current()?.ok_or(TreeError::NoCurrentNode)
    }

    /// A node of this session. Nodes of other sessions count as missing.
    fn node_in_session(&self, id: NodeId) -> Result<Node, TreeError> {
        match self.db.node(id) {
            Ok(node) if node.session_id == self.session => Ok(node),
            Ok(_) | Err(DbError::NotFound(_)) => Err(TreeError::NotFound(format!("#{}", id.0))),
            Err(e) => Err(e.into()),
        }
    }

    fn current_path_ids(&self) -> Result<HashSet<NodeId>, TreeError> {
        Ok(match self.session()?.current_node_id {
            Some(c) => self.db.path_to_root(c)?.into_iter().map(|n| n.id).collect(),
            None => HashSet::new(),
        })
    }
}

/// The whole tree in memory, for views that need every node.
struct Index {
    nodes: HashMap<NodeId, Node>,
    children: HashMap<NodeId, Vec<NodeId>>,
    roots: Vec<NodeId>,
}

impl Index {
    fn load(db: &Db, session: SessionId) -> Result<Self, DbError> {
        let mut index = Self {
            nodes: HashMap::new(),
            children: HashMap::new(),
            roots: Vec::new(),
        };
        // Oldest first, so children and roots come out in creation order.
        for node in db.list_nodes(session)? {
            match node.parent_id {
                Some(parent) => index.children.entry(parent).or_default().push(node.id),
                None => index.roots.push(node.id),
            }
            index.nodes.insert(node.id, node);
        }
        Ok(index)
    }

    fn children_of(&self, id: NodeId) -> &[NodeId] {
        self.children.get(&id).map_or(&[], Vec::as_slice)
    }

    fn is_leaf(&self, id: NodeId) -> bool {
        self.children_of(id).is_empty()
    }

    /// `id` followed by its ancestors up to the root.
    fn ancestors(&self, id: NodeId) -> Vec<NodeId> {
        let mut chain = vec![id];
        let mut at = id;
        while let Some(parent) = self.nodes[&at].parent_id {
            chain.push(parent);
            at = parent;
        }
        chain
    }
}

/// The label shown in the tree: the node's label, else a preview of its text.
fn display_label(node: &Node) -> String {
    match node
        .label
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        Some(label) => truncate(label),
        None => preview(node),
    }
}

/// A one-line preview of a node's text, with the speaker for character lines.
fn preview(node: &Node) -> String {
    let text = node
        .content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let text = match (node.kind, node.speaker.as_deref()) {
        (NodeKind::Character, Some(who)) if !text.is_empty() => format!("{who}: {text}"),
        _ => text,
    };
    if text.is_empty() {
        "(empty)".to_string()
    } else {
        truncate(&text)
    }
}

fn truncate(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= MAX_LABEL_CHARS {
        return text;
    }
    let cut: String = text.chars().take(MAX_LABEL_CHARS - 1).collect();
    format!("{}\u{2026}", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Db, SessionId) {
        let db = Db::open_in_memory().unwrap();
        let session = db.create_session("story", None).unwrap().id;
        (db, session)
    }

    /// Adds a labelled node under `parent` without moving the current node.
    fn add(db: &Db, session: SessionId, parent: Option<NodeId>, label: &str) -> NodeId {
        db.add_node(&NewNode {
            session_id: session,
            parent_id: parent,
            kind: NodeKind::User,
            speaker: None,
            content: format!("text of {label}"),
            label: Some(label.into()),
            active_lore: vec![],
        })
        .unwrap()
        .id
    }

    /// The README's example tree. Node ids follow creation order:
    ///
    /// ```text
    /// 1 Start
    /// └─ 2 Meet Mira at the harbor
    ///    ├─ 3 Enter the lighthouse
    ///    │  ├─ 5 Trust the keeper ─ 8 Learn the keeper's secret
    ///    │  └─ 6 Steal the fogglass ─ 9 The order hunts you
    ///    └─ 4 Sail into the fog ─ 7 Shipwreck on the reef
    /// ```
    fn readme_tree(db: &Db, s: SessionId) -> StoryTree<'_> {
        let start = add(db, s, None, "Start");
        let meet = add(db, s, Some(start), "Meet Mira at the harbor");
        let lighthouse = add(db, s, Some(meet), "Enter the lighthouse");
        let fog = add(db, s, Some(meet), "Sail into the fog");
        let trust = add(db, s, Some(lighthouse), "Trust the keeper");
        let steal = add(db, s, Some(lighthouse), "Steal the fogglass");
        let wreck = add(db, s, Some(fog), "Shipwreck on the reef");
        let secret = add(db, s, Some(trust), "Learn the keeper's secret");
        let hunt = add(db, s, Some(steal), "The order hunts you");
        assert_eq!(
            [start, meet, lighthouse, fog, trust, steal, wreck, secret, hunt].map(|n| n.0),
            [1, 2, 3, 4, 5, 6, 7, 8, 9]
        );
        let tree = StoryTree::new(db, s);
        tree.jump(hunt).unwrap();
        tree
    }

    fn fact_texts(snapshot: &Snapshot) -> Vec<&str> {
        snapshot.facts.iter().map(|f| f.text.as_str()).collect()
    }

    // ---- building the story ----

    #[test]
    fn the_first_turn_becomes_the_root_and_later_turns_chain() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        assert!(tree.current().unwrap().is_none());

        let a = tree.append(Turn::system("Opening scene.")).unwrap();
        let b = tree.append(Turn::user("I enter.")).unwrap();
        let c = tree.append(Turn::character("Mira", "Hello.")).unwrap();
        assert_eq!(a.parent_id, None);
        assert_eq!(b.parent_id, Some(a.id));
        assert_eq!(c.parent_id, Some(b.id));
        assert_eq!(tree.current().unwrap().unwrap().id, c.id);
        assert_eq!(c.speaker.as_deref(), Some("Mira"));
    }

    #[test]
    fn turns_keep_their_label_and_lore() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        let node = tree
            .append(
                Turn::narrator("Fog.")
                    .with_label("Fog rolls in")
                    .with_lore(vec!["fogglass".into()]),
            )
            .unwrap();
        assert_eq!(node.label.as_deref(), Some("Fog rolls in"));
        assert_eq!(
            tree.snapshot(node.id).unwrap().node.active_lore,
            ["fogglass"]
        );
    }

    #[test]
    fn writing_after_going_back_makes_a_sibling() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        let root = tree.append(Turn::system("Start")).unwrap();
        let first = tree.append(Turn::user("Left")).unwrap();
        tree.undo().unwrap();
        let second = tree.append(Turn::user("Right")).unwrap();

        assert_eq!(second.parent_id, Some(root.id));
        assert_ne!(first.id, second.id);
        let kids: Vec<_> = db
            .children(root.id)
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(kids, [first.id, second.id]);
        // The old storyline is untouched.
        assert_eq!(db.node(first.id).unwrap().content, "Left");
    }

    // ---- rewinding ----

    #[test]
    fn undo_steps_back_and_stops_at_the_root() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        assert!(matches!(tree.undo(), Err(TreeError::NoCurrentNode)));

        let root = tree.append(Turn::system("Start")).unwrap();
        let next = tree.append(Turn::user("Go")).unwrap();
        assert_eq!(tree.undo().unwrap().node.id, root.id);
        assert_eq!(tree.current().unwrap().unwrap().id, root.id);
        assert!(matches!(tree.undo(), Err(TreeError::AtRoot)));
        // The node we left is still there.
        assert!(db.node(next.id).is_ok());
    }

    #[test]
    fn jumping_hides_facts_learned_later() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        let a = tree.append(Turn::system("Start")).unwrap();
        tree.learn_fact("The harbor is foggy.").unwrap();
        let b = tree.append(Turn::user("Enter the lighthouse")).unwrap();
        tree.learn_fact("The keeper is hiding something.").unwrap();
        let c = tree.append(Turn::user("Steal the fogglass")).unwrap();
        tree.learn_fact("The order will hunt us.").unwrap();

        let at_c = tree.snapshot(c.id).unwrap();
        assert_eq!(fact_texts(&at_c).len(), 3);

        let at_b = tree.jump(b.id).unwrap();
        assert_eq!(
            fact_texts(&at_b),
            ["The harbor is foggy.", "The keeper is hiding something."]
        );
        let at_a = tree.jump(a.id).unwrap();
        assert_eq!(fact_texts(&at_a), ["The harbor is foggy."]);

        // Jumping forward again brings them back: nothing was deleted.
        assert_eq!(fact_texts(&tree.jump(c.id).unwrap()).len(), 3);
    }

    #[test]
    fn a_new_storyline_does_not_see_facts_from_the_old_one() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        let fork = tree.append(Turn::system("Start")).unwrap();
        tree.learn_fact("Shared fact.").unwrap();
        tree.append(Turn::user("Trust the keeper")).unwrap();
        tree.learn_fact("The keeper is an ally.").unwrap();

        tree.jump(fork.id).unwrap();
        let other = tree.append(Turn::user("Sail into the fog")).unwrap();
        tree.learn_fact("There is a reef.").unwrap();

        let snapshot = tree.snapshot(other.id).unwrap();
        assert_eq!(fact_texts(&snapshot), ["Shared fact.", "There is a reef."]);
        assert_eq!(db.list_facts(s).unwrap().len(), 3, "nothing was deleted");
    }

    #[test]
    fn jumping_restores_the_summary_in_effect() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        let a = tree.append(Turn::system("Start")).unwrap();
        let b = tree.append(Turn::user("Two")).unwrap();
        let c = tree.append(Turn::user("Three")).unwrap();
        db.add_summary(b.id, "Up to two.").unwrap();

        assert_eq!(
            tree.snapshot(c.id).unwrap().summary.unwrap().text,
            "Up to two."
        );
        assert_eq!(tree.jump(b.id).unwrap().summary.unwrap().text, "Up to two.");
        assert!(tree.jump(a.id).unwrap().summary.is_none());
    }

    #[test]
    fn snapshots_carry_the_path_and_active_lore() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        tree.append(Turn::system("Start")).unwrap();
        let b = tree
            .append(Turn::user("Ask about the lamp").with_lore(vec!["lamp".into()]))
            .unwrap();
        let snapshot = tree.jump(b.id).unwrap();
        assert_eq!(snapshot.path.len(), 2);
        assert_eq!(snapshot.node.active_lore, ["lamp"]);
    }

    // ---- finding nodes ----

    #[test]
    fn goto_accepts_ids_bookmarks_and_labels() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);

        assert_eq!(tree.resolve("#3").unwrap(), NodeId(3));
        assert_eq!(tree.resolve("3").unwrap(), NodeId(3));
        assert_eq!(tree.resolve("  #3  ").unwrap(), NodeId(3));
        assert_eq!(tree.resolve("enter the LIGHTHOUSE").unwrap(), NodeId(3));

        db.set_bookmark(NodeId(7), Some("wreck")).unwrap();
        assert_eq!(tree.goto("wreck").unwrap().node.id, NodeId(7));
        assert_eq!(tree.current().unwrap().unwrap().id, NodeId(7));
    }

    #[test]
    fn a_bookmark_beats_a_label_and_a_label_beats_a_number() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        db.set_bookmark(NodeId(4), Some("Start")).unwrap(); // also node 1's label
        assert_eq!(tree.resolve("Start").unwrap(), NodeId(4));

        db.set_label(NodeId(6), Some("2")).unwrap(); // looks like node 2
        assert_eq!(tree.resolve("2").unwrap(), NodeId(6));
        assert_eq!(tree.resolve("#2").unwrap(), NodeId(2));
    }

    #[test]
    fn unknown_and_ambiguous_targets_are_errors() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        for bad in ["", "   ", "#", "#x", "#999", "999", "nowhere"] {
            assert!(
                matches!(tree.resolve(bad), Err(TreeError::NotFound(_))),
                "{bad:?}"
            );
        }

        db.set_label(NodeId(3), Some("Crossroads")).unwrap();
        db.set_label(NodeId(4), Some("crossroads")).unwrap();
        match tree.resolve("CROSSROADS") {
            Err(TreeError::Ambiguous { candidates, .. }) => {
                assert_eq!(candidates, [NodeId(3), NodeId(4)]);
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
        let message = tree.resolve("crossroads").unwrap_err().to_string();
        assert!(message.contains("#3, #4"), "{message}");
    }

    #[test]
    fn other_sessions_nodes_are_invisible() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        let other = db.create_session("other", None).unwrap().id;
        let foreign = add(&db, other, None, "Foreign");
        db.set_bookmark(foreign, Some("theirs")).unwrap();

        assert!(matches!(
            tree.resolve(&format!("#{}", foreign.0)),
            Err(TreeError::NotFound(_))
        ));
        assert!(matches!(
            tree.resolve("theirs"),
            Err(TreeError::NotFound(_))
        ));
        assert!(matches!(tree.jump(foreign), Err(TreeError::NotFound(_))));
        assert!(matches!(tree.prune(foreign), Err(TreeError::NotFound(_))));
        assert!(matches!(
            tree.snapshot(foreign),
            Err(TreeError::NotFound(_))
        ));
        assert!(db.node(foreign).is_ok(), "the other session is untouched");
    }

    // ---- branching and bookmarks ----

    #[test]
    fn branch_reports_the_fork_and_optionally_bookmarks_it() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        tree.jump(NodeId(3)).unwrap(); // "Enter the lighthouse" already has two continuations

        let plain = tree.branch(None).unwrap();
        assert_eq!(plain.node, NodeId(3));
        assert_eq!(plain.existing_children, 2);
        assert_eq!(plain.label, None);
        assert_eq!(db.node(NodeId(3)).unwrap().bookmark, None);

        let named = tree.branch(Some("  inside  ")).unwrap();
        assert_eq!(named.label.as_deref(), Some("inside"));
        assert_eq!(tree.resolve("inside").unwrap(), NodeId(3));

        // Blank labels are ignored.
        assert_eq!(tree.branch(Some("  ")).unwrap().label, None);
    }

    #[test]
    fn branching_then_writing_adds_a_storyline() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        tree.append(Turn::system("Start")).unwrap();
        tree.append(Turn::user("Left")).unwrap();
        assert_eq!(tree.branches().unwrap().len(), 1);

        tree.undo().unwrap();
        let point = tree.branch(Some("crossroads")).unwrap();
        assert_eq!(point.existing_children, 1);
        tree.append(Turn::user("Right")).unwrap();
        assert_eq!(tree.branches().unwrap().len(), 2);
    }

    #[test]
    fn branch_and_mark_need_a_current_node() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        assert!(matches!(tree.branch(None), Err(TreeError::NoCurrentNode)));
        assert!(matches!(tree.mark("x"), Err(TreeError::NoCurrentNode)));
        assert!(matches!(tree.unmark(), Err(TreeError::NoCurrentNode)));
        assert!(matches!(
            tree.learn_fact("x"),
            Err(TreeError::NoCurrentNode)
        ));
    }

    #[test]
    fn marking_moves_a_bookmark_and_unmarking_clears_it() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        let a = tree.append(Turn::system("Start")).unwrap();
        tree.mark("decision").unwrap();
        let b = tree.append(Turn::user("Choose")).unwrap();
        assert_eq!(tree.resolve("decision").unwrap(), a.id);

        tree.mark("decision").unwrap(); // reuse the name on the new node
        assert_eq!(tree.resolve("decision").unwrap(), b.id);
        assert_eq!(db.node(a.id).unwrap().bookmark, None);

        tree.unmark().unwrap();
        assert!(matches!(
            tree.resolve("decision"),
            Err(TreeError::NotFound(_))
        ));
        assert!(matches!(tree.mark("  "), Err(TreeError::NotFound(_))));
    }

    // ---- pruning ----

    #[test]
    fn pruning_another_branch_leaves_the_current_node_alone() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s); // current is 9
        let pruned = tree.prune(NodeId(4)).unwrap(); // "Sail into the fog" + shipwreck
        assert_eq!(
            pruned,
            Pruned {
                removed: 2,
                current: Some(NodeId(9))
            }
        );
        assert_eq!(tree.current().unwrap().unwrap().id, NodeId(9));
        assert!(db.node(NodeId(7)).is_err());
        assert_eq!(db.list_nodes(s).unwrap().len(), 7);
    }

    #[test]
    fn pruning_an_ancestor_of_the_current_node_moves_back_to_its_parent() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        let pruned = tree.prune(NodeId(3)).unwrap(); // lighthouse and everything under it
        assert_eq!(pruned.removed, 5); // nodes 3, 5, 6, 8, and 9
        assert_eq!(pruned.current, Some(NodeId(2)));
        assert_eq!(tree.current().unwrap().unwrap().id, NodeId(2));
        // The other storyline survives and the story can continue.
        tree.append(Turn::user("Onward")).unwrap();
        assert_eq!(tree.branches().unwrap().len(), 2);
    }

    #[test]
    fn pruning_the_current_node_itself_steps_back() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        let pruned = tree.prune(NodeId(9)).unwrap();
        assert_eq!(
            pruned,
            Pruned {
                removed: 1,
                current: Some(NodeId(6))
            }
        );
    }

    #[test]
    fn pruning_the_root_empties_the_story() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        let pruned = tree.prune(NodeId(1)).unwrap();
        assert_eq!(
            pruned,
            Pruned {
                removed: 9,
                current: None
            }
        );
        assert!(tree.current().unwrap().is_none());
        assert_eq!(tree.render().unwrap(), "(the story has not started)");
        // And a new story can begin.
        let node = tree.append(Turn::system("Again")).unwrap();
        assert_eq!(node.parent_id, None);
    }

    #[test]
    fn pruning_removes_the_facts_and_summaries_of_pruned_nodes() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        db.add_fact(NodeId(8), "secret").unwrap();
        db.add_fact(NodeId(2), "shared").unwrap();
        db.add_summary(NodeId(8), "summary").unwrap();

        tree.prune(NodeId(5)).unwrap();
        let facts: Vec<_> = db
            .list_facts(s)
            .unwrap()
            .into_iter()
            .map(|f| f.text)
            .collect();
        assert_eq!(facts, ["shared"]);
        assert!(db.summary_at(NodeId(9)).unwrap().is_none());
    }

    #[test]
    fn pruned_bookmarks_stop_resolving() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        db.set_bookmark(NodeId(7), Some("wreck")).unwrap();
        tree.prune(NodeId(4)).unwrap();
        assert!(matches!(tree.resolve("wreck"), Err(TreeError::NotFound(_))));
    }

    // ---- listing branches ----

    #[test]
    fn branches_list_every_tip_with_its_fork_and_depth() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        let branches = tree.branches().unwrap();

        let summary: Vec<_> = branches
            .iter()
            .map(|b| {
                (
                    b.tip.0,
                    b.label.as_str(),
                    b.depth,
                    b.forked_at.map(|n| n.0),
                    b.contains_current,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (7, "Shipwreck on the reef", 4, Some(2), false),
                (8, "Learn the keeper's secret", 5, Some(3), false),
                (9, "The order hunts you", 5, Some(3), true),
            ]
        );
    }

    #[test]
    fn a_straight_story_is_a_single_branch_with_no_fork() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        assert!(tree.branches().unwrap().is_empty());
        tree.append(Turn::system("Start")).unwrap();
        tree.append(Turn::user("One")).unwrap();
        let branches = tree.branches().unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].forked_at, None);
        assert_eq!(branches[0].depth, 2);
    }

    #[test]
    fn branch_labels_prefer_bookmark_then_label_then_text() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        tree.append(Turn::system("Start")).unwrap();
        tree.append(Turn::user("Left")).unwrap();
        tree.undo().unwrap();
        tree.append(Turn::character("Mira", "Right,   please"))
            .unwrap();
        tree.undo().unwrap();
        let c = tree
            .append(Turn::user("Middle").with_label("Center"))
            .unwrap();
        db.set_bookmark(c.id, Some("pinned")).unwrap();

        let labels: Vec<_> = tree
            .branches()
            .unwrap()
            .into_iter()
            .map(|b| b.label)
            .collect();
        assert_eq!(labels, ["Left", "Mira: Right, please", "pinned"]);
    }

    #[test]
    fn the_current_node_in_the_middle_marks_every_branch_below_it() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        tree.jump(NodeId(3)).unwrap();
        let flags: Vec<_> = tree
            .branches()
            .unwrap()
            .into_iter()
            .map(|b| (b.tip.0, b.contains_current))
            .collect();
        assert_eq!(flags, [(7, false), (8, true), (9, true)]);
    }

    // ---- text rendering ----

    #[test]
    fn renders_the_readme_tree() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        let expected = "\
\u{25cf} Start                                         [#1]
\u{2514}\u{2500}\u{25cf} Meet Mira at the harbor                     [#2]
  \u{251c}\u{2500}\u{25cf} Enter the lighthouse                      [#3]
  \u{2502} \u{251c}\u{2500}\u{25cb} Trust the keeper                        [#5]
  \u{2502} \u{2502} \u{2514}\u{2500}\u{25cb} Learn the keeper's secret             [#8]
  \u{2502} \u{2514}\u{2500}\u{25cf} Steal the fogglass                      [#6]
  \u{2502}   \u{2514}\u{2500}\u{25c6} The order hunts you   \u{2190} you are here  [#9]
  \u{2514}\u{2500}\u{25cb} Sail into the fog                         [#4]
    \u{2514}\u{2500}\u{25cb} Shipwreck on the reef                   [#7]

\u{25cf} on current path   \u{25c6} current node   \u{25cb} other branch
";
        assert_eq!(tree.render().unwrap(), expected);
    }

    #[test]
    fn rendered_ids_line_up() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        let rendered = tree.render().unwrap();
        let columns: Vec<_> = rendered
            .lines()
            .filter(|l| l.contains("[#"))
            .map(|l| l.chars().take_while(|&c| c != '[').count())
            .collect();
        assert_eq!(columns.len(), 9);
        assert!(columns.windows(2).all(|w| w[0] == w[1]), "{columns:?}");
    }

    #[test]
    fn rendering_follows_the_current_node() {
        let (db, s) = setup();
        let tree = readme_tree(&db, s);
        tree.jump(NodeId(7)).unwrap();
        let rendered = tree.render().unwrap();
        let line = |id: u8| {
            rendered
                .lines()
                .find(|l| l.ends_with(&format!("[#{id}]")))
                .unwrap()
                .to_string()
        };
        assert!(line(7).contains('\u{25c6}') && line(7).contains("you are here"));
        assert!(line(4).contains('\u{25cf}'), "ancestors are on the path");
        assert!(
            line(9).contains('\u{25cb}'),
            "the old tip is now another branch"
        );
        assert_eq!(rendered.matches("you are here").count(), 1);
    }

    #[test]
    fn rendering_shows_bookmarks_and_previews_unlabelled_nodes() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        tree.append(Turn::system("The scene\nopens   here."))
            .unwrap();
        tree.append(Turn::character("Mira", "Ah, another one who got lost?"))
            .unwrap();
        tree.mark("greeting").unwrap();
        let rendered = tree.render().unwrap();
        assert!(rendered.contains("The scene opens here."), "{rendered}");
        assert!(
            rendered.contains("Mira: Ah, another one who got lost? \u{2605}greeting"),
            "{rendered}"
        );
    }

    #[test]
    fn long_labels_are_shortened() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        tree.append(Turn::user("x".repeat(500))).unwrap();
        let rendered = tree.render().unwrap();
        let line = rendered.lines().next().unwrap();
        assert!(line.contains('\u{2026}'));
        assert!(line.chars().count() < 80, "{line}");
        assert_eq!(
            preview(&db.node(NodeId(1)).unwrap()).chars().count(),
            MAX_LABEL_CHARS
        );
    }

    #[test]
    fn empty_text_still_gets_a_label() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        tree.append(Turn::user("   ")).unwrap();
        assert!(tree.render().unwrap().contains("(empty)"));
    }

    #[test]
    fn several_roots_are_all_shown() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        add(&db, s, None, "First story");
        add(&db, s, None, "Second story");
        let rendered = tree.render().unwrap();
        assert!(rendered.contains("First story") && rendered.contains("Second story"));
    }

    #[test]
    fn very_long_stories_render_and_navigate() {
        let (db, s) = setup();
        let tree = StoryTree::new(&db, s);
        let mut first = None;
        for i in 0..3000 {
            let node = tree.append(Turn::user(format!("turn {i}"))).unwrap();
            first.get_or_insert(node.id);
        }
        let rendered = tree.render().unwrap();
        assert_eq!(rendered.lines().filter(|l| l.contains("[#")).count(), 3000);
        assert_eq!(tree.jump(first.unwrap()).unwrap().path.len(), 1);
        assert_eq!(tree.branches().unwrap().len(), 1);
    }
}
