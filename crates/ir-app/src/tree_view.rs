//! The story tree screen: the whole tree drawn on a canvas you can pan and zoom.
//!
//! The tree grows from the top down. Nodes are the design system's `TreeNode`, colored dots
//! that open a card with the title and date on hover, painted inside an egui `Scene`, which
//! gives panning (drag or scroll) and zooming (Ctrl+scroll or pinch) for free. Click a node
//! to open it: the node grows into a card with the whole message and the buttons to jump there
//! or collapse its branch. Click the node again to jump, or use the card's close button.
//! Right-click a node for the same actions.

use std::collections::{HashMap, HashSet};

use eframe::egui::epaint::CubicBezierShape;
use eframe::egui::text::LayoutJob;
use eframe::egui::{
    Align, CornerRadius, FontId, Id, Key, Layout, Pos2, Rect, Scene, Sense, Shadow, Shape, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2,
};
use ir_core::db::{Node, NodeId, NodeKind};
use ir_ui::components::tree_node::{
    left_anchor, right_anchor, HEIGHT as NODE_HEIGHT, WIDTH as NODE_WIDTH,
};
use ir_ui::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, IconKind, Label, TooltipExt, TreeNode,
    TreeNodeState,
};
use ir_ui::{motion, spacing, Theme, ThemeExt, RING_WIDTH};

use crate::backend::TreeData;
use crate::format;
use crate::tree_layout::{self, Placed, TreeLayout};

/// Horizontal distance between levels: a node plus the gap the curves of the edges cross.
const COLUMN_WIDTH: f32 = NODE_WIDTH + 2.0 * spacing::XXL;

/// Vertical distance between the nodes of a level: a node plus the gap.
const ROW_HEIGHT: f32 = NODE_HEIGHT + spacing::MD;

/// The zoom limits of the canvas.
const MIN_ZOOM: f32 = 0.15;
const MAX_ZOOM: f32 = 2.0;

/// How much one press of a zoom button zooms.
const ZOOM_STEP: f32 = 1.25;

/// The size of the view before the canvas has been measured, in scene units.
const DEFAULT_VIEW: Vec2 = Vec2::new(1000.0, 600.0);

/// Space kept around the tree when fitting it into view.
const FIT_MARGIN: f32 = 60.0;

/// Size of the card an opened node grows into, in scene units.
const CARD_WIDTH: f32 = 300.0;
const CARD_HEIGHT: f32 = 212.0;

/// How many lines of the message the card shows.
const CARD_TEXT_ROWS: usize = 4;

/// The card shows its content once it has opened this far, so the text never spills out of a
/// card that is still small.
const CARD_CONTENT_FROM: f32 = 0.55;

/// Width of the right-click menu on a node.
const CONTEXT_MENU_WIDTH: f32 = 170.0;

/// State of the tree screen.
pub struct TreeView {
    selected: Option<NodeId>,
    /// The node whose card is drawn: the selected one, or the one that is still closing.
    shown: Option<NodeId>,
    /// Where the user put cards, in scene units. Nodes not here are where the layout puts them.
    moved: HashMap<NodeId, Pos2>,
    /// Moves that were finished and are waiting to be saved: see [`TreeView::take_moves`].
    moves: Vec<(NodeId, Pos2)>,
    /// The user asked to put all cards back; see [`TreeView::take_reset`].
    reset: bool,
    /// The part of the screen the canvas covers, as of the last frame.
    canvas: Rect,
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
            shown: None,
            moved: HashMap::new(),
            moves: Vec::new(),
            reset: false,
            canvas: Rect::NOTHING,
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
        self.moved = data
            .positions
            .iter()
            .map(|(id, (x, y))| (*id, Pos2::new(*x, *y)))
            .collect();
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

    /// Where a node's top-left corner is on the canvas: where the user put it, else where the
    /// layout does.
    fn position(&self, placed: &Placed) -> Pos2 {
        self.moved
            .get(&placed.id)
            .copied()
            .unwrap_or_else(|| Self::laid_out(placed))
    }

    /// Where the automatic layout puts a node.
    fn laid_out(placed: &Placed) -> Pos2 {
        Pos2::new(placed.depth as f32 * COLUMN_WIDTH, placed.row * ROW_HEIGHT)
    }

