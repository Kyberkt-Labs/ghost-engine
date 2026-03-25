use ghost_core::{GhostEngine, GhostError, GhostWebView};

use crate::{ActionResult, run_on_element};

/// Click an interactive element by ghost-id. Dispatches a full click sequence:
/// mousedown → mouseup → click, then calls `el.click()` as a fallback to
/// ensure default actions (form submit, link navigation) fire.
pub fn click(
    engine: &GhostEngine,
    webview: &GhostWebView,
    ghost_id: u32,
) -> Result<ActionResult, GhostError> {
    run_on_element(engine, webview, ghost_id, r#"
el.focus();
el.dispatchEvent(new MouseEvent('mousedown', {bubbles:true,cancelable:true,view:window}));
el.dispatchEvent(new MouseEvent('mouseup',   {bubbles:true,cancelable:true,view:window}));
el.click();
return true;
"#)?;
    Ok(ActionResult::Ok)
}

/// Hover over an element — fires mouseenter + mouseover.
pub fn hover(
    engine: &GhostEngine,
    webview: &GhostWebView,
    ghost_id: u32,
) -> Result<ActionResult, GhostError> {
    run_on_element(engine, webview, ghost_id, r#"
el.dispatchEvent(new MouseEvent('mouseenter', {bubbles:false,cancelable:false,view:window}));
el.dispatchEvent(new MouseEvent('mouseover',  {bubbles:true, cancelable:true, view:window}));
return true;
"#)?;
    Ok(ActionResult::Ok)
}

/// Focus an element.
pub fn focus(
    engine: &GhostEngine,
    webview: &GhostWebView,
    ghost_id: u32,
) -> Result<ActionResult, GhostError> {
    run_on_element(engine, webview, ghost_id, "el.focus(); return true;")?;
    Ok(ActionResult::Ok)
}
