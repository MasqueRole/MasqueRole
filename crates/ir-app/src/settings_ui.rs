//! The settings screen.

use eframe::egui::Ui;
use ir_core::client::{is_paid_provider, PRESETS};
use ir_core::settings::{NodeLabel, Settings};
use ir_ui::components::{
    Alert, Badge, BadgeVariant, Button, ButtonVariant, Card, FormField, Input, Label, Select,
    Slider, Switch, Tabs,
};
use ir_ui::{spacing, ThemeMode};

/// The settings tabs.
const TAB_NAMES: [&str; 5] = ["General", "Generation", "Memory", "Story", "Limits"];

/// The theme choices, in the order the tabs show them.
const THEMES: [(&str, ThemeMode); 3] = [
    ("System", ThemeMode::System),
    ("Light", ThemeMode::Light),
    ("Dark", ThemeMode::Dark),
];

/// What the settings screen asks the app to do besides saving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsAction {
    /// Remove the stored API key.
    RemoveKey,
}

/// The badge shown next to the API key field. A missing key only looks like a problem when
/// the provider needs one.
fn key_status(key_required: bool, key_available: bool) -> (&'static str, BadgeVariant) {
    match (key_available, key_required) {
        (true, _) => ("A key is available", BadgeVariant::Secondary),
        (false, false) => ("No key (optional)", BadgeVariant::Outline),
        (false, true) => ("No key", BadgeVariant::Destructive),
    }
}

/// The title of the warning shown for providers that bill by usage.
const COST_WARNING_TITLE: &str = "This provider can cost money";

/// The warning shown for providers that bill by usage.
const COST_WARNING: &str = "Hosted providers charge for every request, and a long story sends a lot of text with each message, so costs can grow fast. Neither MasqueRole nor its creators are liable for API costs, including unexpectedly high ones. Set a token limit under Limits and keep an eye on your provider's usage page.";

/// The settings being edited. Numbers are `f64` because the sliders edit `f64`.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsForm {
    settings: Settings,
    /// Index into [`PRESETS`].
    provider: usize,
    /// A new API key. Empty keeps the stored one.
    pub api_key: String,
    /// The color theme. It applies at once and is not part of the saved settings.
    pub theme: ThemeMode,
    /// The tab being shown.
    tab: usize,
    token_limit: f64,
    node_label: usize,
    temperature: f64,
    max_tokens: f64,
    max_context_tokens: f64,
    recent_messages: f64,
    summarize_after: f64,
}

impl SettingsForm {
    /// A form editing a copy of `settings`.
    pub fn new(settings: &Settings) -> Self {
        Self {
            settings: settings.clone(),
            provider: PRESETS
                .iter()
                .position(|p| p.id == settings.api.provider)
                .unwrap_or(0),
            api_key: String::new(),
            theme: ThemeMode::System,
            tab: 0,
            token_limit: f64::from(settings.limits.token_limit),
            node_label: usize::from(settings.tree.node_label == NodeLabel::Manual),
            temperature: f64::from(settings.api.temperature),
            max_tokens: f64::from(settings.api.max_tokens),
            max_context_tokens: f64::from(settings.context.max_context_tokens),
            recent_messages: f64::from(settings.context.recent_messages),
            summarize_after: f64::from(settings.context.summarize_after),
        }
    }

    /// The settings the form describes.
    pub fn to_settings(&self) -> Settings {
        let mut s = self.settings.clone();
        s.api.provider = PRESETS[self.provider].id.to_string();
        s.api.base_url = s.api.base_url.trim().to_string();
        s.api.model = s.api.model.trim().to_string();
        s.api.temperature = self.temperature as f32;
        s.api.max_tokens = self.max_tokens.round() as u32;
        s.context.max_context_tokens = self.max_context_tokens.round() as u32;
        s.context.recent_messages = self.recent_messages.round() as u32;
        s.context.summarize_after = self.summarize_after.round() as u32;
        s.limits.token_limit = self.token_limit.round() as u32;
        s.tree.node_label = if self.node_label == 1 {
            NodeLabel::Manual
        } else {
            NodeLabel::Auto
        };
        s
    }

    /// The new key to store, if one was typed.
    pub fn new_api_key(&self) -> Option<String> {
        let key = self.api_key.trim();
        (!key.is_empty()).then(|| key.to_string())
    }

