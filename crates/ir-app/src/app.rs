//! The application: window layout, screens, and the event loop.

use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui::{Align, Context, Layout, Ui};
use ir_core::cards::Card as _;
use ir_core::character::Character;
use ir_core::commands;
use ir_core::db::{Session, SessionId};
use ir_core::lorebook::Lorebook;
use ir_core::settings::Settings;
use ir_core::APP_NAME;
use ir_ui::components::{
    Button, ButtonSize, ButtonVariant, Dialog, FormField, Input, Label, Panel, ScrollArea, Select,
    Toast, Toasts,
};
use ir_ui::{Theme, ThemeMode};

use crate::backend::{Backend, Event, Request, Transcript};
use crate::characters::{CharacterForm, WorldForm};
use crate::format::Notice;
use crate::library::Entry;
use crate::settings_ui::{SettingsAction, SettingsForm};
use crate::{chat, sidebar};

/// Width of the sidebar.
const SIDEBAR_WIDTH: f32 = 260.0;

/// The name a new session starts with.
const DEFAULT_SESSION_NAME: &str = "New story";

/// The screen shown in the main area.
pub enum Screen {
    /// The story.
    Chat,
    /// The settings.
    Settings(SettingsForm),
    /// A character being edited.
    Character(CharacterForm),
    /// A world being edited.
    World(WorldForm),
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

/// What a form's footer buttons asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormAction {
    Save,
    Cancel,
    Delete,
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
    /// The screen in the main area.
    pub screen: Screen,
    theme_mode: ThemeMode,
    /// The current settings.
    pub settings: Settings,
    /// Whether an API key is available for the current provider.
    pub key_available: bool,
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
    /// The chat input state.
    pub chat: ChatState,
    /// The new-session dialog, when open.
    pub new_session: Option<NewSessionForm>,
    /// A pending confirmation.
    pub confirm: Option<Confirm>,
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
            screen: Screen::Chat,
            theme_mode: ThemeMode::System,
            settings: Settings::default(),
            key_available: false,
            sessions: Vec::new(),
            transcript: None,
            notices: Vec::new(),
            characters: Vec::new(),
            worlds: Vec::new(),
            chat: ChatState::default(),
            new_session: None,
            confirm: None,
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
            }
            Event::Sessions(sessions) => self.sessions = sessions,
            Event::Library { characters, worlds } => {
                self.characters = characters;
                self.worlds = worlds;
            }
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

    fn open_settings(&mut self) {
        self.screen = Screen::Settings(SettingsForm::new(&self.settings));
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

    fn apply_confirm(&mut self, confirm: Confirm) {
        match confirm {
            Confirm::DeleteSession { id, .. } => self.backend.send(Request::DeleteSession(id)),
            Confirm::DeleteCharacter { path, .. } => {
                if matches!(&self.screen, Screen::Character(f) if f.path.as_ref() == Some(&path)) {
                    self.screen = Screen::Chat;
                }
                self.backend.send(Request::DeleteCharacter(path));
            }
            Confirm::DeleteWorld { path, .. } => {
                if matches!(&self.screen, Screen::World(f) if f.path.as_ref() == Some(&path)) {
                    self.screen = Screen::Chat;
                }
                self.backend.send(Request::DeleteWorld(path));
            }
            Confirm::Command(text) => self.backend.send(Request::Send(text)),
        }
    }

    fn form_action(&mut self, action: FormAction) {
        match (action, &mut self.screen) {
            (FormAction::Cancel, _) => self.screen = Screen::Chat,
            (FormAction::Save, Screen::Settings(form)) => {
                let request = Request::SaveSettings {
                    settings: form.to_settings(),
                    api_key: form.new_api_key(),
                };
                self.backend.send(request);
                self.screen = Screen::Chat;
            }
            (FormAction::Save, Screen::Character(form)) => {
                let character = form.to_character();
                match character.validate() {
                    Err(e) => form.error = Some(e.to_string()),
                    Ok(()) => {
                        self.backend.send(Request::SaveCharacter {
                            path: form.path.clone(),
                            character,
                        });
                        self.screen = Screen::Chat;
                    }
                }
            }
            (FormAction::Save, Screen::World(form)) => {
                let world = form.to_lorebook();
                match world.validate() {
                    Err(e) => form.error = Some(e.to_string()),
                    Ok(()) => {
                        self.backend.send(Request::SaveWorld {
                            path: form.path.clone(),
                            world,
                        });
                        self.screen = Screen::Chat;
                    }
                }
            }
            (FormAction::Delete, Screen::Character(form)) => {
                if let Some(path) = form.path.clone() {
                    self.confirm = Some(Confirm::DeleteCharacter {
                        path,
                        name: form.name.clone(),
                    });
                }
            }
            (FormAction::Delete, Screen::World(form)) => {
                if let Some(path) = form.path.clone() {
                    self.confirm = Some(Confirm::DeleteWorld {
                        path,
                        name: form.name.clone(),
                    });
                }
            }
            (FormAction::Save | FormAction::Delete, Screen::Chat | Screen::Settings(_)) => {}
        }
    }

    // ---- drawing ----

    /// Draw one frame. Everything the backend has sent since the last frame is applied first.
    pub fn ui(&mut self, ctx: &Context) {
        self.poll();

        Panel::top("top-bar").show(ctx, |ui| self.top_bar(ui, ctx));
        Panel::side("sidebar", SIDEBAR_WIDTH).show(ctx, |ui| sidebar::show(self, ui));
        if self.has_footer() {
            Panel::bottom("footer").show(ctx, |ui| self.footer(ui));
        }
        Panel::central().show(ctx, |ui| self.body(ui));

        self.dialogs(ctx);
        self.toasts.show(ctx);
    }

    fn top_bar(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.add(Label::new(APP_NAME).heading());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(
                        Button::new("Settings")
                            .variant(ButtonVariant::Secondary)
                            .size(ButtonSize::Sm),
                    )
                    .clicked()
                {
                    self.open_settings();
                }
                // Added right to left, so the order here is the reverse of how it reads.
                for (label, mode) in [
                    ("Dark", ThemeMode::Dark),
                    ("Light", ThemeMode::Light),
                    ("System", ThemeMode::System),
                ] {
                    let variant = if self.theme_mode == mode {
                        ButtonVariant::Default
                    } else {
                        ButtonVariant::Outline
                    };
                    if ui
                        .add(Button::new(label).variant(variant).size(ButtonSize::Sm))
                        .clicked()
                    {
                        self.theme_mode = mode;
                        Theme::set_mode(ctx, mode);
                    }
                }
            });
        });
    }

    fn has_footer(&self) -> bool {
        match self.screen {
            Screen::Chat => self.transcript.is_some(),
            _ => true,
        }
    }

    fn footer(&mut self, ui: &mut Ui) {
        if matches!(self.screen, Screen::Chat) {
            chat::composer(self, ui);
            return;
        }
        let can_delete = match &self.screen {
            Screen::Character(f) => f.path.is_some(),
            Screen::World(f) => f.path.is_some(),
            _ => false,
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
        if matches!(self.screen, Screen::Chat) {
            chat::body(self, ui);
            return;
        }
        let mut settings_action = None;
        ScrollArea::vertical().id_salt("form").show(ui, |ui| {
            match &mut self.screen {
                Screen::Settings(form) => settings_action = form.show(self.key_available, ui),
                Screen::Character(form) => form.show(ui),
                Screen::World(form) => form.show(ui),
                Screen::Chat => {}
            };
        });
        if settings_action == Some(SettingsAction::RemoveKey) {
            self.backend.send(Request::DeleteApiKey);
        }
    }

    fn dialogs(&mut self, ctx: &Context) {
        self.new_session_dialog(ctx);
        self.confirm_dialog(ctx);
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
            self.screen = Screen::Chat;
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
            rig.app.screen = Screen::Character(CharacterForm::new());
            rig.frame();
            rig.app.screen = Screen::World(WorldForm::new());
            rig.frame();
            rig.app.screen = Screen::Chat;
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
        rig.app.screen = Screen::Character(CharacterForm {
            path: Some(PathBuf::from("mira.yaml")),
            ..CharacterForm::new()
        });
        rig.app.apply_confirm(Confirm::DeleteCharacter {
            path: PathBuf::from("mira.yaml"),
            name: "Mira".into(),
        });
        assert!(matches!(rig.app.screen, Screen::Chat));
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
    fn saving_settings_sends_them_with_a_typed_key_and_returns_to_chat() {
        let mut rig = Rig::new();
        rig.app.open_settings();
        if let Screen::Settings(form) = &mut rig.app.screen {
            form.api_key = " sk-1 ".into();
        }
        rig.app.form_action(FormAction::Save);
        let sent = rig.sent();
        assert_eq!(sent.len(), 1);
        assert!(
            sent[0].starts_with("SaveSettings") && sent[0].contains(r#"api_key: Some("sk-1")"#),
            "{sent:?}"
        );
        assert!(matches!(rig.app.screen, Screen::Chat));
    }

    #[test]
    fn an_invalid_character_stays_open_with_an_error_and_sends_nothing() {
        let mut rig = Rig::new();
        rig.app.screen = Screen::Character(CharacterForm::new());
        rig.app.form_action(FormAction::Save);
        assert!(rig.sent().is_empty());
        match &rig.app.screen {
            Screen::Character(form) => {
                assert!(form.error.as_deref().is_some_and(|e| e.contains("name")))
            }
            _ => panic!("the editor should stay open"),
        }
        rig.frame(); // the error is drawn
    }

    #[test]
    fn a_valid_character_or_world_is_saved_to_its_file() {
        let mut rig = Rig::new();
        rig.app.screen = Screen::Character(CharacterForm {
            path: Some(PathBuf::from("bo.yaml")),
            name: "Bo".into(),
            ..CharacterForm::new()
        });
        rig.app.form_action(FormAction::Save);
        rig.app.screen = Screen::World(WorldForm {
            name: "Reef".into(),
            ..WorldForm::new()
        });
        rig.app.form_action(FormAction::Save);
        let sent = rig.sent();
        assert!(sent[0].starts_with("SaveCharacter") && sent[0].contains("bo.yaml"));
        assert!(sent[1].starts_with("SaveWorld") && sent[1].contains("path: None"));
        assert!(matches!(rig.app.screen, Screen::Chat));
    }

    #[test]
    fn deleting_from_an_editor_asks_first_and_new_items_cannot_be_deleted() {
        let mut rig = Rig::new();
        rig.app.screen = Screen::World(WorldForm::new());
        rig.app.form_action(FormAction::Delete);
        assert!(
            rig.app.confirm.is_none(),
            "an unsaved world has no file to delete"
        );

        rig.app.screen = Screen::World(WorldForm {
            path: Some(PathBuf::from("coast.yaml")),
            name: "Coast".into(),
            ..WorldForm::new()
        });
        rig.app.form_action(FormAction::Delete);
        assert!(matches!(rig.app.confirm, Some(Confirm::DeleteWorld { .. })));
        assert!(rig.sent().is_empty());
    }

    #[test]
    fn cancel_returns_to_the_chat() {
        let mut rig = Rig::new();
        rig.app.open_settings();
        rig.app.form_action(FormAction::Cancel);
        assert!(matches!(rig.app.screen, Screen::Chat));
        assert!(rig.sent().is_empty());
    }
}
