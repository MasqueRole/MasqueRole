//! The application: window layout, sections, and the event loop.
//!
//! The top navigation picks a section (stories, characters, worlds, or the story tree).
//! Each section has its own sidebar list and main area. Settings open as an overlay.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui::{Align, ColorImage, Context, Layout, TextureHandle, TextureOptions, Ui};
use ir_core::cards::Card as _;
use ir_core::character::Character;
use ir_core::commands;
use ir_core::db::{NodeId, Session, SessionId};
use ir_core::lorebook::Lorebook;
use ir_core::settings::Settings;
use ir_ui::components::{
    Button, ButtonSize, ButtonVariant, Card, Dialog, FormField, IconKind, Input, Logo, Panel,
    ScrollArea, Select, Tabs, Toast, Toasts, TooltipExt, TopBar,
};
use ir_ui::{Theme, ThemeMode};

use crate::backend::{Backend, Event, ImageTarget, Request, Transcript, TreeData};
use crate::characters::{CharacterForm, WorldForm};
use crate::format::Notice;
use crate::library::Entry;
use crate::settings_ui::{SettingsAction, SettingsForm};
use crate::tree_view::TreeView;
use crate::tutorial::Tutorial;
use crate::{branding, chat, sidebar};

/// Width of the sidebar.
const SIDEBAR_WIDTH: f32 = 260.0;

/// Width of the logo in the top bar.
const LOGO_WIDTH: f32 = 44.0;

/// Width of the settings overlay.
const SETTINGS_WIDTH: f32 = 600.0;

/// The settings overlay is at least this tall, so switching tabs does not make it jump.
const SETTINGS_MIN_HEIGHT: f32 = 380.0;

/// The settings overlay never grows beyond this share of the window height.
const SETTINGS_MAX_HEIGHT_SHARE: f32 = 0.75;

/// The name a new session starts with.
const DEFAULT_SESSION_NAME: &str = "New story";

/// A part of the app, chosen in the top navigation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    /// Sessions and the story being told.
    Stories,
    /// The character library and its editor.
    Characters,
    /// The world library and its editor.
    Worlds,
    /// The tree of the open story.
    Tree,
}

impl Section {
    /// Every section, in navigation order.
    pub const ALL: [Section; 4] = [
        Section::Stories,
        Section::Characters,
        Section::Worlds,
        Section::Tree,
    ];

    /// The icon in the top navigation.
    pub fn icon(self) -> IconKind {
        match self {
            Self::Stories => IconKind::Chat,
            Self::Characters => IconKind::User,
            Self::Worlds => IconKind::Globe,
            Self::Tree => IconKind::Branch,
        }
    }

    /// The label in the top navigation.
    pub fn label(self) -> &'static str {
        match self {
            Self::Stories => "Stories",
            Self::Characters => "Characters",
            Self::Worlds => "Worlds",
            Self::Tree => "Tree",
        }
    }
}

/// Something the user must confirm before it happens.
#[derive(Clone, Debug, PartialEq)]
pub enum Confirm {
    /// Delete a session and its whole story.
    DeleteSession { id: SessionId, name: String },
    /// Delete a character file.
    DeleteCharacter { path: PathBuf, name: String },
    /// Delete a world file.
    DeleteWorld { path: PathBuf, name: String },
    /// Run a destructive slash command.
    Command(String),
}

impl Confirm {
    /// The dialog title, explanation, and label of the confirming button.
    fn texts(&self) -> (String, String, &'static str) {
        match self {
            Self::DeleteSession { name, .. } => (
                format!("Delete \u{201c}{name}\u{201d}?"),
                "The session and its whole story tree will be deleted. This cannot be undone.".into(),
                "Delete",
            ),
            Self::DeleteCharacter { name, .. } => (
                format!("Delete {name}?"),
                "The character card file will be deleted. Sessions that already use it keep their copy.".into(),
                "Delete",
            ),
            Self::DeleteWorld { name, .. } => (
                format!("Delete {name}?"),
                "The world file will be deleted. Sessions that use it will no longer see its lore.".into(),
                "Delete",
            ),
            Self::Command(text) => (
                "Run this command?".into(),
                format!("{text}\n\nThis deletes story nodes and cannot be undone."),
                "Run",
            ),
        }
    }
}

/// The state of the chat input.
#[derive(Debug, Default)]
pub struct ChatState {
    /// What is typed.
    pub input: String,
    /// Whether a response is being generated.
    pub generating: bool,
    /// The text of the response so far.
    pub streaming: String,
    /// The id of the input field, once it has been drawn.
    pub input_id: Option<eframe::egui::Id>,
    /// Whether the pointer was over the command suggestions last frame.
    pub suggestions_hovered: bool,
}

/// The new-session dialog.
#[derive(Debug)]
pub struct NewSessionForm {
    name: String,
    /// Index into the character options: 0 is "No character".
    character: usize,
    /// Index into the world options: 0 is "No world".
    world: usize,
}

/// What an editor's footer buttons asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormAction {
    Save,
    Cancel,
    Delete,
}

/// A card telling the user what to do next in an empty main area. Returns whether its
/// button, if it has one, was clicked.
fn empty_state(ui: &mut Ui, title: &str, description: &str, button: Option<&str>) -> bool {
    let mut clicked = false;
    let mut card = Card::new().header(|ui| {
        Card::title(ui, title);
        Card::description(ui, description);
    });
    if let Some(label) = button {
        let clicked = &mut clicked;
        card = card.footer(move |ui| {
            *clicked = ui.add(Button::new(label).icon(IconKind::Plus)).clicked();
        });
    }
    card.show(ui);
    clicked
}

