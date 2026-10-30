//! The backend: everything that touches the database, files, keychain, or network.
//!
//! The UI never does that work. It sends [`Request`]s to one actor task running on the tokio
//! runtime and receives [`Event`]s back over a channel, so the UI thread never blocks. The
//! actor owns the database and handles requests one at a time; model calls run as separate
//! tasks and report back with requests of their own.

use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use futures_util::StreamExt;
use ir_core::character::Character;
use ir_core::client::{
    self, ChatMessage, GenerationParams, LlmError, LlmProvider, OpenAiCompatible, ProviderConfig,
};
use ir_core::commands::{self, CommandContext, CommandOutput};
use ir_core::context::{self, ContextInput};
use ir_core::db::{Db, NewNode, Node, NodeId, Session, SessionId};
use ir_core::lorebook::{LoreEntry, Lorebook};
use ir_core::memory;
use ir_core::settings::{self, SecretStore, Settings};
use ir_core::tree::{StoryTree, Turn};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Notify;

use crate::format::{self, Notice};
use crate::library::{Entry, Library};
use crate::prompts;

/// Builds the model provider for the current settings and API key.
pub type ProviderFactory =
    Arc<dyn Fn(&Settings, Option<String>) -> Result<Arc<dyn LlmProvider>, LlmError> + Send + Sync>;

/// Asks the UI to redraw. Called after every event, from any thread.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// Where API keys are kept.
pub type Secrets = Arc<dyn SecretStore + Send + Sync>;

/// The provider factory for real use: an OpenAI-compatible client.
pub fn default_provider_factory() -> ProviderFactory {
    Arc::new(|settings, key| {
        let client = OpenAiCompatible::new(ProviderConfig::from_settings(&settings.api, key))?;
        Ok(Arc::new(client) as Arc<dyn LlmProvider>)
    })
}

/// A story as the UI shows it: the path from the first message to the current one.
#[derive(Clone, Debug, PartialEq)]
pub struct Transcript {
    /// The session.
    pub session: Session,
    /// The messages on the current path, oldest first.
    pub nodes: Vec<Node>,
    /// The names of the characters in the scene.
    pub characters: Vec<String>,
}

/// Every node of a story, for the tree view.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeData {
    /// The session the nodes belong to.
    pub session_id: SessionId,
    /// All nodes, oldest first.
    pub nodes: Vec<Node>,
    /// The node the story is at.
    pub current: Option<NodeId>,
}

/// What the UI can ask for, plus the messages the backend's own tasks send back to it.
#[derive(Debug)]
pub enum Request {
    /// Load settings, the library, and the sessions, and open the most recent session.
    Init,
    /// Switch to a session.
    OpenSession(SessionId),
    /// Create a session and open it.
    CreateSession {
        /// The session's name.
        name: String,
        /// The name of the world to use.
        world: Option<String>,
        /// The name of a library character to put in the scene.
        character: Option<String>,
    },
    /// Delete a session and its story.
    DeleteSession(SessionId),
    /// Chat input: a message to the story, or a slash command.
    Send(String),
    /// Stop the response that is being generated, keeping what arrived so far.
    Stop,
    /// Use and save these settings. A non-empty `api_key` replaces the stored key.
    SaveSettings {
        /// The new settings.
        settings: Settings,
        /// A new API key, or `None` to keep the stored one.
        api_key: Option<String>,
    },
    /// Remove the stored API key of the current provider.
    DeleteApiKey,
    /// Start or stop sending the whole story tree with every change. The tree view turns
    /// this on while it is open, so the cost is only paid when the tree is visible.
    WatchTree(bool),
    /// Move the story to a node of the story tree.
    JumpTo(NodeId),
    /// Save a character, to `path` if given, else to a new file.
    SaveCharacter {
        /// The file to overwrite.
        path: Option<PathBuf>,
        /// The card.
        character: Character,
    },
    /// Delete a character file.
    DeleteCharacter(PathBuf),
    /// Save a world, to `path` if given, else to a new file.
    SaveWorld {
        /// The file to overwrite.
        path: Option<PathBuf>,
        /// The lorebook.
        world: Lorebook,
    },
    /// Delete a world file.
    DeleteWorld(PathBuf),

    // Sent by the backend's own tasks.
    /// A response finished (or was cut short but has text).
    Generated {
        /// The session it belongs to.
        session: SessionId,
        /// The node it answers.
        parent: NodeId,
        /// The text.
        text: String,
        /// Identifiers of the lore entries that were active.
        lore: Vec<String>,
    },
    /// A response failed with nothing to keep.
    GenerationFailed(String),
    /// The user stopped a response before any text arrived.
    GenerationStopped,
    /// A new summary is ready.
    SummaryReady {
        /// The last node it covers.
        node: NodeId,
        /// The summary.
        text: String,
    },
    /// New facts were extracted.
    FactsReady {
        /// The node they were learned at.
        node: NodeId,
        /// The facts.
        facts: Vec<String>,
    },
}

/// What the backend tells the UI.
#[derive(Debug)]
pub enum Event {
    /// The current settings, and whether an API key is available for the provider.
    Settings {
        /// The settings.
        settings: Settings,
        /// Whether a key is stored or provided through the environment.
        key_available: bool,
    },
    /// All sessions, most recently changed first.
    Sessions(Vec<Session>),
    /// The character and world library.
    Library {
        /// The characters.
        characters: Vec<Entry<Character>>,
        /// The worlds.
        worlds: Vec<Entry<Lorebook>>,
    },
    /// The active story changed.
    Transcript(Transcript),
    /// The whole story tree, sent while the tree view is open. `None` without a session.
    Tree(Option<TreeData>),
    /// There is no active session.
    NoSession,
    /// A message from a command.
    Notice(Notice),
    /// Text of the response being generated.
    Chunk(String),
    /// A response started or ended.
    Generating(bool),
    /// Something worth a brief message, such as a save.
    Info(String),
    /// Something went wrong.
    Error(String),
}

/// The handle the UI uses to talk to the backend.
#[derive(Clone, Debug)]
pub struct Backend {
    tx: UnboundedSender<Request>,
}

impl Backend {
    /// Queue a request. Never blocks.
    pub fn send(&self, request: Request) {
        let _ = self.tx.send(request);
    }

