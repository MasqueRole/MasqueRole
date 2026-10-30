//! Slash commands: parsing and execution.
//!
//! [`parse`] turns chat input into a [`Command`]; [`execute`] runs one against the core
//! APIs and returns a structured [`CommandOutput`] that any front end can render. Nothing
//! here prints, calls the model, or knows about a UI.
//!
//! Commands that need the model do not call it. `/retry` rewinds the story and returns
//! [`CommandOutput::Regenerate`], and the front end generates the new response.

use crate::character::Character;
use crate::db::{Db, DbError, Fact, FactId, NodeId, NodeKind, Session, SessionId, Summary};
use crate::settings::Settings;
use crate::tree::{Branch, BranchPoint, Pruned, Snapshot, StoryTree, TreeError};

/// A command's name, usage, and one-line description. Drives help text and autocomplete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandInfo {
    /// The word after the slash.
    pub name: &'static str,
    /// How to type it.
    pub usage: &'static str,
    /// What it does.
    pub description: &'static str,
}

/// Every command, in the order the README lists them.
pub const COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        name: "save",
        usage: "/save [name]",
        description: "Save the session",
    },
    CommandInfo {
        name: "load",
        usage: "/load <name>",
        description: "Load a session",
    },
    CommandInfo {
        name: "retry",
        usage: "/retry",
        description: "Regenerate the last response",
    },
    CommandInfo {
        name: "undo",
        usage: "/undo",
        description: "Step back to the parent node",
    },
    CommandInfo {
        name: "tree",
        usage: "/tree",
        description: "Show the story tree",
    },
    CommandInfo {
        name: "branch",
        usage: "/branch [label]",
        description: "Fork the story at the current node",
    },
    CommandInfo {
        name: "goto",
        usage: "/goto <node|label>",
        description: "Jump to any node or bookmark",
    },
    CommandInfo {
        name: "mark",
        usage: "/mark <label>",
        description: "Bookmark the current node",
    },
    CommandInfo {
        name: "branches",
        usage: "/branches",
        description: "List all branches and their tips",
    },
    CommandInfo {
        name: "prune",
        usage: "/prune <node>",
        description: "Delete a node and its subtree",
    },
    CommandInfo {
        name: "memory",
        usage: "/memory",
        description: "Show stored facts",
    },
    CommandInfo {
        name: "forget",
        usage: "/forget <id>",
        description: "Remove a fact",
    },
    CommandInfo {
        name: "summary",
        usage: "/summary",
        description: "Show the current summary",
    },
    CommandInfo {
        name: "char",
        usage: "/char <name>",
        description: "Switch or add a character",
    },
    CommandInfo {
        name: "style",
        usage: "/style <profile>",
        description: "Switch style profile",
    },
    CommandInfo {
        name: "narrator",
        usage: "/narrator on|off",
        description: "Toggle narrator mode",
    },
];

/// A parsed slash command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `/save [name]`
    Save {
        /// A new name for the session.
        name: Option<String>,
    },
    /// `/load <name>`
    Load {
        /// The session to switch to.
        name: String,
    },
    /// `/retry`
    Retry,
    /// `/undo`
    Undo,
    /// `/tree`
    Tree,
    /// `/branch [label]`
    Branch {
        /// A bookmark for the fork point.
        label: Option<String>,
    },
    /// `/goto <node|label>`
    Goto {
        /// A node number, `#number`, bookmark, or label.
        target: String,
    },
    /// `/mark <label>`
    Mark {
        /// The bookmark name.
        label: String,
    },
    /// `/branches`
    Branches,
    /// `/prune <node>`
    Prune {
        /// A node number, `#number`, bookmark, or label.
        target: String,
    },
    /// `/memory`
    Memory,
    /// `/forget <id>`
    Forget {
        /// The fact to remove.
        id: FactId,
    },
    /// `/summary`
    Summary,
    /// `/char <name>`
    Char {
        /// The character's name.
        name: String,
    },
    /// `/style <profile>`
    Style {
        /// The style profile name.
        profile: String,
    },
    /// `/narrator on|off`
    Narrator {
        /// Whether narrator mode is on.
        on: bool,
    },
}

impl Command {
    /// Whether the command deletes data, so a front end should ask for confirmation
    /// before running it.
    pub fn needs_confirmation(&self) -> bool {
        matches!(self, Self::Prune { .. })
    }
}

