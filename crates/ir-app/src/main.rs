//! InfiniteRoleplay desktop app.

mod app;
mod backend;
mod branding;
mod characters;
mod chat;
mod fatal;
mod format;
mod library;
mod logging;
mod prompts;
mod settings_ui;
mod sidebar;
mod tree_layout;
mod tree_view;

use std::sync::Arc;

use anyhow::Context as _;
use eframe::egui;
use ir_core::db::{Db, DB_FILE};
use ir_core::settings::{KeychainStore, Settings};
use ir_core::APP_NAME;
use ir_ui::Theme;

use crate::app::App;
use crate::backend::{default_provider_factory, Request, Services};
use crate::library::Library;

fn main() {
    if let Err(e) = run() {
        tracing::error!("startup failed: {e:#}");
        fatal::show(&format!("{e:#}"));
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let data_dir = Settings::data_dir().context("could not find the data directory")?;
    let settings_path = Settings::default_path().context("could not find the settings file")?;
    // An unreadable settings file is reported by the backend; here it only means no log.
    let logging = Settings::load_from(&settings_path).is_ok_and(|s| s.debug.logging);
    if let Some(log) = logging::init(logging, &data_dir) {
        tracing::info!(log = %log.display(), "debug log enabled");
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("could not start the async runtime")?;
    let db = Db::open(&data_dir.join(DB_FILE)).context("could not open the database")?;

    let options = eframe::NativeOptions {
        viewport: {
            let viewport = egui::ViewportBuilder::default()
                .with_title(APP_NAME)
                .with_inner_size([1100.0, 720.0])
                .with_min_inner_size([760.0, 480.0]);
            match branding::window_icon() {
                Some(icon) => viewport.with_icon(icon),
                None => viewport,
            }
        },
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| {
            Theme::apply(&cc.egui_ctx);
            let ctx = cc.egui_ctx.clone();
            let services = Services {
                db,
                library: Library::new(&data_dir),
                settings_path: Some(settings_path),
                secrets: Arc::new(KeychainStore),
                provider: default_provider_factory(),
                wake: Arc::new(move || ctx.request_repaint()),
            };
            let (backend, events) = backend::spawn(runtime.handle(), services);
            backend.send(Request::Init);
            Ok(Box::new(Shell(App::new(backend, events, Some(runtime)))))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Adapts [`App`] to eframe.
struct Shell(App);

impl eframe::App for Shell {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.ui(ctx);
    }
}
