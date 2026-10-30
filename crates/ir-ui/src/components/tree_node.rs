//! Tree node: the visual for one node of the story tree.

use egui::text::LayoutJob;
use egui::{
    Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    Vec2, Widget, WidgetInfo, WidgetType,
};

use super::common::focus_ring;
use crate::theme::{spacing, Theme, ThemeExt, BORDER_WIDTH};

/// Height of every node: the label, and the tag below it.
pub const HEIGHT: f32 = 56.0;

/// Widest a node grows before its label is cut with an ellipsis.
pub const MAX_WIDTH: f32 = 200.0;

/// Height of the tag.
const TAG_HEIGHT: f32 = 20.0;

/// How much a light theme's tag text is darkened, so gold and green stay readable on gray.
const LIGHT_TEXT_DARKENING: f32 = 0.55;

/// Where a node stands relative to the story.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeNodeState {
    /// The node the story is at now. Its tag is gold.
    Current,
    /// A node on the path that was taken. Its tag is green.
    Explored,
    /// A node on an alternative branch. Its tag is gray.
    Open,
}

/// A clickable node: a gray box with the label, and a tag showing the node's state by its
/// color. The caller decides what a click means, such as a preview on the first click and a
/// jump on the second.
#[derive(Clone, Debug)]
pub struct TreeNode {
    label: String,
    tag: String,
    state: TreeNodeState,
    bookmarked: bool,
    selected: bool,
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
    /// An open (alternative) node with this label and no tag text.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            tag: String::new(),
            state: TreeNodeState::Open,
            bookmarked: false,
            selected: false,
        }
    }

    /// The text in the tag, such as a timestamp. The tag's color shows the state.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
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

    /// Highlight the node as the one being previewed.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn colors(state: TreeNodeState, hovered: bool, t: &Theme) -> NodeColors {
        let fill = if hovered {
            Theme::mix(t.muted, t.foreground, 0.1)
        } else {
            t.muted
        };
        // Border and text share a color: gold, green, or gray.
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
        let label = if self.bookmarked {
            format!("\u{2605} {}", self.label)
        } else {
            self.label.clone()
        };

        // Lay the texts out first, in a neutral color, to size the node.
        let mut job = LayoutJob::simple(
            label.clone(),
            FontId::proportional(t.text_sm),
            Color32::PLACEHOLDER,
            MAX_WIDTH - 2.0 * spacing::MD,
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let label_galley = ui.painter().layout_job(job);
        let tag_galley = (!self.tag.is_empty()).then(|| {
            ui.painter().layout_no_wrap(
                self.tag.clone(),
                FontId::proportional(t.text_xs),
                Color32::PLACEHOLDER,
            )
        });
        let tag_width = tag_galley
            .as_ref()
            .map_or(0.0, |g| g.size().x + 2.0 * spacing::SM);

        let content = label_galley.size().x.max(tag_width);
        let size = Vec2::new((content + 2.0 * spacing::MD).min(MAX_WIDTH), HEIGHT);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &label));

        let colors = Self::colors(self.state, response.hovered(), &t);
        let radius = t.corner_md();
        let painter = ui.painter();
        painter.rect(rect, radius, colors.fill, colors.border, StrokeKind::Inside);
        painter.galley(
            Pos2::new(rect.left() + spacing::MD, rect.top() + spacing::SM),
            label_galley,
            colors.text,
        );
        if let Some(galley) = tag_galley {
            let tag_rect = Rect::from_min_size(
                Pos2::new(
                    rect.left() + spacing::MD,
                    rect.bottom() - spacing::SM - TAG_HEIGHT,
                ),
                Vec2::new(tag_width, TAG_HEIGHT),
            );
            let pill = CornerRadius::same(u8::MAX);
            painter.rect(
                tag_rect,
                pill,
                colors.tag_fill,
                Stroke::new(BORDER_WIDTH, colors.tag_border),
                StrokeKind::Inside,
            );
            painter.galley(
                tag_rect.center() - galley.size() / 2.0,
                galley,
                colors.tag_text,
            );
        }

        if self.selected || response.has_focus() {
            focus_ring(ui, rect, radius, &t);
        }
        response.on_hover_cursor(CursorIcon::PointingHand)
    }
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
    fn labels_and_tags_are_readable_on_every_state_in_both_themes() {
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
                        "tag {state:?} dark={} hovered={hovered}: {tag_gap}",
                        t.is_dark
                    );
                }
            }
        }
    }

    #[test]
    fn every_node_is_gray_and_only_the_tag_carries_the_state() {
        for t in [Theme::light(), Theme::dark()] {
            let [current, explored, open] = STATES.map(|s| TreeNode::colors(s, false, &t));
            assert_eq!(current.fill, t.muted);
            assert_eq!(explored.fill, t.muted);
            assert_eq!(open.fill, t.muted);
            assert_eq!(current.tag_border, t.tree_current, "gold");
            assert_eq!(explored.tag_border, t.tree_explored, "green");
            assert_eq!(open.tag_border, t.muted_foreground, "gray");
            for c in [current, explored, open] {
                assert!(
                    c.tag_border != c.fill && c.tag_border != Color32::TRANSPARENT,
                    "the tag has a visible border"
                );
            }
            assert_ne!(current.tag_border, explored.tag_border);
        }
    }

    #[test]
    fn the_tag_text_matches_its_border_in_the_dark_theme() {
        let t = Theme::dark();
        for state in STATES {
            let c = TreeNode::colors(state, false, &t);
            assert_eq!(c.tag_text, c.tag_border, "{state:?}");
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
        let mut rect = egui::Rect::NOTHING;
        h.idle(&mut |ui: &mut Ui| rect = ui.add(TreeNode::new("Enter the lighthouse")).rect);
        let mut clicked = false;
        h.click(rect.center(), &mut |ui: &mut Ui| {
            clicked |= ui.add(TreeNode::new("Enter the lighthouse")).clicked();
        });
        assert!(clicked);
    }

    #[test]
    fn nodes_have_a_fixed_height_and_a_capped_width() {
        let mut h = Harness::new();
        let mut sizes = Vec::new();
        h.idle(&mut |ui: &mut Ui| {
            sizes.clear();
            sizes.push(ui.add(TreeNode::new("x".repeat(300))).rect.size());
            sizes.push(
                ui.add(TreeNode::new("Start").tag("30 Oct 14:32"))
                    .rect
                    .size(),
            );
            sizes.push(ui.add(TreeNode::new("Start")).rect.size());
        });
        assert!(sizes.iter().all(|s| s.y == HEIGHT), "{sizes:?}");
        assert!(sizes[0].x <= MAX_WIDTH + 1.0, "{:?}", sizes[0]);
    }

    #[test]
    fn a_long_tag_widens_a_short_node_and_a_bookmark_adds_a_star() {
        let mut h = Harness::new();
        let (mut plain, mut tagged, mut starred) = (0.0, 0.0, 0.0);
        h.idle(&mut |ui: &mut Ui| {
            plain = ui.add(TreeNode::new("A")).rect.width();
            tagged = ui.add(TreeNode::new("A").tag("30 Oct 14:32")).rect.width();
            starred = ui.add(TreeNode::new("A").bookmarked(true)).rect.width();
        });
        assert!(tagged > plain, "{tagged} vs {plain}");
        assert!(starred > plain);
    }

    #[test]
    fn every_state_renders_selected_and_not_with_and_without_a_tag() {
        let mut h = Harness::new();
        h.idle(&mut |ui: &mut Ui| {
            for state in STATES {
                ui.add(TreeNode::new("Node").state(state));
                ui.add(
                    TreeNode::new("Node")
                        .state(state)
                        .tag("14:32")
                        .selected(true)
                        .bookmarked(true),
                );
            }
        });
    }
}
