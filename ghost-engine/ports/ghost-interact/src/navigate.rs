use ghost_core::{GhostEngine, GhostError, GhostWebView};

use crate::ActionResult;

/// Navigate to a new URL, wait for load, and return Navigated.
pub fn navigate(
    engine: &GhostEngine,
    webview: &GhostWebView,
    url: &str,
) -> Result<ActionResult, GhostError> {
    webview.load(url)?;
    engine.load_and_wait(webview)?;
    Ok(ActionResult::Navigated)
}

/// Go back in history. If there's no history entry, returns Ok instead.
pub fn go_back(
    engine: &GhostEngine,
    webview: &GhostWebView,
) -> Result<ActionResult, GhostError> {
    if webview.go_back() {
        engine.load_and_wait(webview)?;
        Ok(ActionResult::Navigated)
    } else {
        Ok(ActionResult::Ok)
    }
}

/// Go forward in history. If there's no entry ahead, returns Ok instead.
pub fn go_forward(
    engine: &GhostEngine,
    webview: &GhostWebView,
) -> Result<ActionResult, GhostError> {
    if webview.go_forward() {
        engine.load_and_wait(webview)?;
        Ok(ActionResult::Navigated)
    } else {
        Ok(ActionResult::Ok)
    }
}

/// Reload the current page.
pub fn reload(
    engine: &GhostEngine,
    webview: &GhostWebView,
) -> Result<ActionResult, GhostError> {
    webview.reload();
    engine.load_and_wait(webview)?;
    Ok(ActionResult::Navigated)
}
