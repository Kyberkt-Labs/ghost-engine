//! Layout-extraction snapshot tests for Ghost Engine test fixtures.
//!
//! These load each fixture through the full engine pipeline and validate
//! that `extract_layout` produces the expected structural output.
//!
//! Run with:
//!
//! ```sh
//! cargo test -p ghost-interceptor --test extraction_snapshots -- --nocapture
//! ```

use std::path::PathBuf;
use std::time::Duration;

use ghost_core::{GhostEngine, GhostEngineConfig};
use ghost_interceptor::{extract_layout, LayoutNode, LayoutTree};

// ── Fixture helpers ─────────────────────────────────────────────────────────

fn fixtures_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")); // ports/ghost-interceptor/
    let workspace_root = manifest
        .parent() // ports/
        .and_then(|p| p.parent()) // ghost-engine/
        .expect("could not find workspace root");
    let dir = workspace_root.join("tests").join("ghost");
    assert!(dir.is_dir(), "fixtures dir missing: {}", dir.display());
    dir
}

fn fixture_url(relative: &str) -> String {
    let path = fixtures_dir().join(relative);
    assert!(path.is_file(), "fixture not found: {}", path.display());
    format!("file://{}", path.display())
}

fn load_and_extract(fixture: &str) -> LayoutTree {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine.new_webview(&fixture_url(fixture)).unwrap();
    engine.load_and_wait(&wv).unwrap();
    extract_layout(&engine, &wv).unwrap()
}

fn load_and_extract_with_settle(fixture: &str, settle_secs: u64) -> LayoutTree {
    let mut config = GhostEngineConfig::default();
    config.settle_timeout = Duration::from_secs(settle_secs);
    config.quiet_period = Duration::from_millis(500);
    let engine = GhostEngine::new(config).unwrap();
    let wv = engine.new_webview(&fixture_url(fixture)).unwrap();
    engine.load_and_wait(&wv).unwrap();
    extract_layout(&engine, &wv).unwrap()
}

// ── Tree query helpers ──────────────────────────────────────────────────────

fn find_by_tag<'a>(tree: &'a LayoutTree, tag: &str) -> Vec<&'a LayoutNode> {
    tree.nodes.iter().filter(|n| n.tag == tag).collect()
}

fn find_by_id<'a>(tree: &'a LayoutTree, id: &str) -> Option<&'a LayoutNode> {
    tree.nodes.iter().find(|n| n.id.as_deref() == Some(id))
}

fn count_interactive(tree: &LayoutTree) -> usize {
    tree.nodes.iter().filter(|n| n.interactive).count()
}

// ── Tier 1: Static HTML ─────────────────────────────────────────────────────

#[test]
fn t1_hello_extraction() {
    let tree = load_and_extract("tier1_static/hello.html");

    assert!(!tree.is_empty(), "tree should have visible nodes");
    assert_eq!(tree.title.as_deref(), Some("Ghost T1: Hello World"));

    // Should contain BODY, H1, P at minimum.
    let h1s = find_by_tag(&tree, "H1");
    assert_eq!(h1s.len(), 1, "expected exactly 1 H1");
    assert_eq!(h1s[0].text.as_deref(), Some("Hello, Ghost Engine!"));

    let ps = find_by_tag(&tree, "P");
    assert!(!ps.is_empty(), "expected at least 1 P");
    let simple_p = ps
        .iter()
        .find(|n| {
            n.text
                .as_ref()
                .is_some_and(|t| t.contains("simplest possible"))
        });
    assert!(simple_p.is_some(), "expected P with 'simplest possible' text");

    // Root node should be BODY.
    let root = &tree.nodes[tree.root_index().unwrap()];
    assert_eq!(root.tag, "BODY");
    assert!(!root.children.is_empty());
}

