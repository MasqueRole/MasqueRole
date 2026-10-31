//! Tree node: the visual for one node of the story tree, a colored dot.

use egui::{
    Color32, CursorIcon, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, Vec2, Widget,
    WidgetInfo, WidgetType,
};

use super::common::transition;
use super::icon::{self, IconKind};
use crate::theme::{spacing, Theme, ThemeExt, BORDER_WIDTH};

/// Diameter of every node.
pub const SIZE: f32 = 32.0;

/// How much larger a hovered node is drawn, as a fraction of its size.
const HOVER_GROWTH: f32 = 0.18;

/// Seconds the dot takes to grow and shrink.
const GROW_TIME: f32 = 0.12;

/// Gap between a collapsed branch's dot and the ring around it.
const RING_GAP: f32 = 3.0;

/// Size of the bookmark mark inside a dot.
const MARK_SIZE: f32 = 14.0;

/// Widest the hover card gets before its title wraps.
const CARD_WIDTH: f32 = 260.0;

/// Where a node stands relative to the story.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeNodeState {
    /// The node the story is at now. Its dot is gold.
    Current,
    /// A node on the path that was taken. Its dot is green.
    Explored,
    /// A node on an alternative branch. Its dot is gray.
    Open,
}

/// A clickable dot, colored by the node's state. Hovering it opens a card with the node's
/// title and date. The caller decides what a click means, such as a preview on the first
/// click and a jump on the second.
#[derive(Clone, Debug)]
pub struct TreeNode {
    title: String,
    date: String,
    state: TreeNodeState,
    bookmarked: bool,
    selected: bool,
    hidden: usize,
}

impl TreeNode {
    /// An open (alternative) node with this title, shown in the hover card.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            date: String::new(),
            state: TreeNodeState::Open,
            bookmarked: false,
            selected: false,
            hidden: 0,
        }
    }

    /// The date shown below the title in the hover card.
    pub fn date(mut self, date: impl Into<String>) -> Self {
        self.date = date.into();
        self
    }

    /// Set the state.
    pub fn state(mut self, state: TreeNodeState) -> Self {
        self.state = state;
        self
    }

    /// Mark the node as bookmarked with a small bookmark in the dot.
    pub fn bookmarked(mut self, bookmarked: bool) -> Self {
        self.bookmarked = bookmarked;
        self
    }

    /// Highlight the node as the one being previewed.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// The number of nodes a collapsed branch hides here. Zero for an open branch. A
    /// collapsed node gets a ring around its dot, and the hover card says how many it hides.
    pub fn hidden(mut self, hidden: usize) -> Self {
        self.hidden = hidden;
        self
    }

    /// The color of the dot.
    fn color(state: TreeNodeState, t: &Theme) -> Color32 {
        match state {
            TreeNodeState::Current => t.tree_current,
            TreeNodeState::Explored => t.tree_explored,
            TreeNodeState::Open => t.tree_open,
        }
    }
}

impl Widget for TreeNode {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(SIZE), Sense::click());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &self.title));

        let grow = crate::motion::eased(
            ui.ctx(),
            response.id.with("grow"),
            response.hovered() || self.selected,
            GROW_TIME,
            GROW_TIME,
        );
        let radius = SIZE / 2.0 * (1.0 + HOVER_GROWTH * grow);
        let base = Self::color(self.state, &t);
        let hovered_color = Theme::mix(base, t.foreground, 0.2);
        let fill = transition(
            ui,
            response.id.with("fill"),
            if response.hovered() {
                hovered_color
            } else {
                base
            },
        );
        let center = rect.center();
        let painter = ui.painter();
        painter.circle(
            center,
            radius,
            fill,
            Stroke::new(BORDER_WIDTH * 2.0, t.background),
        );
        if self.hidden > 0 {
            painter.circle_stroke(
                center,
                radius + RING_GAP,
                Stroke::new(BORDER_WIDTH * 2.0, base),
            );
        }
        if self.bookmarked {
            icon::paint(
                painter,
                Rect::from_center_size(center, Vec2::splat(MARK_SIZE)),
                t.background,
                IconKind::Bookmark,
            );
        }
        if self.selected || response.has_focus() {
            painter.circle_stroke(
                center,
                radius + RING_GAP + BORDER_WIDTH * 2.0,
                Stroke::new(crate::theme::RING_WIDTH, t.ring),
            );
        }

        response
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_ui(|ui| {
                ui.set_max_width(CARD_WIDTH);
                ui.spacing_mut().item_spacing.y = spacing::XS;
                ui.label(
                    RichText::new(&self.title)
                        .size(t.text_sm)
                        .strong()
                        .color(t.foreground),
                );
                if !self.date.is_empty() {
                    ui.label(
                        RichText::new(&self.date)
                            .size(t.text_xs)
                            .color(t.muted_foreground),
                    );
                }
                if self.hidden > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{} node{} hidden",
                            self.hidden,
                            if self.hidden == 1 { "" } else { "s" }
                        ))
                        .size(t.text_xs)
                        .color(t.muted_foreground),
                    );
                }
            })
    }
}