/// The whole application.
pub struct App {
    /// Sends requests to the backend.
    pub backend: Backend,
    events: mpsc::Receiver<Event>,
    /// Keeps the async runtime alive for as long as the app runs.
    _runtime: Option<tokio::runtime::Runtime>,
    /// Pending notifications.
    pub toasts: Toasts,
    /// The section chosen in the top navigation.
    pub section: Section,
    theme_mode: ThemeMode,
    /// The current settings.
    pub settings: Settings,
    /// Whether an API key is available for the current provider.
    pub key_available: bool,
    /// The settings overlay, when open.
    pub settings_form: Option<SettingsForm>,
    /// The tutorial, when open.
    pub tutorial: Option<Tutorial>,
    /// Whether the first-run tutorial was already offered during this run.
    tutorial_offered: bool,
    /// All sessions.
    pub sessions: Vec<Session>,
    /// The open story, if any.
    pub transcript: Option<Transcript>,
    /// Messages from commands, shown after the story.
    pub notices: Vec<Notice>,
    /// The character library.
    pub characters: Vec<Entry<Character>>,
    /// The world library.
    pub worlds: Vec<Entry<Lorebook>>,
    /// The folder holding the pictures of characters and worlds.
    images_dir: Option<PathBuf>,
    /// Pictures already loaded as textures, by file name. `None` marks one that could not be read,
    /// so it is not retried every frame.
    pictures: HashMap<String, Option<TextureHandle>>,
    /// The character being edited, if any.
    pub character_form: Option<CharacterForm>,
    /// The world being edited, if any.
    pub world_form: Option<WorldForm>,
    /// The chat input state.
    pub chat: ChatState,
    /// The new-session dialog, when open.
    pub new_session: Option<NewSessionForm>,
    /// A pending confirmation.
    pub confirm: Option<Confirm>,
    /// The whole story tree, sent by the backend while the tree section is open.
    pub tree: Option<TreeData>,
    /// View state of the tree section.
    tree_view: TreeView,
    /// Counts the tree updates, so the tree view knows when to rebuild its layout.
    tree_version: u64,
    /// Whether the backend has been told to send the tree.
    tree_watching: bool,
    logo: Option<TextureHandle>,
    logo_tried: bool,
}

impl App {
    /// An app talking to `backend` and listening for its `events`. `runtime` is kept alive
    /// for the app's lifetime.
    pub fn new(
        backend: Backend,
        events: mpsc::Receiver<Event>,
        runtime: Option<tokio::runtime::Runtime>,
    ) -> Self {
        Self {
            backend,
            events,
            _runtime: runtime,
            toasts: Toasts::new(),
            section: Section::Stories,
            theme_mode: ThemeMode::System,
            settings: Settings::default(),
            key_available: false,
            settings_form: None,
            tutorial: None,
            tutorial_offered: false,
            sessions: Vec::new(),
            transcript: None,
            notices: Vec::new(),
            characters: Vec::new(),
            worlds: Vec::new(),
            images_dir: None,
            pictures: HashMap::new(),
            character_form: None,
            world_form: None,
            chat: ChatState::default(),
            new_session: None,
            confirm: None,
            tree: None,
            tree_view: TreeView::new(),
            tree_version: 0,
            tree_watching: false,
            logo: None,
            logo_tried: false,
        }
    }

    // ---- events ----

