//! A headless egui harness for component tests: run frames, click, and press keys.

use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Ui, Vec2};

use crate::{Theme, ThemeMode};

/// Runs a UI for several frames with scripted input.
pub(crate) struct Harness {
    pub ctx: Context,
    time: f64,
}

impl Harness {
    /// A harness using the light palette.
    pub fn new() -> Self {
        let ctx = Context::default();
        Theme::apply(&ctx);
        Theme::set_mode(&ctx, ThemeMode::Light);
        Self { ctx, time: 0.0 }
    }

    /// Run one frame with `events`, building the UI with `add`.
    pub fn frame(&mut self, events: Vec<Event>, add: &mut dyn FnMut(&mut Ui)) {
        self.ctx_frame(events, &mut |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| add(ui));
        });
    }

    /// Run one frame with `events`, calling `add` with the context itself, for components
    /// such as dialogs and toasts that draw above the page.
    pub fn ctx_frame(&mut self, events: Vec<Event>, add: &mut dyn FnMut(&Context)) {
        self.time += 0.1;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let _ = self.ctx.run(input, |ctx| add(ctx));
    }

    /// Run a frame with no input.
    pub fn idle(&mut self, add: &mut dyn FnMut(&mut Ui)) {
        self.frame(Vec::new(), add);
    }

    /// Press and release the primary button at `pos`.
    pub fn click(&mut self, pos: Pos2, add: &mut dyn FnMut(&mut Ui)) {
        self.frame(vec![Event::PointerMoved(pos)], add);
        self.frame(vec![button(pos, true)], add);
        self.frame(vec![button(pos, false)], add);
    }

    /// Press and release a key.
    pub fn key(&mut self, key: Key, add: &mut dyn FnMut(&mut Ui)) {
        self.frame(vec![key_event(key, true)], add);
        self.frame(vec![key_event(key, false)], add);
    }
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn key_event(key: Key, pressed: bool) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}
