//! SQLite storage: sessions, characters, story-tree nodes, facts, and summaries.
//!
//! All SQL lives in this module. The schema is versioned with `PRAGMA user_version`;
//! [`MIGRATIONS`] is append-only, so never edit an entry once released.
//!
//! Facts and summaries are attached to the node where they were created. What is known
//! "at" a node is whatever was created on the path from the root to that node, which is
//! how rewinding hides facts from other branches and later turns.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::settings::{Settings, SettingsError};

/// File name of the database inside the data directory.
pub const DB_FILE: &str = "sessions.db";

/// Schema migrations, applied in order. Index `i` upgrades version `i` to `i + 1`.
const MIGRATIONS: &[&str] = &[
    r#"
    CREATE TABLE sessions (
        id              INTEGER PRIMARY KEY,
        name            TEXT NOT NULL,
        world           TEXT,
        current_node_id INTEGER REFERENCES nodes(id) ON DELETE SET NULL,
        created_at      INTEGER NOT NULL,
        updated_at      INTEGER NOT NULL
    );

    CREATE TABLE characters (
        id         INTEGER PRIMARY KEY,
        session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        name       TEXT NOT NULL,
        card       TEXT NOT NULL,
        UNIQUE (session_id, name)
    );

    CREATE TABLE nodes (
        id          INTEGER PRIMARY KEY,
        session_id  INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        parent_id   INTEGER REFERENCES nodes(id) ON DELETE CASCADE,
        kind        TEXT NOT NULL,
        speaker     TEXT,
        content     TEXT NOT NULL,
        label       TEXT,
        bookmark    TEXT,
        active_lore TEXT NOT NULL DEFAULT '[]',
        created_at  INTEGER NOT NULL
    );
    CREATE INDEX nodes_parent  ON nodes(parent_id);
    CREATE INDEX nodes_session ON nodes(session_id);

    CREATE TABLE facts (
        id         INTEGER PRIMARY KEY,
        session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        node_id    INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
        text       TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX facts_node    ON facts(node_id);
    CREATE INDEX facts_session ON facts(session_id);

    CREATE TABLE summaries (
        id         INTEGER PRIMARY KEY,
        session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        node_id    INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
        text       TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX summaries_node ON summaries(node_id);
"#,
    // Estimated tokens sent and received so far, for the optional usage limit.
    "ALTER TABLE sessions ADD COLUMN tokens_used INTEGER NOT NULL DEFAULT 0;",
    // Where the user moved a node's card in the story tree view.
    r#"
    CREATE TABLE node_positions (
        node_id INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
        x       REAL NOT NULL,
        y       REAL NOT NULL
    );
"#,
    // Scenes with several characters: who saw a message, who it was said to, and who is
    // present from this node on. Facts and summaries can belong to some characters only.
    r#"
    CREATE TABLE node_scenes (
        node_id   INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
        witnesses TEXT,
        addressed TEXT,
        present   TEXT
    );
    ALTER TABLE facts ADD COLUMN known_by TEXT;
    ALTER TABLE summaries ADD COLUMN character TEXT;
"#,
];

/// The schema version this build writes.
pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

/// Errors from the database layer.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// SQLite reported an error.
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Creating the database directory failed.
    #[error("could not create {path}: {source}")]
    Io {
        /// The directory involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The data directory could not be determined.
    #[error(transparent)]
    Settings(#[from] SettingsError),
    /// The database was written by a newer version of the app.
    #[error("database schema version {found} is newer than this app supports ({supported})")]
    NewerSchema {
        /// Version stored in the file.
        found: u32,
        /// Highest version this build understands.
        supported: u32,
    },
    /// A row that should exist does not.
    #[error("{0} not found")]
    NotFound(&'static str),
    /// The request mixes rows from different sessions, or is otherwise inconsistent.
    #[error("invalid request: {0}")]
    Invalid(&'static str),
    /// A stored value could not be decoded.
    #[error("corrupt data: {0}")]
    Corrupt(String),
}

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub i64);

        impl ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                self.0.to_sql()
            }
        }

        impl FromSql for $name {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                i64::column_result(value).map(Self)
            }
        }
    };
}

id_type!(
    /// Identifies a session.
    SessionId
);
id_type!(
    /// Identifies a story-tree node.
    NodeId
);
id_type!(
    /// Identifies a character row.
    CharacterId
);
id_type!(
    /// Identifies a fact.
    FactId
);
id_type!(
    /// Identifies a summary.
    SummaryId
);

/// What produced a node's message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// The player.
    User,
    /// A character; see [`Node::speaker`].
    Character,
    /// The narrator.
    Narrator,
    /// A system note, such as the opening scene.
    System,
}

impl NodeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Character => "character",
            Self::Narrator => "narrator",
            Self::System => "system",
        }
    }

    fn parse(s: &str) -> Result<Self, DbError> {
        match s {
            "user" => Ok(Self::User),
            "character" => Ok(Self::Character),
            "narrator" => Ok(Self::Narrator),
            "system" => Ok(Self::System),
            other => Err(DbError::Corrupt(format!("unknown node kind {other:?}"))),
        }
    }
}

/// A roleplay session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// Row id.
    pub id: SessionId,
    /// Display name.
    pub name: String,
    /// Name of the world (lorebook) in use, if any.
    pub world: Option<String>,
    /// The node the story is currently at.
    pub current_node_id: Option<NodeId>,
    /// Creation time, Unix seconds.
    pub created_at: i64,
    /// Last change, Unix seconds.
    pub updated_at: i64,
    /// Estimated tokens used by this session so far (prompts and responses).
    pub tokens_used: i64,
}

/// A character attached to a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Character {
    /// Row id.
    pub id: CharacterId,
    /// The owning session.
    pub session_id: SessionId,
    /// Character name, unique within the session.
    pub name: String,
    /// The character card as YAML text.
    pub card: String,
}

