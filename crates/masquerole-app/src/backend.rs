//! The backend: everything that touches the database, files, keychain, or network.
//!
//! The UI never does that work. It sends [`Request`]s to one actor task running on the tokio
//! runtime and receives [`Event`]s back over a channel, so the UI thread never blocks. The
//! actor owns the database and handles requests one at a time; model calls run as separate
//! tasks and report back with requests of their own.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use futures_util::future::BoxFuture;
use futures_util::StreamExt;
use masquerole_core::character::Character;
use masquerole_core::client::{
    self, ChatMessage, GenerationParams, LlmError, LlmProvider, OpenAiCompatible, ProviderConfig,
};
use masquerole_core::commands::{self, CommandContext, CommandOutput};
use masquerole_core::context::{self, ContextInput};
use masquerole_core::db::{Db, NewNode, Node, NodeId, NodeKind, NodeScene, Session, SessionId};
use masquerole_core::embed::{self, Embedder, MiniLm};
use masquerole_core::exchange::{self, ExportFormat};
use masquerole_core::improve;
use masquerole_core::lorebook::{LoreEntry, Lorebook};
use masquerole_core::memory::{self, NewFact, SceneCast};
use masquerole_core::recall;
use masquerole_core::scene::{self, Scenes};
use masquerole_core::settings::{self, SecretStore, Settings};
use masquerole_core::tree::{Branch, StoryTree, Turn};
use masquerole_core::update::{self, Release};
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

/// Lets the user choose a picture and returns its bytes, or `None` if they cancelled. Real
/// use opens a file dialog; tests supply their own.
pub type ImagePicker = Arc<dyn Fn() -> BoxFuture<'static, Option<Vec<u8>>> + Send + Sync>;

/// The picker for real use: the system file dialog. It runs off the UI thread.
pub fn default_image_picker() -> ImagePicker {
    Arc::new(|| {
        Box::pin(async {
            let file = rfd::AsyncFileDialog::new()
                .set_title("Choose a picture")
                .add_filter("Pictures", &["png", "jpg", "jpeg", "webp"])
                .pick_file()
                .await?;
            Some(file.read().await)
        })
    })
}

/// The file dialogs for importing and exporting character cards. Real use opens the system
/// dialogs; tests supply their own.
#[derive(Clone)]
pub struct CardFiles {
    /// Lets the user choose a card to import and returns the file's bytes, or `None` if they
    /// cancelled.
    pub open: Arc<dyn Fn() -> BoxFuture<'static, Option<Vec<u8>>> + Send + Sync>,
    /// Lets the user choose where to save an export, given a suggested file name. `None`
    /// means they cancelled.
    pub save: Arc<dyn Fn(String) -> BoxFuture<'static, Option<PathBuf>> + Send + Sync>,
}

/// The card dialogs for real use: the system file dialogs. They run off the UI thread.
pub fn default_card_files() -> CardFiles {
    CardFiles {
        open: Arc::new(|| {
            Box::pin(async {
                let file = rfd::AsyncFileDialog::new()
                    .set_title("Import a character")
                    .add_filter("Character cards", &["png", "json", "yaml", "yml"])
                    .pick_file()
                    .await?;
                Some(file.read().await)
            })
        }),
        save: Arc::new(|suggested| {
            Box::pin(async move {
                let file = rfd::AsyncFileDialog::new()
                    .set_title("Export a character")
                    .set_file_name(suggested)
                    .add_filter("Card in a picture (PNG)", &["png"])
                    .add_filter("Card (JSON)", &["json"])
                    .add_filter("MasqueRole (YAML)", &["yaml"])
                    .save_file()
                    .await?;
                Some(file.path().to_path_buf())
            })
        }),
    }
}

/// What a picked picture is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageTarget {
    /// The character being edited.
    Character,
    /// The world being edited.
    World,
}

/// The provider factory for real use: an OpenAI-compatible client.
pub fn default_provider_factory() -> ProviderFactory {
    Arc::new(|settings, key| {
        let client = OpenAiCompatible::new(ProviderConfig::from_settings(&settings.api, key))?;
        Ok(Arc::new(client) as Arc<dyn LlmProvider>)
    })
}

/// Download and start the installer. Returns whether the app should quit now; `false` means
/// the release page was opened instead because there is no installer for this system.
async fn install(release: &Release) -> Result<bool, update::UpdateError> {
    let Some(asset) = &release.asset else {
        update::open_page(&release.page_url)?;
        return Ok(false);
    };
    let dir = std::env::temp_dir().join("MasqueRole-update");
    let path = update::download(asset, &dir).await?;
    update::launch_installer(&path)?;
    Ok(true)
}

/// A story as the UI shows it: the path from the first message to the current one.
#[derive(Clone, Debug, PartialEq)]
pub struct Transcript {
    /// The session.
    pub session: Session,
    /// The messages on the current path, oldest first.
    pub nodes: Vec<Node>,
    /// The names of the characters in the story.
    pub characters: Vec<String>,
    /// The characters present now. Empty unless the story has several characters.
    pub present: Vec<String>,
    /// The characters the conversation is with now. Empty unless the story has several
    /// characters.
    pub floor: Vec<String>,
    /// Every storyline of the session, each ending at a tip. The sidebar lists them as
    /// sub-chats of the story.
    pub branches: Vec<Branch>,
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
    /// Where the user put cards, as scene coordinates. Nodes not listed keep their place in
    /// the automatic layout.
    pub positions: std::collections::HashMap<NodeId, (f32, f32)>,
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
    /// Take the character with this name out of the open story. Its earlier messages stay.
    RemoveCharacter(String),
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
    /// Move to a node and start a new storyline there: what is written next becomes another
    /// continuation of it.
    BranchFrom(NodeId),
    /// Remember where the user put the card of a node in the tree view.
    MoveNode {
        /// The node.
        node: NodeId,
        /// Horizontal position in scene coordinates.
        x: f32,
        /// Vertical position in scene coordinates.
        y: f32,
    },
    /// Forget all card positions of the open story, so its tree is laid out afresh.
    ResetTreeLayout,
    /// Save a character, to `path` if given, else to a new file.
    SaveCharacter {
        /// The file to overwrite.
        path: Option<PathBuf>,
        /// The card.
        character: Character,
    },
    /// Delete a character file.
    DeleteCharacter(PathBuf),
    /// Have the model write or improve one text of a character card. The answer arrives as
    /// [`Event::Improved`].
    ImproveCharacter {
        /// The text to write.
        field: improve::Field,
        /// The card as it is in the editor, unsaved changes included.
        character: Character,
    },
    /// Let the user choose a character card (PNG, JSON, or YAML) and add it to the library.
    ImportCharacter,
    /// Let the user choose where to save the character in `PathBuf` as a PNG, JSON, or YAML
    /// card.
    ExportCharacter(PathBuf),
    /// Save a world, to `path` if given, else to a new file.
    SaveWorld {
        /// The file to overwrite.
        path: Option<PathBuf>,
        /// The lorebook.
        world: Lorebook,
    },
    /// Delete a world file.
    DeleteWorld(PathBuf),

    /// Remember that the first-run tutorial has been shown, so it does not open again.
    MarkTutorialSeen,
    /// Ask GitHub whether a newer release exists. The answer arrives as
    /// [`Event::UpdateAvailable`]; without one, nothing is sent.
    CheckUpdate,
    /// Download and start the installer of `release`, then ask the UI to quit. A release
    /// without an installer for this system opens its page instead.
    InstallUpdate(Release),
    /// Let the user choose a picture for the editor `target`. The result arrives as
    /// [`Event::ImagePicked`].
    PickImage(ImageTarget),

    // Sent by the backend's own tasks.
    /// A card was read from a file: store it.
    ImportedCharacter {
        /// The character, with its picture already stored.
        character: Character,
        /// What the card had that MasqueRole cannot use.
        skipped: Vec<String>,
    },
    /// Tokens used by a background job, to add to the session's usage.
    Usage {
        /// The session the tokens belong to.
        session: SessionId,
        /// The estimated amount.
        tokens: usize,
    },
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
        /// The character who answered; `None` for the narrator or a story with at most one
        /// character.
        speaker: Option<String>,
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
        /// The character whose point of view it takes; `None` for the shared summary.
        character: Option<String>,
    },
    /// New facts were extracted.
    FactsReady {
        /// The node they were learned at.
        node: NodeId,
        /// The facts, with who knows them.
        facts: Vec<NewFact>,
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
        /// The folder holding their pictures.
        images_dir: PathBuf,
    },
    /// A picture was chosen and stored.
    ImagePicked {
        /// What it is for.
        target: ImageTarget,
        /// The stored file name, to put in the card.
        name: String,
    },
    /// The model finished a text asked for with [`Request::ImproveCharacter`].
    Improved {
        /// The text that was asked for.
        field: improve::Field,
        /// The new text, or `None` when it failed (an error is shown too).
        text: Option<String>,
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
    /// The character who is about to answer, or `None` for the narrator or the end of the
    /// answers.
    Speaker(Option<String>),
    /// Something worth a brief message, such as a save.
    Info(String),
    /// Something went wrong.
    Error(String),
    /// A newer release is published.
    UpdateAvailable(Release),
    /// Installing the update failed or was handed to the browser; the button is usable again.
    UpdateStopped,
    /// The installer is running: close the app so it can replace the files.
    Quit,
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
    /// Lets the user choose a picture.
    pub picker: ImagePicker,
    /// The dialogs for importing and exporting characters.
    pub cards: CardFiles,
    /// The address of the newest-release description. `None` turns update checks off.
    pub update_url: Option<String>,
    /// Where the model for semantic memory is kept. `None` turns semantic memory off.
    pub data_dir: Option<PathBuf>,
    /// A ready embedder, used instead of the downloaded model (for tests).
    pub embedder: Option<Arc<dyn Embedder>>,
    /// Redraws the UI.
    pub wake: Wake,
}

