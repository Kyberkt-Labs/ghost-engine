mod json;
mod markdown;

use ghost_interceptor::{LayoutNode, LayoutTree};

pub use json::to_json;
pub use markdown::to_markdown;

// ── Ghost-ID injection (TSK-3.4) ───────────────────────────────────────────

/// A layout tree annotated with sequential `ghost-id`s on interactive elements.
///
/// Ghost-IDs are stable identifiers that agents use to reference elements
/// in action commands (`click(3)`, `type(7, "hello")`). Only interactive
/// elements receive an ID to keep the token budget small.
pub struct AnnotatedTree<'a> {
    pub tree: &'a LayoutTree,
    /// Maps flat node index → ghost-id. Only present for interactive nodes.
    pub ghost_ids: Vec<Option<u32>>,
    /// Total number of ghost-ids assigned.
    pub interactive_count: u32,
}

impl<'a> AnnotatedTree<'a> {
    /// Walk the tree and assign sequential ghost-ids to interactive elements.
    ///
    /// IDs are assigned in depth-first order starting from the root, which
    /// gives agents a top-to-bottom, left-to-right numbering that matches
    /// visual reading order.
    pub fn from_tree(tree: &'a LayoutTree) -> Self {
        let mut ghost_ids = vec![None; tree.nodes.len()];
        let mut next_id: u32 = 0;

        if let Some(root_idx) = tree.root_index() {
            assign_ids_dfs(tree, root_idx, &mut ghost_ids, &mut next_id);
        }

        Self {
            tree,
            ghost_ids,
            interactive_count: next_id,
        }
    }

    /// Look up the ghost-id for a node index, if it has one.
    pub fn ghost_id(&self, node_index: usize) -> Option<u32> {
        self.ghost_ids.get(node_index).copied().flatten()
    }

    /// Find the node index for a given ghost-id.
    pub fn node_index_for_id(&self, id: u32) -> Option<usize> {
        self.ghost_ids
            .iter()
            .position(|gid| *gid == Some(id))
    }
}

fn assign_ids_dfs(
    tree: &LayoutTree,
    idx: usize,
    ghost_ids: &mut [Option<u32>],
    next_id: &mut u32,
) {
    let Some(node) = tree.nodes.get(idx) else {
        return;
    };

    if node.interactive {
        ghost_ids[idx] = Some(*next_id);
        *next_id += 1;
    }

    for &child_idx in &node.children {
        if child_idx < tree.nodes.len() {
            assign_ids_dfs(tree, child_idx, ghost_ids, next_id);
        }
    }
}

// ── Shared formatting helpers ───────────────────────────────────────────────

/// Build a compact element label: `TAG#id.class1.class2`
fn element_label(node: &LayoutNode) -> String {
    let mut label = node.tag.clone();
    if let Some(id) = &node.id {
        label.push('#');
        label.push_str(id);
    }
    if let Some(cls) = &node.class {
        for c in cls.split_whitespace() {
            label.push('.');
            label.push_str(c);
        }
    }
    label
}

/// True if a node is purely structural (no text, no interactivity, no
/// semantic attributes) — candidates for elision when compressing.
fn is_structural_only(node: &LayoutNode) -> bool {
    !node.interactive
        && node.text.is_none()
        && node.id.is_none()
        && node.role.is_none()
        && node.aria_label.is_none()
        && node.href.is_none()
        && matches!(
            node.tag.as_str(),
            "DIV" | "SPAN" | "SECTION" | "ARTICLE" | "MAIN" | "HEADER"
                | "FOOTER" | "NAV" | "UL" | "OL" | "DL" | "TBODY" | "THEAD"
                | "TFOOT" | "TR" | "COLGROUP" | "FIELDSET" | "FIGURE"
                | "FIGCAPTION" | "DETAILS" | "SUMMARY" | "ASIDE" | "HGROUP"
        )
}
