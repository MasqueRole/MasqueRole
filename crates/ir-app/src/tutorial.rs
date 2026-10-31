//! The tutorial: a few pages of text with screenshots of the app. It opens by itself the
//! first time the app starts, and again from the question mark button in the top bar.

use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};
use ir_ui::components::{Button, ButtonVariant, Dialog, Figure, Label};

/// Width of the tutorial dialog; the screenshots fill it.
const DIALOG_WIDTH: f32 = 640.0;

/// What the screenshots leave of the window height for the text and the buttons.
const CHROME_HEIGHT: f32 = 260.0;

/// The screenshots never get shorter than this, however small the window.
const MIN_IMAGE_HEIGHT: f32 = 160.0;

/// One page: a title, a few sentences, and a screenshot.
struct Page {
    title: &'static str,
    text: &'static str,
    screenshot: &'static [u8],
}

const PAGES: &[Page] = &[
    Page {
        title: "Welcome to MasqueRole",
        text: "Stories that never run out of room. This short tour shows the main parts: Stories, Characters, Worlds, and the Tree are the icons in the rail at the left (hover one to see its name), and each has its own list next to it.",
        screenshot: include_bytes!("../../../assets/tutorial/01-overview.png"),
    },
    Page {
        title: "Connect a provider",
        text: "Click the gear at the top right and open the Provider tab. Choose a provider and paste your API key. Local models, such as Ollama, need no key at all. Hosted providers charge for usage, and the app warns you when you pick one: neither it nor its creators are liable for API costs, so set a token limit.",
        screenshot: include_bytes!("../../../assets/tutorial/02-provider.png"),
    },
    Page {
        title: "Create characters and worlds",
        text: "Open Characters or Worlds and press the + button. Give a character a personality and a greeting, and a world lore entries that are added only when their keywords come up. Both can have a picture.",
        screenshot: include_bytes!("../../../assets/tutorial/03-characters.png"),
    },
    Page {
        title: "Tell your story",
        text: "In Stories, press + to start a session and choose who is in the scene. Write and press Enter to send; Shift+Enter starts a new line. Type a slash to see the commands, and click one to use it.",
        screenshot: include_bytes!("../../../assets/tutorial/04-commands.png"),
    },
    Page {
        title: "Branch, copy, and look around",
        text: "Click a message to branch from it or copy it. Every message has a tag such as #12 that works with /goto. Storylines of the open story are listed below it in the sidebar, and a click on the story title opens its world and characters.",
        screenshot: include_bytes!("../../../assets/tutorial/05-messages.png"),
    },
    Page {
        title: "Explore the story tree",
        text: "The tree grows from left to right, and every message is a card. The blue line marks the path you took, and the pill on a card is green on that path, gold where you are now, and gray on other branches. Hover a card to see its whole title, and drag it to put it where you like: the story remembers the layout. Click it and it opens up to show the whole message: press Jump here, or click the card again, to go back and take a different turn.",
        screenshot: include_bytes!("../../../assets/tutorial/06-tree.png"),
    },
];

/// The state of the open tutorial.
pub struct Tutorial {
    page: usize,
    /// Screenshots already uploaded as textures, by page. `None` marks one that cannot be read.
    textures: Vec<Option<TextureHandle>>,
    loaded: Vec<bool>,
}

impl Tutorial {
    /// The tutorial at its first page.
    pub fn new() -> Self {
        Self {
            page: 0,
            textures: vec![None; PAGES.len()],
            loaded: vec![false; PAGES.len()],
        }
    }

    /// The page being shown, counting from zero.
    #[cfg(test)]
    pub fn page(&self) -> usize {
        self.page
    }

    /// How many pages there are.
    #[cfg(test)]
    pub fn page_count() -> usize {
        PAGES.len()
    }

    /// Go to the next page. Returns `false` on the last page, where the tutorial ends.
    pub fn next(&mut self) -> bool {
        if self.page + 1 < PAGES.len() {
            self.page += 1;
            true
        } else {
            false
        }
    }

    /// Go back one page, if there is one.
    pub fn back(&mut self) {
        self.page = self.page.saturating_sub(1);
    }

