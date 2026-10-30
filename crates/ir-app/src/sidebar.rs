//! The sidebar: sessions, characters, and worlds.

use eframe::egui::Ui;
use ir_ui::components::{Button, ButtonSize, ButtonVariant, Label, ScrollArea};
use ir_ui::spacing;

use crate::app::{App, Confirm, Screen};
use crate::backend::Request;
use crate::characters::{CharacterForm, WorldForm};

/// Width of the small icon buttons next to list rows.
const ICON_WIDTH: f32 = 32.0;

/// Draw the sidebar.
pub fn show(app: &mut App, ui: &mut Ui) {
    ScrollArea::vertical().id_salt("sidebar").show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = spacing::XS;
        sessions(app, ui);
        characters(app, ui);
        worlds(app, ui);
    });
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(spacing::MD);
    ui.add(Label::new(text).small().muted());
    ui.add_space(spacing::XS);
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
    heading(ui, "SESSIONS");
    if ui
        .add(
            Button::new("New session")
                .variant(ButtonVariant::Outline)
                .fill_width(true),
        )
        .clicked()
    {
        app.open_new_session();
    }
    let active = app.transcript.as_ref().map(|t| t.session.id);
    for session in app.sessions.clone() {
        let (open, delete) = row(ui, &session.name, active == Some(session.id));
        if open {
            app.screen = Screen::Chat;
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
    if ui
        .add(
            Button::new("New character")
                .variant(ButtonVariant::Outline)
                .fill_width(true),
        )
        .clicked()
    {
        app.screen = Screen::Character(CharacterForm::new());
    }
    for entry in app.characters.clone() {
        let editing =
            matches!(&app.screen, Screen::Character(f) if f.path.as_ref() == Some(&entry.path));
        let (open, delete) = row(ui, &entry.item.name, editing);
        if open {
            app.screen = Screen::Character(CharacterForm::from_character(
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
    if ui
        .add(
            Button::new("New world")
                .variant(ButtonVariant::Outline)
                .fill_width(true),
        )
        .clicked()
    {
        app.screen = Screen::World(WorldForm::new());
    }
    for entry in app.worlds.clone() {
        let editing =
            matches!(&app.screen, Screen::World(f) if f.path.as_ref() == Some(&entry.path));
        let (open, delete) = row(ui, &entry.item.name, editing);
        if open {
            app.screen = Screen::World(WorldForm::from_lorebook(entry.path.clone(), &entry.item));
        }
        if delete {
            app.confirm = Some(Confirm::DeleteWorld {
                path: entry.path,
                name: entry.item.name,
            });
        }
    }
}
