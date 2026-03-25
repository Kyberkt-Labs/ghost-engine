use ghost_interceptor::LayoutTree;

use crate::{AnnotatedTree, element_label, is_structural_only};

/// Maximum characters of text content to emit per node.
const MAX_TEXT_LEN: usize = 200;

/// Truncate text to at most `MAX_TEXT_LEN` characters, appending "…" if cut.
fn truncate_text(s: &str) -> String {
    if s.chars().count() <= MAX_TEXT_LEN {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(MAX_TEXT_LEN).collect();
        t.push('…');
        t
    }
}

/// Serialize a layout tree to semantic Markdown for LLM consumption.
///
/// Design principles:
/// - **Indented tree** — depth conveys nesting, no closing tags needed.
/// - **Ghost-IDs in brackets** — `[3]` before interactive element labels.
/// - **Structural elision** — pure wrapper DIVs are skipped; children promoted.
/// - **Semantic text** — headings use `#`, links show `[text](href)`, lists
///   show `- item`, table cells separated by `|`.
///
/// Example output:
/// ```text
/// url: https://example.com
/// title: Example Page
/// ---
/// # Welcome
/// [0] [Home](#home) 
/// [1] [About](#about)
/// Hello, world!
/// [2] INPUT text name="email" ph="you@example.com"
/// [3] BUTTON "Submit"
/// ```
pub fn to_markdown(tree: &LayoutTree) -> String {
    let annotated = AnnotatedTree::from_tree(tree);
    let mut out = String::with_capacity(tree.nodes.len() * 40);

    if let Some(url) = &tree.url {
        out.push_str("url: ");
        out.push_str(url);
        out.push('\n');
    }
    if let Some(title) = &tree.title {
        out.push_str("title: ");
        out.push_str(title);
        out.push('\n');
    }
    out.push_str("---\n");

    if let Some(root_idx) = tree.root_index() {
        emit_node_md(tree, &annotated, root_idx, &mut out, 0);
    }

    out
}