    /// A handle connected to nothing but the returned receiver, so UI tests can see which
    /// requests were sent.
    #[cfg(test)]
    pub fn detached() -> (Self, UnboundedReceiver<Request>) {
        let (tx, rx) = unbounded_channel();
        (Self { tx }, rx)
    }
}

/// What the backend needs from its surroundings.
pub struct Services {
    /// The database.
    pub db: Db,
    /// The character and world library.
    pub library: Library,
    /// Where `settings.toml` lives. `None` keeps settings in memory only.
    pub settings_path: Option<PathBuf>,
    /// Where API keys are kept.
    pub secrets: Secrets,
    /// Builds the model provider.
    pub provider: ProviderFactory,
    /// Redraws the UI.
    pub wake: Wake,
}

/// Start the backend on `runtime`. Returns the handle and the receiver for its events.
pub fn spawn(
    runtime: &tokio::runtime::Handle,
    services: Services,
) -> (Backend, mpsc::Receiver<Event>) {
    let (tx, rx) = unbounded_channel();
    let (events_tx, events_rx) = mpsc::channel();
    let actor = Actor {
        db: services.db,
        library: services.library,
        settings_path: services.settings_path,
        secrets: services.secrets,
        provider: services.provider,
        wake: services.wake,
        settings: Settings::default(),
        session: None,
        characters: Vec::new(),
        worlds: Vec::new(),
        cancel: None,
        watch_tree: false,
        requests: tx.clone(),
        events: events_tx,
    };
    runtime.spawn(actor.run(rx));
    (Backend { tx }, events_rx)
}

struct Actor {
    db: Db,
    library: Library,
    settings_path: Option<PathBuf>,
    secrets: Secrets,
    provider: ProviderFactory,
    wake: Wake,
    settings: Settings,
    session: Option<SessionId>,
    characters: Vec<Entry<Character>>,
    worlds: Vec<Entry<Lorebook>>,
    /// Set while a response is being generated.
    cancel: Option<Arc<Notify>>,
    /// Whether the tree view is open and wants every change.
    watch_tree: bool,
    requests: UnboundedSender<Request>,
    events: mpsc::Sender<Event>,
}

/// The inputs of one model call.
struct Prompt {
    messages: Vec<ChatMessage>,
    parent: NodeId,
    lore: Vec<String>,
}

impl Actor {
    async fn run(mut self, mut rx: UnboundedReceiver<Request>) {
        while let Some(request) = rx.recv().await {
            self.handle(request);
        }
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        (self.wake)();
    }

    fn error(&self, message: impl ToString) {
        let message = message.to_string();
        tracing::warn!(%message, "error shown to the user");
        self.emit(Event::Error(message));
    }

    fn handle(&mut self, request: Request) {
        match request {
            Request::Init => self.init(),
            Request::OpenSession(id) => {
                self.stop();
                self.open(id);
            }
            Request::CreateSession {
                name,
                world,
                character,
            } => self.create_session(&name, world, character),
            Request::DeleteSession(id) => self.delete_session(id),
            Request::Send(text) => self.send(&text),
            Request::Stop => self.stop(),
            Request::SaveSettings { settings, api_key } => self.save_settings(settings, api_key),
            Request::WatchTree(on) => {
                self.watch_tree = on;
                if on {
                    self.emit_tree();
                }
            }
            Request::JumpTo(node) => self.jump_to(node),
            Request::DeleteApiKey => {
                match self.secrets.delete(&self.settings.api.provider) {
                    Ok(()) => self.emit(Event::Info("API key removed.".into())),
                    Err(e) => self.error(e),
                }
                self.emit_settings();
            }
            Request::SaveCharacter { path, character } => {
                match self.library.save_character(path.as_deref(), &character) {
                    Ok(_) => {
                        self.emit(Event::Info(format!("Saved {}.", character.name)));
                        self.reload_library();
                    }
                    Err(e) => self.error(e),
                }
            }
            Request::DeleteCharacter(path) | Request::DeleteWorld(path) => {
                match self.library.delete(&path) {
                    Ok(()) => self.reload_library(),
                    Err(e) => self.error(e),
                }
            }
            Request::SaveWorld { path, world } => {
                match self.library.save_world(path.as_deref(), &world) {
                    Ok(_) => {
                        self.emit(Event::Info(format!("Saved {}.", world.name)));
                        self.reload_library();
                    }
                    Err(e) => self.error(e),
                }
            }
            Request::Generated {
                session,
                parent,
                text,
                lore,
            } => self.generated(session, parent, &text, lore),
            Request::GenerationFailed(message) => {
                self.cancel = None;
                self.emit(Event::Generating(false));
                self.error(message);
            }
            Request::GenerationStopped => {
                self.cancel = None;
                self.emit(Event::Generating(false));
            }
            Request::SummaryReady { node, text } => {
                // The node may have been pruned meanwhile; then the summary is moot.
                let _ = self.db.add_summary(node, &text);
            }
            Request::FactsReady { node, facts } => {
                for fact in facts {
                    let _ = self.db.add_fact(node, &fact);
                }
            }
        }
    }

    // ---- start-up, settings, library ----

    fn init(&mut self) {
        if let Some(path) = &self.settings_path {
            match Settings::load_from(path) {
                Ok(settings) => self.settings = settings,
                Err(e) => self.error(e),
            }
        }
        self.reload_library();
        self.emit_settings();
        self.emit_sessions();
        let latest = self
            .db
            .list_sessions()
            .ok()
            .and_then(|s| s.first().map(|s| s.id));
        match latest {
            Some(id) => self.open(id),
            None => self.emit(Event::NoSession),
        }
    }

    fn emit_settings(&self) {
        let key_available = settings::api_key(self.secrets.as_ref(), &self.settings.api.provider)
            .ok()
            .flatten()
            .is_some();
        self.emit(Event::Settings {
            settings: self.settings.clone(),
            key_available,
        });
    }

    fn persist_settings(&self) {
        if let Some(path) = &self.settings_path {
            if let Err(e) = self.settings.save_to(path) {
                self.error(e);
            }
        }
    }