/// A node in the story tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Row id.
    pub id: NodeId,
    /// The owning session.
    pub session_id: SessionId,
    /// The previous node; `None` for the root.
    pub parent_id: Option<NodeId>,
    /// What produced the message.
    pub kind: NodeKind,
    /// Who spoke, for character nodes.
    pub speaker: Option<String>,
    /// The message text.
    pub content: String,
    /// Short label shown in the tree.
    pub label: Option<String>,
    /// Bookmark name, unique per session by convention (see [`Db::set_bookmark`]).
    pub bookmark: Option<String>,
    /// Identifiers of the lorebook entries that were active at this node.
    pub active_lore: Vec<String>,
    /// Creation time, Unix seconds.
    pub created_at: i64,
}

/// Data for a node about to be created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewNode {
    /// The owning session.
    pub session_id: SessionId,
    /// The previous node; `None` for the root.
    pub parent_id: Option<NodeId>,
    /// What produced the message.
    pub kind: NodeKind,
    /// Who spoke, for character nodes.
    pub speaker: Option<String>,
    /// The message text.
    pub content: String,
    /// Short label shown in the tree.
    pub label: Option<String>,
    /// Lorebook entries active at this node.
    pub active_lore: Vec<String>,
}

/// A remembered fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    /// Row id.
    pub id: FactId,
    /// The owning session.
    pub session_id: SessionId,
    /// The node where the fact was learned.
    pub node_id: NodeId,
    /// The fact.
    pub text: String,
    /// Creation time, Unix seconds.
    pub created_at: i64,
    /// The characters who know the fact; `None` means everyone.
    pub known_by: Option<Vec<String>>,
}

/// A summary of the story up to a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    /// Row id.
    pub id: SummaryId,
    /// The owning session.
    pub session_id: SessionId,
    /// The node the summary was written at.
    pub node_id: NodeId,
    /// The summary text.
    pub text: String,
    /// Creation time, Unix seconds.
    pub created_at: i64,
    /// The character whose point of view the summary is written from; `None` for the
    /// shared summary that knows everything.
    pub character: Option<String>,
}

/// Who took part in a message of a scene with several characters. A node without a scene
/// belongs to a story with at most one character: everyone saw it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeScene {
    /// The characters who saw the message; `None` means everyone.
    pub witnesses: Option<Vec<String>>,
    /// The characters the message was said to. Empty when it was not addressed.
    pub addressed: Vec<String>,
    /// The characters present from this node on, when it changes who is present.
    pub present: Option<Vec<String>>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Recursive CTE listing a node and its ancestors, nearest first (`depth` 0 is the node).
/// Bind the node id as `?1`.
const PATH_CTE: &str = "WITH RECURSIVE path(id, parent_id, depth) AS (
        SELECT id, parent_id, 0 FROM nodes WHERE id = ?1
        UNION ALL
        SELECT n.id, n.parent_id, p.depth + 1 FROM nodes n JOIN path p ON n.id = p.parent_id
    )";

const NODE_COLUMNS: &str = "id, session_id, parent_id, kind, speaker, content, label, bookmark, \
     active_lore, created_at";

fn node_from_row(row: &Row<'_>) -> rusqlite::Result<Result<Node, DbError>> {
    let kind: String = row.get(3)?;
    let lore: String = row.get(8)?;
    let node = (|| {
        Ok(Node {
            id: row.get(0)?,
            session_id: row.get(1)?,
            parent_id: row.get(2)?,
            kind: NodeKind::parse(&kind)?,
            speaker: row.get(4)?,
            content: row.get(5)?,
            label: row.get(6)?,
            bookmark: row.get(7)?,
            active_lore: serde_json::from_str(&lore)
                .map_err(|e| DbError::Corrupt(format!("active_lore: {e}")))?,
            created_at: row.get(9)?,
        })
    })();
    Ok(node)
}

fn session_from_row(row: &Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        name: row.get(1)?,
        world: row.get(2)?,
        current_node_id: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        tokens_used: row.get(6)?,
    })
}

/// Read a JSON list of names stored in a text column. A bad value counts as "everyone".
fn names_from_json(text: Option<String>) -> Option<Vec<String>> {
    serde_json::from_str(&text?).ok()
}

fn names_to_json(names: &[String]) -> String {
    serde_json::to_string(names).unwrap_or_else(|_| "[]".into())
}

fn fact_from_row(row: &Row<'_>) -> rusqlite::Result<Fact> {
    Ok(Fact {
        id: row.get(0)?,
        session_id: row.get(1)?,
        node_id: row.get(2)?,
        text: row.get(3)?,
        created_at: row.get(4)?,
        known_by: names_from_json(row.get(5)?),
    })
}

fn summary_from_row(row: &Row<'_>) -> rusqlite::Result<Summary> {
    Ok(Summary {
        id: row.get(0)?,
        session_id: row.get(1)?,
        node_id: row.get(2)?,
        text: row.get(3)?,
        created_at: row.get(4)?,
        character: row.get(5)?,
    })
}

