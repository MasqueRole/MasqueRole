//! The story info page: the world and the characters of the open story, as a full tab in
//! place of the chat, like the info page of a group chat. The world's picture is the
//! backdrop of the page, fading into the background.

use eframe::egui::{
    self,
    epaint::{RectShape, Vertex},
    pos2, vec2, Color32, CornerRadius, Mesh, Rect, Shape, TextureHandle, Ui,
};
use ir_core::lorebook::Lorebook;
use ir_ui::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, IconKind, Label, Panel, Picture,
    ScrollArea, Separator,
};
use ir_ui::spacing;
use ir_ui::theme::{ThemeExt, BORDER_WIDTH};

use crate::app::App;
use crate::backend::Request;
use crate::format;

/// Size of a character's picture in the list.
const AVATAR_SIZE: f32 = 40.0;

/// Height of the world's picture behind the top of the content area.
const BANNER_HEIGHT: f32 = 240.0;

/// A small muted caption above a text, left out when the text is empty.
fn field(ui: &mut Ui, caption: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    ui.add(Label::new(caption).small().muted());
    ui.add(Label::new(text.trim()));
}

/// Paint the world's picture across the top of the whole content area, edge to edge and cut
/// off by its rounded corner, fading into the page background.
fn paint_backdrop(ui: &Ui, texture: &TextureHandle) {
    let t = ui.ir_theme();
    let sheet = Panel::surface_rect(ui.ctx()).shrink(BORDER_WIDTH);
    let rect = Rect::from_min_size(sheet.min, vec2(sheet.width(), BANNER_HEIGHT));
    // Crop the picture from the middle to fill the rect.
    let aspect = texture.aspect_ratio() / (rect.width() / rect.height()).max(f32::EPSILON);
    let (u, v) = if aspect >= 1.0 {
        (1.0 / aspect, 1.0)
    } else {
        (1.0, aspect)
    };
    let uv = Rect::from_min_max(
        pos2((1.0 - u) / 2.0, (1.0 - v) / 2.0),
        pos2((1.0 + u) / 2.0, (1.0 + v) / 2.0),
    );
    let painter = ui.ctx().layer_painter(egui::LayerId::background());
    let corner = CornerRadius {
        nw: t.radius_lg,
        ..CornerRadius::ZERO
    };
    painter.add(RectShape::filled(rect, corner, Color32::WHITE).with_texture(texture.id(), uv));
    // The fade: clear at the top, the card color the sheet is made of at the bottom.
    let mut fade = Mesh::default();
    let corners = [
        (rect.left_top(), Color32::TRANSPARENT),
        (rect.right_top(), Color32::TRANSPARENT),
        (rect.right_bottom(), t.card),
        (rect.left_bottom(), t.card),
    ];
    for (pos, color) in corners {
        fade.vertices.push(Vertex {
            pos,
            uv: egui::epaint::WHITE_UV,
            color,
        });
    }
    fade.indices.extend([0, 1, 2, 0, 2, 3]);
    painter.add(Shape::mesh(fade));
}

fn world_section(ui: &mut Ui, name: &str, world: Option<&Lorebook>) {
    ui.add(Label::new("World").small().muted());
    ui.add(Label::new(name).heading());
    match world {
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
    }
}

/// One character as a list item: picture, name, and a button to take it out of the story.
/// Returns whether remove was clicked.
fn character_item(ui: &mut Ui, name: &str, texture: Option<&TextureHandle>) -> bool {
    let mut remove = false;
    ui.horizontal(|ui| {
        ui.add(
            Picture::new(texture, IconKind::User)
                .size(AVATAR_SIZE)
                .circle(),
        );
        let room = (ui.available_width() - 32.0 - spacing::SM).max(0.0);
        ui.add(Label::new(name).max_width(room));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let button = Button::new("")
                .icon(IconKind::Trash)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Icon);
            remove = ui
                .add(button)
                .on_hover_text("Remove from the story")
                .clicked();
        });
    });
    remove
}

/// Draw the page. The back button returns to the chat.
pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(transcript) = app.transcript.clone() else {
        app.story_info = false;
        return;
    };
    let ctx = ui.ctx().clone();
    let world = transcript.session.world.as_deref().map(|name| {
        let card = app
            .worlds
            .iter()
            .find(|e| e.item.name == name)
            .map(|e| e.item.clone());
        let banner = app.world_picture(&ctx, name);
        (name.to_string(), card, banner)
    });
    let characters: Vec<_> = transcript
        .characters
        .iter()
        .map(|name| {
            let image = app
                .characters
                .iter()
                .find(|e| e.item.name == *name)
                .and_then(|e| e.item.image.clone());
            let texture = image.and_then(|image| app.picture(&ctx, &image));
            (name.clone(), texture)
        })
        .collect();

    let mut remove = None;
    ScrollArea::vertical().id_salt("story-info").show(ui, |ui| {
        let banner = world.as_ref().and_then(|(_, _, banner)| banner.as_ref());
        if let Some(texture) = banner {
            paint_backdrop(ui, texture);
        }
        ui.spacing_mut().item_spacing.y = spacing::MD;
        let back = Button::new("Back")
            .icon(IconKind::ArrowLeft)
            .variant(ButtonVariant::Outline);
        if ui.add(back).clicked() {
            app.story_info = false;
        }
        if banner.is_some() {
            // The title sits low on the picture, where it has faded out.
            ui.add_space(BANNER_HEIGHT - 150.0);
        }
        let room = ui.available_width();
        ui.add(
            Label::new(&transcript.session.name)
                .heading()
                .max_width(room),
        );
        ui.add(
            Label::new(format!(
                "{} \u{b7} {} \u{b7} ~{} tokens used",
                format::plural(transcript.nodes.len(), "message"),
                format::plural(transcript.branches.len().max(1), "storyline"),
                format::tokens(transcript.session.tokens_used)
            ))
            .small()
            .muted(),
        );
        ui.add(Separator::horizontal());
        match &world {
            Some((name, card, _)) => world_section(ui, name, card.as_ref()),
            None => {
                ui.add(Label::new("World").small().muted());
                ui.add(Label::new("This story has no world.").small().muted());
            }
        }
        ui.add(Separator::horizontal());
        ui.add(Label::new("Characters").small().muted());
        if characters.is_empty() {
            ui.add(Label::new("Nobody is in the scene yet.").small().muted());
        }
        for (name, texture) in &characters {
            if character_item(ui, name, texture.as_ref()) {
                remove = Some(name.clone());
            }
        }
    });
    if let Some(name) = remove {
        app.backend.send(Request::RemoveCharacter(name));
    }
}
