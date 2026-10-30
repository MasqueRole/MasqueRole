//! The chat screen: the story so far, and the box for the next message.

use eframe::egui::{self, Key, Modifiers, Ui};
use ir_core::commands::{CommandInfo, COMMANDS};
use ir_core::db::NodeKind;
use ir_core::settings::Settings;
use ir_ui::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, ChatBubble, IconKind, Label,
    ScrollArea, Separator, Suggestion, Suggestions, Textarea, TooltipExt,
};
use ir_ui::spacing;

use crate::app::App;
use crate::backend::Request;
use crate::format;
use crate::prompts;

/// Width of the column holding the Send and Stop buttons.
const BUTTON_COLUMN: f32 = 32.0;

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

/// The token usage shown in the story header: nothing until something was used or a limit
/// is on, and a warning color once the limit is reached.
fn usage_badge(settings: &Settings, used: i64) -> Option<(String, BadgeVariant)> {
    let limits = &settings.limits;
    if !limits.token_limit_enabled {
        return (used > 0).then(|| {
            (
                format!("~{} tokens", format::tokens(used)),
                BadgeVariant::Outline,
            )
        });
    }
    let variant = if limits.reached(used) {
        BadgeVariant::Destructive
    } else {
        BadgeVariant::Outline
    };
    let text = format!(
        "~{} / {} tokens",
        format::tokens(used),
        format::tokens(i64::from(limits.token_limit))
    );
    Some((text, variant))
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
        if let Some((text, variant)) = usage_badge(&app.settings, transcript.session.tokens_used) {
            ui.add(Badge::new(text).variant(variant))
                .ir_tooltip("Estimated tokens used by this story");
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
            create = ui
                .add(Button::new("New session").icon(IconKind::Plus))
                .clicked();
        })
        .show(ui);
    if create {
        app.open_new_session();
    }
}

/// The icon of a slash command, by name.
fn command_icon(name: &str) -> Option<IconKind> {
    Some(match name {
        "save" => IconKind::Save,
        "load" => IconKind::Folder,
        "retry" => IconKind::Refresh,
        "undo" => IconKind::Undo,
        "tree" | "branch" => IconKind::Branch,
        "goto" => IconKind::ArrowRight,
        "mark" => IconKind::Bookmark,
        "branches" => IconKind::List,
        "prune" | "forget" => IconKind::Trash,
        "memory" => IconKind::Database,
        "summary" => IconKind::File,
        "char" => IconKind::User,
        "style" => IconKind::Pen,
        "narrator" => IconKind::Chat,
        _ => return None,
    })
}

/// The suggestions for what is typed: the matching commands, each with its icon.
fn command_suggestions(input: &str) -> Vec<Suggestion> {
    command_hints(input)
        .into_iter()
        .map(|c| Suggestion {
            icon: command_icon(c.name).unwrap_or(IconKind::Chat),
            title: c.usage.to_string(),
            description: c.description.to_string(),
        })
        .collect()
}

/// What choosing a suggestion types into the box: the command and a space, ready for its
/// arguments.
fn completion(input: &str, index: usize) -> Option<String> {
    let command = command_hints(input).get(index).copied()?;
    Some(format!("/{} ", command.name))
}

/// Put the cursor at the end of the message box and focus it.
fn focus_at_end(ctx: &egui::Context, id: egui::Id, text: &str) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        let end = egui::text::CCursor::new(text.chars().count());
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(ctx, id);
    }
    ctx.memory_mut(|m| m.request_focus(id));
}