/// The point a line to the node from above ends at, and one from below starts at, given the
/// node's rectangle.
pub fn top_anchor(rect: Rect) -> Pos2 {
    Pos2::new(rect.center().x, rect.top())
}

/// See [`top_anchor`].
pub fn bottom_anchor(rect: Rect) -> Pos2 {
    Pos2::new(rect.center().x, rect.bottom())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    const STATES: [TreeNodeState; 3] = [
        TreeNodeState::Current,
        TreeNodeState::Explored,
        TreeNodeState::Open,
    ];

    #[test]
    fn the_state_shows_in_the_color_of_the_dot() {
        for t in [Theme::light(), Theme::dark()] {
            assert_eq!(TreeNode::color(TreeNodeState::Current, &t), t.tree_current);
            assert_eq!(
                TreeNode::color(TreeNodeState::Explored, &t),
                t.tree_explored
            );
            assert_eq!(TreeNode::color(TreeNodeState::Open, &t), t.tree_open);
            assert_ne!(
                TreeNode::color(TreeNodeState::Current, &t),
                TreeNode::color(TreeNodeState::Explored, &t)
            );
        }
    }

    #[test]
    fn dots_stand_out_from_the_background_in_both_themes() {
        use crate::components::common::luminance;
        for t in [Theme::light(), Theme::dark()] {
            for state in STATES {
                let gap = (luminance(TreeNode::color(state, &t)) - luminance(t.background)).abs();
                assert!(gap > 0.2, "{state:?} dark={} gap={gap}", t.is_dark);
            }
        }
    }

    #[test]
    fn every_node_is_a_square_dot_whatever_its_text() {
        let mut h = Harness::new();
        let mut sizes = Vec::new();
        h.idle(&mut |ui: &mut Ui| {
            sizes.clear();
            sizes.push(ui.add(TreeNode::new("x".repeat(300))).rect.size());
            sizes.push(
                ui.add(TreeNode::new("Start").date("30 Oct 14:32"))
                    .rect
                    .size(),
            );
            sizes.push(
                ui.add(TreeNode::new("A").bookmarked(true).hidden(3))
                    .rect
                    .size(),
            );
        });
        assert!(sizes.iter().all(|s| *s == Vec2::splat(SIZE)), "{sizes:?}");
    }

    #[test]
    fn clicking_reports_a_click() {
        let mut h = Harness::new();
        let mut rect = Rect::NOTHING;
        h.idle(&mut |ui: &mut Ui| rect = ui.add(TreeNode::new("Enter the lighthouse")).rect);
        let mut clicked = false;
        h.click(rect.center(), &mut |ui: &mut Ui| {
            clicked |= ui.add(TreeNode::new("Enter the lighthouse")).clicked();
        });
        assert!(clicked);
    }

    #[test]
    fn the_resting_dot_draws_no_text() {
        let h = Harness::new();
        let output = h.ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(TreeNode::new("The lighthouse").date("30 Oct 14:32"));
            });
        });
        let drawn = format!("{:?}", output.shapes);
        assert!(!drawn.contains("The lighthouse"), "{drawn}");
        assert!(!drawn.contains("30 Oct"), "{drawn}");
    }

    #[test]
    fn hovering_the_dot_opens_a_card_with_the_title_date_and_hidden_count() {
        let ctx = egui::Context::default();
        crate::Theme::apply(&ctx);
        let mut pointer = None;
        let mut drawn = String::new();
        for frame in 0..20 {
            let mut rect = Rect::NOTHING;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                time: Some(f64::from(frame) * 0.1),
                events: pointer.map(egui::Event::PointerMoved).into_iter().collect(),
                ..Default::default()
            };
            let output = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rect = ui
                        .add(
                            TreeNode::new("The lighthouse")
                                .date("30 Oct 14:32")
                                .hidden(3),
                        )
                        .rect;
                });
            });
            pointer = Some(rect.center());
            drawn = format!("{:?}", output.shapes);
        }
        assert!(drawn.contains("The lighthouse"), "{drawn}");
        assert!(drawn.contains("30 Oct 14:32"));
        assert!(drawn.contains("3 nodes hidden"));
    }

    #[test]
    fn every_state_renders_selected_bookmarked_and_collapsed() {
        let mut h = Harness::new();
        h.idle(&mut |ui: &mut Ui| {
            for state in STATES {
                ui.add(TreeNode::new("Node").state(state));
                ui.add(
                    TreeNode::new("Node")
                        .state(state)
                        .date("14:32")
                        .selected(true)
                        .bookmarked(true)
                        .hidden(2),
                );
            }
        });
    }
}
