mod click;
mod cookies;
mod form;
mod keyboard;
mod navigate;
mod scroll;
mod stamp;

use ghost_core::{GhostEngine, GhostError, GhostWebView, JSValue};
use ghost_interceptor::{extract_layout, LayoutTree};
use ghost_serializer::AnnotatedTree;

pub use stamp::stamp_ghost_ids;

// ── Interaction commands ────────────────────────────────────────────────────

/// An action an agent can issue against a loaded page.
///
/// Ghost-IDs are sequential integers assigned during extraction — agents
/// reference interactive elements by these IDs (e.g. `Click(3)`).
#[derive(Debug, Clone)]
pub enum Action {
    /// Click on the element with the given ghost-id.
    Click(u32),
    /// Hover over the element (fire mouseenter/mouseover).
    Hover(u32),
    /// Focus the element.
    Focus(u32),
    /// Type text into the focused element or the element with the given ghost-id.
    Type(u32, String),
    /// Press a special key (Enter, Escape, Tab, etc.).
    PressKey(u32, SpecialKey),
    /// Scroll the element into view.
    ScrollTo(u32),
    /// Scroll the viewport by (dx, dy) pixels.
    ScrollBy(i32, i32),
    /// Select an `<option>` by value inside a `<select>` element.
    SelectOption(u32, String),
    /// Set checkbox / radio to checked.
    Check(u32),
    /// Set checkbox / radio to unchecked.
    Uncheck(u32),
    /// Navigate to a new URL.
    Navigate(String),
    /// Go back in history.
    GoBack,
    /// Go forward in history.
    GoForward,
    /// Reload the page.
    Reload,
    /// Get all accessible cookies as `name=value` pairs.
    GetCookies,
    /// Set a cookie.
    SetCookie {
        name: String,
        value: String,
        path: Option<String>,
        domain: Option<String>,
    },
    /// Clear all cookies.
    ClearCookies,
}

/// Well-known special keys for `PressKey`.
#[derive(Debug, Clone, Copy)]
pub enum SpecialKey {
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
}

impl SpecialKey {
    fn js_key(self) -> &'static str {
        match self {
            Self::Enter => "Enter",
            Self::Escape => "Escape",
            Self::Tab => "Tab",
            Self::Backspace => "Backspace",
            Self::Delete => "Delete",
            Self::ArrowUp => "ArrowUp",
            Self::ArrowDown => "ArrowDown",
            Self::ArrowLeft => "ArrowLeft",
            Self::ArrowRight => "ArrowRight",
            Self::Home => "Home",
            Self::End => "End",
            Self::PageUp => "PageUp",
            Self::PageDown => "PageDown",
        }
    }
}

// ── Result of executing an action ───────────────────────────────────────────

/// What happened when an action was executed.
#[derive(Debug)]
pub enum ActionResult {
    /// Action completed successfully (no special return value).
    Ok,
    /// Action completed and triggered a navigation; caller should re-extract.
    Navigated,
    /// Cookie query result (for `GetCookies`).
    Cookies(Vec<CookiePair>),
}

/// A single cookie's name and value.
#[derive(Debug, Clone)]
pub struct CookiePair {
    pub name: String,
    pub value: String,
}

// ── Execution entry point ───────────────────────────────────────────────────

/// Execute an [`Action`] against the current page.
///
/// For actions that reference a ghost-id, the page must have been stamped
/// first via [`stamp_ghost_ids`]. Most actions return [`ActionResult::Ok`];
/// navigation actions return [`ActionResult::Navigated`].
pub fn execute(
    engine: &GhostEngine,
    webview: &GhostWebView,
    action: &Action,
) -> Result<ActionResult, GhostError> {
    match action {
        Action::Click(id) => click::click(engine, webview, *id),
        Action::Hover(id) => click::hover(engine, webview, *id),
        Action::Focus(id) => click::focus(engine, webview, *id),
        Action::Type(id, text) => keyboard::type_text(engine, webview, *id, text),
        Action::PressKey(id, key) => keyboard::press_key(engine, webview, *id, *key),
        Action::ScrollTo(id) => scroll::scroll_to(engine, webview, *id),
        Action::ScrollBy(dx, dy) => scroll::scroll_by(engine, webview, *dx, *dy),
        Action::SelectOption(id, val) => form::select_option(engine, webview, *id, val),
        Action::Check(id) => form::check(engine, webview, *id, true),
        Action::Uncheck(id) => form::check(engine, webview, *id, false),
        Action::Navigate(url) => navigate::navigate(engine, webview, url),
        Action::GoBack => navigate::go_back(engine, webview),
        Action::GoForward => navigate::go_forward(engine, webview),
        Action::Reload => navigate::reload(engine, webview),
        Action::GetCookies => cookies::get_cookies(engine, webview),
        Action::SetCookie { name, value, path, domain } => {
            cookies::set_cookie(engine, webview, name, value, path.as_deref(), domain.as_deref())
        },
        Action::ClearCookies => cookies::clear_cookies(engine, webview),
    }
}

/// Convenience: extract layout, stamp ghost-ids, and return both the tree
/// and the annotated mapping. This is the typical flow before interactions.
pub fn extract_and_stamp(
    engine: &GhostEngine,
    webview: &GhostWebView,
) -> Result<LayoutTree, GhostError> {
    let tree = extract_layout(engine, webview)?;
    let annotated = AnnotatedTree::from_tree(&tree);
    stamp_ghost_ids(engine, webview, &annotated)?;
    Ok(tree)
}

// ── Shared JS helpers ───────────────────────────────────────────────────────

/// Generate JS to locate an element by ghost-id, returning an error if not found.
fn resolve_element_js(ghost_id: u32) -> String {
    format!(
        r#"var el = document.querySelector('[data-ghost-id="{ghost_id}"]');
if (!el) throw new Error('ghost-id {ghost_id} not found');"#
    )
}

/// Run a JS snippet that starts with element resolution, then performs an action.
fn run_on_element(
    engine: &GhostEngine,
    webview: &GhostWebView,
    ghost_id: u32,
    action_js: &str,
) -> Result<JSValue, GhostError> {
    let script = format!(
        "(function(){{\n{}\n{}\n}})()",
        resolve_element_js(ghost_id),
        action_js,
    );
    engine.evaluate_js(webview, &script)
}