    /// After the provider changed, fill in its default endpoint. Custom endpoints (a preset
    /// with no URL) keep whatever was typed.
    fn apply_provider(&mut self) {
        let url = PRESETS[self.provider].base_url;
        if !url.is_empty() {
            self.settings.api.base_url = url.to_string();
        }
    }

    /// Whether the chosen provider and endpoint can cost money.
    pub fn is_paid(&self) -> bool {
        is_paid_provider(PRESETS[self.provider].id, &self.settings.api.base_url)
    }

    /// Draw the provider settings flat on the page: the cost warning for paid providers, then
    /// the provider, endpoint, model and API key. The page supplies the title.
    pub fn show_provider(&mut self, key_available: bool, ui: &mut Ui) -> Option<SettingsAction> {
        let mut action = None;
        if self.is_paid() {
            ui.add(Alert::new(COST_WARNING_TITLE, COST_WARNING));
            ui.add_space(spacing::LG);
        }
        ui.add(
            Label::new("Where responses come from. Any OpenAI-compatible endpoint works.")
                .small()
                .muted(),
        );
        let names = PRESETS.iter().map(|p| p.name);
        FormField::new("Provider").show(ui, |ui| {
            if ui
                .add(Select::new("settings-provider", &mut self.provider, names).width(320.0))
                .changed()
            {
                self.apply_provider();
            }
        });
        FormField::new("Base URL").show(ui, |ui| {
            ui.add(Input::new(&mut self.settings.api.base_url).id_salt("settings-url"))
        });
        FormField::new("Model")
            .help("Check your provider's documentation for current model names.")
            .show(ui, |ui| {
                ui.add(Input::new(&mut self.settings.api.model).id_salt("settings-model"))
            });
        FormField::new("API key")
            .help("Stored in your operating system's keychain, never in a file. Leave empty to keep the current key.")
            .show(ui, |ui| {
                ui.add(
                    Input::new(&mut self.api_key)
                        .password(true)
                        .placeholder("Paste a key")
                        .id_salt("settings-key"),
                );
                ui.horizontal(|ui| {
                    let (text, variant) =
                        key_status(PRESETS[self.provider].key_required, key_available);
                    ui.add(Badge::new(text).variant(variant));
                    if key_available
                        && ui
                            .add(Button::new("Remove stored key").variant(ButtonVariant::Ghost))
                            .clicked()
                    {
                        action = Some(SettingsAction::RemoveKey);
                    }
                });
            });
        action
    }