    /// Apply everything the backend has sent since the last frame.
    fn poll(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.handle_event(event);
        }
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Settings {
                settings,
                key_available,
            } => {
                self.settings = settings;
                self.key_available = key_available;
                // The very first start shows the tutorial, once.
                if !self.settings.ui.tutorial_seen && !self.tutorial_offered {
                    self.tutorial_offered = true;
                    self.open_tutorial();
                }
            }
            Event::Sessions(sessions) => self.sessions = sessions,
            Event::Tree(data) => {
                self.tree = data;
                self.tree_version += 1;
            }
            Event::Library {
                characters,
                worlds,
                images_dir,
            } => {
                self.characters = characters;
                self.worlds = worlds;
                self.images_dir = Some(images_dir);
                // A saved or deleted card may have changed its picture; reload lazily.
                self.pictures.clear();
            }
            Event::ImagePicked { target, name } => match target {
                ImageTarget::Character => {
                    if let Some(form) = &mut self.character_form {
                        form.image = Some(name);
                    }
                }
                ImageTarget::World => {
                    if let Some(form) = &mut self.world_form {
                        form.image = Some(name);
                    }
                }
            },
            Event::Transcript(transcript) => {
                let same = self
                    .transcript
                    .as_ref()
                    .is_some_and(|t| t.session.id == transcript.session.id);
                if !same {
                    self.notices.clear();
                    self.chat.streaming.clear();
                }
                self.transcript = Some(transcript);
            }
            Event::NoSession => {
                self.transcript = None;
                self.notices.clear();
            }
            Event::Notice(notice) => self.notices.push(notice),
            Event::Chunk(text) => self.chat.streaming.push_str(&text),
            Event::Generating(on) => {
                self.chat.generating = on;
                if !on {
                    self.chat.streaming.clear();
                }
            }
            Event::Info(message) => self.toasts.push(Toast::new(message)),
            Event::Error(message) => self.toasts.push(Toast::destructive(message)),
        }
    }

    // ---- actions ----

    /// Open the new-session dialog, pre-selecting the first character.
    pub fn open_new_session(&mut self) {
        self.new_session = Some(NewSessionForm {
            name: DEFAULT_SESSION_NAME.to_string(),
            character: usize::from(!self.characters.is_empty()),
            world: 0,
        });
    }

    /// The texture of a stored picture, loaded on first use. `None` when there is no such
    /// picture or it cannot be read.
    fn picture(&mut self, ctx: &Context, name: &str) -> Option<TextureHandle> {
        if let Some(cached) = self.pictures.get(name) {
            return cached.clone();
        }
        let loaded = self.load_picture(ctx, name);
        self.pictures.insert(name.to_string(), loaded.clone());
        loaded
    }

    fn load_picture(&self, ctx: &Context, name: &str) -> Option<TextureHandle> {
        // The name comes from a card file the user can edit, so it must not leave the folder.
        if !crate::library::is_plain_file_name(name) {
            return None;
        }
        let bytes = std::fs::read(self.images_dir.as_ref()?.join(name)).ok()?;
        let picture = eframe::icon_data::from_png_bytes(&bytes).ok()?;
        let size = [picture.width as usize, picture.height as usize];
        let image = ColorImage::from_rgba_unmultiplied(size, &picture.rgba);
        Some(ctx.load_texture(format!("picture-{name}"), image, TextureOptions::LINEAR))
    }

    /// Open the tutorial at its first page.
    pub fn open_tutorial(&mut self) {
        self.tutorial = Some(Tutorial::new());
    }

    /// Close the tutorial and remember that it has been seen.
    fn close_tutorial(&mut self) {
        self.tutorial = None;
        if !self.settings.ui.tutorial_seen {
            self.settings.ui.tutorial_seen = true;
            self.backend.send(Request::MarkTutorialSeen);
        }
    }

    /// Open the settings overlay.
    pub fn open_settings(&mut self) {
        let mut form = SettingsForm::new(&self.settings);
        form.theme = self.theme_mode;
        self.settings_form = Some(form);
    }

    /// Switch to `section`. The tree starts with a fresh view every time it is opened.
    pub fn show_section(&mut self, section: Section) {
        if section == Section::Tree && self.section != Section::Tree {
            self.tree_view = TreeView::new();
        }
        self.section = section;
    }

    /// Send what is typed in the chat input. Commands that delete data wait for confirmation.
    pub fn send_chat(&mut self) {
        let text = std::mem::take(&mut self.chat.input);
        if text.trim().is_empty() {
            return;
        }
        let needs_confirmation = matches!(
            commands::parse(&text),
            Ok(Some(command)) if command.needs_confirmation()
        );
        if needs_confirmation {
            self.confirm = Some(Confirm::Command(text.trim().to_string()));
        } else {
            self.backend.send(Request::Send(text));
        }
    }

    /// Move the story to a node chosen in the tree, and go back to the story to continue from it.
    fn jump_to(&mut self, node: NodeId) {
        self.backend.send(Request::JumpTo(node));
        self.show_section(Section::Stories);
    }

    fn apply_confirm(&mut self, confirm: Confirm) {
        match confirm {
            Confirm::DeleteSession { id, .. } => self.backend.send(Request::DeleteSession(id)),
            Confirm::DeleteCharacter { path, .. } => {
                if self
                    .character_form
                    .as_ref()
                    .is_some_and(|f| f.path.as_ref() == Some(&path))
                {
                    self.character_form = None;
                }
                self.backend.send(Request::DeleteCharacter(path));
            }
            Confirm::DeleteWorld { path, .. } => {
                if self
                    .world_form
                    .as_ref()
                    .is_some_and(|f| f.path.as_ref() == Some(&path))
                {
                    self.world_form = None;
                }
                self.backend.send(Request::DeleteWorld(path));
            }
            Confirm::Command(text) => self.backend.send(Request::Send(text)),
        }
    }

    /// Send the settings being edited, with a newly typed key, and close the overlay.
    fn save_settings(&mut self) {
        if let Some(form) = self.settings_form.take() {
            self.backend.send(Request::SaveSettings {
                settings: form.to_settings(),
                api_key: form.new_api_key(),
            });
        }
    }

    /// Act on the footer buttons of the editor in the current section.
    fn form_action(&mut self, action: FormAction) {
        match (self.section, action) {
            (Section::Characters, FormAction::Cancel) => self.character_form = None,
            (Section::Worlds, FormAction::Cancel) => self.world_form = None,
            (Section::Characters, FormAction::Save) => {
                let Some(form) = &mut self.character_form else {
                    return;
                };
                let character = form.to_character();
                match character.validate() {
                    Err(e) => form.error = Some(e.to_string()),
                    Ok(()) => {
                        self.backend.send(Request::SaveCharacter {
                            path: form.path.clone(),
                            character,
                        });
                        self.character_form = None;
                    }
                }
            }
            (Section::Worlds, FormAction::Save) => {
                let Some(form) = &mut self.world_form else {
                    return;
                };
                let world = form.to_lorebook();
                match world.validate() {
                    Err(e) => form.error = Some(e.to_string()),
                    Ok(()) => {
                        self.backend.send(Request::SaveWorld {
                            path: form.path.clone(),
                            world,
                        });
                        self.world_form = None;
                    }
                }
            }
            (Section::Characters, FormAction::Delete) => {
                if let Some(form) = &self.character_form {
                    if let Some(path) = form.path.clone() {
                        self.confirm = Some(Confirm::DeleteCharacter {
                            path,
                            name: form.name.clone(),
                        });
                    }
                }
            }
            (Section::Worlds, FormAction::Delete) => {
                if let Some(form) = &self.world_form {
                    if let Some(path) = form.path.clone() {
                        self.confirm = Some(Confirm::DeleteWorld {
                            path,
                            name: form.name.clone(),
                        });
                    }
                }
            }
            (Section::Stories | Section::Tree, _) => {}
        }
    }

    // ---- drawing ----

    /// Draw one frame. Everything the backend has sent since the last frame is applied first.
    pub fn ui(&mut self, ctx: &Context) {
        self.poll();
        if self.logo.is_none() && !self.logo_tried {
            self.logo_tried = true;
            self.logo = branding::load_logo(ctx);
        }

        // The backend only sends the whole tree while the tree section is open.
        let wants_tree = self.section == Section::Tree;
        if wants_tree != self.tree_watching {
            self.tree_watching = wants_tree;
            self.backend.send(Request::WatchTree(wants_tree));
        }
        if wants_tree {
            self.tree_view.sync(self.tree.as_ref(), self.tree_version);
        }

        self.top_bar(ctx);
        Panel::side("sidebar", SIDEBAR_WIDTH).show(ctx, |ui| sidebar::show(self, ui));
        Panel::surface(ctx);
        if self.has_footer() {
            Panel::bottom("footer").show(ctx, |ui| self.footer(ui));
        }
        Panel::content().show(ctx, |ui| self.body(ui));

        self.dialogs(ctx);
        self.toasts.show(ctx);
    }

    /// The top bar: the logo at the left, the section tabs in the middle, and the settings
    /// button at the right.
    fn top_bar(&mut self, ctx: &Context) {
        let mut section = self.section;
        let (mut settings, mut help) = (false, false);
        let logo = self.logo.clone();
        TopBar::new("top-bar").show(
            ctx,
            |ui| {
                if let Some(logo) = &logo {
                    ui.add(Logo::new(logo).width(LOGO_WIDTH));
                }
            },
            |ui| {
                let mut selected = Section::ALL.iter().position(|s| *s == section).unwrap_or(0);
                let tabs = Tabs::new(&mut selected, Section::ALL.iter().map(|s| s.label()))
                    .icons(Section::ALL.iter().map(|s| s.icon()))
                    .pills(true);
                if ui.add(tabs).changed() {
                    section = Section::ALL[selected];
                }
            },
            |ui| {
                let button = Button::new("")
                    .icon(IconKind::Settings)
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Icon);
                settings = ui.add(button).ir_tooltip("Settings").clicked();
                // Added second, so it sits to the left of the settings button.
                let button = Button::new("")
                    .icon(IconKind::Help)
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Icon);
                help = ui.add(button).ir_tooltip("Tutorial").clicked();
            },
        );
        if section != self.section {
            self.show_section(section);
        }
        if settings {
            self.open_settings();
        }
        if help {
            self.open_tutorial();
        }
    }

    fn has_footer(&self) -> bool {
        match self.section {
            // The story's message box floats over the page instead of sitting in a footer.
            Section::Stories => false,
            Section::Tree => self.transcript.is_some(),
            Section::Characters => self.character_form.is_some(),
            Section::Worlds => self.world_form.is_some(),
        }
    }

    fn footer(&mut self, ui: &mut Ui) {
        match self.section {
            Section::Stories => {}
            Section::Tree => {
                if let Some(node) = self.tree_view.footer(ui, self.tree.as_ref()) {
                    self.jump_to(node);
                }
            }
            Section::Characters | Section::Worlds => self.editor_footer(ui),
        }
    }

    fn editor_footer(&mut self, ui: &mut Ui) {
        let can_delete = match self.section {
            Section::Characters => self
                .character_form
                .as_ref()
                .is_some_and(|f| f.path.is_some()),
            Section::Worlds => self.world_form.as_ref().is_some_and(|f| f.path.is_some()),
            Section::Stories | Section::Tree => false,
        };
        let mut action = None;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.add(Button::new("Save")).clicked() {
                action = Some(FormAction::Save);
            }
            if ui
                .add(Button::new("Cancel").variant(ButtonVariant::Outline))
                .clicked()
            {
                action = Some(FormAction::Cancel);
            }
            if can_delete
                && ui
                    .add(Button::new("Delete").variant(ButtonVariant::Destructive))
                    .clicked()
            {
                action = Some(FormAction::Delete);
            }
        });
        if let Some(action) = action {
            self.form_action(action);
        }
    }

    fn body(&mut self, ui: &mut Ui) {
        match self.section {
            Section::Stories => chat::body(self, ui),
            Section::Tree => self.tree_body(ui),
            Section::Characters => {
                let texture = self
                    .character_form
                    .as_ref()
                    .and_then(|f| f.image.clone())
                    .and_then(|name| self.picture(ui.ctx(), &name));
                if let Some(form) = &mut self.character_form {
                    let choose = ScrollArea::vertical()
                        .id_salt("character-form")
                        .show(ui, |ui| form.show(ui, texture.as_ref()))
                        .inner;
                    if choose {
                        self.backend
                            .send(Request::PickImage(ImageTarget::Character));
                    }
                } else if empty_state(
                    ui,
                    "Characters",
                    "Pick a character in the list to edit it, or create a new one.",
                    Some("New character"),
                ) {
                    self.character_form = Some(CharacterForm::new());
                }
            }
            Section::Worlds => {
                let texture = self
                    .world_form
                    .as_ref()
                    .and_then(|f| f.image.clone())
                    .and_then(|name| self.picture(ui.ctx(), &name));
                if let Some(form) = &mut self.world_form {
                    let choose = ScrollArea::vertical()
                        .id_salt("world-form")
                        .show(ui, |ui| form.show(ui, texture.as_ref()))
                        .inner;
                    if choose {
                        self.backend.send(Request::PickImage(ImageTarget::World));
                    }
                } else if empty_state(
                    ui,
                    "Worlds",
                    "Pick a world in the list to edit it, or create a new one.",
                    Some("New world"),
                ) {
                    self.world_form = Some(WorldForm::new());
                }
            }
        }
    }

    fn tree_body(&mut self, ui: &mut Ui) {
        if self.transcript.is_none() {
            if empty_state(
                ui,
                "No story open",
                "Open a story in the list to see its tree, or start a new one.",
                Some("New session"),
            ) {
                self.open_new_session();
            }
            return;
        }
        if let Some(node) = self.tree_view.body(ui, self.tree.as_ref()) {
            self.jump_to(node);
        }
    }

    fn dialogs(&mut self, ctx: &Context) {
        if let Some(tutorial) = &mut self.tutorial {
            if !tutorial.show(ctx) {
                self.close_tutorial();
            }
        }
        self.settings_dialog(ctx);
        self.new_session_dialog(ctx);
        self.confirm_dialog(ctx);
    }

    fn settings_dialog(&mut self, ctx: &Context) {
        let Some(form) = &mut self.settings_form else {
            return;
        };
        let key_available = self.key_available;
        let max_height = ctx.screen_rect().height() * SETTINGS_MAX_HEIGHT_SHARE;
        let min_height = SETTINGS_MIN_HEIGHT.min(max_height);
        let (mut open, mut save, mut cancel, mut action) = (true, false, false, None);
        Dialog::new("settings", "Settings")
            .width(SETTINGS_WIDTH)
            .show(ctx, &mut open, |ui| {
                ScrollArea::vertical()
                    .id_salt("settings")
                    .auto_shrink(true)
                    .max_height(max_height)
                    .show(ui, |ui| {
                        // Keep the overlay the same height on every tab.
                        ui.set_min_height(min_height);
                        action = form.show(key_available, ui);
                    });
                Dialog::actions(ui, |ui| {
                    save = ui.add(Button::new("Save")).clicked();
                    cancel = ui
                        .add(Button::new("Cancel").variant(ButtonVariant::Outline))
                        .clicked();
                });
            });

        if form.theme != self.theme_mode {
            self.theme_mode = form.theme;
            Theme::set_mode(ctx, form.theme);
        }
        if action == Some(SettingsAction::RemoveKey) {
            self.backend.send(Request::DeleteApiKey);
        }
        if save {
            self.save_settings();
        } else if cancel || !open {
            self.settings_form = None;
        }
    }

    fn new_session_dialog(&mut self, ctx: &Context) {
        let Some(form) = &mut self.new_session else {
            return;
        };
        let characters: Vec<String> = std::iter::once("No character".to_string())
            .chain(self.characters.iter().map(|e| e.item.name.clone()))
            .collect();
        let worlds: Vec<String> = std::iter::once("No world".to_string())
            .chain(self.worlds.iter().map(|e| e.item.name.clone()))
            .collect();

        let (mut open, mut create, mut cancel) = (true, false, false);
        Dialog::new("new-session", "New session")
            .description("Choose who is in the scene and where it happens.")
            .show(ctx, &mut open, |ui| {
                FormField::new("Name").show(ui, |ui| {
                    ui.add(Input::new(&mut form.name).id_salt("new-session-name"))
                });
                FormField::new("Character").show(ui, |ui| {
                    ui.add(Select::new(
                        "new-session-character",
                        &mut form.character,
                        characters.clone(),
                    ))
                });
                FormField::new("World").show(ui, |ui| {
                    ui.add(Select::new(
                        "new-session-world",
                        &mut form.world,
                        worlds.clone(),
                    ))
                });
                Dialog::actions(ui, |ui| {
                    create = ui.add(Button::new("Create")).clicked();
                    cancel = ui
                        .add(Button::new("Cancel").variant(ButtonVariant::Outline))
                        .clicked();
                });
            });

        if create {
            let request = Request::CreateSession {
                name: form.name.trim().to_string(),
                character: form
                    .character
                    .checked_sub(1)
                    .and_then(|i| characters.get(i + 1))
                    .cloned(),
                world: form
                    .world
                    .checked_sub(1)
                    .and_then(|i| worlds.get(i + 1))
                    .cloned(),
            };
            self.show_section(Section::Stories);
            self.backend.send(request);
            self.new_session = None;
        } else if cancel || !open {
            self.new_session = None;
        }
    }

    fn confirm_dialog(&mut self, ctx: &Context) {
        let Some(confirm) = &self.confirm else {
            return;
        };
        let (title, description, label) = confirm.texts();
        let (mut open, mut yes, mut no) = (true, false, false);
        Dialog::new("confirm", title)
            .description(description)
            .show(ctx, &mut open, |ui| {
                Dialog::actions(ui, |ui| {
                    yes = ui
                        .add(Button::new(label).variant(ButtonVariant::Destructive))
                        .clicked();
                    no = ui
                        .add(Button::new("Cancel").variant(ButtonVariant::Outline))
                        .clicked();
                });
            });
        if yes {
            if let Some(confirm) = self.confirm.take() {
                self.apply_confirm(confirm);
            }
        } else if no || !open {
            self.confirm = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui;
    use ir_core::db::{Node, NodeId, NodeKind};
    use tokio::sync::mpsc::UnboundedReceiver;

    use super::*;

    struct Rig {
        app: App,
        events: mpsc::Sender<Event>,
        requests: UnboundedReceiver<Request>,
        ctx: egui::Context,
    }

    impl Rig {
        fn new() -> Self {
            let (backend, requests) = Backend::detached();
            let (events, events_rx) = mpsc::channel();
            let ctx = egui::Context::default();
            Theme::apply(&ctx);
            Self {
                app: App::new(backend, events_rx, None),
                events,
                requests,
                ctx,
            }
        }

        fn frame(&mut self) {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 720.0),
                )),
                ..Default::default()
            };
            let app = &mut self.app;
            let _ = self.ctx.run(input, |ctx| app.ui(ctx));
        }

        fn frame_with(&mut self, events: Vec<egui::Event>) {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 720.0),
                )),
                // A real keyboard reports held modifiers on the input itself too.
                modifiers: events
                    .iter()
                    .find_map(|e| match e {
                        egui::Event::Key { modifiers, .. } => Some(*modifiers),
                        _ => None,
                    })
                    .unwrap_or_default(),
                events,
                ..Default::default()
            };
            let app = &mut self.app;
            let _ = self.ctx.run(input, |ctx| app.ui(ctx));
        }

        fn push(&mut self, event: Event) {
            self.events.send(event).unwrap();
            self.frame();
        }

        fn sent(&mut self) -> Vec<String> {
            let mut out = Vec::new();
            while let Ok(request) = self.requests.try_recv() {
                out.push(format!("{request:?}"));
            }
            out
        }
    }

    fn node(id: i64, kind: NodeKind, speaker: Option<&str>, text: &str) -> Node {
        Node {
            id: NodeId(id),
            session_id: SessionId(1),
            parent_id: None,
            kind,
            speaker: speaker.map(Into::into),
            content: text.into(),
            label: None,
            bookmark: None,
            active_lore: vec![],
            created_at: 0,
        }
    }

    fn session(id: i64, name: &str) -> Session {
        Session {
            id: SessionId(id),
            name: name.into(),
            world: Some("The Misty Coast".into()),
            current_node_id: Some(NodeId(3)),
            created_at: 0,
            updated_at: 0,
            tokens_used: 0,
        }
    }

    fn transcript(id: i64, name: &str) -> Transcript {
        Transcript {
            session: session(id, name),
            nodes: vec![
                node(
                    1,
                    NodeKind::Character,
                    Some("Mira"),
                    "Ah, another one who got lost?",
                ),
                node(2, NodeKind::User, None, "I follow her."),
                node(3, NodeKind::Narrator, None, "Fog rolls in."),
            ],
            characters: vec!["Mira".into()],
        }
    }

    fn entry<T>(name: &str, item: T) -> Entry<T> {
        Entry {
            path: PathBuf::from(format!("{name}.yaml")),
            item,
        }
    }

    fn library(rig: &mut Rig) {
        let mira = Character::from_yaml("name: Mira").unwrap();
        let world = Lorebook::from_yaml("name: The Misty Coast").unwrap();
        rig.push(Event::Library {
            characters: vec![entry("mira", mira)],
            worlds: vec![entry("coast", world)],
            images_dir: std::env::temp_dir().join("ir-app-test-images"),
        });
    }

    // ---- events ----

    #[test]
    fn every_screen_and_dialog_draws_in_both_themes() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let mut rig = Rig::new();
            Theme::set_mode(&rig.ctx, mode);
            rig.frame(); // the welcome card

            library(&mut rig);
            rig.push(Event::Sessions(vec![
                session(1, "Harbor"),
                session(2, "Reef"),
            ]));
            rig.push(Event::Transcript(transcript(1, "Harbor")));
            rig.push(Event::Notice(Notice {
                text: "tree".into(),
                monospace: true,
            }));
            rig.push(Event::Generating(true));
            rig.push(Event::Chunk("The lamp swings".into()));
            rig.app.chat.input = "/br".into();
            rig.frame();

            rig.app.open_settings();
            rig.frame();
            rig.app.settings_form = None;
            rig.app.show_section(Section::Characters);
            rig.frame(); // the empty state
            rig.app.character_form = Some(CharacterForm::new());
            rig.frame();
            rig.app.show_section(Section::Worlds);
            rig.frame();
            rig.app.world_form = Some(WorldForm::new());
            rig.frame();
            rig.app.show_section(Section::Stories);
            rig.app.open_new_session();
            rig.frame();
            rig.app.new_session = None;
            rig.app.confirm = Some(Confirm::Command("/prune #3".into()));
            rig.frame();
        }
    }

    #[test]
    fn events_update_the_state() {
        let mut rig = Rig::new();
        rig.push(Event::Settings {
            settings: Settings {
                style: ir_core::settings::StyleSettings {
                    narrator_mode: true,
                    ..Default::default()
                },
                ..Settings::default()
            },
            key_available: true,
        });
        assert!(rig.app.settings.style.narrator_mode && rig.app.key_available);

        rig.push(Event::Sessions(vec![session(1, "Harbor")]));
        assert_eq!(rig.app.sessions.len(), 1);

        rig.push(Event::Transcript(transcript(1, "Harbor")));
        assert_eq!(rig.app.transcript.as_ref().unwrap().nodes.len(), 3);

        rig.push(Event::NoSession);
        assert!(rig.app.transcript.is_none());
    }

    #[test]
    fn streaming_text_accumulates_and_clears_when_generation_ends() {
        let mut rig = Rig::new();
        rig.push(Event::Generating(true));
        rig.push(Event::Chunk("Hel".into()));
        rig.push(Event::Chunk("lo".into()));
        assert!(rig.app.chat.generating);
        assert_eq!(rig.app.chat.streaming, "Hello");
        rig.push(Event::Generating(false));
        assert!(!rig.app.chat.generating && rig.app.chat.streaming.is_empty());
    }

    #[test]
    fn notices_belong_to_a_session_and_clear_when_switching() {
        let mut rig = Rig::new();
        rig.push(Event::Transcript(transcript(1, "Harbor")));
        rig.push(Event::Notice(Notice {
            text: "hi".into(),
            monospace: false,
        }));
        rig.push(Event::Transcript(transcript(1, "Harbor"))); // same session refreshed
        assert_eq!(rig.app.notices.len(), 1);
        rig.push(Event::Transcript(transcript(2, "Reef")));
        assert!(rig.app.notices.is_empty());
    }

    #[test]
    fn info_and_errors_become_toasts() {
        let mut rig = Rig::new();
        rig.push(Event::Info("Saved.".into()));
        rig.push(Event::Error("Boom".into()));
        assert_eq!(rig.app.toasts.len(), 2);
    }

    // ---- sending ----

    fn enter(modifiers: egui::Modifiers) -> Vec<egui::Event> {
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: Some(egui::Key::Enter),
                pressed,
                repeat: false,
                modifiers,
            })
            .collect()
    }

    /// A rig with an open story and the chat input focused.
    fn rig_with_focused_input() -> Rig {
        let mut rig = Rig::new();
        rig.push(Event::Transcript(transcript(1, "Harbor")));
        rig.frame();
        let id = rig.app.chat.input_id.expect("the input was drawn");
        rig.ctx.memory_mut(|m| m.request_focus(id));
        rig.frame();
        rig
    }

    #[test]
    fn enter_in_the_message_box_sends_the_message() {
        let mut rig = rig_with_focused_input();
        rig.app.chat.input = "I wave.".into();
        rig.frame_with(enter(egui::Modifiers::NONE));
        assert_eq!(rig.sent(), [r#"Send("I wave.")"#]);
        assert!(rig.app.chat.input.is_empty());
    }

    #[test]
    fn shift_enter_does_not_send() {
        let mut rig = rig_with_focused_input();
        rig.app.chat.input = "First line".into();
        rig.frame_with(enter(egui::Modifiers::SHIFT));
        let sent = rig.sent();
        assert!(sent.is_empty(), "{sent:?}");
        let input = &rig.app.chat.input;
        assert!(
            input.contains("First line") && input.contains(char::from(10)),
            "{input:?}"
        );
    }

    #[test]
    fn enter_without_focus_in_the_message_box_sends_nothing() {
        let mut rig = Rig::new();
        rig.push(Event::Transcript(transcript(1, "Harbor")));
        rig.app.chat.input = "Stay.".into();
        rig.frame_with(enter(egui::Modifiers::NONE));
        assert!(rig.sent().is_empty());
        assert_eq!(rig.app.chat.input, "Stay.");
    }

    #[test]
    fn messages_and_safe_commands_are_sent_immediately() {
        let mut rig = Rig::new();
        rig.app.chat.input = "I open the door.".into();
        rig.app.send_chat();
        rig.app.chat.input = "/tree".into();
        rig.app.send_chat();
        assert_eq!(
            rig.sent(),
            [r#"Send("I open the door.")"#, r#"Send("/tree")"#]
        );
        assert!(rig.app.chat.input.is_empty());
        assert!(rig.app.confirm.is_none());
    }

    #[test]
    fn blank_input_sends_nothing() {
        let mut rig = Rig::new();
        rig.app.chat.input = "  \n ".into();
        rig.app.send_chat();
        assert!(rig.sent().is_empty());
    }

    #[test]
    fn deleting_commands_wait_for_confirmation() {
        let mut rig = Rig::new();
        rig.app.chat.input = "/prune #3".into();
        rig.app.send_chat();
        assert!(rig.sent().is_empty(), "nothing is sent before confirming");
        assert_eq!(rig.app.confirm, Some(Confirm::Command("/prune #3".into())));

        rig.frame();
        let confirm = rig.app.confirm.take().unwrap();
        rig.app.apply_confirm(confirm);
        assert_eq!(rig.sent(), [r#"Send("/prune #3")"#]);
    }

    // ---- dialogs and forms ----

    #[test]
    fn the_new_session_dialog_starts_with_the_first_character() {
        let mut rig = Rig::new();
        rig.app.open_new_session();
        assert_eq!(
            rig.app.new_session.as_ref().unwrap().character,
            0,
            "no characters yet"
        );

        library(&mut rig);
        rig.app.open_new_session();
        let form = rig.app.new_session.as_ref().unwrap();
        assert_eq!(
            (form.character, form.world, form.name.as_str()),
            (1, 0, "New story")
        );
    }

    #[test]
    fn confirming_deletions_sends_requests_and_closes_open_editors() {
        let mut rig = Rig::new();
        rig.app.character_form = Some(CharacterForm {
            path: Some(PathBuf::from("mira.yaml")),
            ..CharacterForm::new()
        });
        rig.app.apply_confirm(Confirm::DeleteCharacter {
            path: PathBuf::from("mira.yaml"),
            name: "Mira".into(),
        });
        assert!(rig.app.character_form.is_none());
        rig.app.apply_confirm(Confirm::DeleteSession {
            id: SessionId(4),
            name: "x".into(),
        });
        rig.app.apply_confirm(Confirm::DeleteWorld {
            path: PathBuf::from("w.yaml"),
            name: "w".into(),
        });
        let sent = rig.sent();
        assert!(
            sent[0].starts_with("DeleteCharacter")
                && sent[1].contains("DeleteSession(SessionId(4))")
        );
        assert!(sent[2].starts_with("DeleteWorld"));
    }

    #[test]
    fn saving_settings_sends_them_with_a_typed_key_and_closes_the_overlay() {
        let mut rig = Rig::new();
        rig.app.open_settings();
        if let Some(form) = &mut rig.app.settings_form {
            form.api_key = " sk-1 ".into();
        }
        rig.app.save_settings();
        let sent = rig.sent();
        assert_eq!(sent.len(), 1);
        assert!(
            sent[0].starts_with("SaveSettings") && sent[0].contains(r#"api_key: Some("sk-1")"#),
            "{sent:?}"
        );
        assert!(rig.app.settings_form.is_none());
    }

    #[test]
    fn an_invalid_character_stays_open_with_an_error_and_sends_nothing() {
        let mut rig = Rig::new();
        rig.app.show_section(Section::Characters);
        rig.app.character_form = Some(CharacterForm::new());
        rig.app.form_action(FormAction::Save);
        assert!(rig.sent().is_empty());
        match &rig.app.character_form {
            Some(form) => assert!(form.error.as_deref().is_some_and(|e| e.contains("name"))),
            None => panic!("the editor should stay open"),
        }
        rig.frame(); // the error is drawn
    }

    #[test]
    fn a_valid_character_or_world_is_saved_to_its_file() {
        let mut rig = Rig::new();
        rig.app.show_section(Section::Characters);
        rig.app.character_form = Some(CharacterForm {
            path: Some(PathBuf::from("bo.yaml")),
            name: "Bo".into(),
            ..CharacterForm::new()
        });
        rig.app.form_action(FormAction::Save);
        rig.app.show_section(Section::Worlds);
        rig.app.world_form = Some(WorldForm {
            name: "Reef".into(),
            ..WorldForm::new()
        });
        rig.app.form_action(FormAction::Save);
        let sent = rig.sent();
        assert!(sent[0].starts_with("SaveCharacter") && sent[0].contains("bo.yaml"));
        assert!(sent[1].starts_with("SaveWorld") && sent[1].contains("path: None"));
        assert!(rig.app.character_form.is_none() && rig.app.world_form.is_none());
    }

    #[test]
    fn deleting_from_an_editor_asks_first_and_new_items_cannot_be_deleted() {
        let mut rig = Rig::new();
        rig.app.show_section(Section::Worlds);
        rig.app.world_form = Some(WorldForm::new());
        rig.app.form_action(FormAction::Delete);
        assert!(
            rig.app.confirm.is_none(),
            "an unsaved world has no file to delete"
        );

        rig.app.world_form = Some(WorldForm {
            path: Some(PathBuf::from("coast.yaml")),
            name: "Coast".into(),
            ..WorldForm::new()
        });
        rig.app.form_action(FormAction::Delete);
        assert!(matches!(rig.app.confirm, Some(Confirm::DeleteWorld { .. })));
        assert!(rig.sent().is_empty());
    }

    #[test]
    fn cancelling_an_editor_closes_it_without_sending() {
        let mut rig = Rig::new();
        rig.app.show_section(Section::Characters);
        rig.app.character_form = Some(CharacterForm::new());
        rig.app.form_action(FormAction::Cancel);
        assert!(rig.app.character_form.is_none());
        assert!(rig.sent().is_empty());
    }

    #[test]
    fn settings_open_as_an_overlay_over_the_current_section() {
        let mut rig = Rig::new();
        rig.app.show_section(Section::Worlds);
        rig.app.open_settings();
        rig.frame();
        assert_eq!(rig.app.section, Section::Worlds, "the section stays put");
        assert!(rig.app.settings_form.is_some());
        rig.app.settings_form = None;
        rig.frame();
        assert!(rig.sent().is_empty());
    }

    #[test]
    fn the_theme_is_chosen_in_the_settings_overlay_and_applies_at_once() {
        let mut rig = Rig::new();
        rig.app.open_settings();
        assert_eq!(
            rig.app.settings_form.as_ref().map(|f| f.theme),
            Some(ThemeMode::System)
        );
        if let Some(form) = &mut rig.app.settings_form {
            form.theme = ThemeMode::Dark;
        }
        rig.frame();
        assert_eq!(rig.app.theme_mode, ThemeMode::Dark);
        assert!(rig.ctx.style().visuals.dark_mode);
        rig.app.open_settings();
        assert_eq!(
            rig.app.settings_form.as_ref().map(|f| f.theme),
            Some(ThemeMode::Dark),
            "the form starts from the current theme"
        );
    }

    // ---- tutorial ----

    fn settings_event(seen: bool) -> Event {
        Event::Settings {
            settings: Settings {
                ui: ir_core::settings::UiSettings {
                    tutorial_seen: seen,
                },
                ..Settings::default()
            },
            key_available: false,
        }
    }

    #[test]
    fn the_first_start_opens_the_tutorial_once() {
        let mut rig = Rig::new();
        rig.push(settings_event(false));
        assert!(rig.app.tutorial.is_some());

        // Closing it remembers that it was seen, and later settings events do not reopen it.
        rig.app.close_tutorial();
        assert!(rig.app.tutorial.is_none());
        assert_eq!(rig.sent(), ["MarkTutorialSeen"]);
        rig.push(settings_event(false));
        assert!(rig.app.tutorial.is_none(), "not offered twice in one run");
    }

    #[test]
    fn a_seen_tutorial_stays_closed_and_reopening_does_not_write_again() {
        let mut rig = Rig::new();
        rig.push(settings_event(true));
        assert!(rig.app.tutorial.is_none());

        rig.app.open_tutorial();
        rig.frame();
        assert!(rig.app.tutorial.is_some());
        rig.app.close_tutorial();
        assert!(rig.sent().is_empty(), "nothing to remember a second time");
    }

    #[test]
    fn the_tutorial_draws_over_every_section_in_both_themes() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let mut rig = Rig::new();
            Theme::set_mode(&rig.ctx, mode);
            rig.app.open_tutorial();
            for section in Section::ALL {
                rig.app.show_section(section);
                rig.frame();
            }
        }
    }

    // ---- command suggestions ----

    fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn typing_a_slash_floats_the_matching_commands_and_clicking_one_completes_it() {
        let mut rig = rig_with_focused_input();
        rig.app.chat.input = "/br".into();
        rig.frame();
        rig.frame();
        let id = egui::Id::new("command-suggestions");
        let list = rig
            .ctx
            .memory(|m| m.area_rect(id))
            .expect("the suggestions are shown while typing a command");

        // The second suggestion, /branches, is the lower row.
        let target = egui::pos2(list.center().x, list.bottom() - 24.0);
        rig.frame_with(vec![egui::Event::PointerMoved(target)]);
        rig.frame_with(vec![press(target, true)]);
        rig.frame_with(vec![press(target, false)]);
        assert_eq!(rig.app.chat.input, "/branches ");
        assert!(rig.sent().is_empty(), "choosing a command does not run it");
    }

    #[test]
    fn plain_text_shows_no_suggestions() {
        let mut rig = rig_with_focused_input();
        rig.app.chat.input = "I wave".into();
        rig.frame();
        rig.frame();
        let id = egui::Id::new("command-suggestions");
        assert!(rig.ctx.memory(|m| m.area_rect(id)).is_none());
    }

    // ---- pictures ----

    fn png_bytes() -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(6, 4, image::Rgba([20, 120, 220, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    /// A folder with one valid picture, announced to the app like the backend would.
    fn rig_with_a_picture() -> (Rig, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ir-app-pictures-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ok.png"), png_bytes()).unwrap();
        std::fs::write(dir.join("broken.png"), b"not a png").unwrap();
        let mut rig = Rig::new();
        rig.push(Event::Library {
            characters: Vec::new(),
            worlds: Vec::new(),
            images_dir: dir.clone(),
        });
        (rig, dir)
    }

    #[test]
    fn a_picked_picture_goes_to_the_matching_editor_only() {
        let mut rig = Rig::new();
        rig.app.character_form = Some(CharacterForm::new());
        rig.app.world_form = Some(WorldForm::new());
        rig.push(Event::ImagePicked {
            target: ImageTarget::Character,
            name: "a.png".into(),
        });
        assert_eq!(
            rig.app
                .character_form
                .as_ref()
                .and_then(|f| f.image.as_deref()),
            Some("a.png")
        );
        assert_eq!(
            rig.app.world_form.as_ref().and_then(|f| f.image.clone()),
            None
        );
        rig.push(Event::ImagePicked {
            target: ImageTarget::World,
            name: "b.png".into(),
        });
        assert_eq!(
            rig.app.world_form.as_ref().and_then(|f| f.image.as_deref()),
            Some("b.png")
        );
        // With no editor open the picture is simply dropped (and cleaned up at the next start).
        rig.app.world_form = None;
        rig.push(Event::ImagePicked {
            target: ImageTarget::World,
            name: "c.png".into(),
        });
    }

    #[test]
    fn pictures_load_from_the_images_folder_and_bad_or_unsafe_names_do_not() {
        let (mut rig, dir) = rig_with_a_picture();
        let ctx = rig.ctx.clone();
        let loaded = rig
            .app
            .picture(&ctx, "ok.png")
            .expect("a valid picture loads");
        assert!(loaded.aspect_ratio() > 1.0);
        assert!(rig.app.picture(&ctx, "broken.png").is_none(), "not a PNG");
        assert!(rig.app.picture(&ctx, "missing.png").is_none());
        assert!(
            rig.app.picture(&ctx, "../ok.png").is_none(),
            "no leaving the folder"
        );
        assert!(rig.app.picture(&ctx, "").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn editors_draw_with_a_picture_a_missing_one_and_none() {
        let (mut rig, dir) = rig_with_a_picture();
        for name in [Some("ok.png"), Some("gone.png"), None] {
            rig.app.show_section(Section::Characters);
            rig.app.character_form = Some(CharacterForm {
                image: name.map(String::from),
                ..CharacterForm::new()
            });
            rig.frame();
            rig.app.show_section(Section::Worlds);
            rig.app.world_form = Some(WorldForm {
                image: name.map(String::from),
                ..WorldForm::new()
            });
            rig.frame();
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    // ---- sections ----

    #[test]
    fn every_section_draws_with_and_without_content_in_both_themes() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let mut rig = Rig::new();
            Theme::set_mode(&rig.ctx, mode);
            for section in Section::ALL {
                rig.app.show_section(section);
                rig.frame();
            }
            library(&mut rig);
            rig.push(Event::Sessions(vec![session(1, "Harbor")]));
            rig.push(Event::Transcript(transcript(1, "Harbor")));
            for section in Section::ALL {
                rig.app.show_section(section);
                rig.frame();
            }
        }
    }

    #[test]
    fn the_logo_is_loaded_on_the_first_frame_and_the_nav_lists_every_section() {
        let mut rig = Rig::new();
        assert!(rig.app.logo.is_none());
        rig.frame();
        assert!(rig.app.logo.is_some());
        let labels: Vec<_> = Section::ALL.iter().map(|s| s.label()).collect();
        assert_eq!(labels, ["Stories", "Characters", "Worlds", "Tree"]);
    }

    #[test]
    fn each_section_keeps_its_own_editor() {
        let mut rig = Rig::new();
        rig.app.show_section(Section::Characters);
        rig.app.character_form = Some(CharacterForm::new());
        rig.app.show_section(Section::Worlds);
        rig.frame();
        assert!(
            rig.app.world_form.is_none(),
            "worlds start without an editor"
        );
        rig.app.show_section(Section::Characters);
        assert!(rig.app.character_form.is_some());
    }

    // ---- the tree screen ----

    fn tree_data() -> TreeData {
        let shape = [
            (1, None),
            (2, Some(1)),
            (3, Some(2)),
            (4, Some(2)),
            (5, Some(3)),
        ];
        TreeData {
            session_id: SessionId(1),
            nodes: shape
                .into_iter()
                .map(|(id, parent)| {
                    let mut n = node(id, NodeKind::User, None, &format!("message {id}"));
                    n.parent_id = parent.map(NodeId);
                    n
                })
                .collect(),
            current: Some(NodeId(5)),
        }
    }

    #[test]
    fn the_backend_is_asked_for_the_tree_only_while_the_tree_screen_is_open() {
        let mut rig = Rig::new();
        rig.frame();
        assert!(rig.sent().is_empty(), "the chat does not need the tree");

        rig.app.show_section(Section::Tree);
        rig.frame();
        rig.frame();
        assert_eq!(
            rig.sent(),
            ["WatchTree(true)"],
            "asked once, not every frame"
        );

        rig.app.show_section(Section::Stories);
        rig.frame();
        rig.frame();
        assert_eq!(rig.sent(), ["WatchTree(false)"]);
    }

    #[test]
    fn the_tree_screen_draws_the_data_it_receives_in_both_themes() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let mut rig = Rig::new();
            Theme::set_mode(&rig.ctx, mode);
            rig.push(Event::Transcript(transcript(1, "Harbor")));
            rig.app.show_section(Section::Tree);
            rig.frame();
            rig.push(Event::Tree(Some(tree_data())));
            assert_eq!(rig.app.tree.as_ref().map(|t| t.nodes.len()), Some(5));
            rig.frame();
            rig.push(Event::Tree(None));
            assert!(rig.app.tree.is_none());
            rig.frame();
        }
    }

    #[test]
    fn jumping_from_the_tree_sends_the_request_and_returns_to_the_chat() {
        let mut rig = Rig::new();
        rig.app.show_section(Section::Tree);
        rig.app.jump_to(NodeId(3));
        assert_eq!(rig.app.section, Section::Stories);
        assert_eq!(rig.sent(), ["JumpTo(NodeId(3))"]);
    }
}