    fn texture(&mut self, ctx: &Context, index: usize) -> Option<TextureHandle> {
        if !self.loaded[index] {
            self.loaded[index] = true;
            self.textures[index] = load_screenshot(ctx, index);
        }
        self.textures[index].clone()
    }

    /// Draw the tutorial. Returns `false` once the user has finished it or closed it.
    pub fn show(&mut self, ctx: &Context) -> bool {
        let page = &PAGES[self.page];
        let texture = self.texture(ctx, self.page);
        let last = self.page + 1 == PAGES.len();
        let max_height = (ctx.screen_rect().height() - CHROME_HEIGHT).max(MIN_IMAGE_HEIGHT);
        let (mut open, mut next, mut back, mut skip) = (true, false, false, false);

        Dialog::new("tutorial", page.title)
            .description(page.text)
            .width(DIALOG_WIDTH)
            .show(ctx, &mut open, |ui| {
                // The screenshot is as large as the window allows, so it is never cut off.
                ui.vertical_centered(|ui| match &texture {
                    Some(texture) => {
                        let width = (max_height * texture.aspect_ratio()).min(DIALOG_WIDTH);
                        ui.add(Figure::new(texture).width(width));
                    }
                    None => {
                        ui.add(Label::new("The screenshot could not be loaded.").muted());
                    }
                });
                ui.add(
                    Label::new(format!("{} of {}", self.page + 1, PAGES.len()))
                        .small()
                        .muted(),
                );
                Dialog::actions(ui, |ui| {
                    let label = if last { "Get started" } else { "Next" };
                    next = ui.add(Button::new(label)).clicked();
                    if self.page > 0 {
                        back = ui
                            .add(Button::new("Back").variant(ButtonVariant::Outline))
                            .clicked();
                    }
                    if !last {
                        skip = ui
                            .add(Button::new("Skip").variant(ButtonVariant::Ghost))
                            .clicked();
                    }
                });
            });

        if back {
            self.back();
        }
        if next && !self.next() {
            return false;
        }
        open && !skip
    }
}

fn load_screenshot(ctx: &Context, index: usize) -> Option<TextureHandle> {
    let picture = eframe::icon_data::from_png_bytes(PAGES[index].screenshot).ok()?;
    let size = [picture.width as usize, picture.height as usize];
    let image = ColorImage::from_rgba_unmultiplied(size, &picture.rgba);
    Some(ctx.load_texture(format!("tutorial-{index}"), image, TextureOptions::LINEAR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_screenshot_is_a_valid_png_wider_than_tall_or_square() {
        for (i, page) in PAGES.iter().enumerate() {
            let picture = eframe::icon_data::from_png_bytes(page.screenshot)
                .unwrap_or_else(|e| panic!("page {i} screenshot: {e}"));
            assert!(
                picture.width >= 64 && picture.width >= picture.height,
                "page {i}"
            );
        }
    }

    #[test]
    fn every_page_has_a_title_and_text() {
        for page in PAGES {
            assert!(
                !page.title.is_empty() && page.text.len() > 40,
                "{}",
                page.title
            );
        }
        assert_eq!(Tutorial::page_count(), PAGES.len());
    }

    #[test]
    fn pages_move_forward_and_back_and_the_last_one_ends_the_tour() {
        let mut tour = Tutorial::new();
        assert_eq!(tour.page(), 0);
        tour.back();
        assert_eq!(tour.page(), 0, "no page before the first");
        for expected in 1..PAGES.len() {
            assert!(tour.next());
            assert_eq!(tour.page(), expected);
        }
        assert!(!tour.next(), "next on the last page finishes");
        assert_eq!(tour.page(), PAGES.len() - 1);
        tour.back();
        assert_eq!(tour.page(), PAGES.len() - 2);
    }

    #[test]
    fn every_page_draws() {
        let ctx = Context::default();
        ir_ui::Theme::apply(&ctx);
        let mut tour = Tutorial::new();
        for _ in 0..PAGES.len() {
            let mut open = false;
            let _ = ctx.run(Default::default(), |ctx| open = tour.show(ctx));
            assert!(open, "drawing alone does not close the tutorial");
            tour.next();
        }
    }
}