    /// Draw the form.
    pub fn show(&mut self, ui: &mut Ui) {
        let mut tab = self.tab;
        ui.add(Tabs::new(&mut tab, TAB_NAMES));
        self.tab = tab;
        ui.add_space(spacing::MD);

        if self.tab == 0 {
            Card::new()
                .header(|ui| Card::title(ui, "Appearance"))
                .content(|ui| {
                    let mut index = THEMES
                        .iter()
                        .position(|(_, m)| *m == self.theme)
                        .unwrap_or(0);
                    FormField::new("Theme")
                        .help("System follows your operating system.")
                        .show(ui, |ui| {
                            ui.add(Tabs::new(&mut index, THEMES.iter().map(|(name, _)| *name)))
                        });
                    self.theme = THEMES[index].1;
                    ui.add(
                        Switch::new(&mut self.settings.ui.reduce_motion)
                            .label("Reduce motion (applies after saving)"),
                    );
                })
                .show(ui);
            ui.add_space(spacing::LG);
        }
        if self.tab == 0 {
            Card::new()
                .header(|ui| {
                    Card::title(ui, "Troubleshooting");
                    Card::description(
                        ui,
                        "For bug reports. API keys are never written to the log.",
                    );
                })
                .content(|ui| {
                    ui.add(
                        Switch::new(&mut self.settings.debug.logging)
                            .label("Write a debug log (applies after a restart)"),
                    );
                })
                .show(ui);
            ui.add_space(spacing::LG);
        }
        if self.tab == 1 {
            Card::new()
                .header(|ui| Card::title(ui, "Generation"))
                .content(|ui| {
                    FormField::new("Temperature")
                        .help("Higher is more creative, lower is more predictable.")
                        .show(ui, |ui| {
                            ui.add(
                                Slider::new(&mut self.temperature, 0.0..=2.0)
                                    .step(0.05)
                                    .decimals(2),
                            )
                        });
                    FormField::new("Maximum response length")
                        .help("In tokens.")
                        .show(ui, |ui| {
                            ui.add(Slider::new(&mut self.max_tokens, 64.0..=8192.0).step(64.0))
                        });
                    ui.add(
                        Switch::new(&mut self.settings.api.stream)
                            .label("Show responses as they are written"),
                    );
                })
                .show(ui);
            ui.add_space(spacing::LG);
        }
        if self.tab == 2 {
            Card::new()
                .header(|ui| {
                    Card::title(ui, "Context and memory");
                    Card::description(ui, "How much of the story is sent with every message.");
                })
                .content(|ui| {
                    FormField::new("Context budget")
                        .help("Upper limit, in tokens, for everything sent to the model.")
                        .show(ui, |ui| {
                            ui.add(
                                Slider::new(&mut self.max_context_tokens, 1000.0..=200_000.0)
                                    .step(1000.0),
                            )
                        });
                    FormField::new("Recent messages")
                        .help("Kept word for word.")
                        .show(ui, |ui| {
                            ui.add(Slider::new(&mut self.recent_messages, 2.0..=100.0).step(1.0))
                        });
                    FormField::new("Summarize after")
                        .help("Older messages are condensed once this many pile up.")
                        .show(ui, |ui| {
                            ui.add(Slider::new(&mut self.summarize_after, 5.0..=200.0).step(1.0))
                        });
                    ui.add(
                        Switch::new(&mut self.settings.memory.enabled).label("Remember the story"),
                    );
                    ui.add(
                        Switch::new(&mut self.settings.memory.fact_extraction)
                            .label("Extract facts automatically")
                            .enabled(self.settings.memory.enabled),
                    );
                })
                .show(ui);
            ui.add_space(spacing::LG);
        }
        if self.tab == 3 {
            Card::new()
                .header(|ui| Card::title(ui, "Style"))
                .content(|ui| {
                    FormField::new("Style profile")
                        .help("A short description, such as noir or slice of life.")
                        .show(ui, |ui| {
                            ui.add(
                                Input::new(&mut self.settings.style.profile)
                                    .id_salt("settings-style"),
                            )
                        });
                    ui.add(
                        Switch::new(&mut self.settings.style.narrator_mode).label("Narrator mode"),
                    );
                })
                .show(ui);
            ui.add_space(spacing::LG);
        }
        if self.tab == 3 {
            Card::new()
                .header(|ui| Card::title(ui, "Story tree"))
                .content(|ui| {
                    ui.add(
                        Switch::new(&mut self.settings.tree.auto_checkpoint)
                            .label("Create a node after every turn"),
                    );
                    ui.add(
                        Switch::new(&mut self.settings.tree.show_tree_on_branch)
                            .label("Show the tree after branching"),
                    );
                    FormField::new("Node labels").show(ui, |ui| {
                        ui.add(
                            Select::new(
                                "settings-labels",
                                &mut self.node_label,
                                ["Generated automatically", "Ask every time"],
                            )
                            .width(320.0),
                        )
                    });
                })
                .show(ui);
            ui.add_space(spacing::LG);
        }
        if self.tab == 4 {
            Card::new()
                .header(|ui| {
                    Card::title(ui, "Token limit");
                    Card::description(
                        ui,
                        "Stops a story from using more tokens than you want to pay for. Usage is estimated and counts everything sent to and received from the provider, including summaries.",
                    );
                })
                .content(|ui| {
                    ui.add(
                        Switch::new(&mut self.settings.limits.token_limit_enabled)
                            .label("Limit the tokens one story may use"),
                    );
                    FormField::new("Limit per story")
                        .help("Once a story reaches this, it stops generating until you raise the limit or turn it off.")
                        .show(ui, |ui| {
                            ui.add(
                                Slider::new(&mut self.token_limit, 10_000.0..=2_000_000.0)
                                    .step(10_000.0)
                                    .enabled(self.settings.limits.token_limit_enabled),
                            )
                        });
                })
                .show(ui);
            ui.add_space(spacing::LG);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_survive_a_trip_through_the_form() {
        let defaults = Settings::default();
        assert_eq!(SettingsForm::new(&defaults).to_settings(), defaults);
    }

    #[test]
    fn edits_are_written_back_and_numbers_are_rounded() {
        let mut form = SettingsForm::new(&Settings::default());
        form.settings.api.model = "  new-model  ".into();
        form.temperature = 0.55;
        form.max_tokens = 1023.6;
        form.recent_messages = 7.4;
        form.node_label = 1;
        let s = form.to_settings();
        assert_eq!(s.api.model, "new-model");
        assert!((s.api.temperature - 0.55).abs() < 1e-6);
        assert_eq!(s.api.max_tokens, 1024);
        assert_eq!(s.context.recent_messages, 7);
        assert_eq!(s.tree.node_label, NodeLabel::Manual);
    }

    #[test]
    fn the_token_limit_is_edited_in_the_form_and_written_back() {
        let mut form = SettingsForm::new(&Settings::default());
        form.settings.limits.token_limit_enabled = true;
        form.token_limit = 50_000.4;
        let limits = form.to_settings().limits;
        assert!(limits.token_limit_enabled);
        assert_eq!(limits.token_limit, 50_000);
    }

    #[test]
    fn every_tab_draws_and_the_tab_names_match_the_tabs() {
        let ctx = eframe::egui::Context::default();
        ir_ui::Theme::apply(&ctx);
        let mut form = SettingsForm::new(&Settings::default());
        for tab in 0..TAB_NAMES.len() {
            form.tab = tab;
            let _ = ctx.run(Default::default(), |ctx| {
                eframe::egui::CentralPanel::default().show(ctx, |ui| {
                    form.show(ui);
                });
            });
        }
        assert_eq!(TAB_NAMES.len(), 5);
    }

    #[test]
    fn choosing_a_provider_fills_in_its_endpoint_but_custom_keeps_yours() {
        let mut form = SettingsForm::new(&Settings::default());
        form.provider = PRESETS.iter().position(|p| p.id == "ollama").unwrap();
        form.apply_provider();
        assert_eq!(form.to_settings().api.base_url, "http://localhost:11434/v1");
        assert_eq!(form.to_settings().api.provider, "ollama");

        form.settings.api.base_url = "https://my.example/v1".into();
        form.provider = 0; // custom
        form.apply_provider();
        assert_eq!(form.to_settings().api.base_url, "https://my.example/v1");
    }

    #[test]
    fn paid_providers_are_flagged_so_the_warning_shows() {
        let mut form = SettingsForm::new(&Settings::default());
        let pick = |form: &mut SettingsForm, id: &str| {
            form.provider = PRESETS.iter().position(|p| p.id == id).unwrap();
            form.apply_provider();
        };
        pick(&mut form, "openai");
        assert!(form.is_paid());
        pick(&mut form, "ollama");
        assert!(!form.is_paid());
        form.settings.api.base_url = "https://my.example/v1".into();
        form.provider = 0;
        assert!(form.is_paid());
        form.settings.api.base_url = "http://localhost:5000/v1".into();
        assert!(!form.is_paid());
        assert!(COST_WARNING.contains("liable"));
    }

    #[test]
    fn an_unknown_provider_shows_as_custom() {
        let mut settings = Settings::default();
        settings.api.provider = "something-else".into();
        assert_eq!(SettingsForm::new(&settings).provider, 0);
    }

    #[test]
    fn a_missing_key_is_only_a_problem_for_providers_that_need_one() {
        assert_eq!(
            key_status(true, false),
            ("No key", BadgeVariant::Destructive)
        );
        assert_eq!(
            key_status(false, false),
            ("No key (optional)", BadgeVariant::Outline)
        );
        assert_eq!(key_status(true, true).1, BadgeVariant::Secondary);
        assert_eq!(key_status(false, true).0, "A key is available");
    }

    #[test]
    fn only_a_typed_key_is_sent_for_storage() {
        let mut form = SettingsForm::new(&Settings::default());
        assert_eq!(form.new_api_key(), None);
        form.api_key = "   ".into();
        assert_eq!(form.new_api_key(), None);
        form.api_key = "  sk-123 ".into();
        assert_eq!(form.new_api_key().as_deref(), Some("sk-123"));
    }
}
