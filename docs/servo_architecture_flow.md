# Ghost Engine — System Architecture

> Deep technical reference for how Ghost Engine embeds and modifies Mozilla's Servo
> rendering engine to produce an AI-optimized headless browser.

---

## Table of Contents

- [High-Level Architecture](#high-level-architecture)
- [Component Map](#component-map)
- [Request Lifecycle](#request-lifecycle)
- [The Interception Pipeline](#the-interception-pipeline)
- [Serialization Layer](#serialization-layer)
- [Interaction Model](#interaction-model)
- [MCP Server Architecture](#mcp-server-architecture)
- [Multi-Tab & Session Management](#multi-tab--session-management)
- [Performance Architecture](#performance-architecture)
- [Build & Binary Layout](#build--binary-layout)

---

## High-Level Architecture

Ghost Engine is a **modified Servo embedding** — not a wrapper around a compiled browser binary. It compiles Mozilla’s core engine components (HTML parser, CSS engine, JS engine) directly into its own Rust binary and replaces the pixel-rendering pipeline with a structured data extraction layer.

```
+------------------------------------------------------------------+
|                        Ghost Engine                              |
|                                                                  |
|  +-----------+   +-----------+   +-----------+                   |
|  | ghost-cli  |  | ghost-mcp |   |  Your App |  <-- Consumers   |
|  | (binary)   |  | (binary)  |   | (library) |                  |
|  +-----+------+  +-----+-----+  +-----+-----+                  |
|        |              |              |                            |
|        +--------------+--------------+                           |
|                       |                                          |
|                 +-----v------+                                   |
|                 | ghost-core |  <-- Embedding API                |
|                 +-----+------+                                   |
|                       |                                          |
|       +---------------+----------------+                         |
|       |               |                |                         |
| +-----v-------+ +-----v------+ +------v--------+                |
| |   ghost-    | |   ghost-   | |    ghost-      |                |
| | interceptor | | serializer | |   interact     |                |
| +-----+-------+ +-----+------+ +------+--------+                |
|       |               |                |                         |
| +-----v---------------v----------------v--------------------+    |
| |                    Patched Servo                           |    |
| |  +----------+  +--------+  +------------+  +----------+   |    |
| |  | html5ever|  | Stylo  |  |SpiderMonkey|  |   net/   |   |    |
| |  | (parser) |  | (CSS)  |  |   (JS)     |  |  (HTTP)  |   |    |
| |  +----------+  +--------+  +------------+  +----------+   |    |
| |                                                            |    |
| |  X WebRender (REMOVED -- no pixel painting)                |    |
| +------------------------------------------------------------+    |
+------------------------------------------------------------------+
```

---

## Component Map

### Core Crates (Ghost Engine)

| Crate | Type | Purpose |
|-------|------|---------|
| `ghost-core` | Library | Servo embedding API — lifecycle, configuration, session state |
| `ghost-interceptor` | Library | Layout tree extraction from Servo's internal structures |
| `ghost-serializer` | Library | Token-optimized JSON / Markdown output with ghost-id injection |
| `ghost-interact` | Library | DOM interaction — click, type, scroll, navigate via SpiderMonkey |
| `ghost-cli` | Binary | One-shot CLI and interactive REPL front-end |
| `ghost-mcp` | Binary | MCP server exposing 15 tools over JSON-RPC (stdio + HTTP) |

### Servo Components (Embedded)

| Component | Role in Ghost Engine |
|-----------|---------------------|
| **html5ever** | HTML parsing — spec-compliant tokenizer and tree builder |
| **Stylo** | CSS engine — selector matching, cascade, computed styles, layout |
| **SpiderMonkey** | JavaScript engine — full ES2023+, JIT compilation |
| **net** | HTTP stack — TLS, CORS, cookies, HTTP/2 |
| **script** | DOM bindings — bridges JS to Rust DOM implementation |
| **constellation** | Process/pipeline orchestrator — manages page lifecycle |
| **compositing** | Event dispatch — adapted for headless; no GPU compositing |

### Removed / Disabled

| Component | Why Removed |
|-----------|-------------|
| **WebRender** | GPU pixel-painting pipeline — AI agents don't need pixels |
| **Winit / Glutin** | Window creation and OpenGL context — no visible window |
| **surfman** | GPU surface management — no rendering surfaces needed |
| **GStreamer** | Media playback — `media-stack=dummy` for headless operation |

---

## Request Lifecycle

When Ghost Engine processes a URL, it follows this pipeline:

```
URL Input
    |
    v
+---------------------------------------------+
| 1. NETWORK FETCH                            |
|    net/ -> DNS -> TLS -> HTTP/2 -> Response  |
|    Cookies, CORS, redirects handled here     |
+-----------------+---------------------------+
                  | HTML bytes
                  v
+---------------------------------------------+
| 2. HTML PARSING                             |
|    html5ever -> Token stream -> DOM Tree     |
|    <script> tags trigger JS evaluation       |
+-----------------+---------------------------+
                  | DOM Tree
                  v
+---------------------------------------------+
| 3. CSS COMPUTATION                          |
|    Stylo -> Selector matching -> Cascade     |
|    -> Computed styles for every DOM node      |
+-----------------+---------------------------+
                  | Styled DOM
                  v
+---------------------------------------------+
| 4. LAYOUT                                   |
|    Stylo -> Box generation -> Flexbox/Grid   |
|    -> Absolute positions (x, y, w, h)        |
|    for every visible node                    |
+-----------------+---------------------------+
                  | Layout Tree (geometry)
                  v
+---------------------------------------------+
| 5. X PAINT (REMOVED)                        |
|    WebRender would rasterize to pixels here  |
|    Ghost Engine stops the pipeline instead   |
+-----------------+---------------------------+
                  |
                  v
+---------------------------------------------+
| 6. GHOST INTERCEPTION                       |
|    ghost-interceptor traverses the layout    |
|    tree via JS evaluation, extracts:         |
|    - Tag, ID, classes, ARIA attributes       |
|    - Bounding box (x, y, width, height)      |
|    - Text content, href, src, alt            |
|    - Visibility state (computed)             |
+-----------------+---------------------------+
                  | Raw layout nodes
                  v
+---------------------------------------------+
| 7. GHOST SERIALIZATION                      |
|    ghost-serializer:                         |
|    - Filters invisible/zero-size nodes       |
|    - Assigns sequential ghost-ids            |
|    - Outputs JSON or Markdown                |
+-----------------+---------------------------+
                  | Structured output
                  v
            AI Agent / LLM
```

### Timing Breakdown (Typical Page)

| Phase | Typical Duration | Notes |
|-------|-----------------|-------|
| Network fetch | 100–500 ms | Depends on server and network latency |
| HTML parse | 5–20 ms | html5ever is extremely fast |
| CSS computation | 10–50 ms | Stylo parallelizes across CPU cores |
| Layout | 10–40 ms | Single pass for most pages |
| JS execution | 50–2000 ms | Heavy SPAs take longer |
| Ghost interception | 5–15 ms | Direct tree traversal |
| Serialization | 2–10 ms | In-memory string building |
| **Total (typical)** | **200–800 ms** | Sub-second for most sites |

---

## The Interception Pipeline

Ghost Engine's key innovation is **where** it extracts data: directly from Servo's internally computed layout tree — not by injecting JavaScript to reverse-engineer the rendering from the outside.

### How `ghost-interceptor` Works

```
                    Servo's Internal State
                    ----------------------
                           |
        +------------------+------------------+
        |                  |                  |
   DOM Tree          Style System        Layout Tree
   (html5ever)       (Stylo)            (Box Tree)
        |                  |                  |
        v                  v                  v
  +----------+      +----------+      +------------+
  | Elements |      | Computed |      |  Fragment   |
  | Attrs    |      | Styles   |      |  Geometry   |
  | Text     |      | Display  |      |  x,y,w,h   |
  +----------+      | Visibility|     +------------+
                    | Opacity  |
                    +----------+
                           |
                           v
                 +-------------------+
                 | ghost-interceptor |
                 |                   |
                 |  Evaluates JS in  |
                 |  page context to  |
                 |  traverse DOM +   |
                 |  computed styles  |
                 |  + geometry       |
                 +---------+---------+
                           |
                           v
                   Raw LayoutNode[]
                   {tag, id, classes,
                    x, y, w, h, text,
                    visible, interactive}
```

### Visibility Filtering

Ghost Engine applies a multi-layer visibility filter to exclude nodes that a human wouldn't see:

| Filter | What It Catches |
|--------|----------------|
| `display: none` | Elements removed from layout entirely |
| `visibility: hidden` | Elements that occupy space but are invisible |
| `opacity: 0` | Fully transparent elements |
| `width == 0` or `height == 0` | Collapsed elements (common in trackers) |
| Off-screen position | Elements positioned outside the viewport |
| Clipped by `overflow: hidden` | Elements hidden by parent clipping |
| `aria-hidden="true"` | Elements explicitly marked as hidden |

### Wait-for-Selector (SPA Support)

Modern SPAs render content asynchronously. Ghost Engine provides a Playwright-style wait mechanism:

```
ghost-core: wait_for_selector("div.results", timeout=10s)
    |
    +-> Poll layout tree every 100ms
    +-> Check if matching element exists AND is visible
    +-> If found -> return immediately
    +-> If timeout -> return partial result + warning
```

This is critical for pages like React apps that render a loading spinner first, then swap in real content after API calls complete.

---

## Serialization Layer

`ghost-serializer` transforms raw layout nodes into formats optimized for LLM consumption.

### JSON Format

Minimal, token-efficient structure. Every interactive element gets a `ghost-id` for agent interaction.

```json
{
  "url": "https://example.com",
  "title": "Example Page",
  "viewport": { "width": 1920, "height": 1080 },
  "nodes": [
    {
      "ghost-id": 1,
      "tag": "input",
      "type": "text",
      "placeholder": "Search...",
      "bounds": { "x": 100, "y": 200, "w": 300, "h": 40 },
      "interactive": true
    },
    {
      "ghost-id": 2,
      "tag": "button",
      "text": "Submit",
      "bounds": { "x": 410, "y": 200, "w": 80, "h": 40 },
      "interactive": true
    }
  ]
}
```

### Markdown Format

Semantic, human-readable output. Ghost-ids appear as bracketed annotations on interactive elements.

```markdown
# Example Page

[1] Search... (input)
[2] Submit (button)

## Results

- Item one — $19.99
  [3] Add to Cart (button)
- Item two — $24.99
  [4] Add to Cart (button)
```

### Ghost-ID System

Every interactive element (links, buttons, inputs, selects, checkboxes) receives a sequential integer ID. These IDs are:

- **Stable within a single extraction** — the same element always has the same ID in one snapshot
- **Ephemeral across extractions** — IDs may change after navigation or DOM mutation
- **Used for agent commands** — `click 3`, `type 1 hello`, `select 5 option-value`

```
Extraction 1:           Extraction 2 (after click):
[1] Search input        [1] Search input (same)
[2] Submit button       [2] Submit button (same)
                        [3] Result link (new)
                        [4] Result link (new)
```

---

## Interaction Model

`ghost-interact` maps high-level agent commands to native DOM events via SpiderMonkey's JS evaluation API.

### Command -> Event Flow

```
Agent Command            Ghost Engine                 Servo/SpiderMonkey
-------------            ------------                 ------------------
click(3)         ->  Lookup ghost-id 3          ->  document.querySelector(...)
                    in ID cache                    .dispatchEvent(new MouseEvent('click'))
                                                   |
                                                   v
                                               Event handlers fire
                                               DOM may update
                                                   |
                                                   v
                                               Re-extract layout
                                                   |
                                                   v
                                               Return updated tree
```

### Supported Interactions

| Command | Synthesized Events | Notes |
|---------|-------------------|-------|
| `click(id)` | `mousedown` -> `mouseup` -> `click` | Full click sequence |
| `type(id, text)` | `focus` -> `keydown` -> `input` -> `keyup` per char | Triggers React onChange |
| `key(id, key)` | `keydown` -> `keypress` -> `keyup` | Special keys: Enter, Tab, Escape |
| `hover(id)` | `mouseenter` -> `mouseover` | Triggers CSS `:hover` and JS handlers |
| `scroll(id)` | `scrollIntoView()` + `scroll` event | Scrolls element into viewport |
| `scrollby(dx, dy)` | `window.scrollBy()` + `scroll` event | Viewport-level scrolling |
| `select(id, value)` | Set `.value` + `change` event | Dropdown selection |
| `check(id)` / `uncheck(id)` | Set `.checked` + `change` event | Checkbox / radio toggle |
| `navigate(url)` | Full navigation cycle | Resets DOM, re-loads |
| `back()` / `forward()` | `history.back()` / `history.forward()` | Session history navigation |
| `reload()` | Full page reload | Clears and reloads |

### Re-Extraction After Interaction

After every interaction, Ghost Engine:

1. Waits for the DOM to settle (configurable quiet period)
2. Re-runs the interception pipeline
3. Assigns fresh ghost-ids
4. Returns the updated serialized layout

This gives the agent an always-current view of the page state.

---

## MCP Server Architecture

`ghost-mcp` exposes Ghost Engine as a set of 15 tools over the Model Context Protocol.

```
+--------------------------------------------------------------+
|                         ghost-mcp                            |
|                                                              |
|  +--------------------------------------------------------+  |
|  |                    Transport Layer                      |  |
|  |  +----------+  +----------------+  +----------------+  |  |
|  |  |  Stdio   |  |  Streamable    |  |   SSE          |  |  |
|  |  | (local)  |  |  HTTP          |  |  (legacy)      |  |  |
|  |  +----+-----+  +-------+--------+  +--------+-------+  |  |
|  |       +-----------------+--------------------+          |  |
|  +-------------------------+---------------------------+---+  |
|                            | JSON-RPC 2.0                    |
|  +-------------------------v---------------------------+     |
|  |                    Tool Router                      |     |
|  |                                                     |     |
|  |  Navigation:  ghost_navigate, ghost_extract         |     |
|  |               ghost_screenshot                      |     |
|  |  Interaction: ghost_click, ghost_type, ghost_scroll |     |
|  |  JavaScript:  ghost_evaluate_js, ghost_get_cookies  |     |
|  |               ghost_set_cookie                      |     |
|  |  Multi-Tab:   ghost_new_tab, ghost_switch_tab       |     |
|  |               ghost_close_tab, ghost_list_tabs      |     |
|  |  Network:     ghost_block_urls, ghost_perf          |     |
|  +-------------------------+---------------------------+     |
|                            |                                 |
|  +-------------------------v---------------------------+     |
|  |                 Session Manager                     |     |
|  |  +---------+  +---------+  +---------+              |     |
|  |  | Tab 0   |  | Tab 1   |  | Tab N   |              |     |
|  |  | (active)|  |         |  |         |              |     |
|  |  +----+----+  +---------+  +---------+              |     |
|  +-------+------------------------------------------------+  |
|          |                                                   |
|  +-------v------------------------------------------------+  |
|  |                 GhostEngine Instance                    |  |
|  |  ghost-core -> interceptor -> serializer -> interact   |  |
|  +---------------------------------------------------------+  |
+--------------------------------------------------------------+
```

### Transport Modes

| Mode | Endpoint | Use Case |
|------|----------|----------|
| **Stdio** | stdin/stdout | Local agents (Claude Desktop, VS Code Copilot) |
| **Streamable HTTP** | `POST /mcp` | Remote agents, cloud deployments |
| **SSE** (legacy) | `GET /sse` + `POST /messages` | Older MCP clients |

### Error Handling

Every tool returns structured errors that agents can reason about:

| Error Category | Example | Agent Can... |
|---------------|---------|-------------|
| `ElementNotFound` | ghost-id doesn't exist | Re-extract and retry with correct ID |
| `NavigationTimeout` | Page took too long | Increase timeout or try different URL |
| `JavaScriptError` | JS evaluation failed | Adjust the script |
| `Panic` | Servo hit unsupported API | Skip this page, report to user |
| `SessionExpired` | Tab was closed | Open a new tab |

---

## Multi-Tab & Session Management

Ghost Engine maintains stateful browser sessions across tool calls:

```
MCP Connection
    |
    v
+--------------------------------+
|         Browser Session         |
|                                 |
|  Shared: Cookies, DNS cache,    |
|          Connection pool        |
|                                 |
|  +-----+  +-----+  +-----+    |
|  |Tab 0|  |Tab 1|  |Tab 2|    |
|  | DOM |  | DOM |  | DOM |    |
|  | JS  |  | JS  |  | JS  |    |
|  |State|  |State|  |State|    |
|  +-----+  +-----+  +-----+    |
|                                 |
|  Active tab: Tab 0              |
+--------------------------------+
```

- Each MCP connection gets an **isolated session**
- Cookies, JS state, and DOM persist across tool calls within a session
- Multiple tabs share a connection pool and cookie jar
- Resources are cleaned up when the connection closes

---

## Performance Architecture

### Resource Budgeting

Ghost Engine can selectively skip expensive resource types:

```rust
ResourceBudget {
    skip_images: true,      // Don't download images
    skip_fonts: true,       // Use system fonts only
    skip_media: true,       // Skip video/audio
    skip_stylesheets: false, // CSS is needed for layout
    max_resource_bytes: 5MB, // Cap per-resource download size
}
```

This dramatically reduces load time and memory for agent workflows where visual fidelity doesn't matter.

### Memory Profile

| Component | Typical RSS | Notes |
|-----------|-------------|-------|
| Servo core (idle) | ~30 MB | Engine initialized, no page |
| Simple page loaded | ~50–80 MB | Static HTML + CSS + minimal JS |
| Complex SPA | ~100–150 MB | React/Vue app with full JS execution |
| **Chromium comparison** | ~200–500 MB | Same pages in headless Chrome |

### Connection Optimization

- **HTTP/2 multiplexing** — multiple resources over one connection
- **Keep-Alive** — reuse TCP connections across requests within a session
- **OS DNS cache** — avoids redundant DNS lookups

---

## Build & Binary Layout

### Workspace Structure

```
ghost-engine/
|-- Cargo.toml              <-- Workspace root
|-- build-release.sh        <-- Production build script
|-- ports/
|   |-- ghost-cli/          <-- CLI binary
|   |-- ghost-mcp/          <-- MCP server binary
|   |-- ghost-core/         <-- Core embedding library
|   |-- ghost-interceptor/  <-- Layout extraction
|   |-- ghost-serializer/   <-- JSON/Markdown output
|   |-- ghost-interact/     <-- DOM interaction
|   |-- servoshell/         <-- Patched Servo shell
|-- components/             <-- Servo engine components
|   |-- script/             <-- DOM + JS bindings
|   |-- layout/             <-- Layout engine
|   |-- net/                <-- HTTP stack
|   |-- fonts/              <-- Font loading
|   |-- ...                 <-- 30+ more components
|-- resources/              <-- Runtime resources
```

### Build Profiles

| Profile | Use Case | Flags |
|---------|----------|-------|
| `dev` | Local development | Debug symbols, no optimization |
| `release` | Testing | Optimized, debug symbols retained |
| `production-stripped` | Distribution | LTO, `codegen-units=1`, `opt-level=s`, stripped |

### Produced Binaries

| Binary | Description | Typical Size (stripped) |
|--------|-------------|----------------------|
| `ghost` | CLI + REPL | ~40–60 MB |
| `ghost-mcp` | MCP server | ~40–60 MB |

Both are **single, self-contained executables**. No browser download required. No runtime dependencies beyond system libc and TLS libraries.
