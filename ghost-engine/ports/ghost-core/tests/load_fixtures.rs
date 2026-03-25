//! Integration tests that load Ghost Engine test fixtures through the
//! [`ghost_core`] API.
//!
//! These are **compile-checked** but require a full Servo runtime to
//! actually execute.  Run with:
//!
//! ```sh
//! cargo test -p ghost-core --test load_fixtures -- --nocapture
//! ```
//!
//! Test fixtures live in `tests/ghost/` at the workspace root.

use std::path::PathBuf;
use std::rc::Rc;

use ghost_core::{GhostEngine, GhostEngineConfig, LoadStatus};

/// Resolve the workspace-root `tests/ghost/` directory.
fn fixtures_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")); // ports/ghost-core/
    let workspace_root = manifest
        .parent() // ports/
        .and_then(|p| p.parent()) // ghost-engine/
        .expect("could not find workspace root");
    let dir = workspace_root.join("tests").join("ghost");
    assert!(dir.is_dir(), "fixtures dir missing: {}", dir.display());
    dir
}

/// Convert a fixture path to a `file://` URL.
fn fixture_url(relative: &str) -> String {
    let path = fixtures_dir().join(relative);
    assert!(path.is_file(), "fixture not found: {}", path.display());
    format!("file://{}", path.display())
}

// ── Tier 1: Static HTML ─────────────────────────────────────────────────────

#[test]
fn t1_hello_loads() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine.new_webview(&fixture_url("tier1_static/hello.html")).unwrap();
    engine.load_and_wait(&wv).unwrap();

    assert_eq!(wv.page_title().as_deref(), Some("Ghost T1: Hello World"));
    assert!(wv.is_loaded());

    let progress = wv.load_progress();
    assert!(progress.started_at.is_some());
    assert!(progress.head_parsed_at.is_some());
    assert!(progress.complete_at.is_some());
}

#[test]
fn t1_semantic_structure_loads() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier1_static/semantic_structure.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    assert_eq!(
        wv.page_title().as_deref(),
        Some("Ghost T1: Semantic Structure")
    );
}

#[test]
fn t1_table_and_list_loads() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier1_static/table_and_list.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    assert_eq!(
        wv.page_title().as_deref(),
        Some("Ghost T1: Table & List")
    );
}

// ── Tier 2: Vanilla JS ──────────────────────────────────────────────────────

#[test]
fn t2_dom_manipulation() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier2_vanilla_js/dom_manipulation.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    // The script changes document.title after DOM manipulation.
    assert_eq!(wv.page_title().as_deref(), Some("Ghost T2: DOM Ready"));
}

#[test]
fn t2_async_timers() {
    let mut config = GhostEngineConfig::default();
    // Ensure settle window is wide enough for the chained timeouts (≈150ms).
    config.settle_timeout = std::time::Duration::from_secs(3);
    config.quiet_period = std::time::Duration::from_millis(500);

    let engine = GhostEngine::new(config).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier2_vanilla_js/async_timers.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    assert_eq!(wv.page_title().as_deref(), Some("Ghost T2: Timers Done"));
}

#[test]
fn t2_promises() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier2_vanilla_js/promises.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    assert_eq!(wv.page_title().as_deref(), Some("Ghost T2: Promises Done"));
}

#[test]
fn t2_event_listeners() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier2_vanilla_js/event_listeners.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    assert_eq!(wv.page_title().as_deref(), Some("Ghost T2: Events Ready"));
}

#[test]
fn t2_json_render() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier2_vanilla_js/json_render.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    assert_eq!(
        wv.page_title().as_deref(),
        Some("Ghost T2: JSON Rendered")
    );
}

// ── Tier 3: React SPA ───────────────────────────────────────────────────────
//
// Note: Tier 3 tests load React from unpkg.com CDN. They require network
// access and will fail in fully-offline environments. They are still
// included because TSK-1.12 specifically requires "Basic React" test
// coverage, and the Milestone 1 goal is to "run against a React/Vue SPA."

#[test]
fn t3_react_counter() {
    let mut config = GhostEngineConfig::default();
    config.settle_timeout = std::time::Duration::from_secs(5);

    let engine = GhostEngine::new(config).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier3_react/counter.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    // React sets the title via useEffect after first render.
    assert_eq!(
        wv.page_title().as_deref(),
        Some("Ghost T3: React Count 0")
    );
}

#[test]
fn t3_react_todo() {
    let mut config = GhostEngineConfig::default();
    config.settle_timeout = std::time::Duration::from_secs(5);

    let engine = GhostEngine::new(config).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier3_react/todo_app.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    assert_eq!(
        wv.page_title().as_deref(),
        Some("Ghost T3: React Todo (3 items)")
    );
}

#[test]
fn t3_react_async_fetch() {
    let mut config = GhostEngineConfig::default();
    // The fake fetch has a 150ms delay; settle must be long enough.
    config.settle_timeout = std::time::Duration::from_secs(5);
    config.quiet_period = std::time::Duration::from_millis(500);

    let engine = GhostEngine::new(config).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier3_react/async_fetch.html"))
        .unwrap();
    engine.load_and_wait(&wv).unwrap();

    assert_eq!(
        wv.page_title().as_deref(),
        Some("Ghost T3: Fetch Loaded (3)")
    );
}

