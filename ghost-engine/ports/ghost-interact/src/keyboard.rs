use ghost_core::{GhostEngine, GhostError, GhostWebView};

use crate::{ActionResult, SpecialKey, run_on_element};

/// Type a string into the element character by character.
///
/// For each character: sets the element value (or textContent for
/// contentEditable), then fires input + change events. The element is
/// focused first.
pub fn type_text(
    engine: &GhostEngine,
    webview: &GhostWebView,
    ghost_id: u32,
    text: &str,
) -> Result<ActionResult, GhostError> {
    // Build a JS string literal for the text, escaping for safety.
    let escaped = js_escape_for_template(text);
    let action = format!(r#"
el.focus();
var text = "{escaped}";
if (el.isContentEditable) {{
  for (var i = 0; i < text.length; i++) {{
    el.textContent += text[i];
    el.dispatchEvent(new InputEvent('input', {{bubbles:true,cancelable:true,inputType:'insertText',data:text[i]}}));
  }}
}} else {{
  for (var i = 0; i < text.length; i++) {{
    el.value += text[i];
    el.dispatchEvent(new InputEvent('input', {{bubbles:true,cancelable:true,inputType:'insertText',data:text[i]}}));
  }}
}}
el.dispatchEvent(new Event('change', {{bubbles:true}}));
return true;
"#);
    run_on_element(engine, webview, ghost_id, &action)?;
    Ok(ActionResult::Ok)
}

/// Press a special key on the element.
pub fn press_key(
    engine: &GhostEngine,
    webview: &GhostWebView,
    ghost_id: u32,
    key: SpecialKey,
) -> Result<ActionResult, GhostError> {
    let key_name = key.js_key();
    let action = format!(r#"
el.focus();
var opts = {{key:'{key_name}',code:'{key_name}',bubbles:true,cancelable:true}};
el.dispatchEvent(new KeyboardEvent('keydown', opts));
el.dispatchEvent(new KeyboardEvent('keyup', opts));
return true;
"#);
    run_on_element(engine, webview, ghost_id, &action)?;
    Ok(ActionResult::Ok)
}

/// Escape a string for embedding inside a JS double-quoted string literal.
pub(crate) fn js_escape_for_template(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if c < '\x20' => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            },
            c => out.push(c),
        }
    }
    out
}