/// Why input could not be read as a command.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// There is no such command.
    #[error("unknown command /{0}")]
    UnknownCommand(String),
    /// A required argument is missing.
    #[error("missing argument. Usage: {usage}")]
    MissingArgument {
        /// How the command is typed.
        usage: &'static str,
    },
    /// The command takes no argument.
    #[error("this command takes no arguments. Usage: {usage}")]
    UnexpectedArgument {
        /// How the command is typed.
        usage: &'static str,
    },
    /// The argument is not valid.
    #[error("{reason}. Usage: {usage}")]
    InvalidArgument {
        /// How the command is typed.
        usage: &'static str,
        /// What is wrong with it.
        reason: String,
    },
}

/// Read chat input as a command. Input that does not start with `/` is an ordinary
/// message, and yields `Ok(None)`. Command names are case-insensitive; arguments keep
/// their spacing and case.
pub fn parse(input: &str) -> Result<Option<Command>, ParseError> {
    let Some(body) = input.trim().strip_prefix('/') else {
        return Ok(None);
    };
    let (word, rest) = match body.split_once(char::is_whitespace) {
        Some((word, rest)) => (word, rest.trim()),
        None => (body, ""),
    };
    let info = COMMANDS
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(word))
        .ok_or_else(|| ParseError::UnknownCommand(word.to_string()))?;
    let arg = (!rest.is_empty()).then_some(rest);

    let required = || arg.ok_or(ParseError::MissingArgument { usage: info.usage });
    let none = || match arg {
        Some(_) => Err(ParseError::UnexpectedArgument { usage: info.usage }),
        None => Ok(()),
    };
    let invalid = |reason: &str| ParseError::InvalidArgument {
        usage: info.usage,
        reason: reason.to_string(),
    };

    let command = match info.name {
        "save" => Command::Save {
            name: arg.map(Into::into),
        },
        "load" => Command::Load {
            name: required()?.into(),
        },
        "retry" => none().map(|()| Command::Retry)?,
        "undo" => none().map(|()| Command::Undo)?,
        "tree" => none().map(|()| Command::Tree)?,
        "branch" => Command::Branch {
            label: arg.map(Into::into),
        },
        "goto" => Command::Goto {
            target: required()?.into(),
        },
        "mark" => Command::Mark {
            label: required()?.into(),
        },
        "branches" => none().map(|()| Command::Branches)?,
        "prune" => Command::Prune {
            target: required()?.into(),
        },
        "memory" => none().map(|()| Command::Memory)?,
        "forget" => {
            let text = required()?;
            let number = text.strip_prefix('#').unwrap_or(text);
            let id = number
                .parse()
                .map_err(|_| invalid("the fact id must be a number"))?;
            Command::Forget { id: FactId(id) }
        }
        "summary" => none().map(|()| Command::Summary)?,
        "char" => Command::Char {
            name: required()?.into(),
        },
        "style" => Command::Style {
            profile: required()?.into(),
        },
        "narrator" => Command::Narrator {
            on: match required()?.to_lowercase().as_str() {
                "on" => true,
                "off" => false,
                _ => return Err(invalid("expected on or off")),
            },
        },
        other => unreachable!("command table and parser disagree about /{other}"),
    };
    Ok(Some(command))
}

/// What a command did, in a form any front end can render.
#[derive(Clone, Debug, PartialEq)]
pub enum CommandOutput {
    /// `/save`: the session as stored. Everything is stored as you go, so saving only
    /// renames.
    Saved {
        /// The session.
        session: Session,
    },
    /// `/load`: the active session changed.
    Loaded {
        /// The session now active.
        session: Session,
        /// The state at its current node, or `None` if the story has not started.
        snapshot: Option<Snapshot>,
    },
    /// `/retry`: the story was rewound so a new response can be generated from `from`.
    /// The previous response is kept as another branch.
    Regenerate {
        /// The state to generate from.
        from: Snapshot,
        /// The response that was set aside, if there was one.
        replaced: Option<NodeId>,
    },
    /// `/undo` or `/goto`: the story moved to another node.
    Moved(Snapshot),
    /// `/tree`: the story tree as text.
    Tree(String),
    /// `/branch`: the fork point.
    Branched(BranchPoint),
    /// `/mark`: the node that was bookmarked.
    Marked {
        /// The bookmarked node.
        node: NodeId,
        /// The bookmark name.
        label: String,
    },
    /// `/branches`: every storyline.
    Branches(Vec<Branch>),
    /// `/prune`: what was deleted.
    Pruned(Pruned),
    /// `/memory`: the facts known at the current node.
    Facts(Vec<Fact>),
    /// `/forget`: the fact that was removed.
    Forgot(Fact),
    /// `/summary`: the summary in effect, if any.
    Summary(Option<Summary>),
    /// `/char`: the character now in the scene.
    Character {
        /// The character's name as stored.
        name: String,
        /// False if the character was already in the session.
        added: bool,
    },
    /// `/style`: the active style profile.
    Style(String),
    /// `/narrator`: whether narrator mode is on.
    Narrator(bool),
}