/// The embedding model, once it is loaded. Filled by a background task.
type SharedEmbedder = Arc<Mutex<Option<Arc<dyn Embedder>>>>;

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
        picker: services.picker,
        cards: services.cards,
        update_url: services.update_url,
        data_dir: services.data_dir,
        embedder: Arc::new(Mutex::new(services.embedder)),
        loading_embedder: Arc::new(AtomicBool::new(false)),
        wake: services.wake,
        settings: Settings::default(),
        session: None,
        characters: Vec::new(),
        worlds: Vec::new(),
        cancel: None,
        queue: VecDeque::new(),
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
    picker: ImagePicker,
    cards: CardFiles,
    update_url: Option<String>,
    data_dir: Option<PathBuf>,
    embedder: SharedEmbedder,
    /// Set while the model of semantic memory is being downloaded or loaded.
    loading_embedder: Arc<AtomicBool>,
    wake: Wake,
    settings: Settings,
    session: Option<SessionId>,
    characters: Vec<Entry<Character>>,
    worlds: Vec<Entry<Lorebook>>,
    /// Set while a response is being generated.
    cancel: Option<Arc<Notify>>,
    /// The characters who still have to answer the last message, in order. `None` is the
    /// narrator, or the only voice of a story with at most one character.
    queue: VecDeque<Option<String>>,
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
    speaker: Option<String>,
}

/// A story with several characters, as it stands at one node.
struct Group {
    cast: Vec<Character>,
    names: Vec<String>,
    path: Vec<Node>,
    scenes: Scenes,
    present: Vec<String>,
}

/// Read a card file. A picture inside it is stored in the library and set as the character's
/// picture. Returns the character and what was left out.
fn read_card(bytes: &[u8], library: &Library) -> Result<(Character, Vec<String>), String> {
    if bytes.len() > crate::images::MAX_FILE_BYTES {
        return Err("That file is too large to be a character card.".into());
    }
    let imported = exchange::import(bytes).map_err(|e| format!("Could not import: {e}"))?;
    let mut character = imported.character;
    let mut skipped = imported.skipped;
    if let Some(picture) = imported.picture {
        match crate::images::prepare(&picture).and_then(|png| library.save_image(&png)) {
            Ok(name) => character.image = Some(name),
            Err(_) => skipped.push("its picture".into()),
        }
    }
    Ok((character, skipped))
}

/// Write `character` to `target` as PNG, JSON, or YAML, by the extension. A name without an
/// extension becomes a PNG. Returns the file written.
fn write_card(
    character: &Character,
    library: &Library,
    target: PathBuf,
) -> Result<PathBuf, String> {
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let (format, target) = match ExportFormat::from_file_name(name) {
        Some(format) => (format, target),
        None if !name.contains('.') => (ExportFormat::Png, target.with_extension("png")),
        None => return Err("Name the file .png, .json, or .yaml.".into()),
    };
    let bytes = match format {
        ExportFormat::Json => exchange::to_json(character).into_bytes(),
        ExportFormat::Yaml => exchange::to_yaml(character)
            .map_err(|e| e.to_string())?
            .into_bytes(),
        ExportFormat::Png => {
            let picture = character
                .image
                .as_deref()
                .and_then(|image| library.image_path(image))
                .and_then(|path| std::fs::read(path).ok())
                .map_or_else(crate::images::placeholder, Ok)?;
            exchange::to_png(character, &picture).map_err(|e| e.to_string())?
        }
    };
    std::fs::write(&target, bytes).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(target)
}

/// A name made safe to use as a file name.
fn file_stem(name: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if stem.is_empty() {
        "character".into()
    } else {
        stem
    }
}

