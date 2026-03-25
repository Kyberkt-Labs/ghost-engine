//! MCP server core: handles JSON-RPC dispatch and tool execution.

use std::collections::HashMap;
use std::io::{self, BufReader, BufWriter};
use std::time::Instant;

use serde_json::{json, Value};

use ghost_core::{GhostEngine, GhostEngineConfig, GhostError, GhostWebView, JSValue};
use ghost_interact::{Action, ActionResult};

use crate::protocol;
use crate::schema;

// ── Structured error categories (TSK-4.13) ─────────────────────────────────

/// Error categories that agents can reason about programmatically.
#[derive(Debug, Clone, Copy)]
enum ErrorCategory {
    /// Missing or invalid parameter.
    InvalidParams,
    /// No page/tab loaded yet.
    NoPage,
    /// Target element not found by ghost-id.
    ElementNotFound,
    /// Navigation/loading failed.
    NavigationFailed,
    /// JavaScript evaluation error.
    JsError,
    /// Screenshot capture failed.
    ScreenshotFailed,
    /// Page or renderer crashed.
    Crashed,
    /// Operation timed out.
    Timeout,
    /// Internal/unknown error.
    Internal,
    /// Unknown tool name.
    UnknownTool,
    /// Tab not found.
    TabNotFound,
}

impl ErrorCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::InvalidParams => "invalid_params",
            Self::NoPage => "no_page",
            Self::ElementNotFound => "element_not_found",
            Self::NavigationFailed => "navigation_failed",
            Self::JsError => "js_error",
            Self::ScreenshotFailed => "screenshot_failed",
            Self::Crashed => "crashed",
            Self::Timeout => "timeout",
            Self::Internal => "internal",
            Self::UnknownTool => "unknown_tool",
            Self::TabNotFound => "tab_not_found",
        }
    }

    fn recovery_hint(self) -> &'static str {
        match self {
            Self::InvalidParams => "Check the tool schema for required parameters and correct types.",
            Self::NoPage => "Call ghost_navigate with a URL first.",
            Self::ElementNotFound => "Run ghost_extract to get the current page layout and valid ghost-id values.",
            Self::NavigationFailed => "Verify the URL is valid and the site is reachable. Try again or use a different URL.",
            Self::JsError => "Check your JavaScript expression for syntax errors. Use ghost_evaluate_js with simpler expressions to debug.",
            Self::ScreenshotFailed => "The page may not have rendered yet. Try ghost_navigate first, then retry.",
            Self::Crashed => "The page renderer crashed. Open a new tab with ghost_new_tab to continue.",
            Self::Timeout => "The page took too long to respond. Try a simpler page or retry.",
            Self::Internal => "An unexpected error occurred. Try the operation again.",
            Self::UnknownTool => "Use tools/list to see available tools.",
            Self::TabNotFound => "Use ghost_list_tabs to see open tabs and their IDs.",
        }
    }
}

/// A tool error with category, message, and recovery hint.
struct ToolError {
    category: ErrorCategory,
    message: String,
}

impl ToolError {
    fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        Self { category, message: message.into() }
    }

    fn to_mcp_value(&self, tool_name: &str) -> Value {
        json!({
            "content": [{
                "type": "text",
                "text": format!(
                    "Error [{}]: {}\nTool: {}\nHint: {}",
                    self.category.as_str(),
                    self.message,
                    tool_name,
                    self.category.recovery_hint(),
                )
            }],
            "isError": true
        })
    }
}

/// Classify a `GhostError` into an appropriate `ToolError`.
fn classify_error(e: &GhostError, _fallback_cat: ErrorCategory) -> ToolError {
    match e {
        GhostError::Init(msg) => ToolError::new(ErrorCategory::Internal, msg.clone()),
        GhostError::Navigation(msg) => ToolError::new(ErrorCategory::NavigationFailed, msg.clone()),
        GhostError::Timeout => ToolError::new(ErrorCategory::Timeout, "Operation timed out."),
        GhostError::Crashed { reason, .. } => ToolError::new(ErrorCategory::Crashed, reason.clone()),
        GhostError::JavaScript(msg) => ToolError::new(ErrorCategory::JsError, msg.clone()),
        GhostError::Panic(msg) => ToolError::new(ErrorCategory::Crashed, format!("Internal panic: {msg}")),
    }
}