    /// Take the moves the user finished since the last call, to save them.
    pub fn take_moves(&mut self) -> Vec<(NodeId, Pos2)> {
        std::mem::take(&mut self.moves)
    }

    /// Whether the user asked to put all cards back since the last call, so the saved
    /// positions can be forgotten.
    pub fn take_reset(&mut self) -> bool {
        std::mem::take(&mut self.reset)
    }

    /// Put every card back where the automatic layout puts it.
    pub fn tidy(&mut self) {
        self.moved.clear();
        self.moves.clear();
        self.reset = true;
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
            let middle = self.position(placed) + Vec2::new(NODE_WIDTH / 2.0, NODE_HEIGHT / 2.0);
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
        let size = Vec2::new(NODE_WIDTH, NODE_HEIGHT);
        let bounds = self
            .layout
            .placed
            .iter()
            .map(|p| Rect::from_min_size(self.position(p), size))
            .fold(Rect::NOTHING, |a, b| a.union(b));
        self.scene_rect = bounds.expand(FIT_MARGIN);
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
            self.shown = Some(id);
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
                ui.add(Badge::new(format::plural(count, "node")).variant(BadgeVariant::Secondary));
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
                if ui
                    .add(small("Tidy"))
                    .ir_tooltip("Put the cards back in their automatic places")
                    .clicked()
                {
                    self.tidy();
                }
                if ui.add(small("Current")).clicked() {
                    if let Some(current) = data.and_then(|d| d.current) {
                        self.show_current(current);
                    }
                }
                let room = ui.available_width();
                ui.add(
                    Label::new("Click a card to open it. Drag it to move it.")
                        .small()
                        .muted()
                        .max_width(room),
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
        let mut card_action = CardAction::default();
        let mut dragged: Vec<(NodeId, Vec2)> = Vec::new();
        let mut dropped: Vec<NodeId> = Vec::new();
        self.canvas = ui.available_rect_before_wrap();

        // The opened node grows into a card, and the card shrinks back into the node.
        if let Some(selected) = self.selected {
            self.shown = Some(selected);
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.selected = None;
            }
        }
        let opened = motion::eased(
            ui.ctx(),
            Id::new("tree-card"),
            self.selected.is_some(),
            motion::ENTER_SECONDS,
            motion::EXIT_SECONDS,
        );
        if self.selected.is_none() && opened <= 0.001 {
            self.shown = None;
        }

        Scene::new()
            .zoom_range(MIN_ZOOM..=MAX_ZOOM)
            .show(ui, &mut scene_rect, |ui| {
                // Only what is in (or near) view is drawn, so very large stories stay fast.
                let visible = ui.clip_rect().expand(COLUMN_WIDTH);
                let current_path = Stroke::new(RING_WIDTH, t.tree_path);
                let other = Stroke::new(RING_WIDTH, t.muted_foreground.gamma_multiply(0.6));

                // Edges first, so the nodes cover their ends.
                for placed in &self.layout.placed {
                    let Some(parent) = placed.parent.and_then(|p| self.layout.get(p)) else {
                        continue;
                    };
                    let node_rect = |p: &Placed| {
                        Rect::from_min_size(self.position(p), Vec2::new(NODE_WIDTH, NODE_HEIGHT))
                    };
                    let from = right_anchor(node_rect(parent));
                    let to = left_anchor(node_rect(placed));
                    if !visible.intersects(Rect::from_two_pos(from, to)) {
                        continue;
                    }
                    let on_path = self.path.contains(&placed.id) && self.path.contains(&parent.id);
                    ui.painter().add(Self::edge(
                        from,
                        to,
                        if on_path { current_path } else { other },
                    ));
                }

                for placed in &self.layout.placed {
                    let origin = self.position(placed);
                    let area = Rect::from_min_size(origin, Vec2::new(NODE_WIDTH, NODE_HEIGHT));
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
                    // Cards can be moved: the drag is in scene units, whatever the zoom.
                    if response.dragged() {
                        dragged.push((placed.id, response.drag_delta()));
                    }
                    if response.drag_stopped() {
                        dropped.push(placed.id);
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

                if let Some(id) = self.shown.filter(|_| opened > 0.001) {
                    card_action = self.card(ui, data, id, opened);
                }
            });
        self.scene_rect = scene_rect;
        for (id, delta) in dragged {
            if let Some(placed) = self.layout.get(id).copied() {
                let at = self.position(&placed) + delta;
                self.moved.insert(id, at);
            }
        }
        for id in dropped {
            if let Some(at) = self.moved.get(&id).copied() {
                self.moves.push((id, at));
            }
        }
        if card_action.close {
            self.selected = None;
        }
        toggled = toggled.or(card_action.toggle);
        jump = jump.or(card_action.jump);

        if let Some(id) = toggled {
            self.toggle_collapse(id);
        }
        if let Some(id) = clicked {
            jump = jump.or(self.click(id));
        }
        jump
    }
}

/// What the buttons of the open card asked for.
#[derive(Default)]
struct CardAction {
    jump: Option<NodeId>,
    toggle: Option<NodeId>,
    close: bool,
}

impl TreeView {
    /// An edge from `from` (the right side of a parent) to `to` (the left side of a child): a
    /// smooth S-shaped curve that leaves and arrives horizontally.
    fn edge(from: Pos2, to: Pos2, stroke: Stroke) -> Shape {
        let reach = (to.x - from.x) / 2.0;
        Shape::CubicBezier(CubicBezierShape::from_points_stroke(
            [
                from,
                Pos2::new(from.x + reach, from.y),
                Pos2::new(to.x - reach, to.y),
                to,
            ],
            false,
            eframe::egui::Color32::TRANSPARENT,
            stroke,
        ))
    }

    /// Draw the card of node `id`, `opened` of the way open (0 is the node, 1 the whole card).
    /// It grows out of the node's corner, so the node seems to open up.
    fn card(&self, ui: &mut Ui, data: &TreeData, id: NodeId, opened: f32) -> CardAction {
        let mut action = CardAction::default();
        let (Some(placed), Some(node)) = (
            self.layout.get(id),
            self.index.get(&id).map(|&i| &data.nodes[i]),
        ) else {
            return action;
        };
        let t = ui.ir_theme();
        let closed = Rect::from_min_size(self.position(placed), Vec2::new(NODE_WIDTH, NODE_HEIGHT));
        let full = Rect::from_min_size(closed.min, Vec2::new(CARD_WIDTH, CARD_HEIGHT));
        let grown = |from: f32, to: f32| eframe::egui::lerp(from..=to, opened);
        let rect = Rect::from_min_max(
            closed.min,
            Pos2::new(
                grown(closed.max.x, full.max.x),
                grown(closed.max.y, full.max.y),
            ),
        );
        let radius =
            CornerRadius::same(grown(f32::from(t.radius_md), f32::from(t.radius_lg)) as u8);
        let here = data.current == Some(id);

        let painter = ui.painter();
        painter.add(
            Shadow {
                offset: [0, 4],
                blur: 16,
                spread: 0,
                color: eframe::egui::Color32::from_black_alpha(if t.is_dark { 90 } else { 28 })
                    .gamma_multiply(opened),
            }
            .as_shape(rect, radius),
        );
        painter.rect(
            rect,
            radius,
            Theme::mix(t.muted, t.popover, opened),
            Stroke::new(RING_WIDTH, t.tree_path),
            StrokeKind::Inside,
        );
        // Clicks on the card stay on the card instead of reaching the node underneath.
        ui.interact(rect, Id::new("tree-card-body"), Sense::click_and_drag());

        if opened < CARD_CONTENT_FROM {
            return action;
        }
        let inner = full.shrink(spacing::MD);
        let mut content = ui.new_child(
            UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::top_down(Align::Min)),
        );
        content.set_clip_rect(rect.intersect(ui.clip_rect()));
        content.set_opacity((opened - CARD_CONTENT_FROM) / (1.0 - CARD_CONTENT_FROM));
        let ui = &mut content;
        ui.spacing_mut().item_spacing.y = spacing::XS;
        ui.horizontal(|ui| {
            ui.add(Label::new(Self::node_label(node)).max_width(inner.width() - 2.0 * spacing::XL));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let close = Button::new("")
                    .icon(IconKind::Close)
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Icon);
                action.close |= ui.add(close).ir_tooltip("Close").clicked();
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.add(
                Label::new(format::timestamp(node.created_at))
                    .small()
                    .muted(),
            );
            ui.add(Badge::new(format!("#{}", node.id.0)).variant(BadgeVariant::Outline));
            if let Some(name) = &node.bookmark {
                ui.add(
                    Badge::new(format!("\u{2605} {name}"))
                        .variant(BadgeVariant::Secondary)
                        .max_width(140.0),
                );
            }
            if here {
                ui.add(Badge::new("current"));
            }
        });
        let mut text = LayoutJob::simple(
            node.content.clone(),
            FontId::proportional(t.text_sm),
            t.muted_foreground,
            inner.width(),
        );
        text.wrap.max_rows = CARD_TEXT_ROWS;
        text.wrap.overflow_character = Some('\u{2026}');
        ui.add(eframe::egui::Label::new(text));

        // The buttons sit at the bottom of the card.
        let row = Rect::from_min_size(
            Pos2::new(inner.left(), inner.bottom() - 32.0),
            Vec2::new(inner.width(), 32.0),
        );
        let mut buttons = ui.new_child(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::left_to_right(Align::Center)),
        );
        if buttons
            .add(Button::new("Jump here").enabled(!here))
            .clicked()
        {
            action.jump = Some(id);
        }
        if placed.has_children {
            let text = if placed.hidden > 0 {
                "Expand branch"
            } else {
                "Collapse branch"
            };
            if buttons
                .add(Button::new(text).variant(ButtonVariant::Outline))
                .clicked()
            {
                action.toggle = Some(id);
            }
        }
        action
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
            positions: Default::default(),
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
        let middle = view.position(&placed) + Vec2::new(NODE_WIDTH / 2.0, NODE_HEIGHT / 2.0);
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
        let middle = view.position(&placed) + Vec2::new(NODE_WIDTH / 2.0, NODE_HEIGHT / 2.0);
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
        assert!(view.scene_rect.width() > 4.0 * COLUMN_WIDTH);
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
            .map(|p| view.position(p))
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
    fn the_tree_grows_to_the_right_and_siblings_stack_in_a_column() {
        let (view, _) = synced();
        let at = |id| view.position(view.layout.get(NodeId(id)).unwrap());
        // A child is one column right of its parent, and the two children of 2 share one.
        assert!(at(2).x > at(1).x);
        assert_eq!(at(3).x, at(4).x);
        assert!(at(3).y != at(4).y);
        // A parent sits between its children.
        assert!(at(2).y >= at(3).y.min(at(4).y) && at(2).y <= at(3).y.max(at(4).y));
    }

