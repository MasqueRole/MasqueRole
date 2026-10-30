//! Shows every `ir-ui` component in every variant, in light and dark.
//!
//! Run with `cargo run -p ir-ui --example gallery [light|dark] [options]`. Options, handy
//! for screenshots: `scroll=<pixels>` (or `end`) starts scrolled down, `dialog` opens the
//! dialog, and `toasts` shows two toasts.

use eframe::egui;
use ir_ui::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, ChatBubble, Checkbox, Dialog,
    FormField, Icon, Input, Label, LabelTone, Panel, Popover, ScrollArea, Select, Separator,
    Slider, Switch, Tabs, Textarea, Toast, Toasts, TooltipExt, TreeNode, TreeNodeState,
};
use ir_ui::{spacing, Theme, ThemeExt, ThemeMode};

struct Gallery {
    mode: ThemeMode,
    /// Vertical scroll offset to apply on the first frame.
    initial_scroll: Option<f32>,

    name: String,
    secret: String,
    notes: String,
    checked: bool,
    narrator: bool,
    temperature: f64,
    provider: usize,
    tab: usize,
    node_selected: bool,
    dialog_open: bool,
    toasts: Toasts,
    logo: Option<egui::TextureHandle>,
}

impl Gallery {
    fn new(
        mode: ThemeMode,
        initial_scroll: Option<f32>,
        dialog_open: bool,
        show_toasts: bool,
    ) -> Self {
        let mut toasts = Toasts::new();
        if show_toasts {
            toasts.push(Toast::new("Session saved").description("Everything is stored locally."));
            toasts.push(Toast::destructive("Could not reach the provider").seconds(3600.0));
        }
        Self {
            mode,
            initial_scroll,
            name: "Mira Valen".into(),
            secret: "not-a-real-key".into(),
            notes: String::new(),
            checked: true,
            narrator: false,
            temperature: 0.9,
            provider: 1,
            tab: 0,
            node_selected: false,
            dialog_open,
            toasts,
            logo: None,
        }
    }

