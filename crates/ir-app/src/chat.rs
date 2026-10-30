//! The chat screen: the story so far, and the box for the next message.

use eframe::egui::{self, Key, Ui};
use ir_core::commands::{CommandInfo, COMMANDS};
use ir_core::db::NodeKind;
use ir_ui::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, ChatBubble, Label, ScrollArea, Separator,
    Textarea,
};
use ir_ui::spacing;

use crate::app::App;
use crate::backend::Request;
use crate::prompts;

/// Width of the column holding the Send and Stop buttons.
const BUTTON_COLUMN: f32 = 88.0;

/// The most command hints shown at once.
const MAX_HINTS: usize = 6;

/// The commands matching what has been typed after the `/`. Empty unless the input starts
/// with a slash.
pub fn command_hints(input: &str) -> Vec<&'static CommandInfo> {
    let Some(rest) = input.trim_start().strip_prefix('/') else {
        return Vec::new();
    };
    let (word, has_arguments) = match rest.split_once(char::is_whitespace) {
        Some((word, _)) => (word, true),
        None => (rest, false),
    };
    let word = word.to_lowercase();
    COMMANDS
        .iter()
        .filter(|c| {
            if has_arguments {
                c.name == word
            } else {
                c.name.starts_with(&word)
            }
        })
        .take(MAX_HINTS)
        .collect()
}

/// Draw the story. Without a session, draw the welcome card.
pub fn body(app: &mut App, ui: &mut Ui) {
    if app.transcript.is_none() {
        welcome(app, ui);
        return;
    }
    let Some(transcript) = app.transcript.as_ref() else {
        return;
    };

    ui.horizontal(|ui| {
        ui.add(Label::new(&transcript.session.name).heading());
        if let Some(world) = &transcript.session.world {
            ui.add(Badge::new(world).variant(BadgeVariant::Secondary));
        }
        for name in &transcript.characters {
            ui.add(Badge::new(name).variant(BadgeVariant::Outline));
        }
    });
    ui.add(Separator::horizontal());

    let speaker = prompts::speaker(&app.settings, &transcript.characters);
    ScrollArea::vertical()
        .id_salt(("chat", transcript.session.id.0))
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = spacing::MD;
            ui.add_space(spacing::SM);
            for node in &transcript.nodes {
                match node.kind {
                    NodeKind::User => ui.add(ChatBubble::user(&node.content)),
                    NodeKind::Character => ui.add(ChatBubble::character(
                        node.speaker.as_deref().unwrap_or("Character"),
                        &node.content,
                    )),
                    NodeKind::Narrator | NodeKind::System => {
                        ui.add(ChatBubble::narrator(&node.content))
                    }
                };
            }
            if app.chat.generating {
                let text = &app.chat.streaming;
                match &speaker {
                    Some(name) => ui.add(ChatBubble::character(name, text).streaming(true)),
                    None => ui.add(ChatBubble::narrator(text).streaming(true)),
                };
            }
            for notice in &app.notices {
                Card::new()
                    .content(|ui| {
                        let mut label = Label::new(&notice.text).small().muted();
                        if notice.monospace {
                            label = label.monospace();
                        }
                        ui.add(label);
                    })
                    .show(ui);
            }
            ui.add_space(spacing::SM);
        });
}

fn welcome(app: &mut App, ui: &mut Ui) {
    let mut create = false;
    Card::new()
        .header(|ui| {
            Card::title(ui, "Welcome to InfiniteRoleplay");
            Card::description(ui, "Stories that never run out of room.");
        })
        .content(|ui| {
            ui.add(Label::new(
                "Start a session to begin. Pick a character and a world, then just write. \
                 Older events are summarized and important facts remembered, so the plot holds together.",
            ));
            ui.add(
                Label::new("Open Settings first to choose a provider and add an API key.")
                    .small()
                    .muted(),
            );
        })
        .footer(|ui| {
            create = ui.add(Button::new("New session")).clicked();
        })
        .show(ui);
    if create {
        app.open_new_session();
    }
}

/// Draw the message box and its buttons.
pub fn composer(app: &mut App, ui: &mut Ui) {
    for hint in command_hints(&app.chat.input) {
        ui.add(
            Label::new(format!("{}   {}", hint.usage, hint.description))
                .small()
                .muted(),
        );
    }

    let generating = app.chat.generating;
    let (mut send, mut stop) = (false, false);
    ui.horizontal_top(|ui| {
        let width = (ui.available_width() - BUTTON_COLUMN - ui.spacing().item_spacing.x).max(0.0);
        ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
            let response = ui.add(
                Textarea::new(&mut app.chat.input)
                    .rows(2)
                    .placeholder("Write your message, or type / for commands")
                    .id_salt("chat-input"),
            );
            let ctrl_enter = ui.input(|i| i.modifiers.command && i.key_pressed(Key::Enter));
            send |= response.has_focus() && ctrl_enter;
        });
        ui.vertical(|ui| {
            ui.set_width(BUTTON_COLUMN);
            if generating {
                stop |= ui
                    .add(Button::new("Stop").variant(ButtonVariant::Outline))
                    .clicked();
            } else {
                send |= ui.add(Button::new("Send")).clicked();
            }
            ui.add(Label::new("Ctrl+Enter").small().muted());
        });
    });

    if send && !generating {
        app.send_chat();
    }
    if stop {
        app.backend.send(Request::Stop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(input: &str) -> Vec<&'static str> {
        command_hints(input).into_iter().map(|c| c.name).collect()
    }

    #[test]
    fn plain_text_has_no_hints() {
        assert!(names("hello").is_empty());
        assert!(names("").is_empty());
        assert!(names("say /tree").is_empty());
    }

    #[test]
    fn a_bare_slash_lists_the_first_commands() {
        assert_eq!(names("/").len(), MAX_HINTS);
        assert_eq!(names("/")[0], "save");
    }

    #[test]
    fn hints_narrow_as_you_type() {
        assert_eq!(names("/b"), ["branch", "branches"]);
        assert_eq!(names("/br"), ["branch", "branches"]);
        assert_eq!(names("/branche"), ["branches"]);
        assert_eq!(names("/TREE"), ["tree"]);
        assert!(names("/zzz").is_empty());
    }

    #[test]
    fn once_arguments_start_only_the_exact_command_is_shown() {
        assert_eq!(names("/goto #4"), ["goto"]);
        assert_eq!(names("/mark my label"), ["mark"]);
        assert!(
            names("/go #4").is_empty(),
            "a partial word with arguments is not a command"
        );
    }
}