    fn save_settings(&mut self, settings: Settings, api_key: Option<String>) {
        if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
            if let Err(e) = self.secrets.set(&settings.api.provider, key.trim()) {
                self.error(e);
                return;
            }
        }
        self.settings = settings;
        self.persist_settings();
        self.emit_settings();
        self.emit(Event::Info("Settings saved.".into()));
    }

    fn reload_library(&mut self) {
        let (characters, mut errors) = self.library.characters();
        let (worlds, world_errors) = self.library.worlds();
        errors.extend(world_errors);
        self.characters = characters;
        self.worlds = worlds;
        self.emit(Event::Library {
            characters: self.characters.clone(),
            worlds: self.worlds.clone(),
        });
        if !errors.is_empty() {
            self.error(errors.join("\n"));
        }
    }

    // ---- sessions ----

    fn emit_sessions(&self) {
        match self.db.list_sessions() {
            Ok(sessions) => self.emit(Event::Sessions(sessions)),
            Err(e) => self.error(e),
        }
    }

    fn open(&mut self, id: SessionId) {
        match self.db.session(id) {
            Ok(_) => {
                self.session = Some(id);
                self.emit_transcript();
            }
            Err(e) => self.error(e),
        }
    }

    fn emit_transcript(&self) {
        let Some(id) = self.session else {
            self.emit(Event::NoSession);
            if self.watch_tree {
                self.emit(Event::Tree(None));
            }
            return;
        };
        let result = (|| -> Result<Transcript, ir_core::db::DbError> {
            let session = self.db.session(id)?;
            let nodes = match session.current_node_id {
                Some(node) => self.db.path_to_root(node)?,
                None => Vec::new(),
            };
            let characters = self
                .db
                .list_characters(id)?
                .into_iter()
                .map(|c| c.name)
                .collect();
            Ok(Transcript {
                session,
                nodes,
                characters,
            })
        })();
        match result {
            Ok(transcript) => {
                self.emit(Event::Transcript(transcript));
                if self.watch_tree {
                    self.emit_tree();
                }
            }
            Err(e) => self.error(e),
        }
    }

    fn emit_tree(&self) {
        let Some(id) = self.session else {
            self.emit(Event::Tree(None));
            return;
        };
        let result = (|| -> Result<TreeData, ir_core::db::DbError> {
            let current = self.db.session(id)?.current_node_id;
            Ok(TreeData {
                session_id: id,
                nodes: self.db.list_nodes(id)?,
                current,
            })
        })();
        match result {
            Ok(data) => self.emit(Event::Tree(Some(data))),
            Err(e) => self.error(e),
        }
    }

    /// Move the story to `node`, as `/goto` does. Refused while a response is coming in,
    /// since the response would land in the wrong place.
    fn jump_to(&mut self, node: NodeId) {
        let Some(session) = self.session else { return };
        if self.cancel.is_some() {
            self.error("Wait for the response to finish, or stop it first.");
            return;
        }
        match StoryTree::new(&self.db, session).jump(node) {
            Ok(_) => {
                self.emit_transcript();
                self.emit_sessions();
            }
            Err(e) => self.error(e),
        }
    }

    fn create_session(&mut self, name: &str, world: Option<String>, character: Option<String>) {
        let name = if name.trim().is_empty() {
            "New story"
        } else {
            name.trim()
        };
        let result = (|| -> Result<SessionId, String> {
            let session = self
                .db
                .create_session(name, world.as_deref())
                .map_err(|e| e.to_string())?;
            let card = character.as_deref().and_then(|wanted| {
                self.characters
                    .iter()
                    .map(|e| &e.item)
                    .find(|c| c.name.eq_ignore_ascii_case(wanted))
            });
            let opening = match card {
                Some(card) => {
                    let yaml = serde_yaml_ng::to_string(card).map_err(|e| e.to_string())?;
                    self.db
                        .add_character(session.id, &card.name, &yaml)
                        .map_err(|e| e.to_string())?;
                    if card.greeting.trim().is_empty() {
                        Turn::system("The scene begins.")
                    } else {
                        Turn::character(&card.name, card.greeting.trim())
                    }
                }
                None => Turn::system("The scene begins."),
            };
            StoryTree::new(&self.db, session.id)
                .append(opening)
                .map_err(|e| e.to_string())?;
            Ok(session.id)
        })();
        match result {
            Ok(id) => {
                self.stop();
                self.open(id);
                self.emit_sessions();
            }
            Err(e) => self.error(e),
        }
    }

    fn delete_session(&mut self, id: SessionId) {
        if self.session == Some(id) {
            self.stop();
        }
        if let Err(e) = self.db.delete_session(id) {
            self.error(e);
            return;
        }
        if self.session == Some(id) {
            self.session = None;
            let next = self
                .db
                .list_sessions()
                .ok()
                .and_then(|s| s.first().map(|s| s.id));
            match next {
                Some(next) => self.open(next),
                None => self.emit(Event::NoSession),
            }
        }
        self.emit_sessions();
    }

    // ---- chat input and commands ----

    fn send(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let Some(session) = self.session else {
            self.error("Create or open a session first.");
            return;
        };
        if self.cancel.is_some() {
            self.error("Wait for the response to finish, or stop it first.");
            return;
        }
        match commands::parse(text) {
            Err(e) => self.error(e),
            Ok(Some(command)) => self.run_command(session, &command),
            Ok(None) => match StoryTree::new(&self.db, session).append(Turn::user(text)) {
                Ok(_) => {
                    self.emit_transcript();
                    self.start_generation();
                }
                Err(e) => self.error(e),
            },
        }
    }

    fn run_command(&mut self, session: SessionId, command: &commands::Command) {
        let library: Vec<Character> = self.characters.iter().map(|e| e.item.clone()).collect();
        let mut ctx = CommandContext {
            db: &self.db,
            session,
            settings: &mut self.settings,
            characters: &library,
            styles: &[],
        };
        let result = commands::execute(&mut ctx, command);
        let active = ctx.session;
        match result {
            Err(e) => self.error(e),
            Ok(output) => {
                self.session = Some(active);
                if matches!(output, CommandOutput::Style(_) | CommandOutput::Narrator(_)) {
                    self.persist_settings();
                    self.emit_settings();
                }
                self.emit_transcript();
                self.emit_sessions();
                match output {
                    CommandOutput::Regenerate { .. } => self.start_generation(),
                    other => self.emit(Event::Notice(format::notice(&other))),
                }
            }
        }
    }

    // ---- generating responses ----

    fn session_characters(&self, session: SessionId) -> Result<Vec<Character>, String> {
        Ok(self
            .db
            .list_characters(session)
            .map_err(|e| e.to_string())?
            .iter()
            .filter_map(|row| Character::from_yaml(&row.card).ok())
            .collect())
    }

    fn build_prompt(&self, session_id: SessionId) -> Result<Prompt, String> {
        let db_err = |e: ir_core::db::DbError| e.to_string();
        let session = self.db.session(session_id).map_err(db_err)?;
        let current = session
            .current_node_id
            .ok_or("The story has no messages yet.")?;
        let path = self.db.path_to_root(current).map_err(db_err)?;
        let messages: Vec<ChatMessage> = path.iter().map(context::node_message).collect();
        let characters = self.session_characters(session_id)?;

        let world = session
            .world
            .as_deref()
            .and_then(|name| self.worlds.iter().find(|w| w.item.name == name))
            .map(|w| &w.item);
        let window = (self.settings.context.recent_messages as usize).max(1);
        let recent: Vec<&str> = path
            .iter()
            .rev()
            .take(window)
            .map(|n| n.content.as_str())
            .collect();
        let lore: Vec<&LoreEntry> = world.map(|w| w.triggered(&recent)).unwrap_or_default();
        let facts: Vec<String> = self
            .db
            .facts_at(current)
            .map_err(db_err)?
            .into_iter()
            .map(|f| f.text)
            .collect();
        let summary = self.db.summary_at(current).map_err(db_err)?.map(|s| s.text);

        let system_prompt = prompts::system_prompt(&self.settings, &characters);
        let built = context::build(
            &ContextInput {
                system_prompt: &system_prompt,
                characters: &characters,
                world_setting: world.map_or("", |w| w.setting.as_str()),
                lore: &lore,
                facts: &facts,
                summary: summary.as_deref(),
                messages: &messages,
            },
            &self.settings.context,
        );
        Ok(Prompt {
            messages: built.messages,
            parent: current,
            lore: lore.iter().map(|e| e.id().to_string()).collect(),
        })
    }

    fn provider(&self) -> Result<Arc<dyn LlmProvider>, String> {
        let api = &self.settings.api;
        if api.base_url.trim().is_empty() {
            return Err("Set a base URL in Settings.".into());
        }
        let key =
            settings::api_key(self.secrets.as_ref(), &api.provider).map_err(|e| e.to_string())?;
        if key.is_none() && client::find_preset(&api.provider).is_some_and(|p| p.key_required) {
            return Err(format!(
                "No API key for {}. Add one in Settings.",
                api.provider
            ));
        }
        (self.provider)(&self.settings, key).map_err(|e| e.to_string())
    }

    fn start_generation(&mut self) {
        let Some(session) = self.session else { return };
        let prompt = match self.build_prompt(session) {
            Ok(prompt) => prompt,
            Err(e) => return self.error(e),
        };
        let provider = match self.provider() {
            Ok(provider) => provider,
            Err(e) => return self.error(e),
        };
        let cancel = Arc::new(Notify::new());
        self.cancel = Some(cancel.clone());
        tracing::debug!(session = ?session, "generation started");
        self.emit(Event::Generating(true));

        let (requests, events, wake) = (
            self.requests.clone(),
            self.events.clone(),
            self.wake.clone(),
        );
        let params = GenerationParams::from(&self.settings.api);
        let stream = self.settings.api.stream;
        tokio::spawn(async move {
            let on_chunk = |chunk: String| {
                let _ = events.send(Event::Chunk(chunk));
                wake();
            };
            let outcome = generate(
                provider.as_ref(),
                prompt.messages,
                params,
                stream,
                &on_chunk,
                &cancel,
            )
            .await;
            let request = match (outcome.text.is_empty(), outcome.end) {
                (false, end) => {
                    if let End::Failed(e) = end {
                        let _ =
                            events.send(Event::Error(format!("The response was cut short: {e}")));
                    }
                    Request::Generated {
                        session,
                        parent: prompt.parent,
                        text: outcome.text,
                        lore: prompt.lore,
                    }
                }
                (true, End::Cancelled) => Request::GenerationStopped,
                (true, End::Failed(e)) => Request::GenerationFailed(e.to_string()),
                (true, End::Complete) => {
                    Request::GenerationFailed("The model returned an empty response.".into())
                }
            };
            let _ = requests.send(request);
        });
    }

    fn stop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.notify_one();
        }
    }

    fn generated(&mut self, session: SessionId, parent: NodeId, text: &str, lore: Vec<String>) {
        self.cancel = None;
        let result = (|| -> Result<NodeId, String> {
            let current = self
                .db
                .session(session)
                .map_err(|e| e.to_string())?
                .current_node_id;
            let names: Vec<String> = self
                .session_characters(session)?
                .into_iter()
                .map(|c| c.name)
                .collect();
            let turn = prompts::response_turn(&self.settings, &names, text.trim()).with_lore(lore);
            let node = self
                .db
                .add_node(&NewNode {
                    session_id: session,
                    parent_id: Some(parent),
                    kind: turn.kind,
                    speaker: turn.speaker,
                    content: turn.content,
                    label: None,
                    active_lore: turn.active_lore,
                })
                .map_err(|e| e.to_string())?;
            // Move on to the response unless the user went elsewhere in the meantime.
            if current == Some(parent) {
                self.db
                    .set_current_node(session, Some(node.id))
                    .map_err(|e| e.to_string())?;
            }
            Ok(node.id)
        })();
        match result {
            Ok(node) => {
                if self.session == Some(session) {
                    self.emit_transcript();
                }
                self.emit_sessions();
                self.emit(Event::Generating(false));
                self.after_turn(node);
            }
            Err(e) => {
                self.emit(Event::Generating(false));
                self.error(e);
            }
        }
    }

    /// Start the background memory work for a finished turn: summarize old messages and
    /// extract facts. Failures here are reported but never interrupt the story.
    fn after_turn(&self, node: NodeId) {
        if !self.settings.memory.enabled {
            return;
        }
        let Ok(provider) = self.provider() else {
            return;
        };
        let Ok(path) = self.db.path_to_root(node) else {
            return;
        };

        if let Ok(summary) = self.db.summary_at(node) {
            if let Some(job) = memory::plan_summary(&path, summary.as_ref(), &self.settings) {
                let (provider, requests, events) =
                    (provider.clone(), self.requests.clone(), self.events.clone());
                tokio::spawn(async move {
                    match memory::summarize(provider.as_ref(), &job).await {
                        Ok(text) => {
                            let _ = requests.send(Request::SummaryReady {
                                node: job.through_node,
                                text,
                            });
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "summarizing failed");
                            let _ = events.send(Event::Error(format!("Could not summarize: {e}")));
                        }
                    }
                });
            }
        }

        let known: Vec<String> = self
            .db
            .facts_at(node)
            .map(|facts| facts.into_iter().map(|f| f.text).collect())
            .unwrap_or_default();
        let newest = &path[path.len().saturating_sub(2)..];
        if let Some(job) = memory::plan_fact_extraction(newest, &known, &self.settings) {
            let (requests, events) = (self.requests.clone(), self.events.clone());
            tokio::spawn(async move {
                match memory::extract_facts(provider.as_ref(), &job, &known).await {
                    Ok(facts) if !facts.is_empty() => {
                        let _ = requests.send(Request::FactsReady {
                            node: job.node_id,
                            facts,
                        });
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "fact extraction failed");
                        let _ = events.send(Event::Error(format!("Could not extract facts: {e}")));
                    }
                }
            });
        }
    }
}

