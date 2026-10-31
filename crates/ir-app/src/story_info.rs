//! The story info page: the world and the characters of the open story, as a full tab in
//! place of the chat, like the info page of a group chat.

use eframe::egui::Ui;
use ir_core::character::Character;
use ir_core::lorebook::Lorebook;
use ir_ui::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, IconKind, Label, Picture, ScrollArea,
};
use ir_ui::spacing;

use crate::app::App;
use crate::format;

/// Size of a character's or world's picture on the page.
const PICTURE_SIZE: f32 = 88.0;

/// A small muted caption above a text, left out when the text is empty.
fn field(ui: &mut Ui, caption: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    ui.add(Label::new(caption).small().muted());
    ui.add(Label::new(text.trim()));
}

fn world_card(ui: &mut Ui, name: &str, world: Option<&Lorebook>) {
    Card::new()
        .header(|ui| {
            Card::title(ui, "World");
            Card::description(ui, name);
        })
        .content(|ui| match world {
            Some(world) => {
                field(ui, "Setting", &world.setting);
                if !world.entries.is_empty() {
                    ui.add(Label::new("Lore entries").small().muted());
                    for entry in &world.entries {
                        ui.horizontal_wrapped(|ui| {
                            for key in &entry.keys {
                                ui.add(Badge::new(key).variant(BadgeVariant::Outline));
                            }
                        });
                        ui.add(Label::new(&entry.content).small());
                    }
                }
            }
            None => {
                ui.add(
                    Label::new("This world is not in your library any more. The story keeps working without its lore.")
                        .small()
                        .muted(),
                );
            }
        })
        .show(ui);
}

fn character_card(
    ui: &mut Ui,
    name: &str,
    card: Option<&Character>,
    texture: Option<&eframe::egui::TextureHandle>,
) {
    Card::new()
        .header(|ui| {
            Card::title(ui, name);
            Card::description(ui, "Character");
        })
        .content(|ui| match card {
            Some(card) => {
                ui.horizontal_top(|ui| {
                    ui.add(Picture::new(texture, IconKind::User).size(PICTURE_SIZE));
                    ui.vertical(|ui| {
                        field(ui, "Description", &card.description);
                        if !card.personality.is_empty() {
                            ui.add(Label::new("Personality").small().muted());
                            ui.horizontal_wrapped(|ui| {
                                for trait_ in &card.personality {
                                    ui.add(Badge::new(trait_).variant(BadgeVariant::Secondary));
                                }
                            });
                        }
                    });
                });
                field(ui, "Speaking style", &card.speaking_style);
                field(ui, "Background", &card.background);
                field(ui, "Greeting", &card.greeting);
            }
            None => {
                ui.add(
                    Label::new(
                        "This card is not in your library any more. The story keeps its own copy.",
                    )
                    .small()
                    .muted(),
                );
            }
        })
        .show(ui);
}

/// Draw the page. The back button returns to the chat.
pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(transcript) = app.transcript.clone() else {
        app.story_info = false;
        return;
    };
    let world = transcript.session.world.as_deref().map(|name| {
        let card = app
            .worlds
            .iter()
            .find(|e| e.item.name == name)
            .map(|e| e.item.clone());
        (name.to_string(), card)
    });
    let ctx = ui.ctx().clone();
    let characters: Vec<_> = transcript
        .characters
        .iter()
        .map(|name| {
            let card = app
                .characters
                .iter()
                .find(|e| e.item.name == *name)
                .map(|e| e.item.clone());
            let texture = card
                .as_ref()
                .and_then(|c| c.image.clone())
                .and_then(|image| app.picture(&ctx, &image));
            (name.clone(), card, texture)
        })
        .collect();

    ui.horizontal(|ui| {
        let back = Button::new("Back")
            .icon(IconKind::ArrowLeft)
            .variant(ButtonVariant::Outline);
        if ui.add(back).clicked() {
            app.story_info = false;
        }
        ui.add(Label::new(&transcript.session.name).heading());
    });
    ui.add_space(spacing::SM);
    ScrollArea::vertical().id_salt("story-info").show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = spacing::MD;
        ui.add(
            Label::new(format!(
                "{} messages \u{b7} {} storylines \u{b7} ~{} tokens used",
                transcript.nodes.len(),
                transcript.branches.len().max(1),
                format::tokens(transcript.session.tokens_used)
            ))
            .small()
            .muted(),
        );
        match &world {
            Some((name, card)) => world_card(ui, name, card.as_ref()),
            None => {
                Card::new()
                    .header(|ui| {
                        Card::title(ui, "World");
                        Card::description(ui, "This story has no world.");
                    })
                    .show(ui);
            }
        }
        if characters.is_empty() {
            Card::new()
                .header(|ui| {
                    Card::title(ui, "Characters");
                    Card::description(ui, "Nobody is in the scene yet.");
                })
                .show(ui);
        }
        for (name, card, texture) in &characters {
            character_card(ui, name, card.as_ref(), texture.as_ref());
        }
    });
}