impl Group {
    /// The characters the conversation is with, among those present.
    fn floor(&self) -> Vec<String> {
        scene::floor(&self.path, &self.scenes, &self.names)
            .into_iter()
            .filter(|name| self.present.contains(name))
            .collect()
    }
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
            Request::RemoveCharacter(name) => self.remove_character(&name),
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
            Request::BranchFrom(node) => self.branch_from(node),
            Request::MoveNode { node, x, y } => {
                // Bookkeeping, like usage: a failure is logged and never interrupts the story.
                if let Err(e) = self.db.set_node_position(node, x, y) {
                    tracing::warn!(error = %e, "could not save a card position");
                }
            }
            Request::ResetTreeLayout => {
                if let Some(session) = self.session {
                    if let Err(e) = self.db.clear_node_positions(session) {
                        return self.error(e);
                    }
                    self.emit_tree();
                }
            }
            Request::DeleteApiKey => {
                match self.secrets.delete(&self.settings.api.provider) {
                    Ok(()) => self.emit(Event::Info("API key removed.".into())),
                    Err(e) => self.error(e),
                }
                self.emit_settings();
            }
            Request::SaveCharacter { path, character } => {
                let old = path
                    .as_ref()
                    .and_then(|p| Character::load(p).ok())
                    .and_then(|c| c.image);
                match self.library.save_character(path.as_deref(), &character) {
                    Ok(_) => {
                        self.drop_replaced_image(old, character.image.as_deref());
                        self.emit(Event::Info(format!("Saved {}.", character.name)));
                        self.reload_library();
                    }
                    Err(e) => self.error(e),
                }
            }
            Request::DeleteCharacter(path) => {
                let image = Character::load(&path).ok().and_then(|c| c.image);
                self.delete_item(&path, image);
            }
            Request::DeleteWorld(path) => {
                let image = Lorebook::load(&path).ok().and_then(|w| w.image);
                self.delete_item(&path, image);
            }
            Request::ImproveCharacter { field, character } => self.improve(field, character),
            Request::ImportCharacter => self.import_character(),
            Request::ImportedCharacter { character, skipped } => {
                self.store_imported(character, &skipped)
            }
            Request::ExportCharacter(path) => self.export_character(&path),
            Request::PickImage(target) => self.pick_image(target),
            Request::CheckUpdate => self.check_update(),
            Request::InstallUpdate(release) => self.install_update(release),
            Request::MarkTutorialSeen => {
                if !self.settings.ui.tutorial_seen {
                    self.settings.ui.tutorial_seen = true;
                    self.persist_settings();
                }
            }
            Request::SaveWorld { path, world } => {
                let old = path
                    .as_ref()
                    .and_then(|p| Lorebook::load(p).ok())
                    .and_then(|w| w.image);
                match self.library.save_world(path.as_deref(), &world) {
                    Ok(_) => {
                        self.drop_replaced_image(old, world.image.as_deref());
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
                speaker,
            } => self.generated(session, parent, &text, lore, speaker),
            Request::Usage { session, tokens } => self.add_usage(session, tokens),
            Request::GenerationFailed(message) => {
                self.cancel = None;
                self.queue.clear();
                self.emit(Event::Generating(false));
                self.emit(Event::Speaker(None));
                self.error(message);
            }
            Request::GenerationStopped => {
                self.cancel = None;
                self.queue.clear();
                self.emit(Event::Generating(false));
                self.emit(Event::Speaker(None));
            }
            Request::SummaryReady {
                node,
                text,
                character,
            } => {
                // The node may have been pruned meanwhile; then the summary is moot.
                let _ = self.db.add_summary_for(node, &text, character.as_deref());
            }
            Request::FactsReady { node, facts } => {
                for fact in facts {
                    let _ = self
                        .db
                        .add_fact_for(node, &fact.text, fact.known_by.as_deref());
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
        self.remove_unused_images();
        self.emit_settings();
        self.prepare_semantic_memory();
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
        self.prepare_semantic_memory();
        self.emit(Event::Info("Settings saved.".into()));
    }

    /// When semantic memory is on and its model is not loaded, download it if needed and load
    /// it in the background. Stories work without it in the meantime.
    fn prepare_semantic_memory(&self) {
        let memory = &self.settings.memory;
        let Some(data_dir) = self.data_dir.clone() else {
            return;
        };
        if !(memory.enabled && memory.semantic)
            || self.embedder.lock().expect("embedder lock").is_some()
            || self.loading_embedder.swap(true, Ordering::SeqCst)
        {
            return;
        }
        let (events, wake) = (self.events.clone(), self.wake.clone());
        let (slot, loading) = (self.embedder.clone(), self.loading_embedder.clone());
        let tell = move |event: Event| {
            let _ = events.send(event);
            wake();
        };
        tokio::spawn(async move {
            let dir = embed::model_dir(&data_dir);
            if !embed::model_present(&dir) {
                tell(Event::Info(
                    "Downloading the model for semantic memory (about 90 MB, only once)...".into(),
                ));
                if let Err(e) = embed::download_model(&dir).await {
                    tracing::warn!(error = %e, "semantic memory model download failed");
                    tell(Event::Error(e.to_string()));
                    loading.store(false, Ordering::SeqCst);
                    return;
                }
            }
            match tokio::task::spawn_blocking(move || MiniLm::load(&dir)).await {
                Ok(Ok(model)) => {
                    *slot.lock().expect("embedder lock") = Some(Arc::new(model));
                    tell(Event::Info("Semantic memory is ready.".into()));
                }
                Ok(Err(e)) => tell(Event::Error(e.to_string())),
                Err(e) => tell(Event::Error(e.to_string())),
            }
            loading.store(false, Ordering::SeqCst);
        });
    }

    fn delete_item(&mut self, path: &std::path::Path, image: Option<String>) {
        match self.library.delete(path) {
            Ok(()) => {
                if let Some(image) = image {
                    self.library.delete_image(&image);
                }
                self.reload_library();
            }
            Err(e) => self.error(e),
        }
    }

    /// After a card was saved: delete its old picture if the card now has a different one.
    fn drop_replaced_image(&self, old: Option<String>, new: Option<&str>) {
        if let Some(old) = old {
            if new != Some(old.as_str()) {
                self.library.delete_image(&old);
            }
        }
    }

    /// Delete pictures no card refers to, such as one chosen in an editor that was cancelled.
    fn remove_unused_images(&self) {
        let used = self
            .characters
            .iter()
            .filter_map(|c| c.item.image.clone())
            .chain(self.worlds.iter().filter_map(|w| w.item.image.clone()))
            .collect();
        let removed = self.library.remove_unused_images(&used);
        if removed > 0 {
            tracing::debug!(removed, "removed unused pictures");
        }
    }

    /// Let the user choose a card, read it off the backend's thread, and hand the character
    /// back as [`Request::ImportedCharacter`].
    fn import_character(&self) {
        let (open, library, events, requests, wake) = (
            self.cards.open.clone(),
            self.library.clone(),
            self.events.clone(),
            self.requests.clone(),
            self.wake.clone(),
        );
        tokio::spawn(async move {
            let Some(bytes) = open().await else {
                return; // cancelled
            };
            let read = tokio::task::spawn_blocking(move || read_card(&bytes, &library))
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            match read {
                Ok((character, skipped)) => {
                    let _ = requests.send(Request::ImportedCharacter { character, skipped });
                }
                Err(message) => {
                    let _ = events.send(Event::Error(message));
                    wake();
                }
            }
        });
    }

    /// Save an imported character to the library.
    fn store_imported(&mut self, character: Character, skipped: &[String]) {
        match self.library.save_character(None, &character) {
            Ok(_) => {
                let mut message = format!("Imported {}.", character.name);
                if !skipped.is_empty() {
                    message.push_str(&format!(" Left out: {}.", skipped.join(", ")));
                }
                self.emit(Event::Info(message));
                self.reload_library();
            }
            Err(e) => {
                if let Some(image) = &character.image {
                    self.library.delete_image(image);
                }
                self.error(e);
            }
        }
    }

    /// Let the user choose a file, then write the character there in the format its
    /// extension names.
    fn export_character(&self, path: &std::path::Path) {
        let Some(entry) = self.characters.iter().find(|c| c.path == path) else {
            return self.error("That character is no longer in the library.");
        };
        let character = entry.item.clone();
        let (save, library, events, wake) = (
            self.cards.save.clone(),
            self.library.clone(),
            self.events.clone(),
            self.wake.clone(),
        );
        tokio::spawn(async move {
            let suggested = format!("{}.png", file_stem(&character.name));
            let Some(target) = save(suggested).await else {
                return; // cancelled
            };
            let name = character.name.clone();
            let written =
                tokio::task::spawn_blocking(move || write_card(&character, &library, target))
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()));
            let _ = events.send(match written {
                Ok(file) => Event::Info(format!("Exported {name} to {}.", file.display())),
                Err(message) => Event::Error(message),
            });
            wake();
        });
    }

    /// Open the picture chooser without blocking the backend, then store the result.
    fn pick_image(&self, target: ImageTarget) {
        let (picker, library, events, wake) = (
            self.picker.clone(),
            self.library.clone(),
            self.events.clone(),
            self.wake.clone(),
        );
        tokio::spawn(async move {
            let Some(bytes) = picker().await else {
                return; // cancelled
            };
            let stored = tokio::task::spawn_blocking(move || {
                let png = crate::images::prepare(&bytes)?;
                library.save_image(&png)
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            let event = match stored {
                Ok(name) => Event::ImagePicked { target, name },
                Err(message) => Event::Error(message),
            };
            let _ = events.send(event);
            wake();
        });
    }

    fn check_update(&self) {
        let Some(url) = self.update_url.clone() else {
            return;
        };
        let (events, wake) = (self.events.clone(), self.wake.clone());
        tokio::spawn(async move {
            match update::check(&url, update::CURRENT_VERSION, std::env::consts::OS).await {
                Ok(Some(release)) => {
                    let _ = events.send(Event::UpdateAvailable(release));
                    wake();
                }
                Ok(None) => {}
                // A missing connection is normal; the check is quiet.
                Err(e) => tracing::info!("update check failed: {e}"),
            }
        });
    }

    fn install_update(&self, release: Release) {
        let (events, wake) = (self.events.clone(), self.wake.clone());
        tokio::spawn(async move {
            let result = install(&release).await;
            let _ = events.send(match result {
                Ok(true) => Event::Quit,
                Ok(false) => Event::UpdateStopped,
                Err(e) => {
                    tracing::warn!("update failed: {e}");
                    let _ = events.send(Event::Error(format!("Update failed: {e}")));
                    Event::UpdateStopped
                }
            });
            wake();
        });
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
            images_dir: self.library.images_dir(),
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
        let result = (|| -> Result<Transcript, masquerole_core::db::DbError> {
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
            let branches = StoryTree::new(&self.db, id).branches().unwrap_or_default();
            let (present, floor) = match self.group(id) {
                Ok(Some(group)) => {
                    let floor = group.floor();
                    (group.present, floor)
                }
                _ => (Vec::new(), Vec::new()),
            };
            Ok(Transcript {
                session,
                nodes,
                characters,
                present,
                floor,
                branches,
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
        let result = (|| -> Result<TreeData, masquerole_core::db::DbError> {
            let current = self.db.session(id)?.current_node_id;
            Ok(TreeData {
                session_id: id,
                nodes: self.db.list_nodes(id)?,
                current,
                positions: self
                    .db
                    .node_positions(id)?
                    .into_iter()
                    .map(|(node, x, y)| (node, (x, y)))
                    .collect(),
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

    fn branch_from(&mut self, node: NodeId) {
        let Some(session) = self.session else { return };
        if self.cancel.is_some() {
            self.error("Wait for the response to finish, or stop it first.");
            return;
        }
        let tree = StoryTree::new(&self.db, session);
        match tree.jump(node).and_then(|_| tree.branch(None)) {
            Ok(point) => {
                self.emit_transcript();
                self.emit_sessions();
                self.emit(Event::Notice(format::notice(&CommandOutput::Branched(
                    point,
                ))));
            }
            Err(e) => self.error(e),
        }
    }

    /// Have the model write `field` of `character`. Always answers with [`Event::Improved`],
    /// so the editor can stop waiting.
    fn improve(&self, field: improve::Field, character: Character) {
        let fail = |message: String| {
            self.emit(Event::Improved { field, text: None });
            self.error(message);
        };
        let provider = match self.provider() {
            Ok(provider) => provider,
            Err(e) => return fail(e),
        };
        let (events, wake) = (self.events.clone(), self.wake.clone());
        tokio::spawn(async move {
            let text = match improve::improve(provider.as_ref(), field, &character).await {
                Ok(text) => Some(text),
                Err(e) => {
                    tracing::warn!(error = %e, "improving a character text failed");
                    let _ = events.send(Event::Error(format!(
                        "Could not write the {}: {e}",
                        field.label()
                    )));
                    None
                }
            };
            let _ = events.send(Event::Improved { field, text });
            wake();
        });
    }

    /// `/compact`: summarize everything before the newest two messages. The summary arrives
    /// as [`Request::SummaryReady`], like the automatic ones.
    fn compact(&self, session: SessionId) {
        let plan = (|| -> Result<Vec<memory::SummaryJob>, String> {
            let db_err = |e: masquerole_core::db::DbError| e.to_string();
            let Some(current) = self.db.session(session).map_err(db_err)?.current_node_id else {
                return Ok(Vec::new());
            };
            let path = self.db.path_to_root(current).map_err(db_err)?;
            let summary = self.db.summary_at(current).map_err(db_err)?;
            let mut jobs: Vec<memory::SummaryJob> = memory::plan_compact(&path, summary.as_ref())
                .into_iter()
                .collect();
            // In a scene with several characters each one keeps a summary of their own.
            if let Some(group) = self.group(session)? {
                for name in &group.names {
                    let seen: Vec<Node> = scene::witnessed_by(&path, &group.scenes, name)
                        .into_iter()
                        .cloned()
                        .collect();
                    let own = self.db.summary_for(current, Some(name)).map_err(db_err)?;
                    jobs.extend(memory::plan_compact_as(&seen, own.as_ref(), Some(name)));
                }
            }
            Ok(jobs)
        })();
        let jobs = match plan {
            Ok(jobs) if !jobs.is_empty() => jobs,
            Ok(_) => {
                return self.emit(Event::Notice(Notice::plain(
                    "There is nothing to compact yet. The newest two messages always stay.",
                )))
            }
            Err(e) => return self.error(e),
        };
        let message_count = jobs[0].message_count;
        if self.limit_reached(session) {
            return self.error("This story has reached the token limit.");
        }
        let provider = match self.provider() {
            Ok(provider) => provider,
            Err(e) => return self.error(e),
        };
        for job in &jobs {
            self.add_usage(session, job.estimated_tokens());
        }
        self.emit(Event::Notice(Notice::plain(format!(
            "Compacting {message_count} messages..."
        ))));
        let (requests, events, wake) = (
            self.requests.clone(),
            self.events.clone(),
            self.wake.clone(),
        );
        tokio::spawn(async move {
            for job in jobs {
                match memory::summarize(provider.as_ref(), &job).await {
                    Ok(text) => {
                        let _ = requests.send(Request::Usage {
                            session,
                            tokens: context::estimate_tokens(&text),
                        });
                        let _ = requests.send(Request::SummaryReady {
                            node: job.through_node,
                            text,
                            character: job.character.clone(),
                        });
                        if job.character.is_none() {
                            let _ = events.send(Event::Info(format!(
                                "Compacted {} messages.",
                                job.message_count
                            )));
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "compacting failed");
                        let _ = events.send(Event::Error(format!("Could not compact: {e}")));
                    }
                }
            }
            wake();
        });
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

    fn remove_character(&mut self, name: &str) {
        let Some(session) = self.session else { return };
        let result = (|| -> Result<(), String> {
            let found = self
                .db
                .list_characters(session)
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|c| c.name == name);
            match found {
                Some(c) => self.db.remove_character(c.id).map_err(|e| e.to_string()),
                None => Ok(()),
            }
        })();
        match result {
            Ok(()) => self.emit_transcript(),
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
            Ok(None) => self.send_message(session, text),
        }
    }

    /// Store the player's message and start the answers. In a story with several characters
    /// this decides who is spoken to and who answers; otherwise there is one answer.
    fn send_message(&mut self, session: SessionId, text: &str) {
        let result = (|| -> Result<Vec<Option<String>>, String> {
            let group = self.group(session)?;
            let node = StoryTree::new(&self.db, session)
                .append(Turn::user(text))
                .map_err(|e| e.to_string())?;
            let Some(group) = group else {
                return Ok(vec![None]);
            };
            let resolution = scene::resolve(
                text,
                &group.cast,
                &group.present,
                &group.floor(),
                self.settings.style.max_responders as usize,
            );
            self.db
                .set_node_scene(
                    node.id,
                    &NodeScene {
                        witnesses: Some(group.present.clone()),
                        addressed: resolution.addressed.clone(),
                        present: None,
                    },
                )
                .map_err(|e| e.to_string())?;
            Ok(self.voices(resolution.responders))
        })();
        match result {
            Ok(voices) => {
                self.queue = voices.into();
                self.emit_transcript();
                self.next_voice();
            }
            Err(e) => self.error(e),
        }
    }

    /// Who answers, given the characters the message called for: with narrator mode on, or
    /// when nobody was called, the narrator does.
    fn voices(&self, responders: Vec<String>) -> Vec<Option<String>> {
        if self.settings.style.narrator_mode || responders.is_empty() {
            vec![None]
        } else {
            responders.into_iter().map(Some).collect()
        }
    }

    /// Start the next answer in the queue, if there is one.
    fn next_voice(&mut self) {
        if let Some(speaker) = self.queue.pop_front() {
            self.start_generation(speaker);
        }
    }

    /// Answer again after `/retry`. The answer that was set aside names who answers; with
    /// none, the last message decides again.
    fn regenerate(&mut self, session: SessionId, replaced: Option<NodeId>) {
        let voices = match replaced.and_then(|id| self.db.node(id).ok()) {
            Some(node) if node.kind == NodeKind::Character && node.speaker.is_some() => {
                vec![node.speaker]
            }
            Some(_) => vec![None],
            None => self.voices_for_last_message(session),
        };
        self.queue = voices.into();
        self.next_voice();
    }

    /// Who answers the player's last message: those it was addressed to, or else whoever the
    /// scene before it calls for.
    fn voices_for_last_message(&self, session: SessionId) -> Vec<Option<String>> {
        let lookup = || -> Result<Vec<String>, String> {
            let db_err = |e: masquerole_core::db::DbError| e.to_string();
            let current = self
                .db
                .session(session)
                .map_err(db_err)?
                .current_node_id
                .ok_or("no message")?;
            let node = self.db.node(current).map_err(db_err)?;
            let stored = self
                .db
                .scenes_on_path(current)
                .map_err(db_err)?
                .remove(&current)
                .map(|s| s.addressed)
                .unwrap_or_default();
            if !stored.is_empty() {
                return Ok(stored);
            }
            let Some(group) = self.group_at(session, node.parent_id)? else {
                return Ok(Vec::new());
            };
            Ok(scene::resolve(
                &node.content,
                &group.cast,
                &group.present,
                &group.floor(),
                self.settings.style.max_responders as usize,
            )
            .responders)
        };
        let responders = lookup().unwrap_or_default();
        let max = self.settings.style.max_responders.max(1) as usize;
        self.voices(responders.into_iter().take(max).collect())
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
                    CommandOutput::Regenerate { replaced, .. } => self.regenerate(active, replaced),
                    CommandOutput::Whisper { target } => {
                        self.queue = VecDeque::from([Some(target)]);
                        self.next_voice();
                    }
                    CommandOutput::Compact => self.compact(active),
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

    /// The story at its current node as a scene with several characters, or `None` when it
    /// has at most one.
    fn group(&self, session: SessionId) -> Result<Option<Group>, String> {
        let current = self
            .db
            .session(session)
            .map_err(|e| e.to_string())?
            .current_node_id;
        self.group_at(session, current)
    }

    /// Like [`group`](Self::group), at the end of the path to `node`.
    fn group_at(&self, session: SessionId, node: Option<NodeId>) -> Result<Option<Group>, String> {
        let cast = self.session_characters(session)?;
        if cast.len() < 2 {
            return Ok(None);
        }
        let db_err = |e: masquerole_core::db::DbError| e.to_string();
        let (path, scenes) = match node {
            Some(id) => (
                self.db.path_to_root(id).map_err(db_err)?,
                self.db.scenes_on_path(id).map_err(db_err)?,
            ),
            None => (Vec::new(), Scenes::new()),
        };
        let names: Vec<String> = cast.iter().map(|c| c.name.clone()).collect();
        let present = scene::present_on(&path, &scenes, &names);
        Ok(Some(Group {
            cast,
            names,
            path,
            scenes,
            present,
        }))
    }

    /// The prompt for `speaker`, or for the narrator when `None`. In a story with several
    /// characters a speaker's prompt holds only what that character witnessed and knows.
    fn build_prompt(&self, session_id: SessionId, speaker: Option<&str>) -> Result<Prompt, String> {
        let db_err = |e: masquerole_core::db::DbError| e.to_string();
        let session = self.db.session(session_id).map_err(db_err)?;
        let current = session
            .current_node_id
            .ok_or("The story has no messages yet.")?;
        let path = self.db.path_to_root(current).map_err(db_err)?;
        let group = self.group(session_id)?;
        let view = match (&group, speaker) {
            (Some(group), Some(name)) => Some((group, name)),
            _ => None,
        };
        let messages: Vec<ChatMessage> = match view {
            Some((group, name)) => context::scene_messages(&path, &group.scenes, name),
            None => path.iter().map(context::node_message).collect(),
        };
        let all_characters = self.session_characters(session_id)?;
        let (characters, others): (Vec<Character>, Vec<Character>) = match view {
            Some((group, name)) => (
                group
                    .cast
                    .iter()
                    .filter(|c| c.name == name)
                    .cloned()
                    .collect(),
                group
                    .cast
                    .iter()
                    .filter(|c| c.name != name && group.present.contains(&c.name))
                    .cloned()
                    .collect(),
            ),
            None => (all_characters.clone(), Vec::new()),
        };

        let world = session
            .world
            .as_deref()
            .and_then(|name| self.worlds.iter().find(|w| w.item.name == name))
            .map(|w| &w.item);
        let window = (self.settings.context.recent_messages as usize).max(1);
        let recent: Vec<&str> = path
            .iter()
            .rev()
            .filter(|n| match view {
                Some((group, name)) => scene::witnessed(group.scenes.get(&n.id), name),
                None => true,
            })
            .take(window)
            .map(|n| n.content.as_str())
            .collect();
        let lore: Vec<&LoreEntry> = world
            .map(|w| w.triggered(&recent))
            .unwrap_or_default()
            .into_iter()
            .filter(|entry| view.is_none_or(|(_, name)| entry.is_known_by(name)))
            .collect();
        let known: Vec<masquerole_core::db::Fact> = self
            .db
            .facts_at(current)
            .map_err(db_err)?
            .into_iter()
            .filter(|f| match (view, &f.known_by) {
                (Some((_, name)), Some(known_by)) => known_by.iter().any(|n| n == name),
                _ => true,
            })
            .collect();
        // Semantic memory: what fits the newest message goes last in the facts, where
        // trimming drops it last, and older messages that fit are brought back.
        let embedder = self
            .embedder
            .lock()
            .expect("embedder lock")
            .clone()
            .filter(|_| self.settings.memory.enabled && self.settings.memory.semantic);
        let mut recalled = recall::Recall::default();
        if let Some(embedder) = embedder {
            let seen: Vec<&Node> = path
                .iter()
                .filter(|n| match view {
                    Some((group, name)) => scene::witnessed(group.scenes.get(&n.id), name),
                    None => true,
                })
                .collect();
            let older = &seen[..seen.len().saturating_sub(window)];
            let query = path
                .iter()
                .rev()
                .find(|n| n.kind == NodeKind::User)
                .or(path.last())
                .map_or("", |n| n.content.as_str());
            match recall::recall(
                &self.db,
                embedder.as_ref(),
                session_id,
                query,
                &known,
                older,
            ) {
                Ok(found) => recalled = found,
                Err(e) => tracing::warn!(error = %e, "semantic memory failed"),
            }
        }
        let facts: Vec<String> = known
            .iter()
            .filter(|f| !recalled.facts.contains(&f.text))
            .map(|f| f.text.clone())
            .chain(recalled.facts.iter().cloned())
            .collect();
        let summary = match view {
            Some((_, name)) => self.db.summary_for(current, Some(name)).map_err(db_err)?,
            None => self.db.summary_at(current).map_err(db_err)?,
        }
        .map(|s| s.text);

        let system_prompt = match view {
            Some((_, name)) => prompts::speaker_prompt(&self.settings, name),
            None => prompts::system_prompt(&self.settings, &characters),
        };
        let built = context::build(
            &ContextInput {
                system_prompt: &system_prompt,
                characters: &characters,
                others: &others,
                world_setting: world.map_or("", |w| w.setting.as_str()),
                lore: &lore,
                facts: &facts,
                recalled: &recalled.messages,
                summary: summary.as_deref(),
                messages: &messages,
            },
            &self.settings.context,
        );
        Ok(Prompt {
            messages: built.messages,
            parent: current,
            lore: lore.iter().map(|e| e.id().to_string()).collect(),
            speaker: speaker.map(Into::into),
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

    /// Add an estimate of `tokens` to the session's usage. A failure here is only logged: usage
    /// is bookkeeping and must never interrupt the story.
    fn add_usage(&self, session: SessionId, tokens: usize) {
        let tokens = i64::try_from(tokens).unwrap_or(i64::MAX);
        if let Err(e) = self.db.add_tokens(session, tokens) {
            tracing::warn!(error = %e, "could not record token usage");
        }
    }

    /// Whether the session has used up the token limit, if one is on.
    fn limit_reached(&self, session: SessionId) -> bool {
        let used = self.db.session(session).map_or(0, |s| s.tokens_used);
        self.settings.limits.reached(used)
    }

    fn start_generation(&mut self, speaker: Option<String>) {
        let Some(session) = self.session else {
            self.queue.clear();
            return;
        };
        if self.limit_reached(session) {
            self.queue.clear();
            return self.error(format!(
                "This story has reached the token limit ({}). Raise the limit or turn it off in Settings, under Limits.",
                format::tokens(i64::from(self.settings.limits.token_limit))
            ));
        }
        let prompt = match self.build_prompt(session, speaker.as_deref()) {
            Ok(prompt) => prompt,
            Err(e) => {
                self.queue.clear();
                return self.error(e);
            }
        };
        let provider = match self.provider() {
            Ok(provider) => provider,
            Err(e) => {
                self.queue.clear();
                return self.error(e);
            }
        };
        self.add_usage(
            session,
            prompt
                .messages
                .iter()
                .map(context::message_tokens)
                .sum::<usize>()
                + context::REPLY_PRIMING_TOKENS,
        );
        let cancel = Arc::new(Notify::new());
        self.cancel = Some(cancel.clone());
        tracing::debug!(session = ?session, "generation started");
        self.emit(Event::Generating(true));
        self.emit(Event::Speaker(prompt.speaker.clone()));

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
                        speaker: prompt.speaker,
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
        // Stopping ends the whole round of answers, not only the one that is being written.
        self.queue.clear();
        if let Some(cancel) = &self.cancel {
            cancel.notify_one();
        }
    }

    fn generated(
        &mut self,
        session: SessionId,
        parent: NodeId,
        text: &str,
        lore: Vec<String>,
        speaker: Option<String>,
    ) {
        self.cancel = None;
        self.add_usage(session, context::estimate_tokens(text));
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
            let group = self.group_at(session, Some(parent))?;
            let turn = match (&group, &speaker) {
                (Some(_), Some(name)) => Turn::character(name, prompts::clean_reply(text, name)),
                _ => prompts::response_turn(&self.settings, &names, text.trim()),
            }
            .with_lore(lore);
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
            if let Some(group) = &group {
                // Whoever was present saw the answer, unless it answers a whisper: then only
                // those who heard the whisper did.
                let heard = group.scenes.get(&parent).and_then(|s| s.witnesses.clone());
                let witnesses = match heard {
                    Some(heard) if heard.len() < group.present.len() => heard,
                    _ => group.present.clone(),
                };
                self.db
                    .set_node_scene(
                        node.id,
                        &NodeScene {
                            witnesses: Some(witnesses),
                            ..NodeScene::default()
                        },
                    )
                    .map_err(|e| e.to_string())?;
            }
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
                // The next character takes over the streaming bubble at once, so the answer
                // that was just stored is not shown again in it while the next one is prepared.
                if let Some(next) = self.queue.front().cloned() {
                    self.emit(Event::Speaker(next));
                }
                self.after_turn(node);
                if self.queue.is_empty() {
                    self.emit(Event::Generating(false));
                    self.emit(Event::Speaker(None));
                } else {
                    self.next_voice();
                }
            }
            Err(e) => {
                self.queue.clear();
                self.emit(Event::Generating(false));
                self.emit(Event::Speaker(None));
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
        let Some(session) = path.last().map(|n| n.session_id) else {
            return;
        };
        // Background jobs also cost tokens, so they stop when the limit is reached.
        if self.limit_reached(session) {
            return;
        }

        let group = self.group_at(session, Some(node)).ok().flatten();
        let mut jobs: Vec<memory::SummaryJob> = Vec::new();
        if let Ok(summary) = self.db.summary_at(node) {
            jobs.extend(memory::plan_summary(
                &path,
                summary.as_ref(),
                &self.settings,
            ));
        }
        // In a scene with several characters each one also keeps a summary of what they saw.
        if let Some(group) = &group {
            for name in &group.names {
                let seen: Vec<Node> = scene::witnessed_by(&path, &group.scenes, name)
                    .into_iter()
                    .cloned()
                    .collect();
                if let Ok(own) = self.db.summary_for(node, Some(name)) {
                    jobs.extend(memory::plan_summary_as(
                        &seen,
                        own.as_ref(),
                        &self.settings,
                        Some(name),
                    ));
                }
            }
        }
        for job in jobs {
            self.add_usage(session, job.estimated_tokens());
            let (provider, requests, events) =
                (provider.clone(), self.requests.clone(), self.events.clone());
            tokio::spawn(async move {
                match memory::summarize(provider.as_ref(), &job).await {
                    Ok(text) => {
                        let _ = requests.send(Request::Usage {
                            session,
                            tokens: context::estimate_tokens(&text),
                        });
                        let _ = requests.send(Request::SummaryReady {
                            node: job.through_node,
                            text,
                            character: job.character.clone(),
                        });
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "summarizing failed");
                        let _ = events.send(Event::Error(format!("Could not summarize: {e}")));
                    }
                }
            });
        }

        let known: Vec<String> = self
            .db
            .facts_at(node)
            .map(|facts| facts.into_iter().map(|f| f.text).collect())
            .unwrap_or_default();
        let newest = &path[path.len().saturating_sub(2)..];
        let cast = group.as_ref().map(|g| SceneCast {
            names: &g.present,
            scenes: &g.scenes,
        });
        if let Some(job) = memory::plan_scene_fact_extraction(newest, &known, &self.settings, cast)
        {
            self.add_usage(session, job.estimated_tokens());
            let (requests, events) = (self.requests.clone(), self.events.clone());
            let names = group.map(|g| g.names);
            tokio::spawn(async move {
                let found = match &names {
                    Some(names) => {
                        memory::extract_scene_facts(provider.as_ref(), &job, &known, names).await
                    }
                    None => memory::extract_facts(provider.as_ref(), &job, &known)
                        .await
                        .map(|facts| {
                            facts
                                .into_iter()
                                .map(|text| NewFact {
                                    text,
                                    known_by: None,
                                })
                                .collect()
                        }),
                };
                match found {
                    Ok(facts) if !facts.is_empty() => {
                        let _ = requests.send(Request::Usage {
                            session,
                            tokens: facts
                                .iter()
                                .map(|f| context::estimate_tokens(&f.text))
                                .sum(),
                        });
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
    use masquerole_core::client::TextStream;
    use masquerole_core::settings::MemoryStore;

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

        /// Like [`new`](Self::new), but `complete` answers with `reply`.
        fn with_reply(chunks: &[&'static str], reply: &str) -> Arc<Self> {
            Arc::new(Self {
                reply: reply.into(),
                ..Arc::try_unwrap(Self::new(chunks)).ok().expect("fresh mock")
            })
        }

        /// The prompts sent for `speaker`'s answers, oldest first.
        fn prompts_for(&self, speaker: &str) -> Vec<Vec<ChatMessage>> {
            let start = if speaker.is_empty() {
                "You are the game master".to_string()
            } else {
                format!("You are {speaker},")
            };
            self.seen
                .lock()
                .unwrap()
                .iter()
                .filter(|m| m.first().is_some_and(|s| s.content.starts_with(&start)))
                .cloned()
                .collect()
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

    const ORRIN: &str = r#"
name: "Orrin Hale"
description: "A quiet hunter."
personality: [guarded]
background: "Secretly burned the old mill."
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

    /// A small valid PNG, as the picker would return a chosen file.
    fn small_png() -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 200, 90, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    /// A picker that always "chooses" these bytes.
    fn picks(bytes: &[u8]) -> ImagePicker {
        let bytes = bytes.to_vec();
        Arc::new(move || {
            let bytes = bytes.clone();
            Box::pin(async move { Some(bytes) })
        })
    }

    /// Card dialogs whose user always cancels.
    fn no_cards() -> CardFiles {
        CardFiles {
            open: Arc::new(|| Box::pin(async { None })),
            save: Arc::new(|_| Box::pin(async { None })),
        }
    }

    /// Card dialogs that "open" `bytes` and "save" to `target`, remembering the file name
    /// they were asked to suggest.
    fn cards_with(
        bytes: Option<Vec<u8>>,
        target: Option<PathBuf>,
        suggested: Arc<std::sync::Mutex<Option<String>>>,
    ) -> CardFiles {
        CardFiles {
            open: Arc::new(move || {
                let bytes = bytes.clone();
                Box::pin(async move { bytes })
            }),
            save: Arc::new(move |name| {
                *suggested.lock().unwrap() = Some(name);
                let target = target.clone();
                Box::pin(async move { target })
            }),
        }
    }

    /// A picker whose user always cancels.
    fn cancels() -> ImagePicker {
        Arc::new(|| Box::pin(async { None }))
    }

    impl Rig {
        fn start(name: &str, mock: Arc<Mock>, with_key: bool, settings_file: bool) -> Self {
            Self::start_with_picker(name, mock, with_key, settings_file, picks(&small_png()))
        }

        fn start_with_picker(
            name: &str,
            mock: Arc<Mock>,
            with_key: bool,
            settings_file: bool,
            picker: ImagePicker,
        ) -> Self {
            Self::start_full(name, mock, with_key, settings_file, picker, no_cards())
        }

        fn start_full(
            name: &str,
            mock: Arc<Mock>,
            with_key: bool,
            settings_file: bool,
            picker: ImagePicker,
            cards: CardFiles,
        ) -> Self {
            Self::start_embedding(name, mock, with_key, settings_file, picker, cards, None)
        }

        fn start_embedding(
            name: &str,
            mock: Arc<Mock>,
            with_key: bool,
            settings_file: bool,
            picker: ImagePicker,
            cards: CardFiles,
            embedder: Option<Arc<dyn Embedder>>,
        ) -> Self {
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
                picker,
                cards,
                update_url: None,
                data_dir: None,
                embedder,
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

        /// A story with Mira and Orrin, both present.
        fn start_group(&self) -> Transcript {
            self.backend.send(Request::SaveCharacter {
                path: None,
                character: Character::from_yaml(ORRIN).unwrap(),
            });
            self.start_story();
            self.backend.send(Request::Send("/char Orrin Hale".into()));
            self.wait_for_transcript(|t| t.characters.len() == 2)
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
            Event::Library {
                characters, worlds, ..
            } => Some((characters.len(), worlds.len())),
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
        assert_eq!(prompt[0].role, masquerole_core::client::Role::System);
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
    async fn generations_add_to_the_sessions_token_usage() {
        let rig = Rig::start(
            "usage",
            Mock::new(&["A fairly long reply to count."]),
            true,
            false,
        );
        let before = rig.start_story().session.tokens_used;
        assert_eq!(before, 0, "the greeting is not sent to the provider");
        let t = rig.chat("Tell me something.");
        assert!(t.session.tokens_used > 10, "{}", t.session.tokens_used);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_token_limit_stops_generation_when_on_and_not_when_off() {
        let limited = |enabled| Settings {
            limits: masquerole_core::settings::LimitSettings {
                token_limit_enabled: enabled,
                token_limit: 1,
            },
            ..Settings::default()
        };

        let rig = Rig::start("limit-on", Mock::new(&["First.", "Second."]), true, false);
        rig.start_story();
        rig.backend.send(Request::SaveSettings {
            settings: limited(true),
            api_key: None,
        });
        rig.chat("One."); // under the limit when it starts, so it runs
        rig.backend.send(Request::Send("Two.".into()));
        let error = rig.wait_for(|e| match e {
            Event::Error(m) => Some(m.clone()),
            _ => None,
        });
        assert!(error.contains("token limit"), "{error}");
        assert_eq!(rig.mock.seen.lock().unwrap().len(), 1, "no second request");

        let rig = Rig::start("limit-off", Mock::new(&["First.", "Second."]), true, false);
        rig.start_story();
        rig.backend.send(Request::SaveSettings {
            settings: limited(false),
            api_key: None,
        });
        rig.chat("One.");
        let t = rig.chat("Two.");
        assert_eq!(t.nodes.len(), 5, "the limit is off, so both turns ran");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pictures_are_stored_with_their_card_replaced_and_deleted_with_it() {
        let rig = Rig::start("pictures", Mock::new(&["x"]), true, false);
        let images = rig.dir.0.join("library").join("images");

        rig.backend.send(Request::PickImage(ImageTarget::Character));
        let first = rig.wait_for(|e| match e {
            Event::ImagePicked { name, .. } => Some(name.clone()),
            _ => None,
        });
        assert!(images.join(&first).exists(), "the picture is stored");

        let card = |image: &str| Character {
            image: Some(image.to_string()),
            ..Character::from_yaml("name: Bo").unwrap()
        };
        rig.backend.send(Request::SaveCharacter {
            path: None,
            character: card(&first),
        });
        let characters = rig.wait_for(|e| match e {
            Event::Library { characters, .. } if characters.len() == 2 => Some(characters.clone()),
            _ => None,
        });
        let bo = characters.iter().find(|c| c.item.name == "Bo").unwrap();
        assert_eq!(bo.item.image.as_deref(), Some(first.as_str()));
        let path = bo.path.clone();

        rig.backend.send(Request::PickImage(ImageTarget::Character));
        let second = rig.wait_for(|e| match e {
            Event::ImagePicked { name, .. } => Some(name.clone()),
            _ => None,
        });
        rig.backend.send(Request::SaveCharacter {
            path: Some(path.clone()),
            character: card(&second),
        });
        rig.wait_for(|e| matches!(e, Event::Info(m) if m.contains("Saved")).then_some(()));
        rig.wait_for(|e| matches!(e, Event::Library { .. }).then_some(()));
        assert!(
            !images.join(&first).exists(),
            "the replaced picture is deleted"
        );
        assert!(images.join(&second).exists());

        rig.backend.send(Request::DeleteCharacter(path));
        rig.wait_for(|e| {
            matches!(e, Event::Library { characters, .. } if characters.len() == 1).then_some(())
        });
        assert!(
            !images.join(&second).exists(),
            "deleting the card deletes its picture"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pictures_no_card_uses_are_cleaned_up_at_start() {
        let rig = Rig::start("orphans", Mock::new(&["x"]), true, false);
        rig.wait_for(|e| matches!(e, Event::NoSession).then_some(()));
        let images = rig.dir.0.join("library").join("images");
        std::fs::create_dir_all(&images).unwrap();
        std::fs::write(images.join("orphan.png"), b"x").unwrap();

        rig.backend.send(Request::Init);
        rig.wait_for(|e| matches!(e, Event::NoSession).then_some(()));
        assert!(!images.join("orphan.png").exists());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_file_that_is_not_an_image_is_reported() {
        let rig = Rig::start_with_picker(
            "not-image",
            Mock::new(&["x"]),
            true,
            false,
            picks(b"definitely not an image"),
        );
        rig.backend.send(Request::PickImage(ImageTarget::World));
        let error = rig.wait_for(|e| match e {
            Event::Error(m) => Some(m.clone()),
            _ => None,
        });
        assert!(error.contains("not a supported image"), "{error}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelling_the_picture_dialog_does_nothing() {
        let rig = Rig::start_with_picker("cancel", Mock::new(&["x"]), true, false, cancels());
        rig.backend.send(Request::PickImage(ImageTarget::Character));
        rig.backend.send(Request::DeleteApiKey);
        let mut picked = false;
        rig.wait_for(|e| {
            picked |= matches!(e, Event::ImagePicked { .. });
            matches!(e, Event::Info(m) if m.contains("removed")).then_some(())
        });
        std::thread::sleep(Duration::from_millis(200));
        while let Ok(event) = rig.events.try_recv() {
            picked |= matches!(event, Event::ImagePicked { .. });
        }
        assert!(!picked);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_tutorial_is_remembered_in_the_settings_file() {
        let rig = Rig::start("tutorial", Mock::new(&["x"]), true, true);
        let seen = |rig: &Rig| {
            rig.wait_for(|e| match e {
                Event::Settings { settings, .. } => Some(settings.ui.tutorial_seen),
                _ => None,
            })
        };
        assert!(!seen(&rig));

        rig.backend.send(Request::MarkTutorialSeen);
        rig.backend.send(Request::Init); // reloads settings from the file
        assert!(seen(&rig), "the flag was written to settings.toml");
        let file = std::fs::read_to_string(rig.dir.0.join("settings.toml")).unwrap();
        assert!(file.contains("tutorial_seen = true"), "{file}");
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_character_can_be_taken_out_of_the_story() {
        let rig = Rig::start("remove-character", Mock::new(&["x"]), true, false);
        let story = rig.start_story();
        assert!(!story.characters.is_empty());
        let name = story.characters[0].clone();
        rig.backend.send(Request::RemoveCharacter(name.clone()));
        let after = rig.wait_for_transcript(|t| !t.characters.contains(&name));
        assert_eq!(after.nodes.len(), story.nodes.len(), "the messages stay");
        rig.backend.send(Request::RemoveCharacter(name));
        rig.backend.send(Request::OpenSession(after.session.id));
        rig.wait_for_transcript(|t| t.characters.is_empty());
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
    async fn card_positions_are_saved_sent_with_the_tree_and_can_be_reset() {
        let rig = Rig::start("positions", Mock::new(&["Reply."]), true, false);
        rig.start_story();
        rig.chat("Hello.");
        rig.backend.send(Request::WatchTree(true));
        let data = next_tree(&rig).unwrap();
        assert!(data.positions.is_empty(), "nothing was moved yet");
        let (a, b) = (data.nodes[0].id, data.nodes[1].id);

        rig.backend.send(Request::MoveNode {
            node: a,
            x: 12.5,
            y: -3.0,
        });
        rig.backend.send(Request::MoveNode {
            node: a,
            x: 40.0,
            y: 8.0,
        });
        rig.backend.send(Request::MoveNode {
            node: b,
            x: 1.0,
            y: 2.0,
        });
        // The positions come with the tree the next time it is sent.
        rig.backend.send(Request::WatchTree(true));
        let data = next_tree(&rig).unwrap();
        assert_eq!(data.positions.get(&a), Some(&(40.0, 8.0)));
        assert_eq!(data.positions.get(&b), Some(&(1.0, 2.0)));

        rig.backend.send(Request::ResetTreeLayout);
        let data = next_tree(&rig).unwrap();
        assert!(data.positions.is_empty(), "tidying forgets them all");
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compact_summarizes_all_but_the_newest_two_messages() {
        let rig = Rig::start("compact", Mock::new(&["Reply."]), true, false);
        rig.start_story();
        rig.chat("One.");
        rig.chat("Two.");
        let t = rig.chat("Three.");
        assert_eq!(t.nodes.len(), 7);
        rig.backend.send(Request::Send("/compact".into()));
        let info = rig.wait_for(|e| match e {
            Event::Info(m) => Some(m.clone()),
            Event::Error(e) => panic!("unexpected error: {e}"),
            _ => None,
        });
        assert_eq!(info, "Compacted 5 messages.");
        // The summary is stored at the fifth message, so the last two stay untouched.
        rig.backend.send(Request::Send("/summary".into()));
        let notice = rig.wait_for(|e| match e {
            Event::Notice(n) if n.text.contains("Mira likes maps") => Some(n.text.clone()),
            _ => None,
        });
        assert!(notice.contains("Mira likes maps"));
    }

    fn bo() -> Character {
        Character {
            name: "Bo".into(),
            personality: vec!["shy".into()],
            ..Character::default()
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn improving_a_text_answers_with_the_tidied_reply() {
        let mock = Mock::with_reply(
            &["unused"],
            "- brave
- kind",
        );
        let rig = Rig::start("improve", mock, true, false);
        rig.backend.send(Request::ImproveCharacter {
            field: improve::Field::Personality,
            character: bo(),
        });
        let (field, text) = rig.wait_for(|e| match e {
            Event::Improved { field, text } => Some((*field, text.clone())),
            _ => None,
        });
        assert_eq!(field, improve::Field::Personality);
        assert_eq!(
            text.as_deref(),
            Some(
                "brave
kind"
            )
        );
        let seen = rig.mock.seen.lock().unwrap();
        assert!(seen.last().unwrap()[1].content.contains("Name: Bo"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn improving_without_a_key_says_so_and_still_answers() {
        let rig = Rig::start("improve-no-key", Mock::new(&["unused"]), false, false);
        let mut settings = Settings::default();
        settings.api.provider = "xai".into();
        rig.backend.send(Request::SaveSettings {
            settings,
            api_key: None,
        });
        rig.backend.send(Request::ImproveCharacter {
            field: improve::Field::Greeting,
            character: bo(),
        });
        let text = rig.wait_for(|e| match e {
            Event::Improved { text, .. } => Some(text.clone()),
            _ => None,
        });
        assert_eq!(text, None, "the editor stops waiting");
        let error = rig.wait_for(|e| match e {
            Event::Error(m) => Some(m.clone()),
            _ => None,
        });
        assert!(error.contains("No API key"), "{error}");
        assert!(rig.mock.seen.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compact_on_a_short_story_does_nothing_but_say_so() {
        let rig = Rig::start("compact-short", Mock::new(&["Reply."]), true, false);
        rig.start_story();
        rig.backend.send(Request::Send("/compact".into()));
        let notice = rig.wait_for(|e| match e {
            Event::Notice(n) => Some(n.text.clone()),
            _ => None,
        });
        assert!(notice.contains("nothing to compact"), "{notice}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn branching_from_a_message_starts_a_new_storyline_listed_as_a_branch() {
        let rig = Rig::start("branch-from", Mock::new(&["Reply."]), true, false);
        let first = rig.start_story().nodes[0].id;
        let t = rig.chat("Hello.");
        assert_eq!(t.branches.len(), 1);

        rig.backend.send(Request::BranchFrom(first));
        let notice = rig.wait_for(|e| match e {
            Event::Notice(n) => Some(n.text.clone()),
            _ => None,
        });
        assert!(notice.contains("new storyline"), "{notice}");
        let t = rig.chat("Another path.");
        assert_eq!(t.branches.len(), 2, "the old storyline and the new one");
        assert_eq!(t.nodes.len(), 3);
    }

    fn text_of(messages: &[ChatMessage]) -> String {
        messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_character_who_is_called_answers_from_a_prompt_that_is_only_theirs() {
        let rig = Rig::start("group-call", Mock::new(&["I saw nothing."]), true, false);
        rig.start_group();
        let t = rig.chat("Orrin Hale, what did you see?");
        let last = t.nodes.last().unwrap();
        assert_eq!(
            (last.kind, last.speaker.as_deref(), last.content.as_str()),
            (NodeKind::Character, Some("Orrin Hale"), "I saw nothing.")
        );
        assert_eq!(t.present, ["Mira Valen", "Orrin Hale"]);
        assert_eq!(t.floor, ["Orrin Hale"]);

        let prompts = rig.mock.prompts_for("Orrin Hale");
        assert_eq!(prompts.len(), 1);
        let system = &prompts[0][0].content;
        assert!(
            system.contains("Secretly burned the old mill"),
            "his own card"
        );
        assert!(
            system.contains("Mira Valen: A cartographer"),
            "others from outside"
        );
        assert!(
            !system.contains("sarcastic"),
            "nothing of Mira's inner life"
        );
        assert!(!system.contains("Dry, short sentences"));
        assert!(
            rig.mock.prompts_for("Mira Valen").is_empty(),
            "she was not asked"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_conversation_stays_with_whoever_has_the_floor() {
        let rig = Rig::start("group-floor", Mock::new(&["Hm."]), true, false);
        rig.start_group();
        rig.chat("@Orrin Hale hello");
        let t = rig.chat("And what about the lamp?");
        let speakers: Vec<_> = t
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Character)
            .map(|n| n.speaker.as_deref().unwrap())
            .collect();
        assert_eq!(speakers, ["Mira Valen", "Orrin Hale", "Orrin Hale"]);
        // Calling the other one by name hands the floor over.
        let t = rig.chat("Mira, what do you think?");
        assert_eq!(
            t.nodes.last().unwrap().speaker.as_deref(),
            Some("Mira Valen")
        );
        assert_eq!(t.floor, ["Mira Valen"]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn everyone_answers_in_turn_and_later_voices_hear_the_earlier_ones() {
        let rig = Rig::start("group-all", Mock::new(&["Yes."]), true, false);
        rig.start_group();
        let t = rig.chat("Everyone, did you hear that?");
        let answers: Vec<_> = t.nodes[t.nodes.len() - 2..]
            .iter()
            .map(|n| (n.kind, n.speaker.as_deref().unwrap()))
            .collect();
        assert_eq!(
            answers,
            [
                (NodeKind::Character, "Mira Valen"),
                (NodeKind::Character, "Orrin Hale")
            ]
        );
        let orrin = &rig.mock.prompts_for("Orrin Hale")[0];
        assert!(
            text_of(orrin).contains("Mira Valen: Yes."),
            "he heard her answer first"
        );
        assert_eq!(
            orrin.last().unwrap().role,
            masquerole_core::client::Role::User
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_whisper_is_heard_by_one_character_only() {
        let rig = Rig::start("group-whisper", Mock::new(&["Understood."]), true, false);
        rig.start_group();
        let t = rig.chat("/whisper Mira Valen: the vault code is 1234");
        assert_eq!(
            t.nodes.last().unwrap().speaker.as_deref(),
            Some("Mira Valen")
        );

        rig.chat("Orrin Hale, what do you know?");
        let orrin = &rig.mock.prompts_for("Orrin Hale")[0];
        assert!(!text_of(orrin).contains("1234"), "he never heard it");
        assert!(!text_of(orrin).contains("Understood"), "nor her answer");

        rig.chat("Mira Valen, remind me.");
        let mira = rig.mock.prompts_for("Mira Valen");
        assert!(
            text_of(mira.last().unwrap()).contains("1234"),
            "she remembers"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn someone_who_left_neither_answers_nor_hears_what_follows() {
        let rig = Rig::start("group-leave", Mock::new(&["Gone."]), true, false);
        rig.start_group();
        rig.backend.send(Request::Send("/leave Orrin Hale".into()));
        let t = rig.wait_for_transcript(|t| t.present.len() == 1);
        assert_eq!(t.present, ["Mira Valen"]);

        let t = rig.chat("Is anyone there?");
        assert_eq!(
            t.nodes.last().unwrap().speaker.as_deref(),
            Some("Mira Valen")
        );
        // Asking for the one who left reaches nobody: the only one here answers.
        let t = rig.chat("Orrin Hale, come back!");
        assert_eq!(
            t.nodes.last().unwrap().speaker.as_deref(),
            Some("Mira Valen")
        );

        rig.backend.send(Request::Send("/enter Orrin Hale".into()));
        rig.wait_for_transcript(|t| t.present.len() == 2);
        rig.chat("Orrin Hale, what did I miss?");
        let orrin = &rig.mock.prompts_for("Orrin Hale")[0];
        assert!(!text_of(orrin).contains("Is anyone there"), "he was away");
        assert!(text_of(orrin).contains("what did I miss"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn facts_reach_only_the_characters_who_know_them() {
        let rig = Rig::start(
            "group-facts",
            Mock::with_reply(
                &["Noted."],
                "- The vault code is 1234. [known by: Mira Valen]",
            ),
            true,
            false,
        );
        rig.start_group();
        rig.chat("Mira Valen, the vault code is 1234.");

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            rig.backend.send(Request::Send("/memory".into()));
            let text = rig.wait_for(|e| match e {
                Event::Notice(n) => Some(n.text.clone()),
                _ => None,
            });
            if text.contains("vault code") {
                break;
            }
            assert!(Instant::now() < deadline, "facts never arrived: {text}");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        rig.chat("Orrin Hale, what is the code?");
        // The player said it out loud, so it is in the conversation Orrin heard. What the
        // story remembers as a fact is only for Mira.
        let orrin = &rig.mock.prompts_for("Orrin Hale")[0][0].content;
        assert!(!orrin.contains("Known facts"), "{orrin}");
        rig.chat("Mira Valen, what is the code?");
        let mira = rig.mock.prompts_for("Mira Valen");
        let system = &mira.last().unwrap()[0].content;
        assert!(system.contains("- The vault code is 1234."), "{system}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn retry_asks_the_same_character_again_and_narrator_mode_keeps_one_voice() {
        let rig = Rig::start("group-retry", Mock::new(&["Hm."]), true, false);
        rig.start_group();
        rig.chat("Orrin Hale, speak.");
        let t = rig.chat("/retry");
        let last = t.nodes.last().unwrap();
        assert_eq!(last.speaker.as_deref(), Some("Orrin Hale"));
        assert_eq!(rig.mock.prompts_for("Orrin Hale").len(), 2);

        let mut settings = Settings::default();
        settings.style.narrator_mode = true;
        rig.backend.send(Request::SaveSettings {
            settings,
            api_key: None,
        });
        let t = rig.chat("Everyone, speak.");
        let last = t.nodes.last().unwrap();
        assert_eq!(
            (last.kind, last.speaker.as_deref()),
            (NodeKind::Narrator, None)
        );
        assert_eq!(
            rig.mock.prompts_for("").len(),
            1,
            "one narrator for the whole scene"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_next_character_takes_over_before_its_first_word_and_after_the_last_is_stored() {
        let rig = Rig::start("group-stream", Mock::new(&["Yes."]), true, false);
        rig.start_group();
        rig.backend
            .send(Request::Send("Everyone, did you hear that?".into()));
        let mut log: Vec<String> = Vec::new();
        rig.wait_for(|e| {
            match e {
                Event::Transcript(t) => log.push(format!(
                    "stored:{}",
                    t.nodes
                        .last()
                        .and_then(|n| n.speaker.as_deref())
                        .unwrap_or("-")
                )),
                Event::Speaker(name) => {
                    log.push(format!("speaker:{}", name.as_deref().unwrap_or("-")))
                }
                Event::Chunk(_) => log.push("chunk".into()),
                Event::Error(e) => panic!("{e}"),
                _ => {}
            }
            matches!(e, Event::Generating(false)).then_some(())
        });
        let stored = log.iter().position(|l| l == "stored:Mira Valen").unwrap();
        let next = log[stored + 1..]
            .iter()
            .find(|l| l.starts_with("speaker:") || *l == "chunk")
            .unwrap();
        assert_eq!(next, "speaker:Orrin Hale", "{log:?}");
        let orrin_first = log.iter().position(|l| l == "speaker:Orrin Hale").unwrap();
        let chunks_between = log[stored..orrin_first]
            .iter()
            .filter(|l| *l == "chunk")
            .count();
        assert_eq!(chunks_between, 0, "{log:?}");
    }

    fn mira_path(rig: &Rig) -> PathBuf {
        let (characters, _) = Library::new(rig.dir.0.join("library")).characters();
        characters
            .into_iter()
            .find(|c| c.item.name == "Mira Valen")
            .unwrap()
            .path
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn importing_a_png_card_adds_the_character_with_its_picture() {
        let card = Character {
            name: "Seraphine".into(),
            greeting: "Halt!".into(),
            ..Character::default()
        };
        let png = exchange::to_png(&card, &small_png()).unwrap();
        let cards = cards_with(Some(png), None, Default::default());
        let rig = Rig::start_full(
            "import-png",
            Mock::new(&[]),
            false,
            false,
            picks(&small_png()),
            cards,
        );
        rig.backend.send(Request::ImportCharacter);
        let info = rig.wait_for(|e| match e {
            Event::Info(m) if m.starts_with("Imported") => Some(m.clone()),
            Event::Error(e) => panic!("{e}"),
            _ => None,
        });
        assert_eq!(info, "Imported Seraphine.");
        let library = rig.wait_for(|e| match e {
            Event::Library { characters, .. } if characters.len() == 2 => Some(characters.clone()),
            _ => None,
        });
        let seraphine = library.iter().find(|c| c.item.name == "Seraphine").unwrap();
        assert_eq!(seraphine.item.greeting, "Halt!");
        let image = seraphine.item.image.clone().unwrap();
        let stored = Library::new(rig.dir.0.join("library"))
            .image_path(&image)
            .unwrap();
        assert!(stored.is_file());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn importing_reports_what_was_left_out_and_what_went_wrong() {
        let json = r#"{"spec":"chara_card_v2","data":{"name":"Bo","character_book":{"entries":[{"keys":["a"],"content":"b"}]}}}"#;
        let cards = cards_with(Some(json.as_bytes().to_vec()), None, Default::default());
        let rig = Rig::start_full(
            "import-json",
            Mock::new(&[]),
            false,
            false,
            picks(&small_png()),
            cards,
        );
        rig.backend.send(Request::ImportCharacter);
        let info = rig.wait_for(|e| match e {
            Event::Info(m) if m.starts_with("Imported") => Some(m.clone()),
            _ => None,
        });
        assert_eq!(info, "Imported Bo. Left out: its lorebook.");

        let cards = cards_with(Some(b"{ not json".to_vec()), None, Default::default());
        let rig = Rig::start_full(
            "import-bad",
            Mock::new(&[]),
            false,
            false,
            picks(&small_png()),
            cards,
        );
        rig.backend.send(Request::ImportCharacter);
        let error = rig.wait_for(|e| match e {
            Event::Error(m) => Some(m.clone()),
            _ => None,
        });
        assert!(error.starts_with("Could not import"), "{error}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_cancelled_import_changes_nothing() {
        let rig = Rig::start("import-cancel", Mock::new(&[]), false, false);
        rig.backend.send(Request::ImportCharacter);
        rig.backend.send(Request::DeleteApiKey); // a request after it, to know it was handled
        rig.wait_for(|e| matches!(e, Event::Info(_)).then_some(()));
        let (characters, _) = Library::new(rig.dir.0.join("library")).characters();
        assert_eq!(characters.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn exporting_writes_the_format_the_file_name_asks_for() {
        let dir = TempDir::new("export-target");
        std::fs::create_dir_all(&dir.0).unwrap();
        for (file, check) in [
            ("mira.json", "json"),
            ("mira.yaml", "yaml"),
            ("mira.png", "png"),
            ("mira", "png"),
        ] {
            let suggested = Arc::new(std::sync::Mutex::new(None));
            let cards = cards_with(None, Some(dir.0.join(file)), suggested.clone());
            let rig = Rig::start_full(
                "export",
                Mock::new(&[]),
                false,
                false,
                picks(&small_png()),
                cards,
            );
            rig.backend.send(Request::ExportCharacter(mira_path(&rig)));
            let info = rig.wait_for(|e| match e {
                Event::Info(m) if m.starts_with("Exported") => Some(m.clone()),
                Event::Error(e) => panic!("{e}"),
                _ => None,
            });
            assert!(info.starts_with("Exported Mira Valen to "), "{info}");
            assert_eq!(suggested.lock().unwrap().as_deref(), Some("Mira_Valen.png"));

            let written = if file == "mira" {
                dir.0.join("mira.png")
            } else {
                dir.0.join(file)
            };
            let bytes = std::fs::read(&written).unwrap();
            let back = exchange::import(&bytes).unwrap();
            assert_eq!(back.character.name, "Mira Valen", "{check}");
            assert_eq!(back.picture.is_some(), check == "png");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn exporting_to_an_unknown_extension_is_refused() {
        let dir = TempDir::new("export-bad");
        std::fs::create_dir_all(&dir.0).unwrap();
        let target = dir.0.join("mira.txt");
        let cards = cards_with(None, Some(target.clone()), Default::default());
        let rig = Rig::start_full(
            "export-bad-ext",
            Mock::new(&[]),
            false,
            false,
            picks(&small_png()),
            cards,
        );
        rig.backend.send(Request::ExportCharacter(mira_path(&rig)));
        let error = rig.wait_for(|e| match e {
            Event::Error(m) => Some(m.clone()),
            _ => None,
        });
        assert!(error.contains(".png, .json, or .yaml"), "{error}");
        assert!(!target.exists());
        // An unknown character cannot be exported.
        rig.backend
            .send(Request::ExportCharacter(PathBuf::from("nobody.yaml")));
        rig.wait_for(|e| matches!(e, Event::Error(m) if m.contains("no longer")).then_some(()));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn semantic_memory_brings_back_an_old_message_that_fits_the_new_one() {
        let rig = Rig::start_embedding(
            "semantic",
            Mock::new(&["Yes."]),
            true,
            false,
            picks(&small_png()),
            no_cards(),
            Some(Arc::new(masquerole_core::embed::HashEmbedder)),
        );
        let mut settings = Settings::default();
        settings.context.recent_messages = 2;
        settings.memory.semantic = true;
        rig.backend.send(Request::SaveSettings {
            settings,
            api_key: None,
        });
        rig.start_story();
        rig.chat("I ask where the silver key is hidden.");
        rig.chat("We eat bread by the fire.");
        rig.chat("The rain stops at dawn.");
        rig.chat("Where is the silver key again?");
        let last = rig
            .mock
            .seen
            .lock()
            .unwrap()
            .iter()
            .rfind(|m| m[0].content.contains("## Characters"))
            .cloned()
            .unwrap();
        let system = &last[0].content;
        assert!(system.contains("## Remembered from earlier"), "{system}");
        assert!(
            system.contains("Player: I ask where the silver key is hidden."),
            "{system}"
        );
        assert!(!system.contains("bread"), "{system}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn without_the_setting_nothing_is_recalled() {
        let rig = Rig::start_embedding(
            "semantic-off",
            Mock::new(&["Yes."]),
            true,
            false,
            picks(&small_png()),
            no_cards(),
            Some(Arc::new(masquerole_core::embed::HashEmbedder)),
        );
        let mut settings = Settings::default();
        settings.context.recent_messages = 2;
        rig.backend.send(Request::SaveSettings {
            settings,
            api_key: None,
        });
        rig.start_story();
        rig.chat("I ask where the silver key is hidden.");
        rig.chat("We eat bread by the fire.");
        rig.chat("The rain stops at dawn.");
        rig.chat("Where is the silver key again?");
        let last = rig
            .mock
            .seen
            .lock()
            .unwrap()
            .iter()
            .rfind(|m| m[0].content.contains("## Characters"))
            .cloned()
            .unwrap();
        assert!(!last[0].content.contains("Remembered from earlier"));
    }
}
