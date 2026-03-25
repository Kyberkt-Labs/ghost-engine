//! MCP tool definitions for `tools/list`.

use serde_json::{json, Value};

/// Return the full array of tool definitions exposed by ghost-mcp.
pub fn tool_definitions() -> Value {
    json!([
        {
            "name": "ghost_navigate",
            "description": "Navigate to a URL. Returns page title, final URL, and load timing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to navigate to"
                    }
                },
                "required": ["url"]
            }
        },
        {
            "name": "ghost_extract",
            "description": "Extract the current page's visible layout tree as structured content. Interactive elements are annotated with ghost-id numbers that can be used with action tools.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "format": {
                        "type": "string",
                        "enum": ["json", "markdown"],
                        "description": "Output format (default: markdown)"
                    }
                }
            }
        },
        {
            "name": "ghost_click",
            "description": "Click an interactive element by its ghost-id. Returns the updated page layout after the click.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "ghost_id": {
                        "type": "integer",
                        "description": "The ghost-id of the element to click"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["json", "markdown"],
                        "description": "Output format for the re-extracted layout (default: markdown)"
                    }
                },
                "required": ["ghost_id"]
            }
        },
        {
            "name": "ghost_type",
            "description": "Type text into an input element by its ghost-id. Returns the updated page layout.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "ghost_id": {
                        "type": "integer",
                        "description": "The ghost-id of the input element"
                    },
                    "text": {
                        "type": "string",
                        "description": "The text to type"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["json", "markdown"],
                        "description": "Output format for the re-extracted layout (default: markdown)"
                    }
                },
                "required": ["ghost_id", "text"]
            }
        },
        {
            "name": "ghost_scroll",
            "description": "Scroll to an element by ghost-id, or scroll the viewport by a pixel offset. Provide ghost_id to scroll an element into view, or dx/dy for a relative scroll. Returns the updated page layout.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "ghost_id": {
                        "type": "integer",
                        "description": "The ghost-id of the element to scroll into view"
                    },
                    "dx": {
                        "type": "integer",
                        "description": "Horizontal scroll offset in pixels (used when ghost_id is absent)"
                    },
                    "dy": {
                        "type": "integer",
                        "description": "Vertical scroll offset in pixels (used when ghost_id is absent)"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["json", "markdown"],
                        "description": "Output format for the re-extracted layout (default: markdown)"
                    }
                }
            }
        },
        {
            "name": "ghost_screenshot",
            "description": "Capture a PNG screenshot of the current viewport.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "ghost_evaluate_js",
            "description": "Execute a JavaScript expression in the page context and return the serialized result. The script runs in the document's global scope.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "script": {
                        "type": "string",
                        "description": "JavaScript expression to evaluate"
                    }
                },
                "required": ["script"]
            }
        },
        {
            "name": "ghost_get_cookies",
            "description": "Get all non-httpOnly cookies for the current page.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "ghost_set_cookie",
            "description": "Set a cookie for the current page.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Cookie name"
                    },
                    "value": {
                        "type": "string",
                        "description": "Cookie value"
                    },
                    "path": {
                        "type": "string",
                        "description": "Cookie path (default: /)"
                    },
                    "domain": {
                        "type": "string",
                        "description": "Cookie domain"
                    }
                },
                "required": ["name", "value"]
            }
        },
        {
            "name": "ghost_new_tab",
            "description": "Open a new browser tab and navigate it to a URL. The new tab becomes the active tab. Returns tab ID, URL, title, and load timing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to load in the new tab"
                    }
                },
                "required": ["url"]
            }
        },
        {
            "name": "ghost_switch_tab",
            "description": "Switch the active tab by tab ID. Subsequent tool calls operate on this tab.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tab_id": {
                        "type": "integer",
                        "description": "The ID of the tab to activate"
                    }
                },
                "required": ["tab_id"]
            }
        },
        {
            "name": "ghost_close_tab",
            "description": "Close a tab by ID, or close the active tab if no ID is given. When the active tab is closed, the most recent remaining tab becomes active.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tab_id": {
                        "type": "integer",
                        "description": "The ID of the tab to close (default: active tab)"
                    }
                }
            }
        },
        {
            "name": "ghost_list_tabs",
            "description": "List all open tabs with their IDs, URLs, and titles.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "ghost_block_urls",
            "description": "Set URL-blocking patterns for network request interception. Requests whose URL contains any of the given substrings will be cancelled. Pass an empty array to disable blocking. Applies to all existing and future tabs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "patterns": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Array of URL substrings to block (e.g. [\"ads.\", \"tracker.com\", \"analytics\"])"
                    }
                },
                "required": ["patterns"]
            }
        },
        {
            "name": "ghost_perf",
            "description": "Get a performance and memory report for the current page. Returns engine init time, process RSS, page-load timing breakdown (navigation, head-parse, sub-resources, total), and resource-budget statistics (blocked count, bytes saved).",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }
    ])
}