/// How a model call ended.
enum End {
    Complete,
    Cancelled,
    Failed(LlmError),
}

struct Outcome {
    text: String,
    end: End,
}

/// Run one model call, reporting text as it arrives. Stops early when `cancel` is notified,
/// keeping what has arrived.
async fn generate(
    provider: &dyn LlmProvider,
    messages: Vec<ChatMessage>,
    params: GenerationParams,
    stream: bool,
    on_chunk: &(dyn Fn(String) + Sync),
    cancel: &Notify,
) -> Outcome {
    let mut text = String::new();
    if !stream {
        let end = tokio::select! {
            () = cancel.notified() => End::Cancelled,
            result = provider.complete(messages, params) => match result {
                Ok(whole) => {
                    on_chunk(whole.clone());
                    text = whole;
                    End::Complete
                }
                Err(e) => End::Failed(e),
            },
        };
        return Outcome { text, end };
    }

    let opened = tokio::select! {
        () = cancel.notified() => return Outcome { text, end: End::Cancelled },
        result = provider.stream_chat(messages, params) => result,
    };
    let mut chunks = match opened {
        Ok(chunks) => chunks,
        Err(e) => {
            return Outcome {
                text,
                end: End::Failed(e),
            }
        }
    };
    loop {
        tokio::select! {
            () = cancel.notified() => return Outcome { text, end: End::Cancelled },
            item = chunks.next() => match item {
                Some(Ok(chunk)) => {
                    text.push_str(&chunk);
                    on_chunk(chunk);
                }
                Some(Err(e)) => return Outcome { text, end: End::Failed(e) },
                None => return Outcome { text, end: End::Complete },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use futures_util::future::BoxFuture;
    use ir_core::client::TextStream;
    use ir_core::settings::MemoryStore;

    use super::*;

    /// A scripted provider: streams `chunks` with a pause between them, and answers
    /// `complete` (used for summaries and facts) with `reply`.
    struct Mock {
        chunks: Vec<&'static str>,
        delay: Duration,
        reply: String,
        seen: Mutex<Vec<Vec<ChatMessage>>>,
    }

    impl Mock {
        fn new(chunks: &[&'static str]) -> Arc<Self> {
            Arc::new(Self {
                chunks: chunks.to_vec(),
                delay: Duration::from_millis(1),
                reply: "- Mira likes maps.".into(),
                seen: Mutex::new(Vec::new()),
            })
        }

        fn slow(chunks: &[&'static str], delay: Duration) -> Arc<Self> {
            Arc::new(Self {
                delay,
                ..Arc::try_unwrap(Self::new(chunks)).ok().expect("fresh mock")
            })
        }
    }

    impl LlmProvider for Mock {
        fn stream_chat(
            &self,
            messages: Vec<ChatMessage>,
            _: GenerationParams,
        ) -> BoxFuture<'_, Result<TextStream, LlmError>> {
            self.seen.lock().unwrap().push(messages);
            let (chunks, delay) = (self.chunks.clone(), self.delay);
            Box::pin(async move {
                let stream = futures_util::stream::iter(chunks).then(move |chunk| async move {
                    tokio::time::sleep(delay).await;
                    Ok::<String, LlmError>(chunk.to_string())
                });
                Ok(stream.boxed())
            })
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

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("ir-backend-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct Rig {
        backend: Backend,
        events: mpsc::Receiver<Event>,
        mock: Arc<Mock>,
        secrets: Arc<MemoryStore>,
        dir: TempDir,
    }

    const MIRA: &str = r#"
name: "Mira Valen"
description: "A cartographer and reluctant adventurer."
personality: [curious, sarcastic]
speaking_style: "Dry, short sentences."
greeting: "Ah, another one who got lost?"
"#;

    const COAST: &str = r#"
name: "The Misty Coast"
setting: "A coastal region where constant fog makes navigation nearly impossible."
entries:
  - keys: ["lighthouse", "keeper"]
    content: "The Karrow lighthouse has been guarded by an order for centuries."
  - keys: ["fogglass"]
    content: "Fogglass is a rare mineral that briefly clears fog."
"#;

    impl Rig {
        fn start(name: &str, mock: Arc<Mock>, with_key: bool, settings_file: bool) -> Self {
            let dir = TempDir::new(name);
            let library = Library::new(dir.0.join("library"));
            library
                .save_character(None, &Character::from_yaml(MIRA).unwrap())
                .unwrap();
            library
                .save_world(None, &Lorebook::from_yaml(COAST).unwrap())
                .unwrap();
            let secrets = Arc::new(MemoryStore::default());
            if with_key {
                secrets.set("openai-compatible", "test-key").unwrap();
            }
            let provider_mock = mock.clone();
            let services = Services {
                db: Db::open_in_memory().unwrap(),
                library,
                settings_path: settings_file.then(|| dir.0.join("settings.toml")),
                secrets: secrets.clone(),
                provider: Arc::new(move |_, _| Ok(provider_mock.clone() as Arc<dyn LlmProvider>)),
                wake: Arc::new(|| {}),
            };
            let (backend, events) = spawn(&tokio::runtime::Handle::current(), services);
            backend.send(Request::Init);
            Self {
                backend,
                events,
                mock,
                secrets,
                dir,
            }
        }

        /// Wait up to five seconds for an event that `pick` accepts.
        fn wait_for<T>(&self, mut pick: impl FnMut(&Event) -> Option<T>) -> T {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                match self.events.recv_timeout(left) {
                    Ok(event) => {
                        if let Some(found) = pick(&event) {
                            return found;
                        }
                    }
                    Err(_) => panic!("timed out waiting for an event"),
                }
            }
        }

        fn wait_for_transcript(&self, ready: impl Fn(&Transcript) -> bool) -> Transcript {
            self.wait_for(|e| match e {
                Event::Transcript(t) if ready(t) => Some(t.clone()),
                _ => None,
            })
        }

        /// Create a session with Mira, and wait until it is open.
        fn start_story(&self) -> Transcript {
            self.backend.send(Request::CreateSession {
                name: "Harbor".into(),
                world: Some("The Misty Coast".into()),
                character: Some("Mira Valen".into()),
            });
            self.wait_for_transcript(|t| t.nodes.len() == 1)
        }

        /// Send a message and wait until the response is stored and generation ends.
        fn chat(&self, text: &str) -> Transcript {
            self.backend.send(Request::Send(text.into()));
            let mut last = None;
            self.wait_for(|e| match e {
                Event::Transcript(t) => {
                    last = Some(t.clone());
                    None
                }
                Event::Generating(false) => Some(()),
                Event::Error(e) => panic!("unexpected error: {e}"),
                _ => None,
            });
            last.expect("a transcript arrived")
        }
    }

    fn contents(t: &Transcript) -> Vec<&str> {
        t.nodes.iter().map(|n| n.content.as_str()).collect()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn init_reports_the_library_settings_and_no_sessions() {
        let rig = Rig::start("init", Mock::new(&["x"]), true, false);
        let (characters, worlds) = rig.wait_for(|e| match e {
            Event::Library { characters, worlds } => Some((characters.len(), worlds.len())),
            _ => None,
        });
        assert_eq!(
            (characters, worlds),
            (1, 1),
            "the library holds what the tests put in it"
        );
        let key = rig.wait_for(|e| match e {
            Event::Settings { key_available, .. } => Some(*key_available),
            _ => None,
        });
        assert!(key);
        rig.wait_for(|e| matches!(e, Event::NoSession).then_some(()));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn creating_a_session_opens_it_with_the_greeting() {
        let rig = Rig::start("create", Mock::new(&["x"]), true, false);
        let t = rig.start_story();
        assert_eq!(t.session.name, "Harbor");
        assert_eq!(t.characters, ["Mira Valen"]);
        assert_eq!(contents(&t), ["Ah, another one who got lost?"]);
        assert_eq!(t.nodes[0].speaker.as_deref(), Some("Mira Valen"));
        let sessions = rig.wait_for(|e| match e {
            Event::Sessions(s) if !s.is_empty() => Some(s.len()),
            _ => None,
        });
        assert_eq!(sessions, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_message_streams_a_response_and_stores_it() {
        let rig = Rig::start(
            "chat",
            Mock::new(&["Lost? ", "Never. ", "Follow me."]),
            true,
            false,
        );
        rig.start_story();

        rig.backend.send(Request::Send("I follow her.".into()));
        let mut streamed = String::new();
        let mut transcript = None;
        rig.wait_for(|e| match e {
            Event::Chunk(c) => {
                streamed.push_str(c);
                None
            }
            Event::Transcript(t) => {
                transcript = Some(t.clone());
                None
            }
            Event::Generating(false) => Some(()),
            Event::Error(e) => panic!("{e}"),
            _ => None,
        });
        assert_eq!(streamed, "Lost? Never. Follow me.");
        let t = transcript.unwrap();
        assert_eq!(
            contents(&t),
            [
                "Ah, another one who got lost?",
                "I follow her.",
                "Lost? Never. Follow me."
            ]
        );
        assert_eq!(t.nodes[2].speaker.as_deref(), Some("Mira Valen"));

        // The model saw the system prompt, the world and the conversation, in order.
        let seen = rig.mock.seen.lock().unwrap();
        let prompt = &seen[0];
        assert_eq!(prompt[0].role, ir_core::client::Role::System);
        assert!(prompt[0].content.contains("game master"));
        assert!(prompt[0].content.contains("Mira Valen"));
        assert!(prompt[0].content.contains("foggy") || prompt[0].content.contains("fog"));
        assert_eq!(prompt.last().unwrap().content, "I follow her.");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn triggered_lore_is_added_to_the_prompt() {
        let rig = Rig::start("lore", Mock::new(&["ok"]), true, false);
        rig.start_story();
        rig.chat("Tell me about the lighthouse.");
        let seen = rig.mock.seen.lock().unwrap();
        assert!(seen[0][0]
            .content
            .contains("Karrow lighthouse has been guarded"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn slash_commands_run_without_calling_the_model() {
        let rig = Rig::start("command", Mock::new(&["x"]), true, false);
        rig.start_story();
        rig.backend.send(Request::Send("/tree".into()));
        let notice = rig.wait_for(|e| match e {
            Event::Notice(n) => Some(n.clone()),
            _ => None,
        });
        assert!(notice.monospace && notice.text.contains("[#"), "{notice:?}");
        assert!(rig.mock.seen.lock().unwrap().is_empty());

        rig.backend.send(Request::Send("/dance".into()));
        let error = rig.wait_for(|e| match e {
            Event::Error(m) => Some(m.clone()),
            _ => None,
        });
        assert!(error.contains("unknown command"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn retry_regenerates_and_keeps_the_old_response_as_a_branch() {
        let rig = Rig::start("retry", Mock::new(&["Reply."]), true, false);
        rig.start_story();
        rig.chat("Hello.");

        rig.backend.send(Request::Send("/retry".into()));
        rig.wait_for(|e| matches!(e, Event::Generating(true)).then_some(()));
        rig.wait_for(|e| matches!(e, Event::Generating(false)).then_some(()));

        rig.backend.send(Request::Send("/branches".into()));
        let notice = rig.wait_for(|e| match e {
            Event::Notice(n) => Some(n.text.clone()),
            _ => None,
        });
        assert_eq!(notice.matches('#').count(), 2, "two storylines: {notice}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_missing_key_is_reported_before_any_request_when_the_provider_needs_one() {
        let rig = Rig::start("nokey", Mock::new(&["x"]), false, false);
        rig.start_story();
        let mut settings = Settings::default();
        settings.api.provider = "xai".into();
        rig.backend.send(Request::SaveSettings {
            settings,
            api_key: None,
        });
        rig.backend.send(Request::Send("Hello.".into()));
        let error = rig.wait_for(|e| match e {
            Event::Error(m) => Some(m.clone()),
            _ => None,
        });
        assert!(error.contains("No API key"), "{error}");
        assert!(rig.mock.seen.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn local_and_custom_providers_work_without_a_key() {
        for provider in ["openai-compatible", "ollama"] {
            let rig = Rig::start("nokey-local", Mock::new(&["Welcome."]), false, false);
            rig.start_story();
            let mut settings = Settings::default();
            settings.api.provider = provider.into();
            rig.backend.send(Request::SaveSettings {
                settings,
                api_key: None,
            });
            rig.backend.send(Request::Send("Hello.".into()));
            let t = rig.wait_for_transcript(|t| t.nodes.len() == 3);
            assert_eq!(t.nodes[2].content, "Welcome.", "{provider}");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stopping_keeps_the_text_that_arrived() {
        let mock = Mock::slow(
            &["One ", "two ", "three ", "four ", "five"],
            Duration::from_millis(80),
        );
        let rig = Rig::start("stop", mock, true, false);
        rig.start_story();
        rig.backend.send(Request::Send("Count for me.".into()));
        rig.wait_for(|e| matches!(e, Event::Chunk(_)).then_some(()));
        rig.backend.send(Request::Stop);

        let t = rig.wait_for_transcript(|t| t.nodes.len() == 3);
        let reply = &t.nodes[2].content;
        // Stored responses are trimmed, so the trailing space of the first chunk is gone.
        assert!(reply.starts_with("One"), "{reply}");
        assert!(!reply.contains("five"), "stopped early: {reply}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stopping_before_any_text_stores_nothing() {
        let mock = Mock::slow(&["late"], Duration::from_millis(500));
        let rig = Rig::start("stop-early", mock, true, false);
        rig.start_story();
        rig.backend.send(Request::Send("Hello.".into()));
        rig.wait_for(|e| matches!(e, Event::Generating(true)).then_some(()));
        rig.backend.send(Request::Stop);
        rig.wait_for(|e| matches!(e, Event::Generating(false)).then_some(()));

        rig.backend.send(Request::Send("/memory".into()));
        rig.wait_for(|e| matches!(e, Event::Notice(_)).then_some(()));
        // Only the greeting and the player's message are stored.
        rig.backend.send(Request::OpenSession(SessionId(1)));
        let t = rig.wait_for_transcript(|_| true);
        assert_eq!(t.nodes.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn input_is_refused_while_a_response_is_being_generated() {
        let mock = Mock::slow(&["a ", "b ", "c"], Duration::from_millis(100));
        let rig = Rig::start("busy", mock, true, false);
        rig.start_story();
        rig.backend.send(Request::Send("First.".into()));
        rig.wait_for(|e| matches!(e, Event::Generating(true)).then_some(()));
        rig.backend.send(Request::Send("Second.".into()));
        let error = rig.wait_for(|e| match e {
            Event::Error(m) => Some(m.clone()),
            _ => None,
        });
        assert!(error.contains("Wait"), "{error}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn facts_are_extracted_in_the_background_and_shown_by_memory() {
        let rig = Rig::start("facts", Mock::new(&["Fine."]), true, false);
        rig.start_story();
        rig.chat("Hello.");

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            rig.backend.send(Request::Send("/memory".into()));
            let text = rig.wait_for(|e| match e {
                Event::Notice(n) => Some(n.text.clone()),
                _ => None,
            });
            if text.contains("Mira likes maps.") {
                break;
            }
            assert!(Instant::now() < deadline, "facts never arrived: {text}");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn settings_are_saved_to_a_file_and_the_key_goes_to_the_keychain() {
        let rig = Rig::start("settings", Mock::new(&["x"]), false, true);
        rig.wait_for(|e| matches!(e, Event::NoSession).then_some(()));

        let mut settings = Settings::default();
        settings.api.model = "my-model".into();
        settings.style.narrator_mode = true;
        rig.backend.send(Request::SaveSettings {
            settings,
            api_key: Some("  secret-key  ".into()),
        });
        let key_available = rig.wait_for(|e| match e {
            Event::Settings {
                settings,
                key_available,
            } if settings.api.model == "my-model" => Some(*key_available),
            _ => None,
        });
        assert!(key_available);

        let file = std::fs::read_to_string(rig.dir.0.join("settings.toml")).unwrap();
        assert!(file.contains("my-model") && file.contains("narrator_mode = true"));
        assert!(
            !file.contains("secret-key"),
            "the key must not reach the file"
        );
        assert_eq!(
            rig.secrets.get("openai-compatible").unwrap().as_deref(),
            Some("secret-key")
        );

        rig.backend.send(Request::DeleteApiKey);
        rig.wait_for(|e| matches!(e, Event::Info(m) if m.contains("removed")).then_some(()));
        assert_eq!(rig.secrets.get("openai-compatible").unwrap(), None);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_library_can_be_edited() {
        let rig = Rig::start("library", Mock::new(&["x"]), true, false);
        let initial = rig.wait_for(|e| match e {
            Event::Library { characters, .. } => Some(characters.len()),
            _ => None,
        });
        assert_eq!(initial, 1);

        let bo = Character::from_yaml("name: Bo\ngreeting: Hi.").unwrap();
        rig.backend.send(Request::SaveCharacter {
            path: None,
            character: bo,
        });
        let characters = rig.wait_for(|e| match e {
            Event::Library { characters, .. } if characters.len() == 2 => Some(characters.clone()),
            _ => None,
        });
        let path = characters
            .iter()
            .find(|c| c.item.name == "Bo")
            .unwrap()
            .path
            .clone();

        rig.backend.send(Request::DeleteCharacter(path));
        rig.wait_for(|e| {
            matches!(e, Event::Library { characters, .. } if characters.len() == 1).then_some(())
        });

        rig.backend.send(Request::SaveCharacter {
            path: None,
            character: Character::default(),
        });
        let error = rig.wait_for(|e| match e {
            Event::Error(m) => Some(m.clone()),
            _ => None,
        });
        assert!(error.contains("name"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn sessions_can_be_switched_and_deleted() {
        let rig = Rig::start("sessions", Mock::new(&["x"]), true, false);
        let first = rig.start_story().session.id;
        rig.backend.send(Request::CreateSession {
            name: "Reef".into(),
            world: None,
            character: None,
        });
        let second = rig.wait_for_transcript(|t| t.session.name == "Reef");
        assert_eq!(second.nodes.len(), 1);
        assert_eq!(second.nodes[0].content, "The scene begins.");

        rig.backend.send(Request::OpenSession(first));
        rig.wait_for_transcript(|t| t.session.id == first);

        rig.backend.send(Request::DeleteSession(first));
        let after = rig.wait_for_transcript(|t| t.session.name == "Reef");
        assert_eq!(after.session.id, second.session.id);
        rig.backend.send(Request::DeleteSession(second.session.id));
        rig.wait_for(|e| matches!(e, Event::NoSession).then_some(()));
    }

    /// The next tree event, skipping everything else.
    fn next_tree(rig: &Rig) -> Option<TreeData> {
        rig.wait_for(|e| match e {
            Event::Tree(data) => Some(data.clone()),
            _ => None,
        })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_tree_is_sent_only_while_watched_and_includes_every_branch() {
        let rig = Rig::start("watch", Mock::new(&["Reply."]), true, false);
        rig.start_story();

        // Not watching: a whole conversation passes without a tree event.
        rig.chat("Hello.");
        while let Ok(event) = rig.events.try_recv() {
            assert!(
                !matches!(event, Event::Tree(_)),
                "the tree was not requested"
            );
        }

        rig.backend.send(Request::WatchTree(true));
        let data = next_tree(&rig).expect("a session is open");
        assert_eq!(data.nodes.len(), 3);
        assert_eq!(data.current, Some(data.nodes[2].id));

        // A retry adds a second branch, and the tree arrives with the story's changes.
        // (The tree is sent right after the transcript, before generation is reported done.)
        rig.backend.send(Request::Send("/retry".into()));
        let data = rig.wait_for(|e| match e {
            Event::Tree(Some(d)) if d.nodes.len() == 4 => Some(d.clone()),
            _ => None,
        });
        rig.wait_for(|e| matches!(e, Event::Generating(false)).then_some(()));
        assert_eq!(
            data.nodes
                .iter()
                .filter(|n| n.parent_id == Some(data.nodes[1].id))
                .count(),
            2
        );

        rig.backend.send(Request::WatchTree(false));
        rig.backend.send(Request::Send("/tree".into()));
        rig.wait_for(|e| matches!(e, Event::Notice(_)).then_some(()));
        while let Ok(event) = rig.events.try_recv() {
            assert!(!matches!(event, Event::Tree(_)), "watching stopped");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn watching_without_a_session_sends_an_empty_tree() {
        let rig = Rig::start("watch-empty", Mock::new(&["x"]), true, false);
        rig.wait_for(|e| matches!(e, Event::NoSession).then_some(()));
        rig.backend.send(Request::WatchTree(true));
        assert_eq!(next_tree(&rig), None);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn jumping_moves_the_story_and_refreshes_the_transcript_and_tree() {
        let rig = Rig::start("jump", Mock::new(&["Reply."]), true, false);
        rig.start_story();
        rig.chat("Hello.");
        rig.backend.send(Request::WatchTree(true));
        let data = next_tree(&rig).unwrap();
        let first = data.nodes[0].id;

        rig.backend.send(Request::JumpTo(first));
        let t = rig.wait_for_transcript(|t| t.nodes.len() == 1);
        assert_eq!(t.nodes[0].id, first);
        let data = rig.wait_for(|e| match e {
            Event::Tree(Some(d)) if d.current == Some(first) => Some(d.clone()),
            _ => None,
        });
        assert_eq!(data.nodes.len(), 3, "nothing was deleted by jumping back");

        // Writing from there starts a new storyline next to the old one.
        rig.backend.send(Request::Send("Another path.".into()));
        let data = rig.wait_for(|e| match e {
            Event::Tree(Some(d)) if d.nodes.len() == 5 => Some(d.clone()),
            _ => None,
        });
        assert_eq!(
            data.nodes
                .iter()
                .filter(|n| n.parent_id == Some(first))
                .count(),
            2
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn jumping_is_refused_while_generating_and_for_foreign_nodes() {
        let mock = Mock::slow(&["a ", "b ", "c"], Duration::from_millis(100));
        let rig = Rig::start("jump-busy", mock, true, false);
        let first = rig.start_story().nodes[0].id;
        rig.backend.send(Request::Send("Go.".into()));
        rig.wait_for(|e| matches!(e, Event::Generating(true)).then_some(()));
        rig.backend.send(Request::JumpTo(first));
        let error = rig.wait_for(|e| match e {
            Event::Error(m) => Some(m.clone()),
            _ => None,
        });
        assert!(error.contains("Wait"), "{error}");
        rig.wait_for(|e| matches!(e, Event::Generating(false)).then_some(()));

        rig.backend.send(Request::JumpTo(NodeId(9999)));
        let error = rig.wait_for(|e| match e {
            Event::Error(m) => Some(m.clone()),
            _ => None,
        });
        assert!(error.contains("#9999"), "{error}");
    }
}