// ── wait_until API ──────────────────────────────────────────────────────────

#[test]
fn wait_until_head_parsed() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier1_static/hello.html"))
        .unwrap();

    // Wait only until HeadParsed — should return before full load.
    engine.wait_until(&wv, LoadStatus::HeadParsed).unwrap();
    assert!(wv.load_progress().head_parsed_at.is_some());
}

// ── Tier 4: Heavy / Stress ──────────────────────────────────────────────────
//
// These pages exercise features that may be unsupported or crash-prone.
// Tests verify the engine handles them gracefully (complete or error)
// without a process-level crash.

#[test]
fn t4_webgl_canvas() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let url = fixture_url("tier4_heavy/webgl_canvas.html");
    let wv = engine.new_webview(&url).unwrap();
    let result = engine.load_and_wait(&wv);

    // Whether it loads successfully or crashes, the process must survive.
    match result {
        Ok(()) => {
            // Page loaded — title indicates WebGL support status.
            let title = wv.page_title().unwrap_or_default();
            assert!(
                title.starts_with("Ghost T4: WebGL"),
                "unexpected title: {title}"
            );
            assert!(!wv.has_crashed());
        },
        Err(ghost_core::GhostError::Crashed { reason, .. }) => {
            // Content process crashed — that's acceptable for this test,
            // as long as the engine reported it cleanly.
            assert!(!reason.is_empty());
            assert!(wv.has_crashed());
            assert!(wv.crash_info().is_some());
        },
        Err(ghost_core::GhostError::Timeout) => {
            // Timeout is acceptable for heavy pages.
        },
        Err(other) => panic!("unexpected error: {other}"),
    }
}

#[test]
fn t4_web_worker() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let url = fixture_url("tier4_heavy/web_worker.html");
    let wv = engine.new_webview(&url).unwrap();
    let result = engine.load_and_wait(&wv);

    match result {
        Ok(()) => {
            let title = wv.page_title().unwrap_or_default();
            assert!(
                title.starts_with("Ghost T4: Worker"),
                "unexpected title: {title}"
            );
        },
        Err(ghost_core::GhostError::Crashed { reason, .. }) => {
            assert!(!reason.is_empty());
        },
        Err(ghost_core::GhostError::Timeout) => {},
        Err(other) => panic!("unexpected error: {other}"),
    }
}

#[test]
fn t4_deep_dom() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let url = fixture_url("tier4_heavy/deep_dom.html");
    let wv = engine.new_webview(&url).unwrap();
    let result = engine.load_and_wait(&wv);

    match result {
        Ok(()) => {
            let title = wv.page_title().unwrap_or_default();
            assert!(
                title.starts_with("Ghost T4: Deep DOM"),
                "unexpected title: {title}"
            );
        },
        Err(ghost_core::GhostError::Crashed { reason, .. }) => {
            assert!(!reason.is_empty());
        },
        Err(ghost_core::GhostError::Timeout) => {},
        Err(other) => panic!("unexpected error: {other}"),
    }
}

#[test]
fn t4_css_animations() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let url = fixture_url("tier4_heavy/css_animations.html");
    let wv = engine.new_webview(&url).unwrap();
    let result = engine.load_and_wait(&wv);

    match result {
        Ok(()) => {
            let title = wv.page_title().unwrap_or_default();
            assert!(
                title.starts_with("Ghost T4: Animations"),
                "unexpected title: {title}"
            );
        },
        Err(ghost_core::GhostError::Crashed { reason, .. }) => {
            assert!(!reason.is_empty());
        },
        Err(ghost_core::GhostError::Timeout) => {},
        Err(other) => panic!("unexpected error: {other}"),
    }
}

#[test]
fn t4_error_recovery() {
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let url = fixture_url("tier4_heavy/error_recovery.html");
    let wv = engine.new_webview(&url).unwrap();
    let result = engine.load_and_wait(&wv);

    match result {
        Ok(()) => {
            // JS errors should be handled inside the page, not crash the engine.
            assert_eq!(
                wv.page_title().as_deref(),
                Some("Ghost T4: Error Recovery Done")
            );
            assert!(!wv.has_crashed());
        },
        Err(ghost_core::GhostError::Crashed { reason, .. }) => {
            assert!(!reason.is_empty());
        },
        Err(ghost_core::GhostError::Timeout) => {},
        Err(other) => panic!("unexpected error: {other}"),
    }
}

// ── Crash callback API ──────────────────────────────────────────────────────

#[test]
fn crash_callback_not_called_on_success() {
    use std::cell::Cell;
    let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
    let wv = engine
        .new_webview(&fixture_url("tier1_static/hello.html"))
        .unwrap();

    let called = Rc::new(Cell::new(false));
    let called_clone = called.clone();
    wv.set_on_crash(Some(Box::new(move |_info| {
        called_clone.set(true);
    })));

    engine.load_and_wait(&wv).unwrap();
    assert!(!called.get(), "crash callback should not fire on success");
    assert!(!wv.has_crashed());
    assert!(wv.crash_info().is_none());
}
