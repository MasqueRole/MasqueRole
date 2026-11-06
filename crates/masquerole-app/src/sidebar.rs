//! The sidebar: the list that belongs to the section chosen in the top navigation.

use eframe::egui::{Align, Layout, Ui};
use masquerole_ui::components::{
    Button, ButtonSize, ButtonVariant, IconKind, Label, ListItem, MenuItem, ScrollArea, TooltipExt,
};
use masquerole_ui::spacing;

use crate::app::{App, Confirm, Section};
use crate::backend::Request;
use crate::characters::{CharacterForm, WorldForm};

/// Draw the sidebar for the current section.
pub fn show(app: &mut App, ui: &mut Ui) {
    ScrollArea::vertical().id_salt("sidebar").show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = spacing::XS;
        match app.section {
            Section::Stories | Section::Tree => sessions(app, ui),
            Section::Characters => characters(app, ui),
            Section::Worlds => worlds(app, ui),
            Section::Provider => {}
        }
    });
}

/// A list heading with a round button at its right edge that creates a new item. Returns
/// whether the button was clicked.
fn heading(ui: &mut Ui, text: &str, new_item: &str) -> bool {
    heading_with_import(ui, text, new_item, None).0
}

/// Like [`heading`], with a second button left of the first when `import` names its tooltip.
/// Returns `(new clicked, import clicked)`.
fn heading_with_import(
    ui: &mut Ui,
    text: &str,
    new_item: &str,
    import: Option<&str>,
) -> (bool, bool) {
    let (mut new, mut imported) = (false, false);
    ui.add_space(spacing::XS);
    ui.horizontal(|ui| {
        ui.add(Label::new(text).small().muted());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let button = Button::new("").icon(IconKind::Plus).size(ButtonSize::Icon);
            new = ui.add(button).ir_tooltip(new_item).clicked();
            if let Some(tooltip) = import {
                let button = Button::new("")
                    .icon(IconKind::Download)
                    .size(ButtonSize::Icon)
                    .variant(ButtonVariant::Ghost);
                imported = ui.add(button).ir_tooltip(tooltip).clicked();
            }
        });
    });
    ui.add_space(spacing::XS);
    (new, imported)
}

fn empty(ui: &mut Ui, text: &str) {
    ui.add_space(spacing::SM);
    ui.add(Label::new(text).small().muted());
}

/// A list row whose three-dots menu holds a red delete item. Returns `(row clicked, delete
/// chosen)`.
fn row(ui: &mut Ui, id: impl eframe::egui::AsIdSalt, text: &str, selected: bool) -> (bool, bool) {
    let out = ListItem::new(id, text).selected(selected).show(ui, |ui| {
        ui.add(
            MenuItem::new("Delete")
                .icon(IconKind::Trash)
                .destructive(true),
        )
        .clicked()
    });
    (out.clicked, out.menu == Some(true))
}

fn sessions(app: &mut App, ui: &mut Ui) {
    if heading(ui, "STORIES", "New session") {
        app.open_new_session();
    }
    if app.sessions.is_empty() {
        empty(ui, "No stories yet.");
    }
    let active = app.transcript.as_ref().map(|t| t.session.id);
    for session in app.sessions.clone() {
        let (open, delete) = row(
            ui,
            ("session", session.id.0),
            &session.name,
            active == Some(session.id),
        );
        if open {
            app.backend.send(Request::OpenSession(session.id));
        }
        if delete {
            app.confirm = Some(Confirm::DeleteSession {
                id: session.id,
                name: session.name.clone(),
            });
        }
        if active == Some(session.id) && app.section == Section::Stories {
            sub_chats(app, ui);
        }
    }
}

/// The storylines of the open story, indented below its row like the sub-chats of a chat.
/// A story with a single storyline has none to list. Only the stories section shows them.
fn sub_chats(app: &mut App, ui: &mut Ui) {
    let Some(transcript) = &app.transcript else {
        return;
    };
    if transcript.branches.len() < 2 {
        return;
    }
    let mut jump = None;
    for branch in &transcript.branches {
        let label = format!("{} {}", crate::format::tag(branch.tip), branch.label);
        ui.horizontal(|ui| {
            ui.add_space(spacing::LG);
            let out = ListItem::new(("branch", branch.tip.0), &label)
                .selected(branch.contains_current)
                .menu(false)
                .show(ui, |_| ());
            if out.clicked {
                jump = Some(branch.tip);
            }
        });
    }
    if let Some(tip) = jump {
        app.backend.send(Request::JumpTo(tip));
    }
}

fn characters(app: &mut App, ui: &mut Ui) {
    let (new, import) = heading_with_import(
        ui,
        "CHARACTERS",
        "New character",
        Some("Import a character (PNG, JSON, or YAML)"),
    );
    if new {
        app.character_form = Some(CharacterForm::new());
    }
    if import {
        app.backend.send(Request::ImportCharacter);
    }
    if app.characters.is_empty() {
        empty(ui, "No characters yet.");
    }
    for entry in app.characters.clone() {
        let editing = app
            .character_form
            .as_ref()
            .is_some_and(|f| f.path.as_ref() == Some(&entry.path));
        let (open, delete) = row(ui, ("character", &entry.path), &entry.item.name, editing);
        if open {
            app.character_form = Some(CharacterForm::from_character(
                entry.path.clone(),
                &entry.item,
            ));
        }
        if delete {
            app.confirm = Some(Confirm::DeleteCharacter {
                path: entry.path,
                name: entry.item.name,
            });
        }
    }
}

fn worlds(app: &mut App, ui: &mut Ui) {
    if heading(ui, "WORLDS", "New world") {
        app.world_form = Some(WorldForm::new());
    }
    if app.worlds.is_empty() {
        empty(ui, "No worlds yet.");
    }
    for entry in app.worlds.clone() {
        let editing = app
            .world_form
            .as_ref()
            .is_some_and(|f| f.path.as_ref() == Some(&entry.path));
        let (open, delete) = row(ui, ("world", &entry.path), &entry.item.name, editing);
        if open {
            app.world_form = Some(WorldForm::from_lorebook(entry.path.clone(), &entry.item));
        }
        if delete {
            app.confirm = Some(Confirm::DeleteWorld {
                path: entry.path,
                name: entry.item.name,
            });
        }
    }
}
