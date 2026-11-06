//! A window for errors that stop the app from starting.

use eframe::egui;
use masquerole_core::APP_NAME;
use masquerole_ui::components::{Button, Dialog, Panel};
use masquerole_ui::Theme;

/// The dialog's text for a startup failure.
fn description(error: &str) -> String {
    format!("{APP_NAME} could not start.\n\n{error}")
}

struct ErrorWindow {
    error: String,
    open: bool,
}

impl eframe::App for ErrorWindow {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        Panel::central().show(root, |_| {});
        let mut close = false;
        Dialog::new("startup-error", "Something went wrong")
            .description(description(&self.error))
            .show(ctx, &mut self.open, |ui| {
                Dialog::actions(ui, |ui| {
                    close = ui.add(Button::new("Close")).clicked();
                });
            });
        if close || !self.open {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// Tell the user why the app cannot start. Falls back to stderr alone when no window can
/// be opened.
pub fn show(error: &str) {
    eprintln!("{APP_NAME} could not start: {error}");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([520.0, 320.0]),
        ..Default::default()
    };
    let error = error.to_string();
    // If this fails too (no display, no graphics), stderr above is all there is.
    let _ = eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| {
            Theme::apply(&cc.egui_ctx);
            Ok(Box::new(ErrorWindow { error, open: true }))
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_description_names_the_app_and_the_cause() {
        let text = description("could not open the database");
        assert!(text.starts_with("MasqueRole could not start."));
        assert!(text.ends_with("could not open the database"));
    }
}