/// A handle to the database.
#[derive(Debug)]
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (or create) the database at the default location and migrate it.
    pub fn open_default() -> Result<Self, DbError> {
        Self::open(&Settings::data_dir()?.join(DB_FILE))
    }

    /// Open (or create) the database at `path` and migrate it.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|source| DbError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
        }
        Self::from_connection(Connection::open(path)?)
    }

    /// A fresh in-memory database, for tests.
    pub fn open_in_memory() -> Result<Self, DbError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(mut conn: Connection) -> Result<Self, DbError> {
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// The schema version stored in the database.
    pub fn schema_version(&self) -> Result<u32, DbError> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    // ---- sessions ----

    /// Create a session.
    pub fn create_session(&self, name: &str, world: Option<&str>) -> Result<Session, DbError> {
        let t = now();
        self.conn.execute(
            "INSERT INTO sessions (name, world, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
            params![name, world, t],
        )?;
        self.session(SessionId(self.conn.last_insert_rowid()))
    }

    /// Add `tokens` to the session's estimated usage. Amounts of zero or less are ignored.
    pub fn add_tokens(&self, id: SessionId, tokens: i64) -> Result<(), DbError> {
        if tokens <= 0 {
            return Ok(());
        }
        let changed = self.conn.execute(
            "UPDATE sessions SET tokens_used = tokens_used + ?1 WHERE id = ?2",
            params![tokens, id],
        )?;
        if changed == 0 {
            return Err(DbError::NotFound("session"));
        }
        Ok(())
    }

    /// Fetch a session.
    pub fn session(&self, id: SessionId) -> Result<Session, DbError> {
        self.conn
            .query_row(
                "SELECT id, name, world, current_node_id, created_at, updated_at, tokens_used \
                 FROM sessions WHERE id = ?1",
                [id],
                session_from_row,
            )
            .optional()?
            .ok_or(DbError::NotFound("session"))
    }

    /// All sessions, most recently changed first.
    pub fn list_sessions(&self) -> Result<Vec<Session>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, world, current_node_id, created_at, updated_at, tokens_used \
             FROM sessions ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], session_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Find a session by exact name (most recently changed first if several match).
    pub fn find_session_by_name(&self, name: &str) -> Result<Option<Session>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, name, world, current_node_id, created_at, updated_at, tokens_used \
                 FROM sessions WHERE name = ?1 ORDER BY updated_at DESC, id DESC LIMIT 1",
                [name],
                session_from_row,
            )
            .optional()?)
    }

    /// Rename a session.
    pub fn rename_session(&self, id: SessionId, name: &str) -> Result<(), DbError> {
        self.expect_one(
            self.conn.execute(
                "UPDATE sessions SET name = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, name, now()],
            )?,
            "session",
        )
    }

    /// Set the world used by a session.
    pub fn set_session_world(&self, id: SessionId, world: Option<&str>) -> Result<(), DbError> {
        self.expect_one(
            self.conn.execute(
                "UPDATE sessions SET world = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, world, now()],
            )?,
            "session",
        )
    }

    /// Move the session's position in the story tree.
    pub fn set_current_node(&self, id: SessionId, node: Option<NodeId>) -> Result<(), DbError> {
        if let Some(node) = node {
            self.ensure_node_in_session(node, id)?;
        }
        self.expect_one(
            self.conn.execute(
                "UPDATE sessions SET current_node_id = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, node, now()],
            )?,
            "session",
        )
    }

    /// Delete a session and everything in it.
    pub fn delete_session(&self, id: SessionId) -> Result<(), DbError> {
        self.expect_one(
            self.conn
                .execute("DELETE FROM sessions WHERE id = ?1", [id])?,
            "session",
        )
    }

    // ---- characters ----

    /// Attach a character card to a session. Names are unique within a session.
    pub fn add_character(
        &self,
        session: SessionId,
        name: &str,
        card: &str,
    ) -> Result<Character, DbError> {
        self.conn.execute(
            "INSERT INTO characters (session_id, name, card) VALUES (?1, ?2, ?3)",
            params![session, name, card],
        )?;
        Ok(Character {
            id: CharacterId(self.conn.last_insert_rowid()),
            session_id: session,
            name: name.into(),
            card: card.into(),
        })
    }

    /// The characters of a session, in the order they were added.
    pub fn list_characters(&self, session: SessionId) -> Result<Vec<Character>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, name, card FROM characters WHERE session_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([session], |r| {
            Ok(Character {
                id: r.get(0)?,
                session_id: r.get(1)?,
                name: r.get(2)?,
                card: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Remove a character from a session.
    pub fn remove_character(&self, id: CharacterId) -> Result<(), DbError> {
        self.expect_one(
            self.conn
                .execute("DELETE FROM characters WHERE id = ?1", [id])?,
            "character",
        )
    }

    // ---- story tree ----

    /// Add a node. The parent, if any, must belong to the same session.
    pub fn add_node(&self, new: &NewNode) -> Result<Node, DbError> {
        if let Some(parent) = new.parent_id {
            self.ensure_node_in_session(parent, new.session_id)?;
        }
        let lore = serde_json::to_string(&new.active_lore)
            .map_err(|e| DbError::Corrupt(format!("active_lore: {e}")))?;
        self.conn.execute(
            "INSERT INTO nodes (session_id, parent_id, kind, speaker, content, label, \
             active_lore, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                new.session_id,
                new.parent_id,
                new.kind.as_str(),
                new.speaker,
                new.content,
                new.label,
                lore,
                now()
            ],
        )?;
        self.touch(new.session_id)?;
        self.node(NodeId(self.conn.last_insert_rowid()))
    }

    /// Fetch a node.
    pub fn node(&self, id: NodeId) -> Result<Node, DbError> {
        self.conn
            .query_row(
                &format!("SELECT {NODE_COLUMNS} FROM nodes WHERE id = ?1"),
                [id],
                node_from_row,
            )
            .optional()?
            .ok_or(DbError::NotFound("node"))?
    }

    /// All nodes of a session, oldest first.
    pub fn list_nodes(&self, session: SessionId) -> Result<Vec<Node>, DbError> {
        self.collect_nodes(
            &format!("SELECT {NODE_COLUMNS} FROM nodes WHERE session_id = ?1 ORDER BY id"),
            session,
        )
    }

    /// Remember where the card of `node` was put in the tree view. Replaces an earlier position.
    pub fn set_node_position(&self, node: NodeId, x: f32, y: f32) -> Result<(), DbError> {
        if !(x.is_finite() && y.is_finite()) {
            return Err(DbError::Invalid("a position must be a finite number"));
        }
        self.node(node)?;
        self.conn.execute(
            "INSERT INTO node_positions (node_id, x, y) VALUES (?1, ?2, ?3) \
             ON CONFLICT(node_id) DO UPDATE SET x = excluded.x, y = excluded.y",
            params![node, f64::from(x), f64::from(y)],
        )?;
        Ok(())
    }

    /// The positions the user gave to cards of a session's tree, as `(node, x, y)`. Nodes
    /// that were never moved are not listed.
    pub fn node_positions(&self, session: SessionId) -> Result<Vec<(NodeId, f32, f32)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT p.node_id, p.x, p.y FROM node_positions p \
             JOIN nodes n ON n.id = p.node_id WHERE n.session_id = ?1 ORDER BY p.node_id",
        )?;
        let rows = stmt.query_map(params![session], |row| {
            Ok((
                row.get::<_, NodeId>(0)?,
                row.get::<_, f64>(1)? as f32,
                row.get::<_, f64>(2)? as f32,
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Forget the positions of a session's cards, so the tree is laid out afresh.
    pub fn clear_node_positions(&self, session: SessionId) -> Result<(), DbError> {
        self.conn.execute(
            "DELETE FROM node_positions WHERE node_id IN (SELECT id FROM nodes WHERE session_id = ?1)",
            params![session],
        )?;
        Ok(())
    }

    /// The direct children of a node, oldest first.
    pub fn children(&self, id: NodeId) -> Result<Vec<Node>, DbError> {
        self.collect_nodes(
            &format!("SELECT {NODE_COLUMNS} FROM nodes WHERE parent_id = ?1 ORDER BY id"),
            id,
        )
    }

    /// The nodes from the root down to `id`, inclusive.
    pub fn path_to_root(&self, id: NodeId) -> Result<Vec<Node>, DbError> {
        let nodes = self.collect_nodes(
            &format!(
                "{PATH_CTE} SELECT {cols} FROM nodes JOIN path USING (id) ORDER BY path.depth DESC",
                cols = NODE_COLUMNS
                    .split(", ")
                    .map(|c| format!("nodes.{}", c.trim()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            id,
        )?;
        if nodes.is_empty() {
            return Err(DbError::NotFound("node"));
        }
        Ok(nodes)
    }

    /// Set or clear a node's label.
    pub fn set_label(&self, id: NodeId, label: Option<&str>) -> Result<(), DbError> {
        self.expect_one(
            self.conn.execute(
                "UPDATE nodes SET label = ?2 WHERE id = ?1",
                params![id, label],
            )?,
            "node",
        )
    }

    /// Bookmark a node, or clear its bookmark with `None`. A bookmark name is unique within
    /// a session: setting a name that another node holds moves it to this node.
    pub fn set_bookmark(&self, id: NodeId, bookmark: Option<&str>) -> Result<(), DbError> {
        let node = self.node(id)?;
        if let Some(name) = bookmark {
            self.conn.execute(
                "UPDATE nodes SET bookmark = NULL WHERE session_id = ?1 AND bookmark = ?2",
                params![node.session_id, name],
            )?;
        }
        self.conn.execute(
            "UPDATE nodes SET bookmark = ?2 WHERE id = ?1",
            params![id, bookmark],
        )?;
        Ok(())
    }

    /// Find a node by bookmark name within a session.
    pub fn find_bookmark(&self, session: SessionId, name: &str) -> Result<Option<Node>, DbError> {
        let found = self
            .conn
            .query_row(
                &format!(
                    "SELECT {NODE_COLUMNS} FROM nodes WHERE session_id = ?1 AND bookmark = ?2"
                ),
                params![session, name],
                node_from_row,
            )
            .optional()?;
        found.transpose()
    }

    /// Delete a node and its whole subtree, including facts and summaries attached to them.
    /// Returns how many nodes were removed. A session whose current node was removed is left
    /// with no current node.
    pub fn delete_subtree(&self, id: NodeId) -> Result<usize, DbError> {
        let node = self.node(id)?;
        let count: i64 = self.conn.query_row(
            "WITH RECURSIVE sub(id) AS (
                 SELECT id FROM nodes WHERE id = ?1
                 UNION ALL
                 SELECT n.id FROM nodes n JOIN sub ON n.parent_id = sub.id
             ) SELECT COUNT(*) FROM sub",
            [id],
            |r| r.get(0),
        )?;
        self.conn.execute("DELETE FROM nodes WHERE id = ?1", [id])?;
        self.touch(node.session_id)?;
        Ok(count as usize)
    }

    // ---- facts ----

    /// Record a fact learned at `node` that everyone knows.
    pub fn add_fact(&self, node: NodeId, text: &str) -> Result<Fact, DbError> {
        self.add_fact_for(node, text, None)
    }

    /// Record a fact learned at `node`. `known_by` names the characters who know it;
    /// `None` means everyone.
    pub fn add_fact_for(
        &self,
        node: NodeId,
        text: &str,
        known_by: Option<&[String]>,
    ) -> Result<Fact, DbError> {
        let session = self.node(node)?.session_id;
        let t = now();
        self.conn.execute(
            "INSERT INTO facts (session_id, node_id, text, created_at, known_by) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![session, node, text, t, known_by.map(names_to_json)],
        )?;
        Ok(Fact {
            id: FactId(self.conn.last_insert_rowid()),
            session_id: session,
            node_id: node,
            text: text.into(),
            created_at: t,
            known_by: known_by.map(<[String]>::to_vec),
        })
    }

    /// The facts known at `node`: those learned on the path from the root to it. Facts from
    /// other branches or from later nodes are excluded.
    pub fn facts_at(&self, node: NodeId) -> Result<Vec<Fact>, DbError> {
        let mut stmt = self.conn.prepare(&format!(
            "{PATH_CTE} SELECT id, session_id, node_id, text, created_at, known_by FROM facts \
             WHERE node_id IN (SELECT id FROM path) ORDER BY id"
        ))?;
        let rows = stmt.query_map([node], fact_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every fact of a session, regardless of branch.
    pub fn list_facts(&self, session: SessionId) -> Result<Vec<Fact>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, node_id, text, created_at, known_by FROM facts \
             WHERE session_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([session], fact_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Remove a fact.
    pub fn delete_fact(&self, id: FactId) -> Result<(), DbError> {
        self.expect_one(
            self.conn.execute("DELETE FROM facts WHERE id = ?1", [id])?,
            "fact",
        )
    }

    // ---- summaries ----

    /// Store the shared summary written at `node`.
    pub fn add_summary(&self, node: NodeId, text: &str) -> Result<Summary, DbError> {
        self.add_summary_for(node, text, None)
    }

    /// Store a summary written at `node`, from the point of view of `character`, or the
    /// shared one when `None`.
    pub fn add_summary_for(
        &self,
        node: NodeId,
        text: &str,
        character: Option<&str>,
    ) -> Result<Summary, DbError> {
        let session = self.node(node)?.session_id;
        let t = now();
        self.conn.execute(
            "INSERT INTO summaries (session_id, node_id, text, created_at, character) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![session, node, text, t, character],
        )?;
        Ok(Summary {
            id: SummaryId(self.conn.last_insert_rowid()),
            session_id: session,
            node_id: node,
            text: text.into(),
            created_at: t,
            character: character.map(Into::into),
        })
    }

    /// The summary in effect at `node`: the newest one written at the nearest ancestor (or
    /// the node itself) that has one.
    pub fn summary_at(&self, node: NodeId) -> Result<Option<Summary>, DbError> {
        self.summary_for(node, None)
    }

    /// Like [`summary_at`](Self::summary_at), for the summary written from the point of view
    /// of `character` (or the shared one when `None`).
    pub fn summary_for(
        &self,
        node: NodeId,
        character: Option<&str>,
    ) -> Result<Option<Summary>, DbError> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "{PATH_CTE} SELECT s.id, s.session_id, s.node_id, s.text, s.created_at, \
                     s.character FROM summaries s JOIN path p ON s.node_id = p.id \
                     WHERE s.character IS ?2 ORDER BY p.depth ASC, s.id DESC LIMIT 1"
                ),
                params![node, character],
                summary_from_row,
            )
            .optional()?)
    }

    // ---- scenes ----

    /// Record who took part in `node`, replacing an earlier record.
    pub fn set_node_scene(&self, node: NodeId, scene: &NodeScene) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO node_scenes (node_id, witnesses, addressed, present) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                node,
                scene.witnesses.as_deref().map(names_to_json),
                (!scene.addressed.is_empty()).then(|| names_to_json(&scene.addressed)),
                scene.present.as_deref().map(names_to_json),
            ],
        )?;
        Ok(())
    }

    /// The scene records of the nodes on the path from the root to `node`. Nodes without a
    /// record are not in the map.
    pub fn scenes_on_path(
        &self,
        node: NodeId,
    ) -> Result<std::collections::HashMap<NodeId, NodeScene>, DbError> {
        let mut stmt = self.conn.prepare(&format!(
            "{PATH_CTE} SELECT node_id, witnesses, addressed, present FROM node_scenes \
             WHERE node_id IN (SELECT id FROM path)"
        ))?;
        let rows = stmt.query_map([node], |row| {
            Ok((
                row.get::<_, NodeId>(0)?,
                NodeScene {
                    witnesses: names_from_json(row.get(1)?),
                    addressed: names_from_json(row.get(2)?).unwrap_or_default(),
                    present: names_from_json(row.get(3)?),
                },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // ---- helpers ----

    fn collect_nodes(&self, sql: &str, key: impl ToSql) -> Result<Vec<Node>, DbError> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([key], node_from_row)?;
        rows.map(|r| r?).collect()
    }

    fn ensure_node_in_session(&self, node: NodeId, session: SessionId) -> Result<(), DbError> {
        if self.node(node)?.session_id == session {
            Ok(())
        } else {
            Err(DbError::Invalid("node belongs to a different session"))
        }
    }

    fn touch(&self, session: SessionId) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE sessions SET updated_at = ?2 WHERE id = ?1",
            params![session, now()],
        )?;
        Ok(())
    }

    fn expect_one(&self, changed: usize, what: &'static str) -> Result<(), DbError> {
        if changed == 0 {
            Err(DbError::NotFound(what))
        } else {
            Ok(())
        }
    }
}

/// Bring the database up to [`SCHEMA_VERSION`], each step in its own transaction.
fn migrate(conn: &mut Connection) -> Result<(), DbError> {
    let found: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if found > SCHEMA_VERSION {
        return Err(DbError::NewerSchema {
            found,
            supported: SCHEMA_VERSION,
        });
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(found as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as u32 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open_in_memory().unwrap()
    }

    fn new_node(session: SessionId, parent: Option<NodeId>, content: &str) -> NewNode {
        NewNode {
            session_id: session,
            parent_id: parent,
            kind: NodeKind::User,
            speaker: None,
            content: content.into(),
            label: None,
            active_lore: Vec::new(),
        }
    }

    /// Adds a node and returns its id.
    fn add(db: &Db, session: SessionId, parent: Option<NodeId>, content: &str) -> NodeId {
        db.add_node(&new_node(session, parent, content)).unwrap().id
    }

    /// A small tree:
    /// ```text
    /// root ─ a ─ b ─ c
    ///         └─ d
    /// ```
    struct Tree {
        session: SessionId,
        root: NodeId,
        a: NodeId,
        b: NodeId,
        c: NodeId,
        d: NodeId,
    }

    fn tree(db: &Db) -> Tree {
        let session = db.create_session("story", None).unwrap().id;
        let root = add(db, session, None, "root");
        let a = add(db, session, Some(root), "a");
        let b = add(db, session, Some(a), "b");
        let c = add(db, session, Some(b), "c");
        let d = add(db, session, Some(a), "d");
        Tree {
            session,
            root,
            a,
            b,
            c,
            d,
        }
    }

    fn texts(facts: &[Fact]) -> Vec<&str> {
        facts.iter().map(|f| f.text.as_str()).collect()
    }

    #[test]
    fn fresh_database_is_at_the_latest_version() {
        let db = db();
        assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
        for table in ["sessions", "characters", "nodes", "facts", "summaries"] {
            let n: i64 = db
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{table}");
        }
    }

    #[test]
    fn migrations_are_idempotent_and_keep_data() {
        let dir = std::env::temp_dir().join(format!("masquerole-core-db-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join(DB_FILE);

        let id = {
            let db = Db::open(&path).unwrap();
            db.create_session("kept", None).unwrap().id
        };
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(db.session(id).unwrap().name, "kept");

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn newer_schema_is_refused() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(matches!(
            migrate(&mut conn),
            Err(DbError::NewerSchema { .. })
        ));
    }

    #[test]
    fn failed_migration_leaves_version_unchanged() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE sessions (x)").unwrap(); // collides with migration 1
        assert!(migrate(&mut conn).is_err());
        let v: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, 0);
    }

    #[test]
    fn session_crud() {
        let db = db();
        let s = db
            .create_session("Harbor", Some("The Misty Coast"))
            .unwrap();
        assert_eq!(s.world.as_deref(), Some("The Misty Coast"));
        assert_eq!(s.current_node_id, None);

        db.rename_session(s.id, "Lighthouse").unwrap();
        db.set_session_world(s.id, None).unwrap();
        let s2 = db.session(s.id).unwrap();
        assert_eq!(s2.name, "Lighthouse");
        assert_eq!(s2.world, None);

        assert_eq!(
            db.find_session_by_name("Lighthouse").unwrap().unwrap().id,
            s.id
        );
        assert!(db.find_session_by_name("nope").unwrap().is_none());
        assert_eq!(db.list_sessions().unwrap().len(), 1);

        db.delete_session(s.id).unwrap();
        assert!(matches!(db.session(s.id), Err(DbError::NotFound(_))));
        assert!(matches!(
            db.rename_session(s.id, "x"),
            Err(DbError::NotFound(_))
        ));
    }

    #[test]
    fn characters_are_unique_per_session() {
        let db = db();
        let s1 = db.create_session("one", None).unwrap().id;
        let s2 = db.create_session("two", None).unwrap().id;
        let mira = db.add_character(s1, "Mira", "name: Mira").unwrap();
        db.add_character(s2, "Mira", "name: Mira").unwrap(); // other session is fine
        assert!(db.add_character(s1, "Mira", "again").is_err());

        db.add_character(s1, "Keeper", "name: Keeper").unwrap();
        let names: Vec<_> = db
            .list_characters(s1)
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, ["Mira", "Keeper"]);

        db.remove_character(mira.id).unwrap();
        assert_eq!(db.list_characters(s1).unwrap().len(), 1);
        assert!(matches!(
            db.remove_character(mira.id),
            Err(DbError::NotFound(_))
        ));
    }

    #[test]
    fn nodes_round_trip_all_fields() {
        let db = db();
        let s = db.create_session("s", None).unwrap().id;
        let node = db
            .add_node(&NewNode {
                session_id: s,
                parent_id: None,
                kind: NodeKind::Character,
                speaker: Some("Mira".into()),
                content: "Ah, another one who got lost?".into(),
                label: Some("Meet Mira".into()),
                active_lore: vec!["lighthouse".into(), "fogglass".into()],
            })
            .unwrap();
        assert_eq!(db.node(node.id).unwrap(), node);
        assert_eq!(node.kind, NodeKind::Character);
        assert_eq!(node.speaker.as_deref(), Some("Mira"));
        assert_eq!(node.active_lore, ["lighthouse", "fogglass"]);
        assert_eq!(node.bookmark, None);
    }

    #[test]
    fn tree_navigation() {
        let db = db();
        let t = tree(&db);

        let path: Vec<_> = db
            .path_to_root(t.c)
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(path, [t.root, t.a, t.b, t.c]);
        assert_eq!(db.path_to_root(t.root).unwrap().len(), 1);

        let kids: Vec<_> = db
            .children(t.a)
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(kids, [t.b, t.d]);
        assert!(db.children(t.c).unwrap().is_empty());
        assert_eq!(db.list_nodes(t.session).unwrap().len(), 5);

        assert!(matches!(
            db.path_to_root(NodeId(9999)),
            Err(DbError::NotFound(_))
        ));
    }

    #[test]
    fn nodes_cannot_cross_sessions() {
        let db = db();
        let t = tree(&db);
        let other = db.create_session("other", None).unwrap().id;
        assert!(matches!(
            db.add_node(&new_node(other, Some(t.a), "x")),
            Err(DbError::Invalid(_))
        ));
        assert!(matches!(
            db.set_current_node(other, Some(t.a)),
            Err(DbError::Invalid(_))
        ));
    }

    #[test]
    fn current_node_is_tracked_and_cleared_on_prune() {
        let db = db();
        let t = tree(&db);
        db.set_current_node(t.session, Some(t.c)).unwrap();
        assert_eq!(db.session(t.session).unwrap().current_node_id, Some(t.c));

        db.delete_subtree(t.b).unwrap();
        assert_eq!(db.session(t.session).unwrap().current_node_id, None);
    }

    #[test]
    fn labels_and_bookmarks() {
        let db = db();
        let t = tree(&db);
        db.set_label(t.a, Some("Fork")).unwrap();
        assert_eq!(db.node(t.a).unwrap().label.as_deref(), Some("Fork"));

        db.set_bookmark(t.b, Some("decision")).unwrap();
        assert_eq!(
            db.find_bookmark(t.session, "decision").unwrap().unwrap().id,
            t.b
        );

        // Reusing the name moves the bookmark.
        db.set_bookmark(t.d, Some("decision")).unwrap();
        assert_eq!(
            db.find_bookmark(t.session, "decision").unwrap().unwrap().id,
            t.d
        );
        assert_eq!(db.node(t.b).unwrap().bookmark, None);

        db.set_bookmark(t.d, None).unwrap();
        assert!(db.find_bookmark(t.session, "decision").unwrap().is_none());
    }

    #[test]
    fn bookmarks_are_scoped_to_a_session() {
        let db = db();
        let t = tree(&db);
        let other = db.create_session("other", None).unwrap().id;
        let x = add(&db, other, None, "x");
        db.set_bookmark(t.a, Some("here")).unwrap();
        db.set_bookmark(x, Some("here")).unwrap();
        assert_eq!(
            db.find_bookmark(t.session, "here").unwrap().unwrap().id,
            t.a
        );
        assert_eq!(db.find_bookmark(other, "here").unwrap().unwrap().id, x);
    }

    #[test]
    fn delete_subtree_removes_descendants_and_their_data() {
        let db = db();
        let t = tree(&db);
        db.add_fact(t.root, "kept").unwrap();
        db.add_fact(t.b, "gone").unwrap();
        db.add_fact(t.c, "gone too").unwrap();
        db.add_summary(t.c, "gone summary").unwrap();

        assert_eq!(db.delete_subtree(t.b).unwrap(), 2); // b and c
        let remaining: Vec<_> = db
            .list_nodes(t.session)
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(remaining, [t.root, t.a, t.d]);
        assert_eq!(texts(&db.list_facts(t.session).unwrap()), ["kept"]);
        assert_eq!(db.summary_at(t.d).unwrap(), None);
        assert!(matches!(db.node(t.c), Err(DbError::NotFound(_))));
    }

    #[test]
    fn deleting_the_root_removes_everything_in_the_tree() {
        let db = db();
        let t = tree(&db);
        assert_eq!(db.delete_subtree(t.root).unwrap(), 5);
        assert!(db.list_nodes(t.session).unwrap().is_empty());
        assert!(db.session(t.session).is_ok());
    }

    #[test]
    fn facts_are_limited_to_the_path_to_the_node() {
        let db = db();
        let t = tree(&db);
        db.add_fact(t.root, "world exists").unwrap();
        db.add_fact(t.a, "met Mira").unwrap();
        db.add_fact(t.c, "stole the fogglass").unwrap(); // learned later, on one branch
        db.add_fact(t.d, "sailed into the fog").unwrap(); // learned on the other branch

        assert_eq!(
            texts(&db.facts_at(t.c).unwrap()),
            ["world exists", "met Mira", "stole the fogglass"]
        );
        // Rewinding to b hides what was learned at c.
        assert_eq!(
            texts(&db.facts_at(t.b).unwrap()),
            ["world exists", "met Mira"]
        );
        // The other branch never sees c's fact.
        assert_eq!(
            texts(&db.facts_at(t.d).unwrap()),
            ["world exists", "met Mira", "sailed into the fog"]
        );
        assert_eq!(db.list_facts(t.session).unwrap().len(), 4);
    }

    #[test]
    fn facts_can_be_deleted() {
        let db = db();
        let t = tree(&db);
        let fact = db.add_fact(t.a, "wrong").unwrap();
        db.delete_fact(fact.id).unwrap();
        assert!(db.facts_at(t.a).unwrap().is_empty());
        assert!(matches!(db.delete_fact(fact.id), Err(DbError::NotFound(_))));
    }

    #[test]
    fn summary_comes_from_the_nearest_ancestor_and_newest_wins() {
        let db = db();
        let t = tree(&db);
        assert_eq!(db.summary_at(t.c).unwrap(), None);

        db.add_summary(t.a, "early").unwrap();
        assert_eq!(db.summary_at(t.c).unwrap().unwrap().text, "early");

        db.add_summary(t.b, "later").unwrap();
        assert_eq!(db.summary_at(t.c).unwrap().unwrap().text, "later");
        assert_eq!(db.summary_at(t.b).unwrap().unwrap().text, "later");
        // The other branch only sees the shared ancestor's summary.
        assert_eq!(db.summary_at(t.d).unwrap().unwrap().text, "early");

        db.add_summary(t.b, "latest").unwrap();
        assert_eq!(db.summary_at(t.c).unwrap().unwrap().text, "latest");
    }

    #[test]
    fn deleting_a_session_removes_all_its_rows() {
        let db = db();
        let t = tree(&db);
        db.add_character(t.session, "Mira", "name: Mira").unwrap();
        db.add_fact(t.a, "f").unwrap();
        db.add_summary(t.a, "s").unwrap();
        let keep = db.create_session("keep", None).unwrap().id;
        add(&db, keep, None, "survivor");

        db.delete_session(t.session).unwrap();
        for table in ["characters", "nodes", "facts", "summaries"] {
            let n: i64 = db
                .conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE session_id = ?1"),
                    [t.session],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 0, "{table}");
        }
        assert_eq!(db.list_nodes(keep).unwrap().len(), 1);
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let db = db();
        let err = db.conn.execute(
            "INSERT INTO nodes (session_id, kind, content, created_at) VALUES (999, 'user', 'x', 0)",
            [],
        );
        assert!(err.is_err());
    }

    #[test]
    fn unknown_node_kind_is_reported_as_corrupt() {
        let db = db();
        let s = db.create_session("s", None).unwrap().id;
        db.conn
            .execute(
                "INSERT INTO nodes (session_id, kind, content, created_at) VALUES (?1, 'bogus', 'x', 0)",
                [s],
            )
            .unwrap();
        assert!(matches!(db.list_nodes(s), Err(DbError::Corrupt(_))));
    }

    #[test]
    fn adding_nodes_updates_the_session_timestamp() {
        let db = db();
        let s = db.create_session("s", None).unwrap();
        db.conn
            .execute("UPDATE sessions SET updated_at = 0 WHERE id = ?1", [s.id])
            .unwrap();
        add(&db, s.id, None, "x");
        assert!(db.session(s.id).unwrap().updated_at > 0);
    }

    #[test]
    fn card_positions_are_saved_per_story_and_replaced_by_the_latest() {
        let db = db();
        let t = tree(&db);
        let other = db.create_session("other", None).unwrap().id;
        let elsewhere = add(&db, other, None, "root");
        assert!(db.node_positions(t.session).unwrap().is_empty());

        db.set_node_position(t.a, 10.0, 20.5).unwrap();
        db.set_node_position(t.b, -40.0, 0.0).unwrap();
        db.set_node_position(t.a, 11.0, 22.0).unwrap();
        db.set_node_position(elsewhere, 1.0, 2.0).unwrap();
        assert_eq!(
            db.node_positions(t.session).unwrap(),
            [(t.a, 11.0, 22.0), (t.b, -40.0, 0.0)]
        );
        assert_eq!(db.node_positions(other).unwrap(), [(elsewhere, 1.0, 2.0)]);
    }

    #[test]
    fn positions_go_with_their_node_and_can_be_cleared() {
        let db = db();
        let t = tree(&db);
        db.set_node_position(t.c, 1.0, 1.0).unwrap();
        db.set_node_position(t.d, 2.0, 2.0).unwrap();
        db.delete_subtree(t.b).unwrap();
        assert_eq!(db.node_positions(t.session).unwrap(), [(t.d, 2.0, 2.0)]);
        db.clear_node_positions(t.session).unwrap();
        assert!(db.node_positions(t.session).unwrap().is_empty());
    }

    #[test]
    fn a_position_needs_a_real_node_and_finite_numbers() {
        let db = db();
        let t = tree(&db);
        assert!(db.set_node_position(NodeId(9999), 0.0, 0.0).is_err());
        assert!(db.set_node_position(t.a, f32::NAN, 0.0).is_err());
        assert!(db.set_node_position(t.a, 0.0, f32::INFINITY).is_err());
    }

    #[test]
    fn token_usage_accumulates_per_session_and_survives_listing() {
        let db = Db::open_in_memory().unwrap();
        let a = db.create_session("A", None).unwrap();
        let b = db.create_session("B", None).unwrap();
        assert_eq!(a.tokens_used, 0);

        db.add_tokens(a.id, 120).unwrap();
        db.add_tokens(a.id, 30).unwrap();
        db.add_tokens(a.id, 0).unwrap();
        db.add_tokens(a.id, -50).unwrap();
        assert_eq!(db.session(a.id).unwrap().tokens_used, 150);
        assert_eq!(db.session(b.id).unwrap().tokens_used, 0);
        let listed: Vec<_> = db.list_sessions().unwrap();
        assert_eq!(listed.iter().map(|s| s.tokens_used).sum::<i64>(), 150);
        assert!(matches!(
            db.add_tokens(SessionId(999), 5),
            Err(DbError::NotFound(_))
        ));
    }

    #[test]
    fn scenes_facts_and_summaries_can_belong_to_some_characters() {
        let db = db();
        let s = db.create_session("Harbor", None).unwrap().id;
        let a = db.add_node(&new_node(s, None, "a")).unwrap();
        let b = db.add_node(&new_node(s, Some(a.id), "b")).unwrap();

        let scene = NodeScene {
            witnesses: Some(vec!["Mira".into()]),
            addressed: vec!["Mira".into()],
            present: Some(vec!["Mira".into(), "Orrin".into()]),
        };
        db.set_node_scene(b.id, &scene).unwrap();
        let scenes = db.scenes_on_path(b.id).unwrap();
        assert_eq!(scenes.len(), 1);
        assert_eq!(scenes[&b.id], scene);
        assert!(db.scenes_on_path(a.id).unwrap().is_empty());
        db.set_node_scene(b.id, &NodeScene::default()).unwrap();
        assert_eq!(
            db.scenes_on_path(b.id).unwrap()[&b.id],
            NodeScene::default()
        );

        db.add_fact(a.id, "The sea is cold.").unwrap();
        db.add_fact_for(b.id, "Mira hides a key.", Some(&["Mira".to_string()]))
            .unwrap();
        let facts = db.facts_at(b.id).unwrap();
        assert_eq!(facts[0].known_by, None);
        assert_eq!(facts[1].known_by, Some(vec!["Mira".to_string()]));
        assert_eq!(db.list_facts(s).unwrap()[1].known_by, facts[1].known_by);

        db.add_summary(a.id, "shared").unwrap();
        db.add_summary_for(b.id, "Mira's view", Some("Mira"))
            .unwrap();
        assert_eq!(db.summary_at(b.id).unwrap().unwrap().text, "shared");
        let mine = db.summary_for(b.id, Some("Mira")).unwrap().unwrap();
        assert_eq!(
            (mine.text.as_str(), mine.character.as_deref()),
            ("Mira's view", Some("Mira"))
        );
        assert!(db.summary_for(b.id, Some("Orrin")).unwrap().is_none());
    }

    #[test]
    fn deleting_a_node_deletes_its_scene() {
        let db = db();
        let s = db.create_session("Harbor", None).unwrap().id;
        let a = db.add_node(&new_node(s, None, "a")).unwrap();
        let b = db.add_node(&new_node(s, Some(a.id), "b")).unwrap();
        db.set_node_scene(b.id, &NodeScene::default()).unwrap();
        db.delete_session(s).unwrap();
        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM node_scenes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