#[test]
fn t1_semantic_structure_extraction() {
    let tree = load_and_extract("tier1_static/semantic_structure.html");

    assert_eq!(
        tree.title.as_deref(),
        Some("Ghost T1: Semantic Structure")
    );

    // ── Visibility filtering: hidden elements must be excluded ──
    assert!(
        find_by_id(&tree, "display-none").is_none(),
        "display:none element should be filtered"
    );
    assert!(
        find_by_id(&tree, "vis-hidden").is_none(),
        "visibility:hidden element should be filtered"
    );
    assert!(
        find_by_id(&tree, "zero-size").is_none(),
        "zero-size element should be filtered"
    );

    // ── Structural checks ──
    // Navigation links.
    let links = find_by_tag(&tree, "A");
    assert!(links.len() >= 3, "expected >= 3 nav links, got {}", links.len());
    let link_texts: Vec<_> = links.iter().filter_map(|n| n.text.as_deref()).collect();
    assert!(link_texts.contains(&"Home"), "nav should have Home link");
    assert!(link_texts.contains(&"About"), "nav should have About link");
    assert!(link_texts.contains(&"Contact"), "nav should have Contact link");

    // Links should be interactive.
    for link in &links {
        assert!(link.interactive, "A tags should be interactive");
        assert!(link.href.is_some(), "A tags should have href");
    }

    // Form elements in contact section.
    let name_input = find_by_id(&tree, "name");
    assert!(name_input.is_some(), "expected #name input");
    let name_input = name_input.unwrap();
    assert_eq!(name_input.tag, "INPUT");
    assert!(name_input.interactive, "input should be interactive");
    assert_eq!(name_input.placeholder.as_deref(), Some("Your name"));

    let email_input = find_by_id(&tree, "email");
    assert!(email_input.is_some(), "expected #email input");

    let buttons = find_by_tag(&tree, "BUTTON");
    assert!(!buttons.is_empty(), "expected a submit button");
    assert!(buttons[0].interactive, "button should be interactive");

    // Interactive element count: 3 nav links + 2 inputs + 1 button = 6
    let interactive = count_interactive(&tree);
    assert!(
        interactive >= 6,
        "expected >= 6 interactive elements, got {interactive}"
    );
}

#[test]
fn t1_table_and_list_extraction() {
    let tree = load_and_extract("tier1_static/table_and_list.html");

    assert_eq!(tree.title.as_deref(), Some("Ghost T1: Table & List"));

    // Table structure.
    let table = find_by_id(&tree, "data-table");
    assert!(table.is_some(), "expected #data-table");

    let ths = find_by_tag(&tree, "TH");
    assert_eq!(ths.len(), 3, "expected 3 table headers (ID, Name, Role)");
    let th_texts: Vec<_> = ths.iter().filter_map(|n| n.text.as_deref()).collect();
    assert!(th_texts.contains(&"ID"));
    assert!(th_texts.contains(&"Name"));
    assert!(th_texts.contains(&"Role"));

    let tds = find_by_tag(&tree, "TD");
    assert_eq!(tds.len(), 9, "expected 9 table cells (3 rows × 3 cols)");

    // Check specific cell values.
    let alice = tds.iter().find(|n| n.text.as_deref() == Some("Alice"));
    assert!(alice.is_some(), "expected cell with 'Alice'");

    // List items.
    let lis = find_by_tag(&tree, "LI");
    assert_eq!(lis.len(), 8, "expected 8 LI elements (4 + 4)");
}

// ── Tier 2: Vanilla JS (dynamic DOM) ────────────────────────────────────────

#[test]
fn t2_dom_manipulation_extraction() {
    let tree = load_and_extract("tier2_vanilla_js/dom_manipulation.html");

    assert_eq!(tree.title.as_deref(), Some("Ghost T2: DOM Ready"));

    // Heading was changed by script.
    let h1 = find_by_tag(&tree, "H1");
    assert_eq!(h1.len(), 1);
    assert_eq!(h1[0].text.as_deref(), Some("DOM Manipulation Test"));

    // Dynamically created list items.
    let lis = find_by_tag(&tree, "LI");
    assert_eq!(lis.len(), 4, "expected 4 dynamically created LI elements");
    let li_texts: Vec<_> = lis.iter().filter_map(|n| n.text.as_deref()).collect();
    assert!(li_texts.contains(&"Alpha"));
    assert!(li_texts.contains(&"Bravo"));
    assert!(li_texts.contains(&"Charlie"));
    assert!(li_texts.contains(&"Delta"));

    // Status paragraph.
    let status = find_by_id(&tree, "status");
    assert!(status.is_some());
    assert_eq!(status.unwrap().text.as_deref(), Some("ready"));
}

#[test]
fn t2_async_timers_extraction() {
    let tree = load_and_extract_with_settle("tier2_vanilla_js/async_timers.html", 3);

    assert_eq!(tree.title.as_deref(), Some("Ghost T2: Timers Done"));

    // After all timeouts, step should be 3.
    let step = find_by_id(&tree, "step");
    assert!(step.is_some());
    assert_eq!(step.unwrap().text.as_deref(), Some("3"));

    let status = find_by_id(&tree, "status");
    assert_eq!(status.unwrap().text.as_deref(), Some("ready"));
}