fn emit_node_md(
    tree: &LayoutTree,
    annotated: &AnnotatedTree,
    idx: usize,
    out: &mut String,
    depth: usize,
) {
    let node = &tree.nodes[idx];

    // Elide pure structural wrappers.
    if is_structural_only(node) && annotated.ghost_id(idx).is_none() {
        for &child_idx in &node.children {
            emit_node_md(tree, annotated, child_idx, out, depth);
        }
        return;
    }

    let indent = "  ".repeat(depth);
    let gid_prefix = match annotated.ghost_id(idx) {
        Some(id) => format!("[{id}] "),
        None => String::new(),
    };

    // Semantic formatting by tag type.
    match node.tag.as_str() {
        // ── Headings ────────────────────────────────────
        "H1" | "H2" | "H3" | "H4" | "H5" | "H6" => {
            let level = node.tag.as_bytes()[1] - b'0';
            let hashes = "#".repeat(level as usize);
            let text = truncate_text(node.text.as_deref().unwrap_or(""));
            out.push_str(&indent);
            out.push_str(&gid_prefix);
            out.push_str(&hashes);
            out.push(' ');
            out.push_str(&text);
            out.push('\n');
        },

        // ── Links ───────────────────────────────────────
        "A" => {
            let text = truncate_text(node.text.as_deref().unwrap_or("link"));
            let href = node.href.as_deref().unwrap_or("#");
            out.push_str(&indent);
            out.push_str(&gid_prefix);
            out.push('[');
            out.push_str(&text);
            out.push_str("](");
            out.push_str(href);
            out.push_str(")\n");
        },

        // ── List items ──────────────────────────────────
        "LI" => {
            let text = truncate_text(node.text.as_deref().unwrap_or(""));
            out.push_str(&indent);
            out.push_str(&gid_prefix);
            out.push_str("- ");
            out.push_str(&text);
            out.push('\n');
            // Recurse for nested lists or complex content inside LI.
            for &child_idx in &node.children {
                emit_node_md(tree, annotated, child_idx, out, depth + 1);
            }
            return; // children already handled
        },

        // ── Table cells ─────────────────────────────────
        "TH" | "TD" => {
            // Handled inline by the TR parent.
            return;
        },

        "TR" => {
            out.push_str(&indent);
            out.push_str("| ");
            for &child_idx in &node.children {
                let child = &tree.nodes[child_idx];
                // Emit ghost-ID for interactive cells (links/buttons inside TD/TH).
                if let Some(gid) = annotated.ghost_id(child_idx) {
                    out.push('[');
                    out.push_str(&gid.to_string());
                    out.push_str("] ");
                }
                let cell_text = truncate_text(child.text.as_deref().unwrap_or(""));
                out.push_str(&cell_text);
                out.push_str(" | ");
            }
            out.push('\n');
            // Emit children of cells that themselves contain interactive elements.
            for &child_idx in &node.children {
                let child = &tree.nodes[child_idx];
                for &grandchild_idx in &child.children {
                    emit_node_md(tree, annotated, grandchild_idx, out, depth + 1);
                }
            }
            return; // cells handled inline
        },

        // ── Form inputs ─────────────────────────────────
        "INPUT" => {
            out.push_str(&indent);
            out.push_str(&gid_prefix);
            out.push_str("INPUT");
            if let Some(itype) = &node.input_type {
                out.push(' ');
                out.push_str(itype);
            }
            if let Some(name) = &node.name {
                out.push_str(" name=\"");
                out.push_str(name);
                out.push('"');
            }
            if let Some(ph) = &node.placeholder {
                out.push_str(" ph=\"");
                out.push_str(ph);
                out.push('"');
            }
            if let Some(val) = &node.value {
                out.push_str(" val=\"");
                out.push_str(val);
                out.push('"');
            }
            out.push('\n');
        },

        "TEXTAREA" => {
            out.push_str(&indent);
            out.push_str(&gid_prefix);
            out.push_str("TEXTAREA");
            if let Some(name) = &node.name {
                out.push_str(" name=\"");
                out.push_str(name);
                out.push('"');
            }
            if let Some(val) = &node.value {
                out.push_str(" val=\"");
                out.push_str(val);
                out.push('"');
            }
            out.push('\n');
        },

        "SELECT" => {
            out.push_str(&indent);
            out.push_str(&gid_prefix);
            out.push_str("SELECT");
            if let Some(name) = &node.name {
                out.push_str(" name=\"");
                out.push_str(name);
                out.push('"');
            }
            out.push('\n');
            for &child_idx in &node.children {
                emit_node_md(tree, annotated, child_idx, out, depth + 1);
            }
            return;
        },

        // ── Buttons ─────────────────────────────────────
        "BUTTON" => {
            let text = truncate_text(node.text.as_deref().unwrap_or("button"));
            out.push_str(&indent);
            out.push_str(&gid_prefix);
            out.push_str("BUTTON \"");
            out.push_str(&text);
            out.push_str("\"\n");
        },

        // ── Images ──────────────────────────────────────
        "IMG" => {
            let alt = node.aria_label.as_deref().unwrap_or("image");
            out.push_str(&indent);
            out.push_str(&gid_prefix);
            out.push_str("IMG \"");
            out.push_str(alt);
            out.push_str("\"\n");
        },

        // ── Labels ──────────────────────────────────────
        "LABEL" => {
            let text = node.text.as_deref().unwrap_or("");
            if !text.is_empty() {
                out.push_str(&indent);
                out.push_str(&gid_prefix);
                out.push_str("LABEL \"");
                out.push_str(&truncate_text(text));
                out.push_str("\"\n");
            } else {
                // LABEL with no direct text — still emit the tag so children
                // (e.g. an INPUT nested inside) have context.
                out.push_str(&indent);
                out.push_str(&gid_prefix);
                out.push_str("LABEL\n");
            }
        },

        // ── Iframes ──────────────────────────────────
        "IFRAME" => {
            out.push_str(&indent);
            out.push_str(&gid_prefix);
            out.push_str("IFRAME");
            if let Some(src) = &node.iframe_src {
                out.push_str(" src=\"");
                out.push_str(src);
                out.push('"');
            }
            out.push('\n');
        },

        // ── Generic: text nodes, paragraphs, etc. ───────
        _ => {
            // If the node has text, emit it.
            if let Some(text) = &node.text {
                out.push_str(&indent);
                out.push_str(&gid_prefix);
                let label = element_label(node);
                out.push_str(&label);
                if node.shadow_host {
                    out.push_str(" [shadow]");
                }
                out.push_str(" \"");
                out.push_str(&truncate_text(text));
                out.push_str("\"\n");
            } else if annotated.ghost_id(idx).is_some() || node.role.is_some()
                || node.shadow_host
            {
                // Interactive, semantic, or shadow host — still worth showing.
                out.push_str(&indent);
                out.push_str(&gid_prefix);
                out.push_str(&element_label(node));
                if let Some(role) = &node.role {
                    out.push_str(" role=");
                    out.push_str(role);
                }
                if node.shadow_host {
                    out.push_str(" [shadow]");
                }
                out.push('\n');
            }
            // If node has neither text nor special attributes, just recurse.
        },
    }

    // Recurse into children (unless already handled above).
    for &child_idx in &node.children {
        emit_node_md(tree, annotated, child_idx, out, depth + 1);
    }
}