/// Convenience: convert a `Result<Value, ToolError>` to the `Result<Value, String>` that tool methods return.
/// This keeps tool methods returning `Result<Value, String>` for simplicity, encoding the structured
/// error into the string in a parseable format that `handle_tools_call` will intercept.

// ── Server state ────────────────────────────────────────────────────────────

pub struct McpServer {
    engine: GhostEngine,
    /// All open tabs, keyed by a sequential tab ID.
    tabs: HashMap<u32, GhostWebView>,
    /// The currently active tab ID.
    active_tab: Option<u32>,
    /// Counter for assigning tab IDs.
    next_tab_id: u32,
    /// URL block patterns applied to newly created tabs.
    block_patterns: Vec<String>,
}

impl McpServer {
    pub fn new() -> Result<Self, GhostError> {
        let engine = GhostEngine::new(GhostEngineConfig::default())?;
        Ok(Self {
            engine,
            tabs: HashMap::new(),
            active_tab: None,
            next_tab_id: 1,
            block_patterns: Vec::new(),
        })
    }

    /// Run the MCP server loop, reading from stdin and writing to stdout.
    pub fn run(&mut self) {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut reader = BufReader::new(stdin.lock());
        let mut writer = BufWriter::new(stdout.lock());

        loop {
            match protocol::read_message(&mut reader) {
                Ok(Some(msg)) => {
                    if let Some(response) = self.handle_message(&msg) {
                        if let Err(e) = protocol::send_response(&mut writer, &response) {
                            eprintln!("[ghost-mcp] write error: {e}");
                            break;
                        }
                    }
                },
                Ok(None) => break, // EOF — client disconnected
                Err(e) => {
                    eprintln!("[ghost-mcp] read error: {e}");
                    break;
                },
            }
        }

        eprintln!("[ghost-mcp] server exiting");
    }

    /// Run the engine loop, processing requests from a channel.
    /// Used by the HTTP transport — the HTTP server thread sends requests
    /// here and the main thread (which owns the engine) processes them.
    pub fn run_channel(&mut self, rx: std::sync::mpsc::Receiver<crate::http::EngineRequest>) {
        for (msg, resp_tx) in rx {
            let response = self.handle_message(&msg);
            resp_tx.send(response).ok();
        }
        eprintln!("[ghost-mcp] server exiting");
    }

    // ── Dispatch ────────────────────────────────────────────────────────