    /// The app logo, decoded on first use.
    fn logo(&mut self, ctx: &egui::Context) -> Option<&egui::TextureHandle> {
        if self.logo.is_none() {
            let png = include_bytes!("../../../assets/logo.png");
            let icon = eframe::icon_data::from_png_bytes(png).ok()?;
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            );
            self.logo = Some(ctx.load_texture("logo", image, egui::TextureOptions::LINEAR));
        }
        self.logo.as_ref()
    }

    fn section(ui: &mut egui::Ui, title: &str) {
        ui.add_space(spacing::LG);
        ui.add(Label::new(title).heading());
        ui.add(Separator::horizontal());
    }

    fn buttons(ui: &mut egui::Ui) {
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
        ui.add_space(spacing::SM);
        ui.add(Label::new("Full width, for list rows").muted().small());
        ui.scope(|ui| {
            ui.set_max_width(260.0);
            ui.add(
                Button::new("A selected row")
                    .variant(ButtonVariant::Secondary)
                    .fill_width(true),
            );
            ui.add(
                Button::new("A row")
                    .variant(ButtonVariant::Ghost)
                    .fill_width(true),
            );
            ui.add(
                Button::new(
                    "A row whose name is far too long to fit in the space it has been given",
                )
                .variant(ButtonVariant::Ghost)
                .fill_width(true),
            );
        });
    }

    fn text(ui: &mut egui::Ui) {
        Self::section(ui, "Label");
        ui.add(Label::new("Heading").heading());
        ui.add(Label::new("Default text"));
        ui.add(Label::new("Small text").small());
        ui.add(Label::new("Muted text").muted());
        ui.add(Label::new("Error text").tone(LabelTone::Destructive));
        ui.add(
            Label::new(
                "\u{25cf} Monospace  [#1]\n\u{2514}\u{2500}\u{25cb} for aligned output  [#2]",
            )
            .monospace(),
        );

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
    }

    fn inputs(&mut self, ui: &mut egui::Ui) {
        Self::section(ui, "Input, Textarea, and FormField");
        FormField::new("Character name")
            .help("Shown above the character's messages.")
            .required(true)
            .show(ui, |ui| {
                ui.add(Input::new(&mut self.name).placeholder("Name"))
            });
        FormField::new("API key")
            .error("The key was rejected by the provider.")
            .show(ui, |ui| {
                ui.add(
                    Input::new(&mut self.secret)
                        .password(true)
                        .id_salt("secret"),
                )
            });
        FormField::new("Notes").show(ui, |ui| {
            ui.add(
                Textarea::new(&mut self.notes)
                    .placeholder("Anything to remember about this scene")
                    .rows(3),
            )
        });
        FormField::new("Disabled").show(ui, |ui| {
            let mut locked = String::from("Read only");
            ui.add(Input::new(&mut locked).enabled(false))
        });

        Self::section(ui, "Checkbox, Switch, and Slider");
        ui.add(Checkbox::new(&mut self.checked, "Stream responses"));
        ui.add(Checkbox::new(&mut self.checked, "Disabled").enabled(false));
        ui.add(Switch::new(&mut self.narrator).label("Narrator mode"));
        ui.add(
            Switch::new(&mut self.narrator)
                .label("Disabled")
                .enabled(false),
        );
        ui.horizontal(|ui| {
            ui.add_sized([88.0, 20.0], Label::new("Temperature").muted().small());
            ui.add(
                Slider::new(&mut self.temperature, 0.0..=2.0)
                    .step(0.1)
                    .decimals(1),
            );
        });
        ui.add(
            Slider::new(&mut self.temperature, 0.0..=2.0)
                .step(0.1)
                .decimals(1)
                .enabled(false),
        );

        Self::section(ui, "Select");
        ui.add(
            Select::new(
                "provider",
                &mut self.provider,
                ["OpenAI", "xAI", "Ollama (local)"],
            )
            .width(220.0),
        );
        ui.add(
            Select::new("provider-off", &mut self.provider, ["OpenAI", "xAI"])
                .width(220.0)
                .enabled(false),
        );
    }

    fn navigation(&mut self, ui: &mut egui::Ui) {
        Self::section(ui, "Tabs");
        ui.add(Tabs::new(&mut self.tab, ["Chat", "Memory", "Tree"]));
        ui.add_space(spacing::SM);
        ui.add(
            Label::new(match self.tab {
                0 => "The conversation goes here.",
                1 => "Facts and the summary go here.",
                _ => "The story tree goes here.",
            })
            .muted(),
        );

        Self::section(ui, "Tooltip and Popover");
        ui.horizontal(|ui| {
            ui.add(Button::new("Hover me").variant(ButtonVariant::Outline))
                .ir_tooltip("Tooltips explain a control in a few words");
            let trigger = ui.add(Button::new("Open popover").variant(ButtonVariant::Secondary));
            Popover::new("gallery-popover").show(ui, &trigger, |ui| {
                ui.add(Label::new("Popover").heading());
                ui.add(Label::new("Panels anchored to a trigger.").muted().small());
                ui.add(Switch::new(&mut self.narrator).label("Narrator mode"));
            });
        });

        Self::section(ui, "Icon");
        ui.add(
            Label::new("A white texture, painted in the foreground color.")
                .muted()
                .small(),
        );
        let ctx = ui.ctx().clone();
        if let Some(logo) = self.logo(&ctx) {
            ui.add(Icon::new(logo).width(96.0));
        }

        Self::section(ui, "Dialog and Toast");
        ui.horizontal(|ui| {
            if ui.add(Button::new("Open dialog")).clicked() {
                self.dialog_open = true;
            }
            if ui
                .add(Button::new("Show toast").variant(ButtonVariant::Outline))
                .clicked()
            {
                self.toasts
                    .push(Toast::new("Session saved").description("Everything is stored locally."));
            }
            if ui
                .add(Button::new("Show error toast").variant(ButtonVariant::Destructive))
                .clicked()
            {
                self.toasts.push(
                    Toast::destructive("Could not reach the provider")
                        .description("Check the base URL in Settings."),
                );
            }
        });

        Self::section(ui, "ScrollArea");
        ScrollArea::vertical()
            .id_salt("gallery-scroll")
            .max_height(96.0)
            .show(ui, |ui| {
                for i in 1..=12 {
                    ui.add(Label::new(format!("Scrollable line {i}")).muted());
                }
            });
    }

    fn story(&mut self, ui: &mut egui::Ui) {
        Self::section(ui, "ChatBubble");
        ui.add(ChatBubble::narrator("Fog rolls in off the harbor."));
        ui.add(ChatBubble::character(
            "Mira",
            "Ah, another one who got lost?",
        ));
        ui.add(ChatBubble::user("I follow her toward the lighthouse."));
        ui.add(
            ChatBubble::character("Mira", "Keep up, then. The tide waits for no one")
                .streaming(true),
        );

        Self::section(ui, "TreeNode");
        ui.horizontal(|ui| {
            ui.add(TreeNode::new("Current node").state(TreeNodeState::Current));
            ui.add(TreeNode::new("On the path").state(TreeNodeState::Explored));
            ui.add(TreeNode::new("Other branch").state(TreeNodeState::Open));
        });
        ui.horizontal(|ui| {
            ui.add(
                TreeNode::new("Bookmarked")
                    .state(TreeNodeState::Explored)
                    .bookmarked(true),
            );
            if ui
                .add(
                    TreeNode::new("Click to preview")
                        .state(TreeNodeState::Open)
                        .selected(self.node_selected),
                )
                .clicked()
            {
                self.node_selected = !self.node_selected;
            }
        });

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
    }
}

