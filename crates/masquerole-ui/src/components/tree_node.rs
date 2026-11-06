//! Tree node: the visual for one node of the story tree, a small card.

use egui::text::LayoutJob;
use egui::{
    Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Response, RichText, Role, Sense, Stroke,
    StrokeKind, Ui, Vec2, Widget, WidgetInfo,
};

use super::common::transition;
use crate::theme::{spacing, Theme, ThemeExt, BORDER_WIDTH, RING_WIDTH};

/// Height of every node: the label, and the date below it.
pub const HEIGHT: f32 = 56.0;

/// Width of every node. Equal widths keep the columns of the tree tidy.
pub const WIDTH: f32 = 190.0;

/// Height of the date pill.
const TAG_HEIGHT: f32 = 20.0;

/// Widest the hover card gets before its title wraps.
const CARD_WIDTH: f32 = 260.0;

/// How much a light theme's pill text is darkened, so gold and green stay readable on gray.
const LIGHT_TEXT_DARKENING: f32 = 0.55;

/// Where a node stands relative to the story.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeNodeState {
    /// The node the story is at now. Its pill is gold.
    Current,
    /// A node on the path that was taken. Its pill is green.
    Explored,
    /// A node on an alternative branch. Its pill is gray.
    Open,
}

impl TreeNodeState {
    /// What the hover card says about the node.
    fn description(self) -> &'static str {
        match self {
            Self::Current => "Where you are now",
            Self::Explored => "On the path you took",
            Self::Open => "Another branch",
        }
    }
}

/// A clickable card: a gray box with the title, and a pill with the date whose color shows the
/// node's state. Hovering it opens a card with the whole title and where the node stands. The
/// caller decides what a click means, such as opening the node on the first click and a jump
/// on the second.
#[derive(Clone, Debug)]
pub struct TreeNode {
    title: String,
    date: String,
    state: TreeNodeState,
    bookmarked: bool,
    selected: bool,
    hidden: usize,
}

/// How a node is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
struct NodeColors {
    fill: Color32,
    border: Stroke,
    text: Color32,
    tag_fill: Color32,
    tag_border: Color32,
    tag_text: Color32,
}

impl TreeNode {
    /// An open (alternative) node with this title.
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

    /// The date shown in the pill. The pill's color shows the state.
    pub fn date(mut self, date: impl Into<String>) -> Self {
        self.date = date.into();
        self
    }

    /// Set the state.
    pub fn state(mut self, state: TreeNodeState) -> Self {
        self.state = state;
        self
    }

    /// Mark the node as bookmarked with a star.
    pub fn bookmarked(mut self, bookmarked: bool) -> Self {
        self.bookmarked = bookmarked;
        self
    }

    /// Outline the node, to show that it is the one that is open.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// The number of nodes a collapsed branch hides here. Zero for an open branch. The count
    /// shows in front of the title, because a long title is cut off at the end.
    pub fn hidden(mut self, hidden: usize) -> Self {
        self.hidden = hidden;
        self
    }

    fn colors(state: TreeNodeState, hovered: bool, t: &Theme) -> NodeColors {
        let fill = if hovered {
            Theme::mix(t.muted, t.foreground, 0.1)
        } else {
            t.muted
        };
        // The pill's border and text share a color: gold, green, or gray.
        let (tag_border, tag_text) = match state {
            TreeNodeState::Current => (t.tree_current, readable(t.tree_current, t)),
            TreeNodeState::Explored => (t.tree_explored, readable(t.tree_explored, t)),
            TreeNodeState::Open => (t.muted_foreground, t.muted_foreground),
        };
        NodeColors {
            fill,
            border: Stroke::new(BORDER_WIDTH, t.border),
            text: t.foreground,
            tag_fill: tag_border.gamma_multiply(0.14),
            tag_border,
            tag_text,
        }
    }
}

/// `color` as text on a gray node: darkened in the light theme, where the bright state colors
/// would wash out.
fn readable(color: Color32, t: &Theme) -> Color32 {
    if t.is_dark {
        color
    } else {
        Theme::mix(color, t.foreground, LIGHT_TEXT_DARKENING)
    }
}

impl Widget for TreeNode {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.ir_theme();
        let mut title = self.title.clone();
        if self.bookmarked {
            title = format!("\u{2605} {title}");
        }
        if self.hidden > 0 {
            title = format!("[+{}] {title}", self.hidden);
        }