    pub fn handle_message(&mut self, msg: &protocol::JsonRpcRequest) -> Option<Value> {
        // Notifications have no id — process silently, no response.
        match msg.method.as_str() {
            "notifications/initialized" | "notifications/cancelled" => return None,
            _ => {},
        }

        let id = msg.id.clone().unwrap_or(Value::Null);

        let result = match msg.method.as_str() {
            "initialize" => Ok(self.handle_initialize()),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(self.handle_tools_list()),
            "tools/call" => Ok(self.handle_tools_call(&msg.params)),
            "resources/list" => Ok(self.handle_resources_list()),
            "resources/read" => Ok(self.handle_resources_read(&msg.params)),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            other => Err((-32601, format!("Method not found: {other}"))),
        };

        Some(match result {
            Ok(value) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": value
            }),
            Err((code, message)) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": code, "message": message }
            }),
        })
    }

    fn handle_initialize(&self) -> Value {
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {},
                "resources": {}
            },
            "serverInfo": {
                "name": "ghost-mcp",
                "version": "0.1.0"
            }
        })
    }

    fn handle_tools_list(&self) -> Value {
        json!({ "tools": schema::tool_definitions() })
    }

    // ── TSK-4.14: MCP resources ─────────────────────────────────────────

    fn handle_resources_list(&self) -> Value {
        json!({
            "resources": [
                {
                    "uri": "ghost://capabilities",
                    "name": "Ghost Engine Capabilities",
                    "description": "Supported features, known limitations, and browser compatibility information for Ghost Engine.",
                    "mimeType": "text/markdown"
                }
            ]
        })
    }

    fn handle_resources_read(&self, params: &Value) -> Value {
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or("");

        match uri {
            "ghost://capabilities" => json!({
                "contents": [{
                    "uri": "ghost://capabilities",
                    "mimeType": "text/markdown",
                    "text": Self::capabilities_document()
                }]
            }),
            _ => json!({
                "contents": [],
                "isError": true
            }),
        }
    }

    fn capabilities_document() -> &'static str {
        concat!(
            "# Ghost Engine — Capabilities & Limitations\n\n",
            "## Supported Features\n",
            "- **Page loading**: Navigate to any URL, wait for full load including JS execution\n",
            "- **Layout extraction**: Extract visible page content as structured Markdown or JSON with ghost-id annotations\n",
            "- **Iframe traversal**: Same-origin iframes are walked recursively; content merged into the parent layout tree with `iframeSrc` markers\n",
            "- **Shadow DOM traversal**: Open shadow roots are traversed during extraction; host elements marked with `[shadow]`\n",
            "- **SPA route detection**: `history.pushState`/`replaceState` changes are tracked; use `url_changed_since` to detect client-side navigation\n",
            "- **Interactions**: Click, type, scroll, hover, select, check/uncheck via ghost-id targeting\n",
            "- **JavaScript evaluation**: Execute arbitrary JS in page context\n",
            "- **Screenshots**: Capture viewport as PNG via software rendering\n",
            "- **Multi-tab browsing**: Open, switch, close, and list multiple tabs\n",
            "- **Cookie management**: Get, set, and clear cookies\n",
            "- **Network interception**: Block requests by URL pattern (ads, trackers)\n",
            "- **Session persistence**: DOM, cookies, and JS state persist across tool calls\n\n",
            "## Known Limitations\n",
            "- **Web compatibility**: Servo supports ~60-70% of web platform features. Some modern CSS and JS APIs may be unavailable.\n",
            "- **Cross-origin iframes**: Iframes from different origins appear as a single node with `iframeSrc` but their content is inaccessible.\n",
            "- **Closed shadow roots**: Web Components using closed shadow DOM mode cannot be traversed.\n",
            "- **Media**: Audio/video playback is not supported (media-stack=dummy).\n",
            "- **WebGL/Canvas**: Complex canvas or WebGL content is not captured in layout extraction (use screenshot instead).\n",
            "- **File uploads**: `<input type=\"file\">` interactions are not supported.\n\n",
            "## Error Categories\n",
            "When a tool call fails, the error response includes a category for programmatic handling:\n",
            "- `invalid_params` — Missing or wrong parameter type\n",
            "- `no_page` — No page loaded yet; call `ghost_navigate` first\n",
            "- `element_not_found` — ghost-id does not match any element; re-extract to refresh IDs\n",
            "- `navigation_failed` — URL unreachable or load error\n",
            "- `js_error` — JavaScript syntax or runtime error\n",
            "- `screenshot_failed` — Rendering not ready\n",
            "- `crashed` — Page renderer crashed; open a new tab to recover\n",
            "- `timeout` — Operation exceeded time limit\n",
            "- `tab_not_found` — Tab ID does not exist\n",
            "- `unknown_tool` — Unrecognized tool name\n",
            "- `internal` — Unexpected server error\n\n",
            "## Best Practices for Agents\n",
            "1. Always call `ghost_navigate` before any other tool.\n",
            "2. Use `ghost_extract` to get current ghost-ids before clicking or typing.\n",
            "3. After interactions that change the page, the updated layout is returned automatically.\n",
            "4. After SPA navigation (clicking links in React/Vue/Angular), re-extract to get updated ghost-ids.\n",
            "5. Use `ghost_block_urls` early to block ads/trackers for faster loads.\n",
            "6. Use `ghost_screenshot` when layout extraction alone is insufficient (complex visual pages).\n",
            "7. Check error categories to decide recovery strategy.\n",
            "8. Use `ghost_perf` after page load to review timing, memory, and resource-budget stats.\n",
        )
    }

    fn handle_tools_call(&mut self, params: &Value) -> Value {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("");
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        eprintln!("[ghost-mcp] tool: {name}");

        let result = match name {
            "ghost_navigate" => self.tool_navigate(&args),
            "ghost_extract" => self.tool_extract(&args),
            "ghost_click" => self.tool_click(&args),
            "ghost_type" => self.tool_type(&args),
            "ghost_scroll" => self.tool_scroll(&args),
            "ghost_screenshot" => self.tool_screenshot(),
            "ghost_evaluate_js" => self.tool_evaluate_js(&args),
            "ghost_get_cookies" => self.tool_get_cookies(),
            "ghost_set_cookie" => self.tool_set_cookie(&args),
            "ghost_new_tab" => self.tool_new_tab(&args),
            "ghost_switch_tab" => self.tool_switch_tab(&args),
            "ghost_close_tab" => self.tool_close_tab(&args),
            "ghost_list_tabs" => self.tool_list_tabs(),
            "ghost_block_urls" => self.tool_block_urls(&args),
            "ghost_perf" => self.tool_perf(),
            _ => Err(ToolError::new(ErrorCategory::UnknownTool, format!("Unknown tool: {name}"))),
        };

        match result {
            Ok(content) => content,
            Err(te) => te.to_mcp_value(name),
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    fn require_webview(&self) -> Result<&GhostWebView, ToolError> {
        let tab_id = self
            .active_tab
            .ok_or_else(|| ToolError::new(ErrorCategory::NoPage, "No page loaded. Use ghost_navigate first."))?;
        self.tabs
            .get(&tab_id)
            .ok_or_else(|| ToolError::new(ErrorCategory::TabNotFound, format!("Active tab {tab_id} not found.")))
    }

    fn get_format(args: &Value) -> &str {
        args.get("format")
            .and_then(Value::as_str)
            .unwrap_or("markdown")
    }

    /// Extract the DOM, stamp ghost-ids, and serialize to the requested format.
    fn extract_serialized(&self, format: &str) -> Result<Value, ToolError> {
        let wv = self.require_webview()?;
        let tree = ghost_interact::extract_and_stamp(&self.engine, wv)
            .map_err(|e| classify_error(&e, ErrorCategory::Internal))?;
        let output = match format {
            "json" => ghost_serializer::to_json(&tree),
            _ => ghost_serializer::to_markdown(&tree),
        };
        Ok(text_content(&output))
    }

    /// Create a new tab, returning its ID.
    fn create_tab(&mut self, url: &str) -> Result<u32, ToolError> {
        let wv = self
            .engine
            .new_webview_with_options(url, &self.block_patterns)
            .map_err(|e| classify_error(&e, ErrorCategory::NavigationFailed))?;
        self.engine.load_and_wait(&wv)
            .map_err(|e| classify_error(&e, ErrorCategory::NavigationFailed))?;
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.insert(id, wv);
        self.active_tab = Some(id);
        Ok(id)
    }

    // ── Tool implementations ────────────────────────────────────────────

    // TSK-4.3
    fn tool_navigate(&mut self, args: &Value) -> Result<Value, ToolError> {
        let url = args
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::new(ErrorCategory::InvalidParams, "Missing required parameter: url"))?;

        let t0 = Instant::now();

        if let Some(tab_id) = self.active_tab {
            if let Some(wv) = self.tabs.get(&tab_id) {
                wv.load(url).map_err(|e| classify_error(&e, ErrorCategory::NavigationFailed))?;
                self.engine.load_and_wait(wv).map_err(|e| classify_error(&e, ErrorCategory::NavigationFailed))?;
            }
        } else {
            self.create_tab(url)?;
        }

        let elapsed_ms = t0.elapsed().as_millis();
        let wv = self.require_webview()?;
        let title = wv.page_title().unwrap_or_default();
        let final_url = wv.url().map(|u| u.to_string()).unwrap_or_default();
        let tab_id = self.active_tab.unwrap();

        Ok(text_content(&format!(
            "Navigated to: {final_url}\nTitle: {title}\nTab: {tab_id}\nLoad time: {elapsed_ms} ms"
        )))
    }

    // TSK-4.4
    fn tool_extract(&self, args: &Value) -> Result<Value, ToolError> {
        self.extract_serialized(Self::get_format(args))
    }

    // TSK-4.5
    fn tool_click(&mut self, args: &Value) -> Result<Value, ToolError> {
        let ghost_id = args
            .get("ghost_id")
            .and_then(Value::as_u64)
            .ok_or_else(|| ToolError::new(ErrorCategory::InvalidParams, "Missing required parameter: ghost_id"))? as u32;
        let format = Self::get_format(args);

        let wv = self.require_webview()?;
        ghost_interact::execute(&self.engine, wv, &Action::Click(ghost_id))
            .map_err(|e| classify_error(&e, ErrorCategory::ElementNotFound))?;
        self.engine.settle();

        self.extract_serialized(format)
    }

    // TSK-4.5
    fn tool_type(&mut self, args: &Value) -> Result<Value, ToolError> {
        let ghost_id = args
            .get("ghost_id")
            .and_then(Value::as_u64)
            .ok_or_else(|| ToolError::new(ErrorCategory::InvalidParams, "Missing required parameter: ghost_id"))? as u32;
        let text = args
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::new(ErrorCategory::InvalidParams, "Missing required parameter: text"))?;
        let format = Self::get_format(args);

        let wv = self.require_webview()?;
        ghost_interact::execute(
            &self.engine,
            wv,
            &Action::Type(ghost_id, text.to_string()),
        )
        .map_err(|e| classify_error(&e, ErrorCategory::ElementNotFound))?;
        self.engine.settle();

        self.extract_serialized(format)
    }

    // TSK-4.5
    fn tool_scroll(&mut self, args: &Value) -> Result<Value, ToolError> {
        let format = Self::get_format(args);

        let wv = self.require_webview()?;
        if let Some(ghost_id) = args.get("ghost_id").and_then(Value::as_u64) {
            ghost_interact::execute(
                &self.engine,
                wv,
                &Action::ScrollTo(ghost_id as u32),
            )
            .map_err(|e| classify_error(&e, ErrorCategory::ElementNotFound))?;
        } else {
            let dx = args.get("dx").and_then(Value::as_i64).unwrap_or(0) as i32;
            let dy = args.get("dy").and_then(Value::as_i64).unwrap_or(0) as i32;
            ghost_interact::execute(&self.engine, wv, &Action::ScrollBy(dx, dy))
                .map_err(|e| classify_error(&e, ErrorCategory::Internal))?;
        }
        self.engine.settle();

        self.extract_serialized(format)
    }

    // TSK-4.7
    fn tool_screenshot(&self) -> Result<Value, ToolError> {
        let wv = self.require_webview()?;
        let png_bytes = self
            .engine
            .take_screenshot_png(wv)
            .map_err(|e| classify_error(&e, ErrorCategory::ScreenshotFailed))?;

        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

        Ok(json!({
            "content": [{
                "type": "image",
                "data": encoded,
                "mimeType": "image/png"
            }]
        }))
    }

    // TSK-4.6
    fn tool_evaluate_js(&self, args: &Value) -> Result<Value, ToolError> {
        let wv = self.require_webview()?;
        let script = args
            .get("script")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::new(ErrorCategory::InvalidParams, "Missing required parameter: script"))?;

        let result = self
            .engine
            .evaluate_js(wv, script)
            .map_err(|e| classify_error(&e, ErrorCategory::JsError))?;

        Ok(text_content(&jsvalue_to_string(&result)))
    }

    fn tool_get_cookies(&self) -> Result<Value, ToolError> {
        let wv = self.require_webview()?;
        let result = ghost_interact::execute(&self.engine, wv, &Action::GetCookies)
            .map_err(|e| classify_error(&e, ErrorCategory::Internal))?;

        let text = if let ActionResult::Cookies(cookies) = result {
            if cookies.is_empty() {
                "No cookies set.".to_string()
            } else {
                cookies
                    .iter()
                    .map(|c| format!("{}={}", c.name, c.value))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        } else {
            "No cookies returned.".to_string()
        };

        Ok(text_content(&text))
    }

    fn tool_set_cookie(&self, args: &Value) -> Result<Value, ToolError> {
        let wv = self.require_webview()?;
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::new(ErrorCategory::InvalidParams, "Missing required parameter: name"))?
            .to_string();
        let value = args
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::new(ErrorCategory::InvalidParams, "Missing required parameter: value"))?
            .to_string();
        let path = args.get("path").and_then(Value::as_str).map(String::from);
        let domain = args.get("domain").and_then(Value::as_str).map(String::from);

        ghost_interact::execute(
            &self.engine,
            wv,
            &Action::SetCookie {
                name: name.clone(),
                value: value.clone(),
                path,
                domain,
            },
        )
        .map_err(|e| classify_error(&e, ErrorCategory::Internal))?;

        Ok(text_content(&format!("Cookie set: {name}={value}")))
    }

    // ── TSK-4.9: Multi-tab tools ────────────────────────────────────────

    fn tool_new_tab(&mut self, args: &Value) -> Result<Value, ToolError> {
        let url = args
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::new(ErrorCategory::InvalidParams, "Missing required parameter: url"))?;

        let t0 = Instant::now();
        let tab_id = self.create_tab(url)?;
        let elapsed_ms = t0.elapsed().as_millis();
        let wv = self.tabs.get(&tab_id).unwrap();
        let title = wv.page_title().unwrap_or_default();
        let final_url = wv.url().map(|u| u.to_string()).unwrap_or_default();

        Ok(text_content(&format!(
            "New tab {tab_id}: {final_url}\nTitle: {title}\nLoad time: {elapsed_ms} ms\nTotal tabs: {}",
            self.tabs.len()
        )))
    }

    fn tool_switch_tab(&mut self, args: &Value) -> Result<Value, ToolError> {
        let tab_id = args
            .get("tab_id")
            .and_then(Value::as_u64)
            .ok_or_else(|| ToolError::new(ErrorCategory::InvalidParams, "Missing required parameter: tab_id"))? as u32;

        if !self.tabs.contains_key(&tab_id) {
            return Err(ToolError::new(
                ErrorCategory::TabNotFound,
                format!("Tab {tab_id} not found. Open tabs: {:?}", self.tabs.keys().collect::<Vec<_>>()),
            ));
        }

        self.active_tab = Some(tab_id);
        let wv = self.tabs.get(&tab_id).unwrap();
        let title = wv.page_title().unwrap_or_default();
        let url = wv.url().map(|u| u.to_string()).unwrap_or_default();

        Ok(text_content(&format!(
            "Switched to tab {tab_id}\nURL: {url}\nTitle: {title}"
        )))
    }

    // ── TSK-4.10: Session cleanup ───────────────────────────────────────

    fn tool_close_tab(&mut self, args: &Value) -> Result<Value, ToolError> {
        let tab_id = args
            .get("tab_id")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
            .or(self.active_tab)
            .ok_or_else(|| ToolError::new(ErrorCategory::NoPage, "No tab to close."))?;

        if self.tabs.remove(&tab_id).is_none() {
            return Err(ToolError::new(ErrorCategory::TabNotFound, format!("Tab {tab_id} not found.")));
        }

        // If we closed the active tab, switch to the most recent remaining.
        if self.active_tab == Some(tab_id) {
            self.active_tab = self.tabs.keys().max().copied();
        }

        let remaining = self.tabs.len();
        let msg = if let Some(active) = self.active_tab {
            format!("Closed tab {tab_id}. Active tab: {active}. Total tabs: {remaining}")
        } else {
            format!("Closed tab {tab_id}. No tabs remaining.")
        };

        Ok(text_content(&msg))
    }

    fn tool_list_tabs(&self) -> Result<Value, ToolError> {
        if self.tabs.is_empty() {
            return Ok(text_content("No tabs open."));
        }

        let mut lines = Vec::new();
        for (&id, wv) in &self.tabs {
            let active = if self.active_tab == Some(id) {
                " (active)"
            } else {
                ""
            };
            let url = wv.url().map(|u| u.to_string()).unwrap_or_default();
            let title = wv.page_title().unwrap_or_default();
            lines.push(format!("Tab {id}{active}: {title}\n  {url}"));
        }
        lines.sort(); // deterministic order

        Ok(text_content(&lines.join("\n")))
    }

    // ── TSK-4.11: URL blocking ──────────────────────────────────────────

    fn tool_block_urls(&mut self, args: &Value) -> Result<Value, ToolError> {
        let patterns = args
            .get("patterns")
            .and_then(Value::as_array)
            .ok_or_else(|| ToolError::new(ErrorCategory::InvalidParams, "Missing required parameter: patterns (array of strings)"))?
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect::<Vec<_>>();

        // Store for future tabs.
        self.block_patterns = patterns.clone();

        // Apply to all existing tabs.
        for wv in self.tabs.values() {
            wv.set_block_patterns(patterns.clone());
        }

        let count = patterns.len();
        Ok(text_content(&format!(
            "URL blocking updated: {count} pattern(s) active.\nPatterns: {}",
            if patterns.is_empty() {
                "(none — all requests allowed)".to_string()
            } else {
                patterns.join(", ")
            }
        )))
    }

    fn tool_perf(&self) -> Result<Value, ToolError> {
        let wv = self.require_webview()?;
        let report = self.engine.perf_report(wv);

        fn ms(d: std::time::Duration) -> f64 {
            (d.as_secs_f64() * 1000.0 * 10.0).round() / 10.0
        }
        fn opt_ms(d: Option<std::time::Duration>) -> Value {
            d.map(|d| json!(ms(d))).unwrap_or(Value::Null)
        }

        let load = wv.load_timing();
        let result = json!({
            "engine_init_ms": ms(report.engine_init),
            "rss_mb": (report.rss_bytes as f64 / (1024.0 * 1024.0) * 10.0).round() / 10.0,
            "load_timing": {
                "navigation_ms": opt_ms(load.navigation),
                "head_parse_ms": opt_ms(load.head_parse),
                "subresources_ms": opt_ms(load.subresources),
                "total_ms": opt_ms(load.total),
            },
            "resources_blocked": report.resources_blocked,
            "bytes_saved": report.bytes_saved,
        });

        Ok(text_content(&serde_json::to_string_pretty(&result).unwrap()))
    }
}