/// Draw the message box and its buttons.
pub fn composer(app: &mut App, ui: &mut Ui) {
    let generating = app.chat.generating;
    let (mut send, mut stop) = (false, false);
    let mut input_rect = egui::Rect::NOTHING;
    ui.horizontal(|ui| {
        let width = (ui.available_width() - BUTTON_COLUMN - ui.spacing().item_spacing.x).max(0.0);
        ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
            // Enter sends and Shift+Enter starts a new line. The key must be taken before the
            // text field sees it, or the field would insert a line break.
            let focused = app
                .chat
                .input_id
                .is_some_and(|id| ui.memory(|m| m.has_focus(id)));
            if focused {
                send |= ui.input_mut(|i| {
                    // `consume_key` ignores Shift, so it is checked by hand.
                    !i.modifiers.shift
                        && (i.consume_key(Modifiers::NONE, Key::Enter)
                            || i.consume_key(Modifiers::COMMAND, Key::Enter))
                });
            }
            let response = ui.add(
                Textarea::new(&mut app.chat.input)
                    .rows(2)
                    .placeholder("Write your message, or type / for commands")
                    .id_salt("chat-input"),
            );
            app.chat.input_id = Some(response.id);
            input_rect = response.rect;
        });
        // Icon-only buttons, aligned with the middle of the message box.
        ui.vertical(|ui| {
            ui.set_width(BUTTON_COLUMN);
            ui.add_space(spacing::SM);
            if generating {
                let button = Button::new("")
                    .icon(IconKind::Stop)
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Icon);
                stop |= ui.add(button).ir_tooltip("Stop").clicked();
            } else {
                let button = Button::new("").icon(IconKind::Send).size(ButtonSize::Icon);
                let hint = "Send (Enter). Shift+Enter starts a new line.";
                send |= ui.add(button).ir_tooltip(hint).clicked();
            }
        });
    });

    // The matching commands float above the box. A press on the list takes focus from the
    // box, so the list also stays open while the pointer is on it.
    let focused = app
        .chat
        .input_id
        .is_some_and(|id| ui.memory(|m| m.has_focus(id)));
    let suggestions = if focused || app.chat.suggestions_hovered {
        command_suggestions(&app.chat.input)
    } else {
        Vec::new()
    };
    let output = Suggestions::new("command-suggestions", input_rect, &suggestions).show(ui.ctx());
    app.chat.suggestions_hovered = output.hovered && !suggestions.is_empty();
    if let Some(completed) = output.picked.and_then(|i| completion(&app.chat.input, i)) {
        app.chat.input = completed;
        if let Some(id) = app.chat.input_id {
            focus_at_end(ui.ctx(), id, &app.chat.input);
        }
    }

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
    fn every_command_has_its_own_icon() {
        for command in COMMANDS {
            assert!(
                command_icon(command.name).is_some(),
                "/{} has no icon",
                command.name
            );
        }
        assert_eq!(command_icon("nonsense"), None);
    }

    #[test]
    fn suggestions_follow_the_hints_and_completing_types_the_command() {
        let suggestions = command_suggestions("/br");
        let titles: Vec<_> = suggestions.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["/branch [label]", "/branches"]);
        assert_eq!(suggestions[1].icon, IconKind::List);
        assert!(command_suggestions("hello").is_empty());

        assert_eq!(completion("/br", 1).as_deref(), Some("/branches "));
        assert_eq!(completion("/br", 0).as_deref(), Some("/branch "));
        assert_eq!(completion("/zzz", 0), None);
        assert_eq!(completion("/br", 9), None);
    }

    #[test]
    fn the_usage_badge_depends_on_the_limit() {
        let mut settings = Settings::default();
        assert_eq!(usage_badge(&settings, 0), None, "nothing to show yet");
        let (text, variant) = usage_badge(&settings, 12_345).unwrap();
        assert_eq!(
            (text.as_str(), variant),
            ("~12.3k tokens", BadgeVariant::Outline)
        );

        settings.limits.token_limit_enabled = true;
        settings.limits.token_limit = 20_000;
        let (text, variant) = usage_badge(&settings, 12_345).unwrap();
        assert_eq!(
            (text.as_str(), variant),
            ("~12.3k / 20k tokens", BadgeVariant::Outline)
        );
        let (_, variant) = usage_badge(&settings, 20_000).unwrap();
        assert_eq!(variant, BadgeVariant::Destructive);
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