        // Lay the texts out first, in a neutral color, to place them.
        let mut job = LayoutJob::simple(
            title.clone(),
            FontId::proportional(t.text_sm),
            Color32::PLACEHOLDER,
            WIDTH - 2.0 * spacing::MD,
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('\u{2026}');
        let title_galley = ui.painter().layout_job(job);
        let date_galley = (!self.date.is_empty()).then(|| {
            ui.painter().layout_no_wrap(
                self.date.clone(),
                FontId::proportional(t.text_xs),
                Color32::PLACEHOLDER,
            )
        });

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(WIDTH, HEIGHT), Sense::click_and_drag());
        response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &title));

        let mut colors = Self::colors(self.state, response.hovered(), &t);
        colors.fill = transition(ui, response.id.with("fill"), colors.fill);
        let radius = t.corner_md();
        let painter = ui.painter();
        painter.rect(rect, radius, colors.fill, colors.border, StrokeKind::Inside);
        if self.selected || response.has_focus() {
            painter.rect_stroke(
                rect,
                radius,
                Stroke::new(RING_WIDTH, t.tree_path),
                StrokeKind::Inside,
            );
        }
        painter.galley(
            Pos2::new(rect.left() + spacing::MD, rect.top() + spacing::SM),
            title_galley,
            colors.text,
        );
        if let Some(galley) = date_galley {
            let width = galley.size().x + 2.0 * spacing::SM;
            let pill_rect = Rect::from_min_size(
                Pos2::new(
                    rect.left() + spacing::MD,
                    rect.bottom() - spacing::SM - TAG_HEIGHT,
                ),
                Vec2::new(width, TAG_HEIGHT),
            );
            painter.rect(
                pill_rect,
                CornerRadius::same(u8::MAX),
                colors.tag_fill,
                Stroke::new(BORDER_WIDTH, colors.tag_border),
                StrokeKind::Inside,
            );
            painter.galley(
                pill_rect.center() - galley.size() / 2.0,
                galley,
                colors.tag_text,
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
                ui.label(
                    RichText::new(self.state.description())
                        .size(t.text_xs)
                        .color(t.muted_foreground),
                );
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

/// The point an edge to the node from its parent ends at: the middle of its left side.
pub fn left_anchor(rect: Rect) -> Pos2 {
    Pos2::new(rect.left(), rect.center().y)
}

/// The point an edge to a child starts at: the middle of the node's right side.
pub fn right_anchor(rect: Rect) -> Pos2 {
    Pos2::new(rect.right(), rect.center().y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::common::luminance;
    use crate::components::testing::Harness;

    const STATES: [TreeNodeState; 3] = [
        TreeNodeState::Current,
        TreeNodeState::Explored,
        TreeNodeState::Open,
    ];

    #[test]
    fn labels_and_pills_are_readable_on_every_state_in_both_themes() {
        for t in [Theme::light(), Theme::dark()] {
            for state in STATES {
                for hovered in [false, true] {
                    let c = TreeNode::colors(state, hovered, &t);
                    let label_gap = (luminance(c.fill) - luminance(c.text)).abs();
                    let tag_gap = (luminance(c.fill) - luminance(c.tag_text)).abs();
                    assert!(
                        label_gap > 0.35,
                        "label {state:?} dark={} hovered={hovered}: {label_gap}",
                        t.is_dark
                    );
                    assert!(
                        tag_gap > 0.25,
                        "pill {state:?} dark={} hovered={hovered}: {tag_gap}",
                        t.is_dark
                    );
                }
            }
        }
    }

    #[test]
    fn every_node_is_gray_and_only_the_pill_carries_the_state() {
        for t in [Theme::light(), Theme::dark()] {
            let [current, explored, open] = STATES.map(|s| TreeNode::colors(s, false, &t));
            assert_eq!(current.fill, t.muted);
            assert_eq!(explored.fill, t.muted);
            assert_eq!(open.fill, t.muted);
            assert_eq!(current.tag_border, t.tree_current, "gold");
            assert_eq!(explored.tag_border, t.tree_explored, "green");
            assert_eq!(open.tag_border, t.muted_foreground, "gray");
            assert_ne!(current.tag_border, explored.tag_border);
        }
    }

    #[test]
    fn hovering_changes_the_fill() {
        let t = Theme::dark();
        for state in STATES {
            let (rest, hover) = (
                TreeNode::colors(state, false, &t),
                TreeNode::colors(state, true, &t),
            );
            assert_ne!(rest.fill, hover.fill, "{state:?}");
        }
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
    fn every_node_has_the_same_size_whatever_its_text() {
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
        assert!(
            sizes.iter().all(|s| *s == Vec2::new(WIDTH, HEIGHT)),
            "{sizes:?}"
        );
    }

    #[test]
    fn edges_attach_to_the_middle_of_the_sides() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(WIDTH, HEIGHT));
        assert_eq!(left_anchor(rect), Pos2::new(10.0, 20.0 + HEIGHT / 2.0));
        assert_eq!(
            right_anchor(rect),
            Pos2::new(10.0 + WIDTH, 20.0 + HEIGHT / 2.0)
        );
    }

    #[test]
    fn hovering_opens_a_card_with_the_whole_title_and_where_the_node_stands() {
        let ctx = egui::Context::default();
        crate::Theme::apply(&ctx);
        let mut pointer = None;
        let mut drawn = String::new();
        let title = "A title that is far longer than the card on the canvas can show";
        for frame in 0..20 {
            let mut rect = Rect::NOTHING;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                time: Some(f64::from(frame) * 0.1),
                // The pointer moves once and then rests, as a tooltip needs.
                events: pointer
                    .filter(|_| frame == 1)
                    .map(egui::Event::PointerMoved)
                    .into_iter()
                    .collect(),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    rect = ui
                        .add(
                            TreeNode::new(title)
                                .state(TreeNodeState::Explored)
                                .hidden(3),
                        )
                        .rect;
                });
            });
            output.textures_delta.clear();
            pointer = Some(rect.center());
            drawn = format!("{:?}", output.shapes);
        }
        assert!(drawn.contains("far longer than the card"), "{drawn}");
        assert!(drawn.contains("On the path you took"));
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
