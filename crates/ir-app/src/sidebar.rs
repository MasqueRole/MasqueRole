//! The sidebar: the list that belongs to the section chosen in the top navigation.

use eframe::egui::Ui;
use ir_ui::components::{Button, ButtonSize, ButtonVariant, Label, ScrollArea};
use ir_ui::spacing;

use crate::app::{App, Confirm, Section};
use crate::backend::Request;
use crate::characters::{CharacterForm, WorldForm};

/// Width of the small icon buttons next to list rows.
const ICON_WIDTH: f32 = 32.0;

/// Draw the sidebar for the current section.
pub fn show(app: &mut App, ui: &mut Ui) {
    ScrollArea::vertical().id_salt("sidebar").show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = spacing::XS;
        match app.section {
            Section::Stories | Section::Tree => sessions(app, ui),
            Section::Characters => characters(app, ui),
            Section::Worlds => worlds(app, ui),
        }
    });
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(spacing::XS);
    ui.add(Label::new(text).small().muted());
    ui.add_space(spacing::XS);
}

fn create_button(ui: &mut Ui, text: &str) -> bool {
    ui.add(
        Button::new(text)
            .variant(ButtonVariant::Outline)
            .fill_width(true),
    )
    .clicked()
}

fn empty(ui: &mut Ui, text: &str) {
    ui.add_space(spacing::SM);
    ui.add(Label::new(text).small().muted());
}

/// A list row with a delete button at its right edge. Returns `(row clicked, delete clicked)`.
fn row(ui: &mut Ui, text: &str, selected: bool) -> (bool, bool) {
    let (mut open, mut delete) = (false, false);
    ui.horizontal(|ui| {
        let width = (ui.available_width() - ICON_WIDTH - ui.spacing().item_spacing.x).max(0.0);
        // A scope, not `allocate_ui`, so the row is exactly as tall as its button.
        ui.scope(|ui| {
            ui.set_max_width(width);
            let variant = if selected {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Ghost
            };
            open = ui
                .add(Button::new(text).variant(variant).fill_width(true))
                .clicked();
        });
        delete = ui
            .add(
                Button::new("\u{d7}")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Icon),
            )
            .clicked();
    });
    (open, delete)
}

fn sessions(app: &mut App, ui: &mut Ui) {
    heading(ui, "STORIES");
    if create_button(ui, "New session") {
        app.open_new_session();
    }
    if app.sessions.is_empty() {
        empty(ui, "No stories yet.");
    }
    let active = app.transcript.as_ref().map(|t| t.session.id);
    for session in app.sessions.clone() {
        let (open, delete) = row(ui, &session.name, active == Some(session.id));
        if open {
            app.backend.send(Request::OpenSession(session.id));
        }
        if delete {
            app.confirm = Some(Confirm::DeleteSession {
                id: session.id,
                name: session.name.clone(),
            });
        }
    }
}

fn characters(app: &mut App, ui: &mut Ui) {
    heading(ui, "CHARACTERS");
    if create_button(ui, "New character") {
        app.character_form = Some(CharacterForm::new());
    }
    if app.characters.is_empty() {
        empty(ui, "No characters yet.");
    }
    for entry in app.characters.clone() {
        let editing = app
            .character_form
            .as_ref()
            .is_some_and(|f| f.path.as_ref() == Some(&entry.path));
        let (open, delete) = row(ui, &entry.item.name, editing);
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
    heading(ui, "WORLDS");
    if create_button(ui, "New world") {
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
        let (open, delete) = row(ui, &entry.item.name, editing);
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
