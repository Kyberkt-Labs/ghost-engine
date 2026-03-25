use ghost_core::{GhostEngine, GhostError, GhostWebView, JSValue};

use crate::{ActionResult, CookiePair};

/// Read all cookies accessible via `document.cookie` (non-httpOnly).
pub fn get_cookies(
    engine: &GhostEngine,
    webview: &GhostWebView,
) -> Result<ActionResult, GhostError> {
    let result = engine.evaluate_js(webview, "(function(){ return document.cookie; })()")?;
    let cookie_str = match result {
        JSValue::String(s) => s,
        _ => String::new(),
    };

    let cookies = cookie_str
        .split(';')
        .filter_map(|pair| {
            let pair = pair.trim();
            if pair.is_empty() {
                return None;
            }
            let (name, value) = match pair.split_once('=') {
                Some((n, v)) => (n.trim().to_string(), v.trim().to_string()),
                None => (pair.to_string(), String::new()),
            };
            Some(CookiePair { name, value })
        })
        .collect();

    Ok(ActionResult::Cookies(cookies))
}

/// Set a cookie via `document.cookie`.
pub fn set_cookie(
    engine: &GhostEngine,
    webview: &GhostWebView,
    name: &str,
    value: &str,
    path: Option<&str>,
    domain: Option<&str>,
) -> Result<ActionResult, GhostError> {
    let escaped_name = crate::keyboard::js_escape_for_template(name);
    let escaped_value = crate::keyboard::js_escape_for_template(value);
    let mut cookie_js = format!(r#"var c = "{escaped_name}={escaped_value}";"#);

    if let Some(path) = path {
        let escaped = crate::keyboard::js_escape_for_template(path);
        cookie_js.push_str(&format!(r#" c += "; path={escaped}";"#));
    }
    if let Some(domain) = domain {
        let escaped = crate::keyboard::js_escape_for_template(domain);
        cookie_js.push_str(&format!(r#" c += "; domain={escaped}";"#));
    }

    cookie_js.push_str(" document.cookie = c; return true;");

    let script = format!("(function(){{ {cookie_js} }})()");
    engine.evaluate_js(webview, &script)?;
    Ok(ActionResult::Ok)
}

/// Clear all cookies: for each cookie visible via `document.cookie`, set
/// it expired. Note: httpOnly cookies cannot be cleared this way.
pub fn clear_cookies(
    engine: &GhostEngine,
    webview: &GhostWebView,
) -> Result<ActionResult, GhostError> {
    let script = r#"(function(){
var cookies = document.cookie.split(';');
for (var i = 0; i < cookies.length; i++) {
  var name = cookies[i].split('=')[0].trim();
  if (name) {
    document.cookie = name + '=;expires=Thu, 01 Jan 1970 00:00:00 GMT;path=/';
  }
}
return cookies.length;
})()"#;
    engine.evaluate_js(webview, script)?;
    Ok(ActionResult::Ok)
}
