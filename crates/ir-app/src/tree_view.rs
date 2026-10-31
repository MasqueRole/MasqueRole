//! The story tree screen: the whole tree drawn on a canvas you can pan and zoom.
//!
//! The tree grows from the top down. Nodes are the design system's `TreeNode`, colored dots
//! that open a card with the title and date on hover, painted inside an egui `Scene`, which
//! gives panning (drag or scroll) and zooming (Ctrl+scroll or pinch) for free. Click a node
//! to preview it, click it again to jump there. Right-click a node, or use the preview
//! buttons, to collapse a branch.

use std::collections::{HashMap, HashSet};

use eframe::egui::{Align, Layout, Pos2, Rect, Scene, Shape, Stroke, Ui, UiBuilder, Vec2};
use ir_core::db::{Node, NodeId, NodeKind};
use ir_ui::components::tree_node::{bottom_anchor, top_anchor, SIZE as NODE_SIZE};
use ir_ui::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, IconKind, Label, ScrollArea, TreeNode,
    TreeNodeState,
};
use ir_ui::{spacing, ThemeExt, BORDER_WIDTH, RING_WIDTH};

use crate::backend::TreeData;
use crate::format;
use crate::tree_layout::{self, Placed, TreeLayout};

/// Horizontal distance between neighbouring nodes of a level: a dot plus the gap.
const COLUMN_WIDTH: f32 = NODE_SIZE + spacing::XL;

/// Vertical distance between levels: a dot plus the gap for edges.
const ROW_HEIGHT: f32 = NODE_SIZE + spacing::XXL;

/// The zoom limits of the canvas.
const MIN_ZOOM: f32 = 0.15;
const MAX_ZOOM: f32 = 2.0;

/// How much one press of a zoom button zooms.
const ZOOM_STEP: f32 = 1.25;

/// The size of the view before the canvas has been measured, in scene units.
const DEFAULT_VIEW: Vec2 = Vec2::new(1000.0, 600.0);

/// Space kept around the tree when fitting it into view.
const FIT_MARGIN: f32 = 60.0;

/// Width of the column of buttons in the preview.
const PREVIEW_BUTTONS: f32 = 140.0;

/// Width of the right-click menu on a node.
const CONTEXT_MENU_WIDTH: f32 = 170.0;

/// State of the tree screen.
pub struct TreeView {
    selected: Option<NodeId>,
    collapsed: HashSet<NodeId>,
    /// The part of the canvas in view, in scene units.
    scene_rect: Rect,
    layout: TreeLayout,
    path: HashSet<NodeId>,
    /// Index of each node in the data the layout was built from.
    index: HashMap<NodeId, usize>,
    /// The data version the layout was built from.
    built_for: Option<u64>,
    /// The layout must be rebuilt, for example after collapsing a branch.
    dirty: bool,
    /// Center on the current node as soon as there is data.
    center_pending: bool,
    /// The node whose collapse was toggled last, to keep the selection visible.
    last_toggled: Option<NodeId>,
}

impl Default for TreeView {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeView {
    /// A view that centers on the current node when the data arrives.
    pub fn new() -> Self {
        Self {
            selected: None,
            collapsed: HashSet::new(),
            scene_rect: Rect::ZERO,
            layout: TreeLayout::default(),
            path: HashSet::new(),
            index: HashMap::new(),
            built_for: None,
            dirty: false,
            center_pending: true,
            last_toggled: None,
        }
    }

    /// The node being previewed.
    #[cfg(test)]
    pub fn selected(&self) -> Option<NodeId> {
        self.selected
    }

    /// Whether `id`'s branch is collapsed.
    #[cfg(test)]
    pub fn is_collapsed(&self, id: NodeId) -> bool {
        self.collapsed.contains(&id)
    }

    /// Bring the layout up to date with `data`, which has the given `version`. Call every
    /// frame; it only does work when the data or the collapsed set changed.
    pub fn sync(&mut self, data: Option<&TreeData>, version: u64) {
        let Some(data) = data else {
            self.layout = TreeLayout::default();
            self.path.clear();
            self.index.clear();
            self.built_for = Some(version);
            self.dirty = false;
            return;
        };
        if self.built_for == Some(version) && !self.dirty {
            return;
        }
        self.layout = tree_layout::layout(&data.nodes, &self.collapsed);
        self.path = tree_layout::path_to(&data.nodes, data.current);
        self.index = data
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id, i))
            .collect();
        self.built_for = Some(version);
        self.dirty = false;