impl eframe::App for Gallery {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        Panel::top("top").show(ctx, |ui| {
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

        Panel::central().show(ctx, |ui| {
            let mut scroll = egui::ScrollArea::vertical();
            if let Some(offset) = self.initial_scroll.take() {
                scroll = scroll.vertical_scroll_offset(offset);
            }
            scroll.show(ui, |ui| {
                Self::buttons(ui);
                Self::text(ui);
                self.inputs(ui);
                self.navigation(ui);
                self.story(ui);
            });
        });

        let mut close = false;
        Dialog::new("gallery-dialog", "Prune this branch?")
            .description("The node and everything after it will be deleted. This cannot be undone.")
            .show(ctx, &mut self.dialog_open, |ui| {
                Dialog::actions(ui, |ui| {
                    if ui
                        .add(Button::new("Prune").variant(ButtonVariant::Destructive))
                        .clicked()
                    {
                        close = true;
                    }
                    if ui
                        .add(Button::new("Cancel").variant(ButtonVariant::Outline))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        if close {
            self.dialog_open = false;
        }

        self.toasts.show(ctx);
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let mode = if args.iter().any(|a| a == "light") {
        ThemeMode::Light
    } else if args.iter().any(|a| a == "dark") {
        ThemeMode::Dark
    } else {
        ThemeMode::System
    };
    let initial_scroll = args.iter().find_map(|a| match a.as_str() {
        "end" => Some(100_000.0),
        other => other.strip_prefix("scroll=")?.parse().ok(),
    });
    let dialog_open = args.iter().any(|a| a == "dialog");
    let show_toasts = args.iter().any(|a| a == "toasts");
    eframe::run_native(
        "ir-ui gallery",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([760.0, 700.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            Theme::apply(&cc.egui_ctx);
            Theme::set_mode(&cc.egui_ctx, mode);
            Ok(Box::new(Gallery::new(
                mode,
                initial_scroll,
                dialog_open,
                show_toasts,
            )))
        }),
    )
}
