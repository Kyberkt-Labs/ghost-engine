# ghost-mcp — Ghost Engine MCP Server

A [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) server that exposes Ghost Engine as browser-automation tools for AI agents. Agents can navigate websites, extract structured content, interact with elements, take screenshots, and manage browser sessions — all via standard MCP tool calls.

## Quick Start

### Build

```sh
cd ghost-engine
cargo build -p ghost-mcp --release
```

The binary is at `target/release/ghost-mcp`.

### Run (stdio — default)

```sh
ghost-mcp
```

Reads JSON-RPC 2.0 from stdin, writes to stdout (Content-Length framing). For local MCP clients like Claude Desktop and VS Code.

### Run (HTTP — remote access)

```sh
# Listen on localhost:3100
ghost-mcp --http

# Custom port
ghost-mcp --http --port 8080

# Listen on all interfaces (accessible over network)
ghost-mcp --http --host 0.0.0.0 --port 3100
```

The HTTP server exposes two MCP transport endpoints:

| Endpoint | Transport | Description |
|----------|-----------|-------------|
| `POST /mcp` | Streamable HTTP | Modern MCP transport — send JSON-RPC, get JSON-RPC back |
| `GET /sse` + `POST /messages` | SSE | Legacy MCP transport — server-sent events |
| `GET /health` | — | Health check (returns `{"status":"ok"}`) |

## Agent Configuration

### Stdio (local)

#### Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "ghost": {
      "command": "/path/to/ghost-mcp"
    }
  }
}
```

#### VS Code Copilot / GitHub Copilot

Add to `.vscode/mcp.json` in your workspace:

```json
{
  "servers": {
    "ghost": {
      "type": "stdio",
      "command": "/path/to/ghost-mcp"
    }
  }
}
```

### HTTP (remote)

For any MCP client that supports Streamable HTTP or SSE transport:

**Claude Desktop (HTTP):**

```json
{
  "mcpServers": {
    "ghost": {
      "url": "http://YOUR_HOST:3100/mcp"
    }
  }
}
```

**VS Code (HTTP):**

```json
{
  "servers": {
    "ghost": {
      "type": "http",
      "url": "http://YOUR_HOST:3100/mcp"
    }
  }
}
```

**SSE transport (legacy clients):**

```json
{
  "servers": {
    "ghost": {
      "type": "sse",
      "url": "http://YOUR_HOST:3100/sse"
    }
  }
}
```

### Generic MCP Client

Any MCP-compatible client that supports `stdio`, `streamable-http`, or `sse` transport can connect. The server implements MCP protocol version `2024-11-05`.

## Available Tools (15)

### Navigation & Extraction

| Tool | Description |
|------|-------------|
| `ghost_navigate` | Navigate to a URL. Returns page title, final URL, and load timing. |
| `ghost_extract` | Extract the visible layout tree as structured Markdown or JSON with ghost-id annotations. |
| `ghost_screenshot` | Capture a PNG screenshot of the current viewport (returned as base64). |

### Interactions

| Tool | Description |
|------|-------------|
| `ghost_click` | Click an element by ghost-id. Returns updated layout. |
| `ghost_type` | Type text into an input element by ghost-id. Returns updated layout. |
| `ghost_scroll` | Scroll to an element by ghost-id, or scroll viewport by pixel offset. |

### JavaScript & Cookies

| Tool | Description |
|------|-------------|
| `ghost_evaluate_js` | Execute JavaScript in the page context and return the result. |
| `ghost_get_cookies` | Get all non-httpOnly cookies for the current page. |
| `ghost_set_cookie` | Set a cookie (name, value, optional path/domain). |

### Multi-Tab Management

| Tool | Description |
|------|-------------|
| `ghost_new_tab` | Open a new tab at a URL. The new tab becomes active. |
| `ghost_switch_tab` | Switch the active tab by tab ID. |
| `ghost_close_tab` | Close a tab by ID (or the active tab). |
| `ghost_list_tabs` | List all open tabs with IDs, URLs, and titles. |

### Network & Performance

| Tool | Description |
|------|-------------|
| `ghost_block_urls` | Set URL-blocking patterns to cancel matching network requests (ads, trackers). |
| `ghost_perf` | Get a performance report: engine init time, RSS memory, load timing breakdown, blocked resources & bytes saved. |

## Resources

The server exposes one MCP resource:

- **`ghost://capabilities`** — A Markdown document describing supported features, known limitations, error categories, and best practices for agents. Agents can read this via `resources/read` to self-discover what Ghost Engine can do.

## Error Handling

Every tool error includes structured information that agents can reason about:

```
Error [category]: Human-readable message
Tool: tool_name
Hint: Recovery suggestion
```

### Error Categories

| Category | Meaning | Recovery |
|----------|---------|----------|
| `invalid_params` | Missing or wrong parameter | Check tool schema |
| `no_page` | No page loaded | Call `ghost_navigate` first |
| `element_not_found` | ghost-id doesn't match | Re-run `ghost_extract` for fresh IDs |
| `navigation_failed` | URL unreachable | Verify URL, retry |
| `js_error` | JavaScript error | Fix script syntax |
| `screenshot_failed` | Rendering not ready | Navigate first, then retry |
| `crashed` | Renderer crashed | Open a new tab |
| `timeout` | Operation too slow | Try simpler page |
| `tab_not_found` | Tab ID doesn't exist | Use `ghost_list_tabs` |
| `unknown_tool` | Tool name not recognized | Use `tools/list` |
| `internal` | Unexpected error | Retry |

## Typical Agent Workflow

```
1. ghost_navigate({ url: "https://example.com" })
   → "Navigated to: https://example.com\nTitle: Example\nTab: 1\nLoad time: 342 ms"

2. ghost_extract({ format: "markdown" })
   → Structured Markdown with [ghost-id=N] annotations on interactive elements

3. ghost_click({ ghost_id: 5 })
   → Updated layout after clicking element 5

4. ghost_type({ ghost_id: 12, text: "search query" })
   → Updated layout after typing into input element 12

5. ghost_screenshot()
   → Base64-encoded PNG image of the current viewport
```

## Session Behavior

- **Persistent state**: DOM, cookies, JavaScript globals, and browsing history persist across tool calls within one MCP connection.
- **Multi-tab**: Each tab is independent. Interactions target the active tab.
- **Cleanup**: When the MCP connection closes (stdin EOF), all tabs and the browser engine are cleaned up automatically.

## Build Features

| Feature | Default | Description |
|---------|---------|-------------|
| `headless-shell` | ✅ | Headless rendering (no GUI window) |
| `js-jit` | ✅ | SpiderMonkey JIT compilation for faster JS |
