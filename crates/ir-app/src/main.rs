//! InfiniteRoleplay desktop app.

use eframe::egui;
use ir_core::APP_NAME;

/// Root application state.
struct App;

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |_ui| {});
    }
}

fn main() -> anyhow::Result<()> {
    eframe::run_native(
        APP_NAME,
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(App))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