#[test]
fn t2_promises_extraction() {
    let tree = load_and_extract("tier2_vanilla_js/promises.html");

    assert_eq!(tree.title.as_deref(), Some("Ghost T2: Promises Done"));

    // Promise chain result: 21 * 2 = 42.
    let result = find_by_id(&tree, "result");
    assert!(result.is_some());
    assert_eq!(result.unwrap().text.as_deref(), Some("42"));

    // Dynamically appended element: final:84.
    let final_el = find_by_id(&tree, "final");
    assert!(final_el.is_some(), "expected dynamically created #final element");
    assert_eq!(final_el.unwrap().text.as_deref(), Some("final:84"));
}

#[test]
fn t2_event_listeners_extraction() {
    let tree = load_and_extract("tier2_vanilla_js/event_listeners.html");

    assert_eq!(tree.title.as_deref(), Some("Ghost T2: Events Ready"));

    // Button should be interactive.
    let btn = find_by_id(&tree, "btn-click");
    assert!(btn.is_some(), "expected #btn-click button");
    let btn = btn.unwrap();
    assert_eq!(btn.tag, "BUTTON");
    assert!(btn.interactive);
    assert_eq!(btn.text.as_deref(), Some("Click Me"));

    // Text input should be interactive.
    let input = find_by_id(&tree, "input-text");
    assert!(input.is_some(), "expected #input-text");
    let input = input.unwrap();
    assert_eq!(input.tag, "INPUT");
    assert!(input.interactive);
    assert_eq!(input.placeholder.as_deref(), Some("Type here"));

    // Status should be ready (set by script before any events fire).
    let status = find_by_id(&tree, "status");
    assert_eq!(status.unwrap().text.as_deref(), Some("ready"));
}

#[test]
fn t2_json_render_extraction() {
    let tree = load_and_extract("tier2_vanilla_js/json_render.html");

    assert_eq!(tree.title.as_deref(), Some("Ghost T2: JSON Rendered"));

    // Table should have dynamically rendered rows.
    let tds = find_by_tag(&tree, "TD");
    assert_eq!(tds.len(), 6, "expected 6 cells (3 rows × 2 cols)");

    // Check data values.
    let names: Vec<_> = tds.iter().filter_map(|n| n.text.as_deref()).collect();
    assert!(names.contains(&"Alice"), "expected Alice in table");
    assert!(names.contains(&"Bob"), "expected Bob in table");
    assert!(names.contains(&"Carol"), "expected Carol in table");
    assert!(names.contains(&"95"), "expected score 95 in table");
}

// ── Geometry sanity checks ──────────────────────────────────────────────────

#[test]
fn geometry_is_populated() {
    let tree = load_and_extract("tier1_static/hello.html");

    // Every visible node should have non-zero width (page is 1920px default).
    let body = &tree.nodes[tree.root_index().unwrap()];
    assert!(body.rect.w > 0, "BODY should have positive width");
    assert!(body.rect.h > 0, "BODY should have positive height");

    let h1 = find_by_tag(&tree, "H1").first().copied().unwrap();
    assert!(h1.rect.w > 0, "H1 should have positive width");
    assert!(h1.rect.h > 0, "H1 should have positive height");
}

// ── wait_for_selector ───────────────────────────────────────────────────────

#[test]
fn wait_for_selector_finds_existing_element() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier1_static/hello.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    // h1 exists immediately after load — should return instantly.
    engine
        .wait_for_selector(&wv, "h1", Duration::from_secs(2))
        .unwrap();
}

#[test]
fn wait_for_selector_times_out_for_missing_element() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier1_static/hello.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    let result = engine.wait_for_selector(&wv, "#nonexistent", Duration::from_millis(200));
    assert!(
        matches!(result, Err(ghost_core::GhostError::Timeout)),
        "should timeout for missing selector"
    );
}

#[test]
fn wait_for_selector_skips_hidden_elements() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier1_static/semantic_structure.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    // #display-none exists in DOM but is display:none — should timeout.
    let result = engine.wait_for_selector(&wv, "#display-none", Duration::from_millis(200));
    assert!(
        matches!(result, Err(ghost_core::GhostError::Timeout)),
        "should timeout for display:none element"
    );

    // #vis-hidden exists but is visibility:hidden — should timeout.
    let result = engine.wait_for_selector(&wv, "#vis-hidden", Duration::from_millis(200));
    assert!(
        matches!(result, Err(ghost_core::GhostError::Timeout)),
        "should timeout for visibility:hidden element"
    );

    // Visible element should succeed.
    engine
        .wait_for_selector(&wv, "#home", Duration::from_secs(2))
        .unwrap();
}
