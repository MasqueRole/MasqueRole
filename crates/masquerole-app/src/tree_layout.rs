//! Placing story-tree nodes on a grid.
//!
//! The tree grows left to right: a node's column is its depth, and siblings are stacked in
//! rows. A parent sits at the middle of its children. Collapsed nodes hide their
//! descendants. Everything here is iterative, so very long stories cannot overflow the stack.

use std::collections::{HashMap, HashSet};

use masquerole_core::db::{Node, NodeId};

/// Where one node goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// The node.
    pub id: NodeId,
    /// Its parent, if it has one in this tree.
    pub parent: Option<NodeId>,
    /// Distance from the root; the column.
    pub depth: usize,
    /// The row, in units of one node height. Fractional for parents between children.
    pub row: f32,
    /// Whether the node has any children, shown or hidden.
    pub has_children: bool,
    /// How many descendants are hidden because this node is collapsed.
    pub hidden: usize,
}

/// A finished layout.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TreeLayout {
    /// The visible nodes, parents before their children.
    pub placed: Vec<Placed>,
    index: HashMap<NodeId, usize>,
    /// The number of rows used.
    pub rows: f32,
    /// The deepest column used.
    pub max_depth: usize,
}

impl TreeLayout {
    /// The placement of `id`, if it is visible.
    pub fn get(&self, id: NodeId) -> Option<&Placed> {
        self.index.get(&id).map(|&i| &self.placed[i])
    }
}

/// Lay out `nodes`, hiding the descendants of every node in `collapsed`.
///
/// Nodes whose parent is not among `nodes` are treated as roots. Roots and siblings keep
/// the order of `nodes` (oldest first), and separate roots are stacked one under another.
pub fn layout(nodes: &[Node], collapsed: &HashSet<NodeId>) -> TreeLayout {
    let known: HashSet<NodeId> = nodes.iter().map(|n| n.id).collect();
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    let mut roots: Vec<NodeId> = Vec::new();
    let mut parent_of: HashMap<NodeId, NodeId> = HashMap::new();
    for node in nodes {
        match node.parent_id.filter(|p| known.contains(p)) {
            Some(parent) => {
                children.entry(parent).or_default().push(node.id);
                parent_of.insert(node.id, parent);
            }
            None => roots.push(node.id),
        }
    }

    // Subtree sizes over the whole tree, to say how many nodes a collapsed branch hides.
    let full_order = preorder(&roots, |id| {
        children.get(&id).map_or(&[][..], Vec::as_slice)
    });
    let mut subtree: HashMap<NodeId, usize> = HashMap::with_capacity(full_order.len());
    for &id in full_order.iter().rev() {
        let below: usize = children
            .get(&id)
            .map_or(0, |kids| kids.iter().map(|k| subtree[k]).sum());
        subtree.insert(id, below + 1);
    }

    // The visible tree: a collapsed node shows no children.
    let visible_children = |id: NodeId| -> &[NodeId] {
        if collapsed.contains(&id) {
            &[]
        } else {
            children.get(&id).map_or(&[][..], Vec::as_slice)
        }
    };
    let order = preorder(&roots, visible_children);

    // Depth: one more than the parent, which comes earlier in pre-order.
    let mut depth: HashMap<NodeId, usize> = HashMap::with_capacity(order.len());
    for &id in &order {
        let d = parent_of.get(&id).map_or(0, |p| depth[p] + 1);
        depth.insert(id, d);
    }

    // Rows: visible leaves take consecutive rows in order; a parent is centered on its
    // first and last child. Children follow their parent in pre-order, so walking the order
    // backwards finishes every child before its parent.
    let mut row: HashMap<NodeId, f32> = HashMap::with_capacity(order.len());
    let mut next_row = 0.0_f32;
    for &id in &order {
        if visible_children(id).is_empty() {
            row.insert(id, next_row);
            next_row += 1.0;
        }
    }
    for &id in order.iter().rev() {
        if let (Some(&first), Some(&last)) =
            (visible_children(id).first(), visible_children(id).last())
        {
            row.insert(id, (row[&first] + row[&last]) / 2.0);
        }
    }

    let placed: Vec<Placed> = order
        .iter()
        .map(|&id| Placed {
            id,
            parent: parent_of.get(&id).copied(),
            depth: depth[&id],
            row: row[&id],
            has_children: children.contains_key(&id),
            hidden: if collapsed.contains(&id) {
                subtree[&id] - 1
            } else {
                0
            },
        })
        .collect();
    let index = placed.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
    TreeLayout {
        max_depth: placed.iter().map(|p| p.depth).max().unwrap_or(0),
        rows: next_row,
        placed,
        index,
    }
}