/// Why a command failed.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// The input was not a valid command.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// A story-tree operation failed.
    #[error(transparent)]
    Tree(#[from] TreeError),
    /// The database failed.
    #[error(transparent)]
    Db(#[from] DbError),
    /// There is no response to regenerate.
    #[error("there is no response to retry")]
    NothingToRetry,
    /// No session has that name.
    #[error("no session named {0:?}")]
    SessionNotFound(String),
    /// No character card has that name.
    #[error("no character named {name:?}. Available: {}", list(.available))]
    UnknownCharacter {
        /// What was asked for.
        name: String,
        /// The names that exist.
        available: Vec<String>,
    },
    /// No style profile has that name.
    #[error("no style named {name:?}. Available: {}", list(.available))]
    UnknownStyle {
        /// What was asked for.
        name: String,
        /// The names that exist.
        available: Vec<String>,
    },
    /// The session has no fact with that id.
    #[error("no fact with id {0}")]
    FactNotFound(i64),
    /// A character card could not be stored.
    #[error("could not encode the character card: {0}")]
    Card(#[from] serde_yaml_ng::Error),
}

fn list(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

/// Everything a command may read or change.
pub struct CommandContext<'a> {
    /// The database.
    pub db: &'a Db,
    /// The active session. `/load` changes it.
    pub session: SessionId,
    /// The settings. `/style` and `/narrator` change them; saving is up to the caller.
    pub settings: &'a mut Settings,
    /// The character cards `/char` can add.
    pub characters: &'a [Character],
    /// The style profiles `/style` accepts. If empty, any name is accepted.
    pub styles: &'a [String],
}

/// Parse and run chat input. Ordinary messages yield `Ok(None)`.
pub fn run(
    ctx: &mut CommandContext<'_>,
    input: &str,
) -> Result<Option<CommandOutput>, CommandError> {
    match parse(input)? {
        Some(command) => execute(ctx, &command).map(Some),
        None => Ok(None),
    }
}

/// Run a command.
pub fn execute(
    ctx: &mut CommandContext<'_>,
    command: &Command,
) -> Result<CommandOutput, CommandError> {
    let tree = StoryTree::new(ctx.db, ctx.session);
    Ok(match command {
        Command::Save { name } => {
            if let Some(name) = name {
                ctx.db.rename_session(ctx.session, name)?;
            }
            CommandOutput::Saved {
                session: ctx.db.session(ctx.session)?,
            }
        }
        Command::Load { name } => {
            let session = ctx
                .db
                .list_sessions()?
                .into_iter()
                .find(|s| s.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| CommandError::SessionNotFound(name.clone()))?;
            ctx.session = session.id;
            let snapshot = match session.current_node_id {
                Some(node) => Some(StoryTree::new(ctx.db, session.id).snapshot(node)?),
                None => None,
            };
            CommandOutput::Loaded { session, snapshot }
        }
        Command::Retry => {
            let current = tree.current()?.ok_or(CommandError::NothingToRetry)?;
            match current.kind {
                NodeKind::User => CommandOutput::Regenerate {
                    from: tree.snapshot(current.id)?,
                    replaced: None,
                },
                NodeKind::Character | NodeKind::Narrator => {
                    let parent = current.parent_id.ok_or(CommandError::NothingToRetry)?;
                    CommandOutput::Regenerate {
                        from: tree.jump(parent)?,
                        replaced: Some(current.id),
                    }
                }
                NodeKind::System => return Err(CommandError::NothingToRetry),
            }
        }
        Command::Undo => CommandOutput::Moved(tree.undo()?),
        Command::Tree => CommandOutput::Tree(tree.render()?),
        Command::Branch { label } => CommandOutput::Branched(tree.branch(label.as_deref())?),
        Command::Goto { target } => CommandOutput::Moved(tree.goto(target)?),
        Command::Mark { label } => CommandOutput::Marked {
            node: tree.mark(label)?,
            label: label.clone(),
        },
        Command::Branches => CommandOutput::Branches(tree.branches()?),
        Command::Prune { target } => {
            let node = tree.resolve(target)?;
            CommandOutput::Pruned(tree.prune(node)?)
        }
        Command::Memory => CommandOutput::Facts(match tree.current()? {
            Some(node) => ctx.db.facts_at(node.id)?,
            None => Vec::new(),
        }),
        Command::Forget { id } => {
            let fact = ctx
                .db
                .list_facts(ctx.session)?
                .into_iter()
                .find(|f| f.id == *id)
                .ok_or(CommandError::FactNotFound(id.0))?;
            ctx.db.delete_fact(fact.id)?;
            CommandOutput::Forgot(fact)
        }
        Command::Summary => CommandOutput::Summary(match tree.current()? {
            Some(node) => ctx.db.summary_at(node.id)?,
            None => None,
        }),
        Command::Char { name } => {
            let card = ctx
                .characters
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| CommandError::UnknownCharacter {
                    name: name.clone(),
                    available: ctx.characters.iter().map(|c| c.name.clone()).collect(),
                })?;
            let present = ctx
                .db
                .list_characters(ctx.session)?
                .iter()
                .any(|c| c.name == card.name);
            if !present {
                ctx.db
                    .add_character(ctx.session, &card.name, &serde_yaml_ng::to_string(card)?)?;
            }
            CommandOutput::Character {
                name: card.name.clone(),
                added: !present,
            }
        }
        Command::Style { profile } => {
            let known = ctx.styles.iter().find(|s| s.eq_ignore_ascii_case(profile));
            let profile = match (known, ctx.styles.is_empty()) {
                (Some(known), _) => known.clone(),
                (None, true) => profile.clone(),
                (None, false) => {
                    return Err(CommandError::UnknownStyle {
                        name: profile.clone(),
                        available: ctx.styles.to_vec(),
                    })
                }
            };
            ctx.settings.style.profile = profile.clone();
            CommandOutput::Style(profile)
        }
        Command::Narrator { on } => {
            ctx.settings.style.narrator_mode = *on;
            CommandOutput::Narrator(*on)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Turn;

    fn cmd(input: &str) -> Command {
        parse(input).unwrap().unwrap()
    }

    // ---- parsing ----

    #[test]
    fn every_command_parses() {
        let cases: &[(&str, Command)] = &[
            ("/save", Command::Save { name: None }),
            (
                "/save Harbor run",
                Command::Save {
                    name: Some("Harbor run".into()),
                },
            ),
            (
                "/load Harbor run",
                Command::Load {
                    name: "Harbor run".into(),
                },
            ),
            ("/retry", Command::Retry),
            ("/undo", Command::Undo),
            ("/tree", Command::Tree),
            ("/branch", Command::Branch { label: None }),
            (
                "/branch what if",
                Command::Branch {
                    label: Some("what if".into()),
                },
            ),
            (
                "/goto #12",
                Command::Goto {
                    target: "#12".into(),
                },
            ),
            (
                "/goto the lighthouse",
                Command::Goto {
                    target: "the lighthouse".into(),
                },
            ),
            (
                "/mark big choice",
                Command::Mark {
                    label: "big choice".into(),
                },
            ),
            ("/branches", Command::Branches),
            ("/prune 7", Command::Prune { target: "7".into() }),
            ("/memory", Command::Memory),
            ("/forget 3", Command::Forget { id: FactId(3) }),
            ("/forget #3", Command::Forget { id: FactId(3) }),
            ("/summary", Command::Summary),
            (
                "/char Mira Valen",
                Command::Char {
                    name: "Mira Valen".into(),
                },
            ),
            (
                "/style noir",
                Command::Style {
                    profile: "noir".into(),
                },
            ),
            ("/narrator on", Command::Narrator { on: true }),
            ("/narrator off", Command::Narrator { on: false }),
        ];
        for (input, expected) in cases {
            assert_eq!(&cmd(input), expected, "{input}");
        }
    }

    #[test]
    fn plain_messages_are_not_commands() {
        for input in ["", "   ", "hello", "I say /tree to myself", "what/ever"] {
            assert_eq!(parse(input), Ok(None), "{input:?}");
        }
    }

    #[test]
    fn command_names_ignore_case_but_arguments_keep_theirs() {
        assert_eq!(cmd("/TREE"), Command::Tree);
        assert_eq!(
            cmd("/Mark  Big Choice "),
            Command::Mark {
                label: "Big Choice".into()
            }
        );
        assert_eq!(cmd("  /narrator ON  "), Command::Narrator { on: true });
    }

    #[test]
    fn whitespace_between_word_and_argument_can_be_any_kind() {
        assert_eq!(
            cmd("/goto\t#4"),
            Command::Goto {
                target: "#4".into()
            }
        );
        assert_eq!(
            cmd("/goto   #4"),
            Command::Goto {
                target: "#4".into()
            }
        );
    }

    #[test]
    fn unknown_commands_are_reported_with_their_name() {
        assert_eq!(
            parse("/dance"),
            Err(ParseError::UnknownCommand("dance".into()))
        );
        assert_eq!(parse("/"), Err(ParseError::UnknownCommand(String::new())));
        assert_eq!(
            parse("/dance now").unwrap_err().to_string(),
            "unknown command /dance"
        );
    }

    #[test]
    fn missing_arguments_show_the_usage() {
        for (input, usage) in [
            ("/load", "/load <name>"),
            ("/goto", "/goto <node|label>"),
            ("/mark", "/mark <label>"),
            ("/prune", "/prune <node>"),
            ("/forget", "/forget <id>"),
            ("/char", "/char <name>"),
            ("/style", "/style <profile>"),
            ("/narrator", "/narrator on|off"),
            ("/goto   ", "/goto <node|label>"),
        ] {
            assert_eq!(
                parse(input),
                Err(ParseError::MissingArgument { usage }),
                "{input}"
            );
        }
        assert!(parse("/goto")
            .unwrap_err()
            .to_string()
            .contains("Usage: /goto <node|label>"));
    }

    #[test]
    fn commands_without_arguments_reject_extras() {
        for (input, usage) in [
            ("/retry now", "/retry"),
            ("/undo 2", "/undo"),
            ("/tree all", "/tree"),
            ("/branches x", "/branches"),
            ("/memory x", "/memory"),
            ("/summary x", "/summary"),
        ] {
            assert_eq!(
                parse(input),
                Err(ParseError::UnexpectedArgument { usage }),
                "{input}"
            );
        }
    }

    #[test]
    fn invalid_arguments_explain_themselves() {
        let forget = parse("/forget abc").unwrap_err();
        assert!(matches!(forget, ParseError::InvalidArgument { .. }));
        assert!(forget.to_string().contains("must be a number"));
        assert!(parse("/forget 1.5").is_err());
        assert!(parse("/forget #").is_err());

        let narrator = parse("/narrator maybe").unwrap_err();
        assert!(narrator.to_string().contains("expected on or off"));
    }

    #[test]
    fn the_command_table_is_consistent() {
        for info in COMMANDS {
            assert!(
                info.usage.starts_with(&format!("/{}", info.name)),
                "{}",
                info.name
            );
            assert!(!info.description.is_empty());
            // Every table entry is known to the parser: it never answers UnknownCommand.
            let err = parse(&format!("/{}", info.name));
            assert!(
                !matches!(err, Err(ParseError::UnknownCommand(_))),
                "{}",
                info.name
            );
        }
        let mut names: Vec<_> = COMMANDS.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), COMMANDS.len(), "duplicate command names");
    }

    #[test]
    fn the_table_matches_the_readme() {
        // The README's command table is the specification: every row must exist here
        // with the same usage and description, and nothing extra may be here.
        let readme = include_str!("../../../README.md");
        let rows: Vec<(String, String)> = readme
            .lines()
            .filter_map(|line| {
                // Markdown tables escape a `|` inside a cell as `\|`, so protect those
                // before splitting into cells.
                let line = line.replace("\\|", "\u{1}");
                let cells: Vec<&str> = line.split('|').map(str::trim).collect();
                let usage = cells.get(1)?.strip_prefix('`')?.strip_suffix('`')?;
                usage.starts_with('/').then(|| {
                    (
                        usage.replace('\u{1}', "|"),
                        cells.get(2).copied().unwrap_or("").to_string(),
                    )
                })
            })
            .collect();
        let ours: Vec<(String, String)> = COMMANDS
            .iter()
            .map(|c| (c.usage.to_string(), c.description.to_string()))
            .collect();
        assert_eq!(rows, ours);
    }

    #[test]
    fn only_prune_needs_confirmation() {
        for info in COMMANDS {
            // Parse with a plausible argument, whatever the command takes.
            let command = parse(&format!("/{} 1", info.name))
                .or_else(|_| parse(&format!("/{} on", info.name)))
                .or_else(|_| parse(&format!("/{}", info.name)))
                .unwrap()
                .unwrap();
            assert_eq!(
                command.needs_confirmation(),
                info.name == "prune",
                "{}",
                info.name
            );
        }
    }

    // ---- execution ----

    struct World {
        db: Db,
        settings: Settings,
        session: SessionId,
        library: Vec<Character>,
        styles: Vec<String>,
    }

    impl World {
        fn new() -> Self {
            let db = Db::open_in_memory().unwrap();
            let session = db.create_session("Harbor", None).unwrap().id;
            Self {
                db,
                settings: Settings::default(),
                session,
                library: vec![
                    Character::from_yaml("name: Mira Valen\ndescription: Cartographer").unwrap(),
                    Character::from_yaml("name: Orrin Karrow").unwrap(),
                ],
                styles: vec!["default".into(), "noir".into()],
            }
        }

        fn tree(&self) -> StoryTree<'_> {
            StoryTree::new(&self.db, self.session)
        }

        /// system(1) → user(2) → Mira(3), current at 3.
        fn with_story(self) -> Self {
            let tree = self.tree();
            tree.append(Turn::system("Opening.").with_label("Start"))
                .unwrap();
            tree.append(Turn::user("I walk in.")).unwrap();
            tree.append(Turn::character("Mira", "Ah, another one who got lost?"))
                .unwrap();
            self
        }

        fn run(&mut self, input: &str) -> Result<Option<CommandOutput>, CommandError> {
            let mut ctx = CommandContext {
                db: &self.db,
                session: self.session,
                settings: &mut self.settings,
                characters: &self.library,
                styles: &self.styles,
            };
            let result = run(&mut ctx, input);
            self.session = ctx.session;
            result
        }

        fn ok(&mut self, input: &str) -> CommandOutput {
            self.run(input).unwrap().expect("a command")
        }

        fn current_id(&self) -> Option<i64> {
            self.tree().current().unwrap().map(|n| n.id.0)
        }
    }

    #[test]
    fn plain_messages_run_as_nothing() {
        let mut w = World::new();
        assert!(w.run("just talking").unwrap().is_none());
    }

    #[test]
    fn parse_errors_surface_from_run() {
        let mut w = World::new();
        assert!(matches!(
            w.run("/dance"),
            Err(CommandError::Parse(ParseError::UnknownCommand(_)))
        ));
    }

    #[test]
    fn save_renames_only_when_given_a_name() {
        let mut w = World::new();
        let CommandOutput::Saved { session } = w.ok("/save") else {
            panic!()
        };
        assert_eq!(session.name, "Harbor");
        let CommandOutput::Saved { session } = w.ok("/save Lighthouse run") else {
            panic!()
        };
        assert_eq!(session.name, "Lighthouse run");
        assert_eq!(w.db.session(w.session).unwrap().name, "Lighthouse run");
    }

    #[test]
    fn load_switches_the_active_session() {
        let mut w = World::new().with_story();
        let other = w.db.create_session("Reef", None).unwrap().id;
        let first = w.session;

        let CommandOutput::Loaded { session, snapshot } = w.ok("/load reef") else {
            panic!()
        };
        assert_eq!(session.id, other);
        assert!(snapshot.is_none(), "the reef story has not started");
        assert_eq!(w.session, other);

        // Commands now act on the loaded session.
        assert!(matches!(w.ok("/memory"), CommandOutput::Facts(f) if f.is_empty()));

        let CommandOutput::Loaded { snapshot, .. } = w.ok("/load Harbor") else {
            panic!()
        };
        assert_eq!(w.session, first);
        assert_eq!(snapshot.unwrap().node.id, NodeId(3));
    }

    #[test]
    fn load_of_an_unknown_name_fails_and_changes_nothing() {
        let mut w = World::new();
        let before = w.session;
        assert!(
            matches!(w.run("/load nowhere"), Err(CommandError::SessionNotFound(n)) if n == "nowhere")
        );
        assert_eq!(w.session, before);
    }

    #[test]
    fn undo_moves_to_the_parent() {
        let mut w = World::new().with_story();
        let CommandOutput::Moved(snapshot) = w.ok("/undo") else {
            panic!()
        };
        assert_eq!(snapshot.node.id, NodeId(2));
        assert_eq!(w.current_id(), Some(2));

        w.ok("/undo");
        assert!(matches!(
            w.run("/undo"),
            Err(CommandError::Tree(TreeError::AtRoot))
        ));
    }

    #[test]
    fn retry_sets_the_response_aside_and_rewinds_to_the_prompt() {
        let mut w = World::new().with_story();
        let CommandOutput::Regenerate { from, replaced } = w.ok("/retry") else {
            panic!()
        };
        assert_eq!(from.node.id, NodeId(2));
        assert_eq!(replaced, Some(NodeId(3)));
        assert_eq!(w.current_id(), Some(2));
        assert!(w.db.node(NodeId(3)).is_ok(), "the old response is kept");

        // Generating again adds a second branch next to the first.
        w.tree()
            .append(Turn::character("Mira", "Lost? Never."))
            .unwrap();
        assert_eq!(w.tree().branches().unwrap().len(), 2);
    }

    #[test]
    fn retry_on_a_player_turn_regenerates_from_it() {
        let mut w = World::new().with_story();
        w.ok("/undo");
        let CommandOutput::Regenerate { from, replaced } = w.ok("/retry") else {
            panic!()
        };
        assert_eq!((from.node.id, replaced), (NodeId(2), None));
        assert_eq!(w.current_id(), Some(2));
    }

    #[test]
    fn retry_with_nothing_to_regenerate_is_an_error() {
        let mut w = World::new();
        assert!(matches!(w.run("/retry"), Err(CommandError::NothingToRetry)));
        w.tree().append(Turn::system("Opening.")).unwrap();
        assert!(matches!(w.run("/retry"), Err(CommandError::NothingToRetry)));
        // A response with no parent has nothing to regenerate from either.
        let other = w.db.create_session("Solo", None).unwrap().id;
        StoryTree::new(&w.db, other)
            .append(Turn::narrator("Alone."))
            .unwrap();
        w.session = other;
        assert!(matches!(w.run("/retry"), Err(CommandError::NothingToRetry)));
    }

    #[test]
    fn tree_renders_the_story() {
        let mut w = World::new().with_story();
        let CommandOutput::Tree(text) = w.ok("/tree") else {
            panic!()
        };
        assert!(text.contains("Start") && text.contains("you are here"));
        assert!(text.contains("[#3]"));
    }

    #[test]
    fn branch_mark_and_goto_work_together() {
        let mut w = World::new().with_story();
        w.ok("/undo");
        let CommandOutput::Branched(point) = w.ok("/branch crossroads") else {
            panic!()
        };
        assert_eq!(point.node, NodeId(2));
        assert_eq!(point.label.as_deref(), Some("crossroads"));
        assert_eq!(point.existing_children, 1);

        let CommandOutput::Marked { node, label } = w.ok("/mark decision point") else {
            panic!()
        };
        assert_eq!((node, label.as_str()), (NodeId(2), "decision point"));

        w.ok("/goto #1");
        assert_eq!(w.current_id(), Some(1));
        let CommandOutput::Moved(snapshot) = w.ok("/goto decision point") else {
            panic!()
        };
        assert_eq!(snapshot.node.id, NodeId(2));
        w.ok("/goto start"); // by label, ignoring case
        assert_eq!(w.current_id(), Some(1));
        assert!(matches!(
            w.run("/goto nowhere"),
            Err(CommandError::Tree(TreeError::NotFound(_)))
        ));
    }

    #[test]
    fn branches_lists_storylines() {
        let mut w = World::new().with_story();
        w.ok("/retry");
        w.tree()
            .append(Turn::character("Mira", "Another take."))
            .unwrap();
        let CommandOutput::Branches(branches) = w.ok("/branches") else {
            panic!()
        };
        assert_eq!(branches.len(), 2);
        assert_eq!(branches.iter().filter(|b| b.contains_current).count(), 1);
    }

    #[test]
    fn prune_deletes_by_node_reference() {
        let mut w = World::new().with_story();
        let CommandOutput::Pruned(pruned) = w.ok("/prune #3") else {
            panic!()
        };
        assert_eq!(pruned.removed, 1);
        assert_eq!(pruned.current, Some(NodeId(2)));
        assert_eq!(w.current_id(), Some(2));

        w.ok("/mark keep");
        let CommandOutput::Pruned(pruned) = w.ok("/prune keep") else {
            panic!()
        };
        assert_eq!(pruned.removed, 1);
        assert!(matches!(
            w.run("/prune #99"),
            Err(CommandError::Tree(TreeError::NotFound(_)))
        ));
    }

    #[test]
    fn memory_shows_only_what_is_known_at_the_current_node() {
        let mut w = World::new().with_story();
        w.db.add_fact(NodeId(2), "Mira owes a favor.").unwrap();
        w.db.add_fact(NodeId(3), "The lamp is cracked.").unwrap();

        let CommandOutput::Facts(facts) = w.ok("/memory") else {
            panic!()
        };
        assert_eq!(facts.len(), 2);
        w.ok("/undo");
        let CommandOutput::Facts(facts) = w.ok("/memory") else {
            panic!()
        };
        assert_eq!(
            facts.iter().map(|f| f.text.as_str()).collect::<Vec<_>>(),
            ["Mira owes a favor."]
        );
    }

    #[test]
    fn memory_and_summary_are_empty_before_the_story_starts() {
        let mut w = World::new();
        assert!(matches!(w.ok("/memory"), CommandOutput::Facts(f) if f.is_empty()));
        assert!(matches!(w.ok("/summary"), CommandOutput::Summary(None)));
    }

    #[test]
    fn forget_removes_a_fact_of_this_session_only() {
        let mut w = World::new().with_story();
        let fact = w.db.add_fact(NodeId(2), "Wrong fact.").unwrap();

        let other = w.db.create_session("Other", None).unwrap().id;
        let foreign_node = StoryTree::new(&w.db, other)
            .append(Turn::system("x"))
            .unwrap();
        let foreign = w.db.add_fact(foreign_node.id, "Not ours.").unwrap();

        assert!(matches!(
            w.run(&format!("/forget {}", foreign.id.0)),
            Err(CommandError::FactNotFound(id)) if id == foreign.id.0
        ));
        assert!(
            w.db.list_facts(other).unwrap().len() == 1,
            "the other session is untouched"
        );

        let CommandOutput::Forgot(removed) = w.ok(&format!("/forget #{}", fact.id.0)) else {
            panic!()
        };
        assert_eq!(removed.text, "Wrong fact.");
        assert!(w.db.list_facts(w.session).unwrap().is_empty());
        assert!(matches!(
            w.run(&format!("/forget {}", fact.id.0)),
            Err(CommandError::FactNotFound(_))
        ));
    }

    #[test]
    fn summary_shows_the_one_in_effect() {
        let mut w = World::new().with_story();
        w.db.add_summary(NodeId(2), "The player arrived.").unwrap();
        let CommandOutput::Summary(Some(summary)) = w.ok("/summary") else {
            panic!()
        };
        assert_eq!(summary.text, "The player arrived.");
        w.ok("/goto #1");
        assert!(matches!(w.ok("/summary"), CommandOutput::Summary(None)));
    }

    #[test]
    fn char_adds_a_card_once_and_stores_it_as_yaml() {
        let mut w = World::new();
        let CommandOutput::Character { name, added } = w.ok("/char mira valen") else {
            panic!()
        };
        assert_eq!((name.as_str(), added), ("Mira Valen", true));

        let stored = w.db.list_characters(w.session).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(Character::from_yaml(&stored[0].card).unwrap(), w.library[0]);

        // Naming her again switches to her without duplicating.
        let CommandOutput::Character { added, .. } = w.ok("/char Mira Valen") else {
            panic!()
        };
        assert!(!added);
        w.ok("/char Orrin Karrow");
        assert_eq!(w.db.list_characters(w.session).unwrap().len(), 2);
    }

    #[test]
    fn char_with_an_unknown_name_lists_the_choices() {
        let mut w = World::new();
        let err = w.run("/char Nobody").unwrap_err();
        assert!(
            matches!(&err, CommandError::UnknownCharacter { available, .. } if available.len() == 2)
        );
        assert!(err.to_string().contains("Mira Valen, Orrin Karrow"));
        assert!(w.db.list_characters(w.session).unwrap().is_empty());
    }

    #[test]
    fn style_switches_to_a_known_profile() {
        let mut w = World::new();
        let CommandOutput::Style(profile) = w.ok("/style NOIR") else {
            panic!()
        };
        assert_eq!(profile, "noir", "the stored spelling is used");
        assert_eq!(w.settings.style.profile, "noir");

        let err = w.run("/style western").unwrap_err();
        assert!(matches!(err, CommandError::UnknownStyle { .. }));
        assert!(err.to_string().contains("default, noir"));
        assert_eq!(
            w.settings.style.profile, "noir",
            "a failed switch changes nothing"
        );
    }

    #[test]
    fn style_accepts_any_name_when_no_profiles_are_known() {
        let mut w = World::new();
        w.styles.clear();
        w.ok("/style western");
        assert_eq!(w.settings.style.profile, "western");
    }

    #[test]
    fn narrator_toggles_the_setting() {
        let mut w = World::new();
        assert!(!w.settings.style.narrator_mode);
        assert_eq!(w.ok("/narrator on"), CommandOutput::Narrator(true));
        assert!(w.settings.style.narrator_mode);
        assert_eq!(w.ok("/narrator off"), CommandOutput::Narrator(false));
        assert!(!w.settings.style.narrator_mode);
    }

    #[test]
    fn story_commands_need_a_started_story() {
        let mut w = World::new();
        for input in ["/undo", "/branch", "/mark x"] {
            assert!(
                matches!(
                    w.run(input),
                    Err(CommandError::Tree(TreeError::NoCurrentNode))
                ),
                "{input}"
            );
        }
        let CommandOutput::Tree(text) = w.ok("/tree") else {
            panic!()
        };
        assert!(text.contains("has not started"));
        assert!(matches!(w.ok("/branches"), CommandOutput::Branches(b) if b.is_empty()));
    }
}