        // Keep the selection on something visible: a node hidden by collapsing is
        // replaced by the branch that hides it, and a deleted node by nothing.
        if let Some(selected) = self.selected {
            if !self.index.contains_key(&selected) {
                self.selected = None;
            } else if self.layout.get(selected).is_none() {
                self.selected = self
                    .last_toggled
                    .filter(|id| self.layout.get(*id).is_some());
            }
        }
        if self.center_pending {
            if let Some(current) = data.current {
                self.center_on(current);
                self.center_pending = false;
            } else if !data.nodes.is_empty() {
                self.fit();
                self.center_pending = false;
            }
        }
    }

    /// Where a node's top-left corner is on the canvas.
    fn position(placed: &Placed) -> Pos2 {
        Pos2::new(placed.row * COLUMN_WIDTH, placed.depth as f32 * ROW_HEIGHT)
    }

    /// The canvas size of the view, or a default before it has been measured.
    fn view_size(&self) -> Vec2 {
        if self.scene_rect.is_finite() && self.scene_rect.size().min_elem() > 0.0 {
            self.scene_rect.size()
        } else {
            DEFAULT_VIEW
        }
    }

    /// Pan so `id` is in the middle of the view, keeping the zoom.
    pub fn center_on(&mut self, id: NodeId) {
        if let Some(placed) = self.layout.get(id) {
            let middle = Self::position(placed) + Vec2::splat(NODE_SIZE / 2.0);
            self.scene_rect = Rect::from_center_size(middle, self.view_size());
        }
    }

    /// Bring the current node into view. If a collapsed branch hides it, those branches are
    /// expanded first.
    pub fn show_current(&mut self, current: NodeId) {
        if self.layout.get(current).is_some() {
            self.center_on(current);
        } else {
            self.collapsed.retain(|id| !self.path.contains(id));
            self.dirty = true;
            self.center_pending = true;
        }
    }

    /// Zoom and pan so the whole tree is in view.
    pub fn fit(&mut self) {
        if self.layout.placed.is_empty() {
            return;
        }
        let size = Vec2::new(
            self.layout.rows.max(1.0) * COLUMN_WIDTH,
            (self.layout.max_depth as f32 + 1.0) * ROW_HEIGHT,
        );
        let bounds = Rect::from_min_size(Pos2::ZERO, size).expand(FIT_MARGIN);
        self.scene_rect = bounds;
    }

    /// Zoom in (`factor > 1`) or out (`factor < 1`) around the middle of the view.
    pub fn zoom(&mut self, factor: f32) {
        let center = if self.scene_rect.is_finite() && self.scene_rect.size().min_elem() > 0.0 {
            self.scene_rect.center()
        } else {
            Pos2::ZERO
        };
        self.scene_rect = Rect::from_center_size(center, self.view_size() / factor);
    }

    /// Handle a click on a node. The first click selects it for preview; clicking the
    /// selected node again returns it, meaning "jump there".
    pub fn click(&mut self, id: NodeId) -> Option<NodeId> {
        if self.selected == Some(id) {
            Some(id)
        } else {
            self.selected = Some(id);
            None
        }
    }

    /// Collapse the branch below `id`, or expand it if it is collapsed.
    pub fn toggle_collapse(&mut self, id: NodeId) {
        if !self.collapsed.remove(&id) {
            self.collapsed.insert(id);
        }
        self.last_toggled = Some(id);
        self.dirty = true;
    }

    /// The label of a node: its own, else a preview of its text with who said it.
    pub fn node_label(node: &Node) -> String {
        if let Some(label) = node
            .label
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
        {
            return label.to_string();
        }
        let text = format::preview(node);
        match (node.kind, node.speaker.as_deref()) {
            (NodeKind::Character, Some(who)) => format!("{who}: {text}"),
            (NodeKind::User, _) => format!("You: {text}"),
            _ => text,
        }
    }

    // ---- drawing ----

    /// Draw the header and the canvas. Returns the node to jump to, if the user chose one.
    pub fn body(&mut self, ui: &mut Ui, data: Option<&TreeData>) -> Option<NodeId> {
        self.header(ui, data);
        let Some(data) = data else {
            ui.add(Label::new("There is no story to show yet.").muted());
            return None;
        };
        if data.nodes.is_empty() {
            ui.add(Label::new("The story has no messages yet.").muted());
            return None;
        }
        self.canvas(ui, data)
    }

    fn header(&mut self, ui: &mut Ui, data: Option<&TreeData>) {
        ui.horizontal(|ui| {
            ui.add(Label::new("Story tree").heading());
            if let Some(data) = data {
                let count = data.nodes.len();
                ui.add(
                    Badge::new(format!("{count} node{}", if count == 1 { "" } else { "s" }))
                        .variant(BadgeVariant::Secondary),
                );
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let small = |text: &str| {
                    Button::new(text)
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                };
                if ui.add(small("+")).clicked() {
                    self.zoom(ZOOM_STEP);
                }
                if ui.add(small("\u{2212}")).clicked() {
                    self.zoom(1.0 / ZOOM_STEP);
                }
                if ui.add(small("Fit")).clicked() {
                    self.fit();
                }
                if ui.add(small("Current")).clicked() {
                    if let Some(current) = data.and_then(|d| d.current) {
                        self.show_current(current);
                    }
                }
                ui.add(
                    Label::new("Drag to pan. Ctrl+scroll to zoom.")
                        .small()
                        .muted(),
                );
            });
        });
    }

    fn canvas(&mut self, ui: &mut Ui, data: &TreeData) -> Option<NodeId> {
        let t = ui.ir_theme();
        let mut clicked = None;
        let mut toggled = None;
        let mut jump = None;
        let mut scene_rect = self.scene_rect;

        Scene::new()
            .zoom_range(MIN_ZOOM..=MAX_ZOOM)
            .show(ui, &mut scene_rect, |ui| {
                // Only what is in (or near) view is drawn, so very large stories stay fast.
                let visible = ui.clip_rect().expand(COLUMN_WIDTH);
                let current_path = Stroke::new(RING_WIDTH, t.foreground);
                let other = Stroke::new(BORDER_WIDTH, t.muted_foreground);

                // Edges first, so the nodes cover their ends.
                for placed in &self.layout.placed {
                    let Some(parent) = placed.parent.and_then(|p| self.layout.get(p)) else {
                        continue;
                    };
                    let node_rect =
                        |p: &Placed| Rect::from_min_size(Self::position(p), Vec2::splat(NODE_SIZE));
                    let from = bottom_anchor(node_rect(parent));
                    let to = top_anchor(node_rect(placed));
                    if !visible.intersects(Rect::from_two_pos(from, to)) {
                        continue;
                    }
                    let bend = from.y + (to.y - from.y) / 2.0;
                    let on_path = self.path.contains(&placed.id) && self.path.contains(&parent.id);
                    ui.painter().add(Shape::line(
                        vec![from, Pos2::new(from.x, bend), Pos2::new(to.x, bend), to],
                        if on_path { current_path } else { other },
                    ));
                }

                for placed in &self.layout.placed {
                    let origin = Self::position(placed);
                    let area = Rect::from_min_size(origin, Vec2::splat(NODE_SIZE));
                    if !visible.intersects(area) {
                        continue;
                    }
                    let Some(node) = self.index.get(&placed.id).map(|&i| &data.nodes[i]) else {
                        continue;
                    };
                    let state = if data.current == Some(placed.id) {
                        TreeNodeState::Current
                    } else if self.path.contains(&placed.id) {
                        TreeNodeState::Explored
                    } else {
                        TreeNodeState::Open
                    };
                    let widget = TreeNode::new(Self::node_label(node))
                        .state(state)
                        .date(crate::format::timestamp(node.created_at))
                        .hidden(placed.hidden)
                        .bookmarked(node.bookmark.is_some())
                        .selected(self.selected == Some(placed.id));

                    let place = UiBuilder::new()
                        .max_rect(area)
                        .layout(Layout::left_to_right(Align::Min));
                    let response = ui.scope_builder(place, |ui| ui.add(widget)).inner;
                    if response.clicked() {
                        clicked = Some(placed.id);
                    }
                    response.context_menu(|ui| {
                        ui.set_width(CONTEXT_MENU_WIDTH);
                        if ui
                            .add(
                                Button::new("Jump here")
                                    .variant(ButtonVariant::Ghost)
                                    .fill_width(true),
                            )
                            .clicked()
                        {
                            jump = Some(placed.id);
                            ui.close_menu();
                        }
                        if placed.has_children {
                            let text = if placed.hidden > 0 {
                                "Expand branch"
                            } else {
                                "Collapse branch"
                            };
                            if ui
                                .add(
                                    Button::new(text)
                                        .variant(ButtonVariant::Ghost)
                                        .fill_width(true),
                                )
                                .clicked()
                            {
                                toggled = Some(placed.id);
                                ui.close_menu();
                            }
                        }
                    });
                }
            });
        self.scene_rect = scene_rect;

        if let Some(id) = toggled {
            self.toggle_collapse(id);
        }
        if let Some(id) = clicked {
            jump = jump.or(self.click(id));
        }
        jump
    }

    /// Draw the preview of the selected node. Returns the node to jump to, if the user chose
    /// one with the button.
    pub fn footer(&mut self, ui: &mut Ui, data: Option<&TreeData>) -> Option<NodeId> {
        let node = self
            .selected
            .zip(data)
            .and_then(|(id, d)| d.nodes.iter().find(|n| n.id == id));
        let Some(node) = node else {
            ui.add(
                Label::new("Click a node to preview it, and click it again to jump there. Right-click for more.")
                    .small()
                    .muted(),
            );
            return None;
        };
        let current = data.and_then(|d| d.current);
        let placed = self.layout.get(node.id).copied();
        let (mut jump, mut toggle, mut close) = (None, false, false);

        ui.horizontal_top(|ui| {
            let width =
                (ui.available_width() - PREVIEW_BUTTONS - ui.spacing().item_spacing.x).max(0.0);
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.add(Label::new(Self::node_label(node)));
                    ui.add(Badge::new(format!("#{}", node.id.0)).variant(BadgeVariant::Outline));
                    if let Some(name) = &node.bookmark {
                        ui.add(
                            Badge::new(format!("\u{2605} {name}")).variant(BadgeVariant::Secondary),
                        );
                    }
                    if current == Some(node.id) {
                        ui.add(Badge::new("current"));
                    }
                });
                ScrollArea::vertical()
                    .id_salt("tree-preview")
                    .max_height(5.0 * spacing::XL)
                    .auto_shrink(true)
                    .show(ui, |ui| {
                        ui.add(Label::new(&node.content).small().muted());
                    });
            });
            ui.vertical(|ui| {
                ui.set_width(PREVIEW_BUTTONS);
                let here = current == Some(node.id);
                if ui
                    .add(Button::new("Jump here").enabled(!here).fill_width(true))
                    .clicked()
                {
                    jump = Some(node.id);
                }
                if let Some(placed) = placed.filter(|p| p.has_children) {
                    let text = if placed.hidden > 0 {
                        "Expand branch"
                    } else {
                        "Collapse branch"
                    };
                    toggle = ui
                        .add(
                            Button::new(text)
                                .variant(ButtonVariant::Outline)
                                .fill_width(true),
                        )
                        .clicked();
                }
                close = ui
                    .add(
                        Button::new("Close preview")
                            .icon(IconKind::Close)
                            .variant(ButtonVariant::Ghost)
                            .fill_width(true),
                    )
                    .clicked();
            });
        });
        if toggle {
            self.toggle_collapse(node.id);
        }
        if close {
            self.selected = None;
        }
        jump
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui;
    use ir_core::db::SessionId;
    use ir_ui::{Theme, ThemeMode};

    use super::*;

    fn node(
        id: i64,
        parent: Option<i64>,
        kind: NodeKind,
        speaker: Option<&str>,
        text: &str,
    ) -> Node {
        Node {
            id: NodeId(id),
            session_id: SessionId(1),
            parent_id: parent.map(NodeId),
            kind,
            speaker: speaker.map(Into::into),
            content: text.into(),
            label: None,
            bookmark: None,
            active_lore: vec![],
            created_at: 0,
        }
    }

    /// The README's tree, with node 9 current.
    fn data() -> TreeData {
        let shape = [
            (1, None),
            (2, Some(1)),
            (3, Some(2)),
            (4, Some(2)),
            (5, Some(3)),
            (6, Some(3)),
            (7, Some(4)),
            (8, Some(5)),
            (9, Some(6)),
        ];
        TreeData {
            session_id: SessionId(1),
            nodes: shape
                .into_iter()
                .map(|(id, parent)| {
                    node(id, parent, NodeKind::User, None, &format!("message {id}"))
                })
                .collect(),
            current: Some(NodeId(9)),
        }
    }

    fn synced() -> (TreeView, TreeData) {
        let mut view = TreeView::new();
        let data = data();
        view.sync(Some(&data), 1);
        (view, data)
    }

    // ---- state ----

    #[test]
    fn the_first_click_previews_and_the_second_jumps() {
        let mut view = TreeView::new();
        assert_eq!(view.click(NodeId(3)), None);
        assert_eq!(view.selected(), Some(NodeId(3)));
        assert_eq!(view.click(NodeId(3)), Some(NodeId(3)));
        // Clicking another node moves the selection instead.
        assert_eq!(view.click(NodeId(4)), None);
        assert_eq!(view.selected(), Some(NodeId(4)));
    }

    #[test]
    fn syncing_builds_the_layout_once_per_data_version() {
        let (mut view, data) = synced();
        assert_eq!(view.layout.placed.len(), 9);
        assert_eq!(view.path.len(), 5);

        view.layout = TreeLayout::default();
        view.sync(Some(&data), 1);
        assert!(
            view.layout.placed.is_empty(),
            "the same version is not rebuilt"
        );
        view.sync(Some(&data), 2);
        assert_eq!(view.layout.placed.len(), 9, "a new version is");
    }

    #[test]
    fn collapsing_rebuilds_the_layout_and_expanding_restores_it() {
        let (mut view, data) = synced();
        view.toggle_collapse(NodeId(3));
        assert!(view.is_collapsed(NodeId(3)));
        view.sync(Some(&data), 1);
        assert_eq!(view.layout.placed.len(), 5);
        assert_eq!(view.layout.get(NodeId(3)).unwrap().hidden, 4);

        view.toggle_collapse(NodeId(3));
        view.sync(Some(&data), 1);
        assert_eq!(view.layout.placed.len(), 9);
    }

    #[test]
    fn collapsing_a_branch_that_hides_the_selection_selects_the_branch() {
        let (mut view, data) = synced();
        view.click(NodeId(8));
        view.toggle_collapse(NodeId(3));
        view.sync(Some(&data), 1);
        assert_eq!(view.selected(), Some(NodeId(3)));

        // A selection that no longer exists in the data is dropped.
        view.click(NodeId(7));
        let mut smaller = data.clone();
        smaller.nodes.retain(|n| n.id != NodeId(7));
        view.sync(Some(&smaller), 2);
        assert_eq!(view.selected(), None);
    }

    #[test]
    fn showing_the_current_node_expands_a_branch_that_hides_it() {
        let (mut view, data) = synced();
        view.toggle_collapse(NodeId(3)); // hides 9, the current node
        view.sync(Some(&data), 1);
        assert!(view.layout.get(NodeId(9)).is_none());

        view.show_current(NodeId(9));
        view.sync(Some(&data), 1);
        assert!(!view.is_collapsed(NodeId(3)));
        let placed = *view.layout.get(NodeId(9)).expect("revealed");
        let middle = TreeView::position(&placed) + Vec2::splat(NODE_SIZE / 2.0);
        assert_eq!(view.scene_rect.center(), middle, "and centered on");

        // Branches that do not hide it stay collapsed.
        view.toggle_collapse(NodeId(4));
        view.toggle_collapse(NodeId(3));
        view.sync(Some(&data), 1);
        view.show_current(NodeId(9));
        view.sync(Some(&data), 1);
        assert!(view.is_collapsed(NodeId(4)) && !view.is_collapsed(NodeId(3)));
    }

    #[test]
    fn the_view_starts_centered_on_the_current_node_once() {
        let (mut view, data) = synced();
        let placed = *view.layout.get(NodeId(9)).unwrap();
        let middle = TreeView::position(&placed) + Vec2::splat(NODE_SIZE / 2.0);
        assert_eq!(view.scene_rect.center(), middle);

        // New data later does not pull the view away from where the user put it.
        view.scene_rect = view.scene_rect.translate(Vec2::new(500.0, 0.0));
        let before = view.scene_rect;
        view.sync(Some(&data), 2);
        assert_eq!(view.scene_rect, before);
    }

    #[test]
    fn a_story_without_a_current_node_is_fitted() {
        let mut view = TreeView::new();
        let mut data = data();
        data.current = None;
        view.sync(Some(&data), 1);
        assert!(view.scene_rect.height() > 4.0 * ROW_HEIGHT);
    }

    #[test]
    fn centering_and_zooming_keep_the_view_valid() {
        let (mut view, _) = synced();
        let size = view.scene_rect.size();
        view.center_on(NodeId(1));
        assert_eq!(view.scene_rect.size(), size, "centering keeps the zoom");
        view.zoom(2.0);
        assert_eq!(view.scene_rect.size(), size / 2.0);
        view.zoom(0.5);
        assert_eq!(view.scene_rect.size(), size);

        view.center_on(NodeId(12345)); // not in the tree: nothing happens
        assert_eq!(view.scene_rect.size(), size);
    }

    #[test]
    fn fitting_shows_the_whole_tree() {
        let (mut view, _) = synced();
        view.fit();
        let last = view
            .layout
            .placed
            .iter()
            .map(TreeView::position)
            .fold(Pos2::ZERO, |a, b| a.max(b));
        assert!(view.scene_rect.contains(Pos2::ZERO) && view.scene_rect.contains(last));
    }

    #[test]
    fn zooming_works_before_the_view_has_been_measured() {
        let mut view = TreeView::new();
        view.zoom(2.0);
        assert_eq!(view.scene_rect.size(), DEFAULT_VIEW / 2.0);
    }

    #[test]
    fn labels_use_the_node_label_or_a_preview_with_the_speaker() {
        let user = node(1, None, NodeKind::User, None, "I open the door.");
        assert_eq!(TreeView::node_label(&user), "You: I open the door.");
        let mira = node(
            2,
            None,
            NodeKind::Character,
            Some("Mira"),
            "Ah, there you are.",
        );
        assert_eq!(TreeView::node_label(&mira), "Mira: Ah, there you are.");
        let narrator = node(3, None, NodeKind::Narrator, None, "Fog.");
        assert_eq!(TreeView::node_label(&narrator), "Fog.");
        let mut named = user.clone();
        named.label = Some("  The door ".into());
        assert_eq!(TreeView::node_label(&named), "The door");
        named.label = Some("   ".into());
        assert_eq!(TreeView::node_label(&named), "You: I open the door.");
    }

    #[test]
    fn the_tree_grows_downward_and_siblings_sit_side_by_side() {
        let (view, _) = synced();
        let at = |id| TreeView::position(view.layout.get(NodeId(id)).unwrap());
        // A child is one level below its parent, and the two children of 2 share a level.
        assert!(at(2).y > at(1).y);
        assert_eq!(at(3).y, at(4).y);
        assert!(at(3).x != at(4).x);
        // A parent sits between its children.
        assert!(at(2).x >= at(3).x.min(at(4).x) && at(2).x <= at(3).x.max(at(4).x));
    }

    // ---- drawing ----

    fn frame(ctx: &egui::Context, add: &mut dyn FnMut(&egui::Context)) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0))),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| add(ctx));
    }

    #[test]
    fn the_screen_draws_in_both_themes_with_selection_collapse_and_bookmarks() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let ctx = egui::Context::default();
            Theme::apply(&ctx);
            Theme::set_mode(&ctx, mode);
            let (mut view, mut data) = synced();
            data.nodes[3].bookmark = Some("decision".into());
            data.nodes[6].label = Some("Shipwreck".into());
            view.click(NodeId(4));
            view.toggle_collapse(NodeId(3));
            view.sync(Some(&data), 2);
            for _ in 0..2 {
                frame(&ctx, &mut |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        assert_eq!(view.body(ui, Some(&data)), None);
                    });
                    egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
                        assert_eq!(view.footer(ui, Some(&data)), None);
                    });
                });
            }
        }
    }

    #[test]
    fn empty_and_missing_data_draw_a_message_and_never_panic() {
        let ctx = egui::Context::default();
        Theme::apply(&ctx);
        let mut view = TreeView::new();
        let empty = TreeData {
            session_id: SessionId(1),
            nodes: vec![],
            current: None,
        };
        for data in [None, Some(&empty)] {
            view.sync(data, 1);
            frame(&ctx, &mut |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert_eq!(view.body(ui, data), None);
                    assert_eq!(view.footer(ui, data), None);
                });
            });
        }
    }

    #[test]
    fn a_large_story_draws_only_what_is_in_view() {
        // 3000 turns in a chain with branches every tenth node: drawing must stay fast
        // because off-screen nodes are skipped.
        let mut nodes = Vec::new();
        for i in 1..=3000_i64 {
            nodes.push(node(
                i,
                (i > 1).then_some(i - 1),
                NodeKind::User,
                None,
                &format!("turn {i}"),
            ));
            if i % 10 == 0 {
                nodes.push(node(10_000 + i, Some(i), NodeKind::Narrator, None, "side"));
            }
        }
        let data = TreeData {
            session_id: SessionId(1),
            nodes,
            current: Some(NodeId(3000)),
        };
        let ctx = egui::Context::default();
        Theme::apply(&ctx);
        let mut view = TreeView::new();
        view.sync(Some(&data), 1);
        let started = std::time::Instant::now();
        for _ in 0..3 {
            frame(&ctx, &mut |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    view.body(ui, Some(&data));
                });
            });
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }
}