/// Depth-first pre-order of the forest below `roots`, visiting children in order.
fn preorder<'a>(roots: &[NodeId], children: impl Fn(NodeId) -> &'a [NodeId]) -> Vec<NodeId> {
    let mut order = Vec::new();
    let mut stack: Vec<NodeId> = roots.iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        order.push(id);
        stack.extend(children(id).iter().rev());
    }
    order
}

/// The ids on the path from the root to `current`, inclusive.
pub fn path_to(nodes: &[Node], current: Option<NodeId>) -> HashSet<NodeId> {
    let parent: HashMap<NodeId, Option<NodeId>> =
        nodes.iter().map(|n| (n.id, n.parent_id)).collect();
    let mut path = HashSet::new();
    let mut at = current;
    while let Some(id) = at {
        if !path.insert(id) {
            break;
        }
        at = parent.get(&id).copied().flatten();
    }
    path
}

#[cfg(test)]
mod tests {
    use masquerole_core::db::{NodeKind, SessionId};

    use super::*;

    fn node(id: i64, parent: Option<i64>) -> Node {
        Node {
            id: NodeId(id),
            session_id: SessionId(1),
            parent_id: parent.map(NodeId),
            kind: NodeKind::User,
            speaker: None,
            content: format!("node {id}"),
            label: None,
            bookmark: None,
            active_lore: vec![],
            created_at: 0,
        }
    }

    /// The README's example:
    /// ```text
    /// 1 ─ 2 ─ 3 ─ 5 ─ 8
    ///      │    └ 6 ─ 9
    ///      └ 4 ─ 7
    /// ```
    fn readme_tree() -> Vec<Node> {
        [
            (1, None),
            (2, Some(1)),
            (3, Some(2)),
            (4, Some(2)),
            (5, Some(3)),
            (6, Some(3)),
            (7, Some(4)),
            (8, Some(5)),
            (9, Some(6)),
        ]
        .into_iter()
        .map(|(id, parent)| node(id, parent))
        .collect()
    }

    fn at(layout: &TreeLayout, id: i64) -> (usize, f32) {
        let p = layout
            .get(NodeId(id))
            .unwrap_or_else(|| panic!("#{id} is not placed"));
        (p.depth, p.row)
    }

    #[test]
    fn a_straight_story_is_one_row() {
        let nodes: Vec<Node> = (1..=4).map(|i| node(i, (i > 1).then_some(i - 1))).collect();
        let l = layout(&nodes, &HashSet::new());
        assert_eq!(l.rows, 1.0);
        assert_eq!(l.max_depth, 3);
        for (i, p) in l.placed.iter().enumerate() {
            assert_eq!((p.depth, p.row), (i, 0.0));
        }
    }

    #[test]
    fn leaves_get_their_own_rows_and_parents_sit_between_their_children() {
        let l = layout(&readme_tree(), &HashSet::new());
        // Three storylines end at 8, 9 and 7, in that order.
        assert_eq!(at(&l, 8), (4, 0.0));
        assert_eq!(at(&l, 9), (4, 1.0));
        assert_eq!(at(&l, 7), (3, 2.0));
        assert_eq!(l.rows, 3.0);
        // A parent is centered on its first and last child.
        assert_eq!(at(&l, 5), (3, 0.0));
        assert_eq!(at(&l, 6), (3, 1.0));
        assert_eq!(at(&l, 3), (2, 0.5));
        assert_eq!(at(&l, 4), (2, 2.0));
        assert_eq!(at(&l, 2), (1, 1.25));
        assert_eq!(at(&l, 1), (0, 1.25));
    }

