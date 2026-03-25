use ghost_interceptor::LayoutTree;

use crate::{AnnotatedTree, element_label, is_structural_only};

/// Maximum characters of text content to emit per node.
const MAX_TEXT_LEN: usize = 200;

/// Serialize a layout tree to strict, minimal JSON for LLM consumption.
///
/// Design principles:
/// - **Flat array** of nodes (not nested) — minimal nesting saves tokens.
/// - **Short keys** — single‐letter where unambiguous (`t` tag, `g` ghost-id,
///   `x`/`y`/`w`/`h` geometry, `c` children, `tx` text).
/// - **Omit defaults** — keys only appear when the value is present/non-empty.
/// - **Elide pure wrappers** — structural DIVs with no text/id/role are
///   omitted; their children are promoted to the parent.
///
/// Output shape:
/// ```json
/// {"url":"…","title":"…","nodes":[
///   {"t":"H1","x":0,"y":0,"w":800,"h":40,"tx":"Hello"},
///   {"t":"A#nav","g":0,"x":0,"y":50,"w":100,"h":20,"tx":"Home","href":"#"},
///   …
/// ]}
/// ```
pub fn to_json(tree: &LayoutTree) -> String {
    let annotated = AnnotatedTree::from_tree(tree);
    let mut out = String::with_capacity(tree.nodes.len() * 80);

    out.push('{');

    if let Some(url) = &tree.url {
        out.push_str("\"url\":");
        write_json_string(&mut out, url);
        out.push(',');
    }
    if let Some(title) = &tree.title {
        out.push_str("\"title\":");
        write_json_string(&mut out, title);
        out.push(',');
    }

    out.push_str("\"nodes\":[");

    let mut first = true;
    if let Some(root_idx) = tree.root_index() {
        emit_node_json(tree, &annotated, root_idx, &mut out, &mut first);
    }

    out.push_str("]}");
    out
}

fn emit_node_json(
    tree: &LayoutTree,
    annotated: &AnnotatedTree,
    idx: usize,
    out: &mut String,
    first: &mut bool,
) {
    let node = &tree.nodes[idx];

    // Elide pure structural wrappers — promote children directly.
    if is_structural_only(node) && annotated.ghost_id(idx).is_none() {
        for &child_idx in &node.children {
            emit_node_json(tree, annotated, child_idx, out, first);
        }
        return;
    }

    if !*first {
        out.push(',');
    }
    *first = false;

    out.push('{');
    let label = element_label(node);
    out.push_str("\"t\":");
    write_json_string(out, &label);

    if let Some(gid) = annotated.ghost_id(idx) {
        out.push_str(",\"g\":");
        out.push_str(&gid.to_string());
    }

    // Geometry.
    out.push_str(",\"x\":");
    out.push_str(&node.rect.x.to_string());
    out.push_str(",\"y\":");
    out.push_str(&node.rect.y.to_string());
    out.push_str(",\"w\":");
    out.push_str(&node.rect.w.to_string());
    out.push_str(",\"h\":");
    out.push_str(&node.rect.h.to_string());

    if let Some(text) = &node.text {
        out.push_str(",\"tx\":");
        if text.chars().count() > MAX_TEXT_LEN {
            let truncated: String = text.chars().take(MAX_TEXT_LEN).collect();
            write_json_string(out, &format!("{truncated}…"));
        } else {
            write_json_string(out, text);
        }
    }
    if let Some(href) = &node.href {
        out.push_str(",\"href\":");
        write_json_string(out, href);
    }
    if let Some(role) = &node.role {
        out.push_str(",\"role\":");
        write_json_string(out, role);
    }
    if let Some(aria) = &node.aria_label {
        out.push_str(",\"aria\":");
        write_json_string(out, aria);
    }
    if let Some(itype) = &node.input_type {
        out.push_str(",\"type\":");
        write_json_string(out, itype);
    }
    if let Some(name) = &node.name {
        out.push_str(",\"name\":");
        write_json_string(out, name);
    }
    if let Some(val) = &node.value {
        out.push_str(",\"val\":");
        write_json_string(out, val);
    }
    if let Some(ph) = &node.placeholder {
        out.push_str(",\"ph\":");
        write_json_string(out, ph);
    }
    if let Some(isrc) = &node.iframe_src {
        out.push_str(",\"iframeSrc\":");
        write_json_string(out, isrc);
    }
    if node.shadow_host {
        out.push_str(",\"shadow\":true");
    }

    out.push('}');

    // Emit children (non-structural ones will appear as siblings in the flat array).
    for &child_idx in &node.children {
        emit_node_json(tree, annotated, child_idx, out, first);
    }
}

/// Write a JSON-escaped string (with quotes) into the output buffer.
fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\x20' => {
                let _ = std::fmt::Write::write_fmt(
                    out,
                    format_args!("\\u{:04x}", c as u32),
                );
            },
            c => out.push(c),
        }
    }
    out.push('"');
}