    #[test]
    fn an_edge_is_a_curve_that_leaves_and_arrives_horizontally() {
        let (from, to) = (Pos2::new(0.0, 0.0), Pos2::new(100.0, 80.0));
        let Shape::CubicBezier(curve) = TreeView::edge(from, to, Stroke::NONE) else {
            panic!("an edge is a curve");
        };
        assert_eq!(curve.points[0], from);
        assert_eq!(curve.points[3], to);
        assert_eq!(curve.points[1].y, from.y, "it leaves level with the parent");
        assert_eq!(curve.points[2].y, to.y, "and arrives level with the child");
        assert!(curve.points[1].x > from.x && curve.points[2].x < to.x);
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
                });
            }
        }
    }

    #[test]
    fn a_clicked_dot_opens_into_a_card_and_escape_closes_it_again() {
        let ctx = egui::Context::default();
        Theme::apply(&ctx);
        let (mut view, data) = synced();
        let mut time = 0.0;
        let mut run = |view: &mut TreeView, events: Vec<egui::Event>| {
            time += 0.05;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0))),
                time: Some(time),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    view.body(ui, Some(&data));
                });
            });
        };
        run(&mut view, vec![]);
        assert_eq!(view.shown, None, "no card before a click");

        view.click(NodeId(4));
        assert_eq!(view.shown, Some(NodeId(4)));
        for _ in 0..12 {
            run(&mut view, vec![]);
        }
        assert_eq!(view.selected(), Some(NodeId(4)), "the card stays open");

        let escape = |pressed| egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        run(&mut view, vec![escape(true), escape(false)]);
        assert_eq!(view.selected(), None);
        // The card is still drawn while it shrinks back, then it is gone.
        assert_eq!(view.shown, Some(NodeId(4)));
        for _ in 0..12 {
            run(&mut view, vec![]);
        }
        assert_eq!(view.shown, None);
    }

    #[test]
    fn saved_positions_replace_the_layout_and_tidying_puts_the_cards_back() {
        let mut view = TreeView::new();
        let mut data = data();
        data.positions.insert(NodeId(4), (500.0, -80.0));
        view.sync(Some(&data), 1);
        let placed = *view.layout.get(NodeId(4)).unwrap();
        assert_eq!(view.position(&placed), Pos2::new(500.0, -80.0));
        let other = *view.layout.get(NodeId(3)).unwrap();
        assert_eq!(view.position(&other), TreeView::laid_out(&other));

        view.tidy();
        assert_eq!(view.position(&placed), TreeView::laid_out(&placed));
        assert!(view.take_reset());
        assert!(!view.take_reset(), "the request is taken once");
    }

    #[test]
    fn fitting_includes_cards_that_were_moved_far_away() {
        let mut view = TreeView::new();
        let mut data = data();
        data.positions.insert(NodeId(4), (5000.0, 3000.0));
        view.sync(Some(&data), 1);
        view.fit();
        assert!(view.scene_rect.contains(Pos2::new(5000.0, 3000.0)));
        assert!(view.scene_rect.contains(Pos2::ZERO));
    }

    /// Run frames of the tree screen with `events`, at a fixed time step.
    fn drive(
        ctx: &egui::Context,
        view: &mut TreeView,
        data: &TreeData,
        time: &mut f64,
        events: Vec<egui::Event>,
    ) {
        *time += 0.05;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0))),
            time: Some(*time),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                view.body(ui, Some(data));
            });
        });
    }

    #[test]
    fn dragging_a_card_moves_it_by_the_distance_in_scene_units_at_any_zoom() {
        for zoom in [0.5, 1.0, 2.0] {
            let ctx = egui::Context::default();
            Theme::apply(&ctx);
            let (mut view, data) = synced();
            let mut time = 0.0;
            view.fit();
            view.zoom(zoom);
            for _ in 0..3 {
                drive(&ctx, &mut view, &data, &mut time, vec![]);
            }
            let placed = *view.layout.get(NodeId(4)).unwrap();
            let before = view.position(&placed);
            // Where the card is on the screen: the scene is fitted into the canvas with its
            // aspect kept, so the middle of `scene_rect` is the middle of the canvas.
            let scale = (view.canvas.width() / view.scene_rect.width())
                .min(view.canvas.height() / view.scene_rect.height());
            let to_screen = |p: Pos2| view.canvas.center() + (p - view.scene_rect.center()) * scale;
            let grab = to_screen(before + Vec2::new(NODE_WIDTH / 2.0, NODE_HEIGHT / 2.0));
            let button = |pressed| egui::Event::PointerButton {
                pos: grab
                    + if pressed {
                        Vec2::ZERO
                    } else {
                        Vec2::new(60.0, 30.0)
                    },
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            drive(
                &ctx,
                &mut view,
                &data,
                &mut time,
                vec![egui::Event::PointerMoved(grab)],
            );
            drive(&ctx, &mut view, &data, &mut time, vec![button(true)]);
            for step in 1..=4 {
                let at = grab + Vec2::new(15.0, 7.5) * step as f32;
                drive(
                    &ctx,
                    &mut view,
                    &data,
                    &mut time,
                    vec![egui::Event::PointerMoved(at)],
                );
            }
            drive(&ctx, &mut view, &data, &mut time, vec![button(false)]);
            drive(&ctx, &mut view, &data, &mut time, vec![]);

            let after = view.position(&placed);
            let moved = after - before;
            let expected = Vec2::new(60.0, 30.0) / scale;
            assert!(
                (moved - expected).length() < 2.0,
                "zoom {zoom}: moved {moved:?}, expected about {expected:?}"
            );
            let saved = view.take_moves();
            assert_eq!(saved.len(), 1, "one finished move is waiting to be saved");
            assert_eq!(saved[0], (NodeId(4), after));
            assert_eq!(view.selected(), None, "dragging is not a click");
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
            positions: Default::default(),
        };
        for data in [None, Some(&empty)] {
            view.sync(data, 1);
            frame(&ctx, &mut |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert_eq!(view.body(ui, data), None);
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
            positions: Default::default(),
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