    #[test]
    fn parents_come_before_their_children() {
        let l = layout(&readme_tree(), &HashSet::new());
        for (i, p) in l.placed.iter().enumerate() {
            if let Some(parent) = p.parent {
                assert!(
                    l.placed[..i].iter().any(|q| q.id == parent),
                    "#{} before its parent",
                    p.id.0
                );
            }
        }
    }

    #[test]
    fn no_two_nodes_share_a_cell() {
        let l = layout(&readme_tree(), &HashSet::new());
        let mut cells: Vec<(usize, i64)> = l
            .placed
            .iter()
            .map(|p| (p.depth, (p.row * 100.0) as i64))
            .collect();
        cells.sort_unstable();
        cells.dedup();
        assert_eq!(cells.len(), l.placed.len());
    }

    #[test]
    fn collapsing_hides_descendants_and_counts_them() {
        let collapsed = HashSet::from([NodeId(3)]);
        let l = layout(&readme_tree(), &collapsed);
        assert!(l.get(NodeId(5)).is_none() && l.get(NodeId(9)).is_none());
        let three = l.get(NodeId(3)).unwrap();
        assert_eq!(
            (three.hidden, three.has_children),
            (4, true),
            "5, 6, 8 and 9 are hidden"
        );
        // The collapsed node behaves like a leaf; the rows close up.
        assert_eq!(at(&l, 3), (2, 0.0));
        assert_eq!(at(&l, 7), (3, 1.0));
        assert_eq!(l.rows, 2.0);
        assert_eq!(l.get(NodeId(4)).unwrap().hidden, 0);
    }

    #[test]
    fn collapsing_the_root_leaves_one_node() {
        let l = layout(&readme_tree(), &HashSet::from([NodeId(1)]));
        assert_eq!(l.placed.len(), 1);
        assert_eq!(l.placed[0].hidden, 8);
    }

    #[test]
    fn collapsing_a_leaf_or_an_unknown_node_changes_nothing() {
        let plain = layout(&readme_tree(), &HashSet::new());
        let odd = layout(&readme_tree(), &HashSet::from([NodeId(8), NodeId(999)]));
        assert_eq!(plain.placed.len(), odd.placed.len());
        assert_eq!(odd.get(NodeId(8)).unwrap().hidden, 0);
    }

    #[test]
    fn several_roots_are_stacked_and_orphans_count_as_roots() {
        let nodes = vec![
            node(1, None),
            node(2, Some(1)),
            node(3, None),
            node(4, Some(99)),
        ];
        let l = layout(&nodes, &HashSet::new());
        assert_eq!(at(&l, 1), (0, 0.0));
        assert_eq!(at(&l, 2), (1, 0.0));
        assert_eq!(at(&l, 3), (0, 1.0));
        assert_eq!(
            at(&l, 4),
            (0, 2.0),
            "its parent is missing, so it is a root"
        );
        assert_eq!(l.get(NodeId(4)).unwrap().parent, None);
    }

    #[test]
    fn an_empty_story_has_an_empty_layout() {
        let l = layout(&[], &HashSet::new());
        assert!(l.placed.is_empty());
        assert_eq!((l.rows, l.max_depth), (0.0, 0));
    }

    #[test]
    fn very_deep_stories_do_not_overflow_the_stack() {
        let nodes: Vec<Node> = (1..=50_000)
            .map(|i| node(i, (i > 1).then_some(i - 1)))
            .collect();
        let l = layout(&nodes, &HashSet::new());
        assert_eq!(l.placed.len(), 50_000);
        assert_eq!(l.max_depth, 49_999);
        let collapsed = layout(&nodes, &HashSet::from([NodeId(10)]));
        assert_eq!(collapsed.get(NodeId(10)).unwrap().hidden, 49_990);
    }

    #[test]
    fn the_current_path_runs_from_the_root_to_the_current_node() {
        let path = path_to(&readme_tree(), Some(NodeId(9)));
        let mut ids: Vec<i64> = path.iter().map(|id| id.0).collect();
        ids.sort_unstable();
        assert_eq!(ids, [1, 2, 3, 6, 9]);
        assert!(path_to(&readme_tree(), None).is_empty());
        assert_eq!(path_to(&readme_tree(), Some(NodeId(1))).len(), 1);
    }
}
