use ghost_core::{GhostEngine, GhostError, GhostWebView};

use crate::{ActionResult, run_on_element};

/// Select an `<option>` by value inside a `<select>` element.
///
/// Sets `.value`, marks the matching `<option>` as `selected`, and fires
/// a `change` event.
pub fn select_option(
    engine: &GhostEngine,
    webview: &GhostWebView,
    ghost_id: u32,
    value: &str,
) -> Result<ActionResult, GhostError> {
    let escaped = crate::keyboard::js_escape_for_template(value);
    let action = format!(r#"
if (el.tagName !== 'SELECT') throw new Error('ghost-id ' + el.getAttribute('data-ghost-id') + ' is not a SELECT');
var val = "{escaped}";
var found = false;
for (var i = 0; i < el.options.length; i++) {{
  if (el.options[i].value === val) {{
    el.selectedIndex = i;
    found = true;
    break;
  }}
}}
if (!found) throw new Error('option value not found: ' + val);
el.dispatchEvent(new Event('change', {{bubbles:true}}));
return true;
"#);
    run_on_element(engine, webview, ghost_id, &action)?;
    Ok(ActionResult::Ok)
}

/// Set a checkbox or radio button's checked state and fire change event.
pub fn check(
    engine: &GhostEngine,
    webview: &GhostWebView,
    ghost_id: u32,
    checked: bool,
) -> Result<ActionResult, GhostError> {
    let checked_str = if checked { "true" } else { "false" };
    let action = format!(r#"
if (el.type !== 'checkbox' && el.type !== 'radio') throw new Error('ghost-id ' + el.getAttribute('data-ghost-id') + ' is not a checkbox/radio');
el.checked = {checked_str};
el.dispatchEvent(new Event('change', {{bubbles:true}}));
el.dispatchEvent(new InputEvent('input', {{bubbles:true}}));
return true;
"#);
    run_on_element(engine, webview, ghost_id, &action)?;
    Ok(ActionResult::Ok)
}
