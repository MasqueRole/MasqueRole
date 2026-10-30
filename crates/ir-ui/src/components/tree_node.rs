//! Tree node: the visual for one node of the story tree.

use egui::text::LayoutJob;
use egui::{
    Color32, CursorIcon, FontId, Pos2, Response, Sense, Shape, Stroke, StrokeKind, Ui, Widget,
    WidgetInfo, WidgetType,
};

use super::common::{contrast_text, focus_ring};
use crate::theme::{spacing, Theme, ThemeExt, BORDER_WIDTH, RING_WIDTH};

const NODE_HEIGHT: f32 = 28.0;

/// Widest a node grows before its label is cut with an ellipsis.
const NODE_MAX_WIDTH: f32 = 200.0;

/// Dash and gap lengths of the outline on unexplored nodes.
const DASH: f32 = 4.0;
const GAP: f32 = 3.0;

/// Where a node stands relative to the story.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeNodeState {
    /// The node the story is at now.
    Current,
    /// A node on the path that was taken.
    Explored,
    /// A node on an alternative branch.
    Open,
}

/// A clickable node. The caller decides what a click means, such as a preview on the first
/// click and a jump on the second.
#[derive(Clone, Debug)]
pub struct TreeNode {
    label: String,
    state: TreeNodeState,
    bookmarked: bool,
    selected: bool,
}

/// How a node is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
struct NodeColors {
    fill: Color32,
    text: Color32,
    border: Stroke,
    dashed: bool,
}

impl TreeNode {
    /// An open (unexplored) node with this label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            state: TreeNodeState::Open,
            bookmarked: false,
            selected: false,
        }
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
        let hover = |fill: Color32| {
            if hovered {
                Theme::mix(fill, t.foreground, 0.1)
            } else {
                fill
            }
        };
        match state {
            TreeNodeState::Current => {
                let fill = hover(t.tree_current);
                NodeColors {
                    fill,
                    text: contrast_text(fill, t.foreground, t.background),
                    border: Stroke::new(RING_WIDTH, t.foreground),
                    dashed: false,
                }
            }
            TreeNodeState::Explored => {
                let fill = hover(t.tree_explored);
                NodeColors {
                    fill,
                    text: contrast_text(fill, t.foreground, t.background),
                    border: Stroke::NONE,
                    dashed: false,
                }
            }
            TreeNodeState::Open => NodeColors {
                fill: hover(t.background),
                text: t.muted_foreground,
                border: Stroke::new(BORDER_WIDTH, t.muted_foreground),
                dashed: true,
            },
        }
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

        // Lay the label out first, in a neutral color, to size the node.
        let mut job = LayoutJob::simple(
            label.clone(),
            FontId::proportional(t.text_sm),
            Color32::PLACEHOLDER,
            NODE_MAX_WIDTH - 2.0 * spacing::MD,
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.painter().layout_job(job);

        let size = egui::vec2(galley.size().x + 2.0 * spacing::MD, NODE_HEIGHT);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &label));

        let colors = Self::colors(self.state, response.hovered(), &t);
        let radius = t.corner_md();
        let painter = ui.painter();
        painter.rect_filled(rect, radius, colors.fill);
        if colors.dashed {
            let corners = [
                rect.left_top(),
                rect.right_top(),
                rect.right_bottom(),
                rect.left_bottom(),
                rect.left_top(),
            ];
            painter.extend(Shape::dashed_line(&corners, colors.border, DASH, GAP));
        } else if colors.border != Stroke::NONE {
            painter.rect_stroke(rect, radius, colors.border, StrokeKind::Inside);
        }
        let pos = Pos2::new(
            rect.left() + spacing::MD,
            rect.center().y - galley.size().y / 2.0,
        );
        painter.galley(pos, galley, colors.text);

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
    fn labels_are_readable_on_every_state_in_both_themes() {
        for t in [Theme::light(), Theme::dark()] {
            for state in STATES {
                for hovered in [false, true] {
                    let c = TreeNode::colors(state, hovered, &t);
                    let gap = (luminance(c.fill) - luminance(c.text)).abs();
                    assert!(
                        gap > 0.35,
                        "{state:?} dark={} hovered={hovered}: {gap}",
                        t.is_dark
                    );
                }
            }
        }
    }

    #[test]
    fn states_use_the_tree_colors_and_only_open_nodes_are_dashed() {
        let t = Theme::light();
        let current = TreeNode::colors(TreeNodeState::Current, false, &t);
        let explored = TreeNode::colors(TreeNodeState::Explored, false, &t);
        let open = TreeNode::colors(TreeNodeState::Open, false, &t);
        assert_eq!(current.fill, t.tree_current);
        assert_eq!(explored.fill, t.tree_explored);
        assert!(open.dashed && !current.dashed && !explored.dashed);
        assert!(
            current.border.width > 0.0,
            "the current node has a strong outline"
        );
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
    fn long_labels_are_capped_and_bookmarks_add_a_star() {
        let mut h = Harness::new();
        let (mut long, mut short, mut starred) = (0.0, 0.0, 0.0);
        h.idle(&mut |ui: &mut Ui| {
            long = ui.add(TreeNode::new("x".repeat(300))).rect.width();
            short = ui.add(TreeNode::new("Start")).rect.width();
            starred = ui.add(TreeNode::new("Start").bookmarked(true)).rect.width();
        });
        assert!(long <= NODE_MAX_WIDTH + 1.0, "{long}");
        assert!(starred > short);
    }

    #[test]
    fn every_state_renders_selected_and_not() {
        let mut h = Harness::new();
        h.idle(&mut |ui: &mut Ui| {
            for state in STATES {
                ui.add(TreeNode::new("Node").state(state));
                ui.add(
                    TreeNode::new("Node")
                        .state(state)
                        .selected(true)
                        .bookmarked(true),
                );
            }
        });
    }
}
