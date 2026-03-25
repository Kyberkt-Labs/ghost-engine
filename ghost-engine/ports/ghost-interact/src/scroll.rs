use ghost_core::{GhostEngine, GhostError, GhostWebView};

use crate::{ActionResult, run_on_element};

/// Scroll the given element into view.
pub fn scroll_to(
    engine: &GhostEngine,
    webview: &GhostWebView,
    ghost_id: u32,
) -> Result<ActionResult, GhostError> {
    run_on_element(engine, webview, ghost_id, r#"
el.scrollIntoView({behavior:'instant',block:'center',inline:'nearest'});
return true;
"#)?;
    Ok(ActionResult::Ok)
}

/// Scroll the viewport by (dx, dy) pixels.
pub fn scroll_by(
    engine: &GhostEngine,
    webview: &GhostWebView,
    dx: i32,
    dy: i32,
) -> Result<ActionResult, GhostError> {
    let script = format!(
        "(function(){{ window.scrollBy({dx},{dy}); return true; }})()"
    );
    engine.evaluate_js(webview, &script)?;
    Ok(ActionResult::Ok)
}
