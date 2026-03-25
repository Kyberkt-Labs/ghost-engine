# Ghost Engine — User Guide

Ghost Engine is an ultra-fast headless browser for AI agents, powered by Mozilla's Servo. It loads web pages, extracts structured content, and lets you interact with elements — all without a visible browser window.

**Three ways to use it:**

| Mode | For | Transport |
|------|-----|-----------|
| **CLI** | One-shot page extraction | Shell command, stdout |
| **Interactive REPL** | Manual exploration & debugging | Shell prompt (`ghost>`) |
| **MCP Server** | AI agent integration (Claude, Copilot, etc.) | JSON-RPC over stdio |

---

## Table of Contents

- [Installation](#installation)
- [1. CLI Mode](#1-cli-mode)
- [2. Interactive REPL Mode](#2-interactive-repl-mode)
- [3. MCP Server Mode](#3-mcp-server-mode)
  - [Claude Desktop](#claude-desktop)
  - [VS Code Copilot](#vs-code-copilot)
  - [Manual JSON-RPC Testing](#manual-json-rpc-testing)
  - [Tool Reference (15 tools)](#tool-reference)
- [Output Formats](#output-formats)
- [Configuration](#configuration)
- [Performance Tuning](#performance-tuning)
- [Troubleshooting](#troubleshooting)

---

## Installation

### Build from source

```bash
cd ghost-engine

# Development build (faster compile, slower runtime)
cargo build -p ghost-cli -p ghost-mcp

# Production build (LTO, stripped — smallest + fastest binary)
./build-release.sh
```

Binaries:
- `target/debug/ghost` (or `target/production_stripped/ghost`) — CLI & interactive mode
- `target/debug/ghost-mcp` (or `target/production_stripped/ghost-mcp`) — MCP server

### Verify the build

```bash
./build-release.sh --verify
```

---

## 1. CLI Mode

Load a URL and extract its content to stdout. One command, one result.

### Basic usage

```bash
# Default tree format
ghost https://www.wikipedia.org/

# JSON output
ghost --format json https://www.wikipedia.org/

# Markdown output (recommended — most readable)
ghost --format markdown https://www.wikipedia.org/

# Short alias
ghost --format md https://example.com/
```

### CLI options

| Flag | Default | Description |
|------|---------|-------------|
| `--format <FORMAT>` | `tree` | Output: `json`, `markdown` (or `md`), `tree` |
| `--width <PX>` | `1920` | Viewport width |
| `--height <PX>` | `1080` | Viewport height |
| `--timeout <SECS>` | `30` | Max seconds to wait for page load |
| `--settle <SECS>` | `2` | Extra seconds for async JS to finish |
| `--quiet <MS>` | `500` | Idle threshold during settle phase |
| `--interactive`, `-i` | off | Enter REPL after load |
| `--log-filter <FILTER>` | none | Tracing directive (e.g. `servo=debug`) |

### Examples

```bash
# Extract Wikipedia as Markdown
ghost --format markdown https://www.wikipedia.org/

# Larger viewport, longer timeout for heavy SPAs
ghost --width 2560 --height 1440 --timeout 60 https://myapp.example.com/

# Pipe to a file
ghost --format json https://news.ycombinator.com > hn.json

# Use with jq
ghost --format json https://example.com | jq '.nodes | length'
```

### Output

The CLI prints load progress to **stderr** and content to **stdout**, so piping and redirecting work naturally:

```
Ghost Engine vServo 0.0.6-...          ← stderr
Viewport: 1920x1080, timeout: 30s     ← stderr
Loading: https://www.wikipedia.org/    ← stderr
[224ms] <head> parsed                  ← stderr
[1.2s] page complete                   ← stderr
Extracted 142 visible nodes            ← stderr
                                       
# Wikipedia                            ← stdout (content)
...
```

---

## 2. Interactive REPL Mode

Load a page and interact with it step by step — like a Python interpreter for the web.

### Start

```bash
ghost --interactive https://www.wikipedia.org/
# or short form
ghost -i https://www.wikipedia.org/

# With markdown format
ghost -i --format markdown https://www.wikipedia.org/
```

### The `ghost>` prompt

After the page loads, you'll see a prompt:

```
Ghost Engine vServo 0.0.6-...
Loading: https://www.wikipedia.org/
[224ms] <head> parsed, <body> available
[1.2s] page complete
Extracted 142 visible nodes

ghost> _
```

Type commands and press Enter. Results print immediately, then the prompt returns.

> **Note:** If the page times out, the REPL still starts — you can work with the partially loaded page.

### Command reference

#### Extraction
```
ghost> extract                    # re-extract layout (default format)
ghost> extract markdown           # extract as Markdown
ghost> extract json               # extract as JSON
```

#### Interaction (by ghost-id)

Every interactive element in the extracted layout has a numbered **ghost-id**. Use these IDs to interact:

```
ghost> click 5                    # click element #5
ghost> type 3 hello world         # type text into input #3
ghost> key 3 Enter                # press Enter in element #3
ghost> hover 7                    # hover over element #7
ghost> focus 3                    # focus element #3
ghost> scroll 12                  # scroll element #12 into view
ghost> scrollby 0 500             # scroll viewport down 500px
ghost> select 8 option-value      # select dropdown option
ghost> check 9                    # check a checkbox
ghost> uncheck 9                  # uncheck a checkbox
```

#### Navigation
```
ghost> nav https://example.com    # navigate to a new URL
ghost> back                       # go back in history
ghost> forward                    # go forward
ghost> reload                     # reload current page
```

#### Other
```
ghost> js document.title          # evaluate JavaScript
ghost> js document.querySelectorAll('a').length
ghost> cookies                    # list all cookies
ghost> help                       # show command summary
ghost> quit                       # exit (also: exit, q, Ctrl+C)
```

#### Special keys for `key` command
`Enter`, `Tab`, `Escape`, `Backspace`, `Delete`, `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`, `Home`, `End`, `PageUp`, `PageDown`

### Example session: Search Wikipedia

```
ghost> extract markdown
Extracted 142 visible nodes
# Wikipedia
[1] input "Search Wikipedia"
[2] button "Search"
...

ghost> type 1 Servo browser engine
ok

ghost> click 2
navigated — re-extracting...
Extracted 89 visible nodes
# Servo (software) — Wikipedia
...

ghost> js document.title
"Servo (software) - Wikipedia"

ghost> quit
bye
```

---

## 3. MCP Server Mode

The MCP server exposes Ghost Engine as 15 tools to any AI agent over the [Model Context Protocol](https://modelcontextprotocol.io/). The agent can navigate, extract, click, type, and reason about web pages in its conversation loop.

### Start the server

#### Stdio mode (local — default)

```bash
ghost-mcp
```

The server reads JSON-RPC 2.0 messages on **stdin** and writes responses to **stdout**. Logs go to **stderr**. Use this when your MCP client launches the server process itself (Claude Desktop, VS Code, etc.).

#### HTTP mode (remote — accessible over network)

```bash
# Listen on localhost:3100
ghost-mcp --http

# Custom port
ghost-mcp --http --port 8080

# Listen on all network interfaces
ghost-mcp --http --host 0.0.0.0 --port 3100
```

HTTP mode starts a web server with two MCP transport endpoints:

| Endpoint | Transport | Description |
|----------|-----------|-------------|
| `POST /mcp` | Streamable HTTP | Modern transport — JSON-RPC in request body, JSON-RPC in response |
| `GET /sse` + `POST /messages` | SSE | Legacy transport — persistent event stream for responses |
| `GET /health` | — | Health check endpoint |

All endpoints include CORS headers for browser-based clients.

### Claude Desktop

Add to your Claude Desktop config:

**macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`  
**Windows:** `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "ghost": {
      "command": "/path/to/ghost-mcp"
    }
  }
}
```

Or if building from source:

```json
{
  "mcpServers": {
    "ghost": {
      "command": "cargo",
      "args": ["run", "-p", "ghost-mcp"],
      "cwd": "/path/to/ghost-engine"
    }
  }
}
```

Restart Claude Desktop. The 15 `ghost_*` tools appear automatically.

**Remote (HTTP) — connect to a running server:**

```json
{
  "mcpServers": {
    "ghost": {
      "url": "http://YOUR_HOST:3100/mcp"
    }
  }
}
```

### VS Code Copilot

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

Or from source:

```json
{
  "servers": {
    "ghost": {
      "type": "stdio",
      "command": "cargo",
      "args": ["run", "-p", "ghost-mcp"],
      "cwd": "/path/to/ghost-engine"
    }
  }
}
```

**Remote (HTTP):**

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

**SSE (legacy remote):**

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

### Manual JSON-RPC Testing

#### Stdio (pipe JSON-RPC directly)

Start the server and paste these lines one at a time:

```bash
ghost-mcp
```

**1. Initialize the session:**
```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test"}}}
```

**2. Navigate to a page:**
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ghost_navigate","arguments":{"url":"https://www.wikipedia.org/"}}}
```

**3. Extract the page content:**
```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ghost_extract","arguments":{"format":"markdown"}}}
```

**4. Click an element:**
```json
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ghost_click","arguments":{"ghost_id":5}}}
```

**5. Get performance report:**
```json
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"ghost_perf","arguments":{}}}
```

**6. Block ads before navigating:**
```json
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"ghost_block_urls","arguments":{"patterns":["ads.","tracker.","analytics"]}}}
```

#### HTTP (curl)

Start the server in HTTP mode, then use `curl`:

```bash
# In terminal 1:
ghost-mcp --http --port 3100

# In terminal 2:

# Initialize
curl -s -X POST http://localhost:3100/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"curl"}}}'

# List available tools
curl -s -X POST http://localhost:3100/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | python3 -m json.tool

# Navigate to a page
curl -s -X POST http://localhost:3100/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ghost_navigate","arguments":{"url":"https://example.com"}}}'

# Extract as markdown
curl -s -X POST http://localhost:3100/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ghost_extract","arguments":{"format":"markdown"}}}'

# Health check
curl http://localhost:3100/health
```

### Tool Reference

#### Navigation & Extraction (3 tools)

| Tool | Parameters | Description |
|------|-----------|-------------|
| **`ghost_navigate`** | `url` (string, required) | Navigate to URL. Returns title, final URL, load timing. |
| **`ghost_extract`** | `format` ("json" or "markdown") | Extract visible layout tree with ghost-id annotations. |
| **`ghost_screenshot`** | *(none)* | Capture viewport as base64 PNG. |

#### Interaction (3 tools)

| Tool | Parameters | Description |
|------|-----------|-------------|
| **`ghost_click`** | `ghost_id` (int, required), `format` | Click element. Returns updated layout. |
| **`ghost_type`** | `ghost_id` (int, required), `text` (string, required), `format` | Type text into input. Returns updated layout. |
| **`ghost_scroll`** | `ghost_id` (int) or `dx`/`dy` (int), `format` | Scroll element into view, or scroll viewport by pixels. |

#### JavaScript & Cookies (3 tools)

| Tool | Parameters | Description |
|------|-----------|-------------|
| **`ghost_evaluate_js`** | `script` (string, required) | Execute JS in page context, return result. |
| **`ghost_get_cookies`** | *(none)* | List all non-httpOnly cookies. |
| **`ghost_set_cookie`** | `name`, `value` (required), `path`, `domain` | Set a cookie. |

#### Multi-Tab (4 tools)

| Tool | Parameters | Description |
|------|-----------|-------------|
| **`ghost_new_tab`** | `url` (string, required) | Open new tab, make it active. |
| **`ghost_switch_tab`** | `tab_id` (int, required) | Switch active tab. |
| **`ghost_close_tab`** | `tab_id` (int, optional) | Close tab (default: active). |
| **`ghost_list_tabs`** | *(none)* | List all tabs with IDs, URLs, titles. |

#### Network & Performance (2 tools)

| Tool | Parameters | Description |
|------|-----------|-------------|
| **`ghost_block_urls`** | `patterns` (string[], required) | Block matching URL substrings. Empty array = disable. |
| **`ghost_perf`** | *(none)* | Performance report: init time, RSS memory, load timing, blocked resources. |

### Typical agent workflow

```
1. ghost_navigate(url="https://example.com")
2. ghost_extract(format="markdown")         → agent reads the page
3. ghost_click(ghost_id=5)                  → agent clicks a link
4. ghost_extract()                          → agent reads the new page
5. ghost_type(ghost_id=3, text="query")     → agent fills a form
6. ghost_click(ghost_id=7)                  → agent submits
7. ghost_extract()                          → agent reads results
8. ghost_perf()                             → check performance
```

### MCP Resources

The server exposes a `ghost://capabilities` resource that agents can read at any time to discover supported features and known limitations.

### Error handling

Every tool returns structured errors with categories agents can reason about:

| Category | Meaning |
|----------|---------|
| `no_page` | No page loaded yet — call `ghost_navigate` first |
| `element_not_found` | ghost-id doesn't match any element |
| `navigation_failed` | URL couldn't be loaded |
| `timeout` | Page didn't load in time |
| `javascript_error` | JS execution failed |
| `invalid_params` | Missing or bad tool arguments |
| `extraction_failed` | Layout extraction failed |
| `screenshot_failed` | Screenshot capture failed |
| `tab_not_found` | Tab ID doesn't exist |
| `unknown_tool` | Unrecognized tool name |
| `internal` | Unexpected internal error |

---

## Output Formats

### Markdown (recommended for agents)

```markdown
# Page Title
url: https://example.com

[1] link "Home" href=/
[2] input "Search..." 
[3] button "Go"
## Main Content
paragraph "Welcome to our site..."
[4] link "Read more" href=/about
```

- `[N]` = ghost-id (only on interactive elements)
- Hierarchy shown via Markdown headings and indentation
- Most token-efficient for LLM consumption

### JSON

```json
{
  "url": "https://example.com",
  "title": "Page Title",
  "nodes": [
    {
      "tag": "a",
      "ghostId": 1,
      "text": "Home",
      "href": "/",
      "interactive": true,
      "rect": {"x": 10, "y": 20, "w": 60, "h": 18},
      "children": []
    }
  ]
}
```

- Full structured data — useful for programmatic processing
- Includes geometry (bounding rectangles), roles, classes, IDs

### Tree (CLI default)

```
url: https://example.com
title: Page Title
nodes: 42

html [0,0 1920x1080]
  body [0,0 1920x1080]
    div#header [0,0 1920x60]
      a [10,20 60x18] *interactive* "Home" href=/
```

- Compact hierarchical dump — useful for debugging
- Shows CSS bounding boxes

---

## Configuration

### Ghost Engine config fields

These are set programmatically in code (Rust API) or use defaults in CLI/MCP:

| Field | Default | Description |
|-------|---------|-------------|
| `viewport_width` | 1920 | Viewport width (px) |
| `viewport_height` | 1080 | Viewport height (px) |
| `load_timeout` | 30s | Max time for page load |
| `settle_timeout` | 2s | Extra time for async JS |
| `quiet_period` | 500ms | Idle detection threshold |
| `connection_timeout` | 10s | Network connection timeout |
| `http_cache_enabled` | true | Enable HTTP cache |
| `http_cache_size` | 5000 | Cache size weight |
| `resource_budget` | *(all allowed)* | Sub-resource filtering |
| `user_agent` | *(Servo default)* | Custom User-Agent |

---

## Performance Tuning

### Resource budgeting

Skip unnecessary downloads (images, fonts, media) — dramatically speeds up extraction for text-focused tasks:

```rust
// Rust API
let config = GhostEngineConfig {
    resource_budget: ResourceBudget {
        skip_images: true,       // skip jpg, png, gif, webp, svg, ico, avif
        skip_fonts: true,        // skip woff, woff2, ttf, otf, eot
        skip_media: true,        // skip mp4, webm, mp3, ogg, wav
        skip_stylesheets: false, // keep CSS (affects layout accuracy)
        max_resource_bytes: 0,   // 0 = no size limit
    },
    ..Default::default()
};
```

Via MCP, use `ghost_block_urls` early to block heavy resources:

```json
{"name": "ghost_block_urls", "arguments": {"patterns": [".jpg", ".png", ".gif", ".webp", ".woff2", ".mp4"]}}
```

### Performance report

Use `ghost_perf` (MCP) or check the report programmatically:

```
engine_init: 45.2ms
rss: 82.3 MB
load: nav=12.1ms, head=156.3ms, sub=890.4ms, total=1058.8ms
blocked: 23 requests, ~1024.5 KB saved
```

---

## Troubleshooting

### "fatal: content process crashed"

A Servo internal panic (e.g., in font shaping, layout, JS engine). The page may use unsupported web features. Try:
1. A different URL to confirm the engine works
2. Block web fonts: `ghost_block_urls(["woff", "ttf", "otf"])`
3. Check stderr logs for the specific panic message

### "page load timed out"

The page didn't reach `Complete` status within the timeout. Common with heavy SPAs (many sub-resources). Options:
- In CLI: `--timeout 60` (increase timeout)
- In interactive mode: the REPL still starts — `extract` works on the partial page
- Via MCP: the tool returns a timeout error, but `ghost_extract` still works

### webrender shader errors

If `cargo test` fails with `couldn't read .../brush_blend_Gl.vert`, the shader build cache is stale:

```bash
cargo clean -p webrender
```

Then rebuild.

### No output from MCP server

The server communicates via stdio. Make sure:
- **stdout** is not redirected somewhere else
- The client sends a valid `initialize` message first
- Logs appear on **stderr** (run with `2>ghost.log` to capture)

### Large pages produce no layout

Some heavy SPAs render nothing until client-side JS completes. Try:
- Increase `--settle` time: `ghost -i --settle 5 <url>`
- Use `ghost_evaluate_js` to check `document.readyState`
- Extract multiple times — the DOM may populate progressively
