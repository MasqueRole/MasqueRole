//! Shows every `ir-ui` component in every variant, in light and dark.
//!
//! Run with `cargo run -p ir-ui --example gallery [light|dark] [end]`.

use eframe::egui;
use ir_ui::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, Label, Separator,
};
use ir_ui::{spacing, Theme, ThemeExt, ThemeMode};

struct Gallery {
    mode: ThemeMode,
    /// Scroll to the bottom on the first frame (`end` argument), for screenshots.
    scroll_to_end: bool,
}

impl Gallery {
    fn section(ui: &mut egui::Ui, title: &str) {
        ui.add_space(spacing::LG);
        ui.add(Label::new(title).heading());
        ui.add(Separator::horizontal());
    }
}

impl eframe::App for Gallery {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(Label::new("ir-ui gallery").heading());
                ui.add_space(spacing::LG);
                for (label, mode) in [
                    ("System", ThemeMode::System),
                    ("Light", ThemeMode::Light),
                    ("Dark", ThemeMode::Dark),
                ] {
                    let variant = if self.mode == mode {
                        ButtonVariant::Default
                    } else {
                        ButtonVariant::Outline
                    };
                    if ui
                        .add(Button::new(label).variant(variant).size(ButtonSize::Sm))
                        .clicked()
                    {
                        self.mode = mode;
                        Theme::set_mode(ctx, mode);
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut scroll = egui::ScrollArea::vertical();
            if std::mem::take(&mut self.scroll_to_end) {
                scroll = scroll.vertical_scroll_offset(100_000.0);
            }
            scroll.show(ui, |ui| {
                Self::section(ui, "Button");
                for (name, variant) in [
                    ("Default", ButtonVariant::Default),
                    ("Secondary", ButtonVariant::Secondary),
                    ("Outline", ButtonVariant::Outline),
                    ("Ghost", ButtonVariant::Ghost),
                    ("Destructive", ButtonVariant::Destructive),
                    ("Link", ButtonVariant::Link),
                ] {
                    ui.horizontal(|ui| {
                        ui.add_sized([88.0, 32.0], Label::new(name).muted().small());
                        for (text, size) in [
                            ("Small", ButtonSize::Sm),
                            ("Medium", ButtonSize::Md),
                            ("Large", ButtonSize::Lg),
                            ("+", ButtonSize::Icon),
                        ] {
                            ui.add(Button::new(text).variant(variant).size(size));
                        }
                        ui.add(Button::new("Disabled").variant(variant).enabled(false));
                    });
                }

                Self::section(ui, "Label");
                ui.add(Label::new("Heading").heading());
                ui.add(Label::new("Default text"));
                ui.add(Label::new("Small text").small());
                ui.add(Label::new("Muted text").muted());
                ui.add(Label::new("Error text").tone(ir_ui::components::LabelTone::Destructive));

                Self::section(ui, "Badge");
                ui.horizontal(|ui| {
                    for (text, variant) in [
                        ("Default", BadgeVariant::Default),
                        ("Secondary", BadgeVariant::Secondary),
                        ("Outline", BadgeVariant::Outline),
                        ("Destructive", BadgeVariant::Destructive),
                    ] {
                        ui.add(Badge::new(text).variant(variant));
                    }
                });

                Self::section(ui, "Separator");
                ui.add(Separator::horizontal());
                ui.horizontal(|ui| {
                    ui.add(Label::new("Left"));
                    ui.add_sized([1.0, 20.0], Separator::vertical());
                    ui.add(Label::new("Right"));
                });

                Self::section(ui, "Card");
                Card::new()
                    .header(|ui| {
                        Card::title(ui, "The Misty Coast");
                        Card::description(ui, "A coastal region wrapped in fog.");
                    })
                    .content(|ui| {
                        ui.add(Label::new("Cards group related content."));
                    })
                    .footer(|ui| {
                        ui.horizontal(|ui| {
                            ui.add(Button::new("Open"));
                            ui.add(Button::new("Cancel").variant(ButtonVariant::Outline));
                        });
                    })
                    .show(ui);

                Self::section(ui, "Tree colors");
                ui.horizontal(|ui| {
                    let t = ui.ir_theme();
                    for (name, color) in [
                        ("current", t.tree_current),
                        ("explored", t.tree_explored),
                        ("open", t.tree_open),
                    ] {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(spacing::XL, spacing::XL),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, t.corner_sm(), color);
                        ui.add(Label::new(name).muted().small());
                    }
                });
                ui.add_space(spacing::XL);
            });
        });
    }
}

fn main() -> eframe::Result {
    let mode = match std::env::args().nth(1).as_deref() {
        Some("light") => ThemeMode::Light,
        Some("dark") => ThemeMode::Dark,
        _ => ThemeMode::System,
    };
    eframe::run_native(
        "ir-ui gallery",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([760.0, 1000.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            Theme::apply(&cc.egui_ctx);
            Theme::set_mode(&cc.egui_ctx, mode);
            Ok(Box::new(Gallery {
                mode,
                scroll_to_end: std::env::args().any(|a| a == "end"),
            }))
        }),
    )
}
