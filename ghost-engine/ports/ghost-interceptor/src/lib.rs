mod extract_js;

use std::collections::HashMap;

use ghost_core::{GhostEngine, GhostError, GhostWebView, JSValue};

pub use extract_js::EXTRACT_LAYOUT_JS;

// ── Layout tree types ───────────────────────────────────────────────────────

/// Bounding rectangle in viewport coordinates (CSS pixels, rounded to i32).
#[derive(Debug, Clone, Default)]
pub struct LayoutRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// A single visible element extracted from the live DOM.
#[derive(Debug, Clone)]
pub struct LayoutNode {
    /// Tag name (e.g. `"DIV"`, `"A"`, `"INPUT"`).
    pub tag: String,
    /// Bounding rectangle in viewport coords.
    pub rect: LayoutRect,
    /// Element `id` attribute, if any.
    pub id: Option<String>,
    /// Element `className`, if non-empty.
    pub class: Option<String>,
    /// Direct text content (not recursive), if non-empty.
    pub text: Option<String>,
    /// Whether this element is interactive (clickable/focusable).
    pub interactive: bool,
    /// ARIA `role`, if set.
    pub role: Option<String>,
    /// ARIA `aria-label`, if set.
    pub aria_label: Option<String>,
    /// `href` attribute (for links).
    pub href: Option<String>,
    /// `type` attribute (for inputs).
    pub input_type: Option<String>,
    /// `name` attribute (for form fields).
    pub name: Option<String>,
    /// Current input value, if applicable.
    pub value: Option<String>,
    /// `placeholder` text, if set.
    pub placeholder: Option<String>,
    /// For `<iframe>` elements: the `src` URL of the iframe.
    /// Present even when the iframe is cross-origin (content unreachable).
    pub iframe_src: Option<String>,
    /// `true` if this element hosts an open shadow root whose children
    /// have been traversed and included in [`Self::children`].
    pub shadow_host: bool,
    /// Indices into [`LayoutTree::nodes`] for visible children.
    pub children: Vec<usize>,
}

/// The full extracted layout snapshot of a page.
#[derive(Debug, Clone)]
pub struct LayoutTree {
    /// Document URL at extraction time.
    pub url: Option<String>,
    /// Document title at extraction time.
    pub title: Option<String>,
    /// Flat array of nodes — the last element is the root (body).
    pub nodes: Vec<LayoutNode>,
}

impl LayoutTree {
    /// Index of the root node (body), if any nodes were extracted.
    pub fn root_index(&self) -> Option<usize> {
        if self.nodes.is_empty() {
            None
        } else {
            Some(self.nodes.len() - 1)
        }
    }

    /// Total number of visible nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether no visible nodes were extracted.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

// ── Extraction ──────────────────────────────────────────────────────────────

/// Inject the extraction script into the webview and return a structured
/// [`LayoutTree`]. The page should already be loaded before calling this.
pub fn extract_layout(
    engine: &GhostEngine,
    webview: &GhostWebView,
) -> Result<LayoutTree, GhostError> {
    let js_result = engine.evaluate_js(webview, EXTRACT_LAYOUT_JS)?;
    parse_layout_tree(js_result)
}

// ── JSValue → LayoutTree parsing ────────────────────────────────────────────

fn parse_layout_tree(value: JSValue) -> Result<LayoutTree, GhostError> {
    let map = match value {
        JSValue::Object(m) => m,
        other => {
            return Err(GhostError::JavaScript(format!(
                "expected Object from extraction script, got {other:?}"
            )));
        },
    };

    let url = get_opt_string(&map, "url");
    let title = get_opt_string(&map, "title");

    let nodes_val = map
        .get("nodes")
        .ok_or_else(|| GhostError::JavaScript("missing 'nodes' in extraction result".into()))?;

    let nodes_arr = match nodes_val {
        JSValue::Array(arr) => arr,
        other => {
            return Err(GhostError::JavaScript(format!(
                "expected Array for 'nodes', got {other:?}"
            )));
        },
    };

    let mut nodes = Vec::with_capacity(nodes_arr.len());
    for (i, node_val) in nodes_arr.iter().enumerate() {
        nodes.push(parse_layout_node(node_val, i)?);
    }

    // Validate child indices are within bounds to prevent panics downstream.
    let len = nodes.len();
    for node in &mut nodes {
        node.children.retain(|&idx| idx < len);
    }

    Ok(LayoutTree { url, title, nodes })
}

fn parse_layout_node(value: &JSValue, index: usize) -> Result<LayoutNode, GhostError> {
    let map = match value {
        JSValue::Object(m) => m,
        other => {
            return Err(GhostError::JavaScript(format!(
                "node[{index}]: expected Object, got {other:?}"
            )));
        },
    };

    let tag = get_string(map, "tag")
        .ok_or_else(|| GhostError::JavaScript(format!("node[{index}]: missing 'tag'")))?;

    let rect = LayoutRect {
        x: get_i32(map, "x"),
        y: get_i32(map, "y"),
        w: get_i32(map, "w"),
        h: get_i32(map, "h"),
    };

    let children = match map.get("children") {
        Some(JSValue::Array(arr)) => arr
            .iter()
            .filter_map(|v| match v {
                JSValue::Number(n) => Some(*n as usize),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };

    let interactive = match map.get("interactive") {
        Some(JSValue::Boolean(b)) => *b,
        _ => false,
    };

    let shadow_host = match map.get("shadowHost") {
        Some(JSValue::Boolean(b)) => *b,
        _ => false,
    };

    Ok(LayoutNode {
        tag,
        rect,
        id: get_opt_string(map, "id"),
        class: get_opt_string(map, "cls"),
        text: get_opt_string(map, "text"),
        interactive,
        role: get_opt_string(map, "role"),
        aria_label: get_opt_string(map, "ariaLabel"),
        href: get_opt_string(map, "href"),
        input_type: get_opt_string(map, "type"),
        name: get_opt_string(map, "name"),
        value: get_opt_string(map, "value"),
        placeholder: get_opt_string(map, "placeholder"),
        iframe_src: get_opt_string(map, "iframeSrc"),
        shadow_host,
        children,
    })
}

// ── JSValue helpers ─────────────────────────────────────────────────────────

fn get_string(map: &HashMap<String, JSValue>, key: &str) -> Option<String> {
    match map.get(key) {
        Some(JSValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn get_opt_string(map: &HashMap<String, JSValue>, key: &str) -> Option<String> {
    get_string(map, key)
}

fn get_i32(map: &HashMap<String, JSValue>, key: &str) -> i32 {
    match map.get(key) {
        Some(JSValue::Number(n)) => *n as i32,
        _ => 0,
    }
}
