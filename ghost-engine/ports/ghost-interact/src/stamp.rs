use ghost_core::{GhostEngine, GhostError, GhostWebView, JSValue};
use ghost_serializer::AnnotatedTree;

/// JavaScript that walks the visible DOM in DFS order and stamps `data-ghost-id`
/// attributes on interactive elements — exactly mirroring the Rust-side
/// [`AnnotatedTree`] assignment so that `document.querySelector('[data-ghost-id="N"]')`
/// finds the right element for agent commands.
///
/// The script accepts a single argument `expectedCount` via the wrapping IIFE.
/// It returns the number of IDs actually stamped; callers should verify this
/// matches `AnnotatedTree::interactive_count`.
const STAMP_JS: &str = r###"
(function(expectedCount) {
  // Remove any stale stamps from a previous extraction cycle.
  var old = document.querySelectorAll('[data-ghost-id]');
  for (var i = 0; i < old.length; i++) old[i].removeAttribute('data-ghost-id');

  var nextId = 0;

  function isVisible(el) {
    if (!(el instanceof Element)) return false;
    var cs = window.getComputedStyle(el);
    if (cs.display === 'none') return false;
    if (cs.visibility === 'hidden') return false;
    if (parseFloat(cs.opacity) === 0) return false;
    var rect = el.getBoundingClientRect();
    if (rect.width === 0 && rect.height === 0) return false;
    return true;
  }

  function isInteractive(el) {
    var tag = el.tagName;
    if (tag === 'A' || tag === 'BUTTON' || tag === 'SELECT' ||
        tag === 'TEXTAREA') return true;
    if (tag === 'INPUT' && el.type !== 'hidden') return true;
    if (el.hasAttribute('onclick') || el.hasAttribute('tabindex')) return true;
    var role = el.getAttribute('role');
    if (role === 'button' || role === 'link' || role === 'tab' ||
        role === 'menuitem' || role === 'checkbox' || role === 'radio' ||
        role === 'switch' || role === 'textbox' || role === 'combobox') return true;
    if (el.isContentEditable) return true;
    return false;
  }

  function walk(el) {
    if (!isVisible(el)) return;
    if (isInteractive(el)) {
      el.setAttribute('data-ghost-id', nextId.toString());
      nextId++;
    }
    for (var i = 0; i < el.children.length; i++) {
      walk(el.children[i]);
    }
  }

  var body = document.body || document.documentElement;
  if (body) walk(body);
  return nextId;
})
"###;

/// Inject `data-ghost-id` attributes into the live DOM so that interaction
/// commands can locate elements by ghost-id.
///
/// Must be called after extraction and [`AnnotatedTree::from_tree`].
/// Typically wrapped by [`super::extract_and_stamp`].
pub fn stamp_ghost_ids(
    engine: &GhostEngine,
    webview: &GhostWebView,
    annotated: &AnnotatedTree,
) -> Result<(), GhostError> {
    let expected = annotated.interactive_count;
    let script = format!("{STAMP_JS}({expected})");
    let result = engine.evaluate_js(webview, &script)?;

    match result {
        JSValue::Number(n) if (n as u32) == expected => Ok(()),
        JSValue::Number(n) => {
            // Mismatch between Rust-side and JS-side counts — DOM may have
            // mutated between extraction and stamping.  Most IDs are still
            // valid so we don't fail.
            eprintln!(
                "warning: ghost-id stamp mismatch: expected {expected}, stamped {n} — \
                 DOM may have changed between extraction and stamping"
            );
            Ok(())
        },
        other => Err(GhostError::JavaScript(format!(
            "stamp_ghost_ids returned unexpected value: {other:?}"
        ))),
    }
}