// ── Free helpers ────────────────────────────────────────────────────────────

fn text_content(text: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }]
    })
}

fn jsvalue_to_string(v: &JSValue) -> String {
    match v {
        JSValue::Undefined => "undefined".to_string(),
        JSValue::Null => "null".to_string(),
        JSValue::Boolean(b) => b.to_string(),
        JSValue::Number(n) => n.to_string(),
        JSValue::String(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

// ── TSK-4.12: Integration tests ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::JsonRpcRequest;
    use crate::schema;

    /// Helper: build a `JsonRpcRequest` for dispatch tests.
    #[allow(dead_code)]
    fn rpc(method: &str, params: Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: method.to_string(),
            params,
        }
    }

    // ── Protocol-level tests ────────────────────────────────────────────

    #[test]
    fn protocol_read_write_roundtrip() {
        use crate::protocol;

        let msg = json!({"jsonrpc":"2.0","id":1,"method":"ping","params":{}});
        let body = serde_json::to_vec(&msg).unwrap();
        let framed = format!("Content-Length: {}\r\n\r\n", body.len());

        let mut input = Vec::new();
        input.extend_from_slice(framed.as_bytes());
        input.extend_from_slice(&body);

        let mut reader = io::BufReader::new(&input[..]);
        let parsed = protocol::read_message(&mut reader).unwrap().unwrap();

        assert_eq!(parsed.method, "ping");
        assert_eq!(parsed.id, Some(json!(1)));

        // Write roundtrip
        let response = json!({"jsonrpc":"2.0","id":1,"result":{}});
        let mut output = Vec::new();
        protocol::send_response(&mut output, &response).unwrap();
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.starts_with("Content-Length:"));
        assert!(output_str.contains("\"result\""));
    }

    #[test]
    fn protocol_eof_returns_none() {
        use crate::protocol;
        let input: &[u8] = &[];
        let mut reader = io::BufReader::new(input);
        let result = protocol::read_message(&mut reader).unwrap();
        assert!(result.is_none());
    }

    // ── Schema tests ────────────────────────────────────────────────────

    #[test]
    fn schema_defines_all_15_tools() {
        let defs = schema::tool_definitions();
        let tools = defs.as_array().unwrap();
        assert_eq!(tools.len(), 15, "Expected 15 tool definitions");

        let names: Vec<&str> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();

        let expected = [
            "ghost_navigate", "ghost_extract", "ghost_click", "ghost_type",
            "ghost_scroll", "ghost_screenshot", "ghost_evaluate_js",
            "ghost_get_cookies", "ghost_set_cookie",
            "ghost_new_tab", "ghost_switch_tab", "ghost_close_tab",
            "ghost_list_tabs", "ghost_block_urls", "ghost_perf",
        ];

        for name in &expected {
            assert!(names.contains(name), "Missing tool definition: {name}");
        }
    }

    #[test]
    fn schema_tools_have_input_schemas() {
        let defs = schema::tool_definitions();
        for tool in defs.as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            assert!(
                tool.get("inputSchema").is_some(),
                "Tool {name} missing inputSchema"
            );
            assert_eq!(
                tool["inputSchema"]["type"].as_str().unwrap(),
                "object",
                "Tool {name} inputSchema must be type: object"
            );
        }
    }

    // ── Error reporting tests (TSK-4.13) ────────────────────────────────

    #[test]
    fn error_category_strings_are_snake_case() {
        let cats = [
            ErrorCategory::InvalidParams,
            ErrorCategory::NoPage,
            ErrorCategory::ElementNotFound,
            ErrorCategory::NavigationFailed,
            ErrorCategory::JsError,
            ErrorCategory::ScreenshotFailed,
            ErrorCategory::Crashed,
            ErrorCategory::Timeout,
            ErrorCategory::Internal,
            ErrorCategory::UnknownTool,
            ErrorCategory::TabNotFound,
        ];

        for cat in &cats {
            let s = cat.as_str();
            assert!(!s.is_empty());
            assert!(
                s.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "Category string must be snake_case: {s}"
            );
        }
    }

    #[test]
    fn error_categories_have_recovery_hints() {
        let cats = [
            ErrorCategory::InvalidParams,
            ErrorCategory::NoPage,
            ErrorCategory::ElementNotFound,
            ErrorCategory::NavigationFailed,
            ErrorCategory::JsError,
            ErrorCategory::ScreenshotFailed,
            ErrorCategory::Crashed,
            ErrorCategory::Timeout,
            ErrorCategory::Internal,
            ErrorCategory::UnknownTool,
            ErrorCategory::TabNotFound,
        ];

        for cat in &cats {
            let hint = cat.recovery_hint();
            assert!(
                hint.len() > 10,
                "Recovery hint for {:?} is too short: {hint}",
                cat
            );
        }
    }

    #[test]
    fn tool_error_produces_mcp_error_value() {
        let te = ToolError::new(ErrorCategory::NoPage, "test message");
        let val = te.to_mcp_value("ghost_extract");

        assert_eq!(val["isError"], true);
        let text = val["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("no_page"), "Missing category in error: {text}");
        assert!(text.contains("test message"), "Missing message in error: {text}");
        assert!(text.contains("ghost_extract"), "Missing tool name in error: {text}");
        assert!(text.contains("Hint:"), "Missing hint in error: {text}");
    }

    #[test]
    fn classify_ghost_error_timeout() {
        let te = classify_error(&GhostError::Timeout, ErrorCategory::Internal);
        assert_eq!(te.category.as_str(), "timeout");
    }

    #[test]
    fn classify_ghost_error_javascript() {
        let te = classify_error(
            &GhostError::JavaScript("SyntaxError".into()),
            ErrorCategory::Internal,
        );
        assert_eq!(te.category.as_str(), "js_error");
        assert!(te.message.contains("SyntaxError"));
    }

    #[test]
    fn classify_ghost_error_crash() {
        let te = classify_error(
            &GhostError::Crashed {
                reason: "OOM".into(),
                backtrace: None,
            },
            ErrorCategory::Internal,
        );
        assert_eq!(te.category.as_str(), "crashed");
    }

    #[test]
    fn classify_ghost_error_panic() {
        let te = classify_error(
            &GhostError::Panic("thread panicked".into()),
            ErrorCategory::Internal,
        );
        assert_eq!(te.category.as_str(), "crashed");
        assert!(te.message.contains("panic"));
    }

    // ── Resource tests (TSK-4.14) ───────────────────────────────────────

    #[test]
    fn capabilities_document_is_non_empty_markdown() {
        let doc = McpServer::capabilities_document();
        assert!(doc.starts_with("# Ghost Engine"));
        assert!(doc.contains("## Supported Features"));
        assert!(doc.contains("## Known Limitations"));
        assert!(doc.contains("## Error Categories"));
        assert!(doc.contains("## Best Practices for Agents"));
    }

    // ── Free helper tests ───────────────────────────────────────────────

    #[test]
    fn text_content_helper() {
        let val = text_content("hello");
        assert_eq!(val["content"][0]["type"], "text");
        assert_eq!(val["content"][0]["text"], "hello");
    }

    #[test]
    fn jsvalue_to_string_variants() {
        assert_eq!(jsvalue_to_string(&JSValue::Undefined), "undefined");
        assert_eq!(jsvalue_to_string(&JSValue::Null), "null");
        assert_eq!(jsvalue_to_string(&JSValue::Boolean(true)), "true");
        assert_eq!(jsvalue_to_string(&JSValue::Number(42.0)), "42");
        assert_eq!(jsvalue_to_string(&JSValue::String("hi".into())), "hi");
    }
}
