<div align="center">

# Ghost Engine

### The AI-Native Browser Engine

**Ultra-fast, zero-dependency headless browser built for AI agents.**<br>
Powered by Mozilla's Servo. Written in Rust. No Chromium required.

[![Rust](https://img.shields.io/badge/Rust-1.86%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MPL%202.0-blue)](LICENSE)
[![MCP](https://img.shields.io/badge/MCP-Compatible-green)](https://modelcontextprotocol.io/)

[Quick Start](#quick-start) \u00b7 [How It Works](#how-it-works) \u00b7 [Documentation](#documentation) \u00b7 [Contributing](#contributing)

</div>

---

## What is Ghost Engine?

Ghost Engine is a **headless browser purpose-built for AI agents**. Unlike tools that wrap Chromium (Puppeteer, Playwright, Browserbase), Ghost Engine compiles Mozilla's core engine components \u2014 HTML parser, CSS engine, JavaScript runtime \u2014 directly into a single lightweight binary and **removes the pixel renderer entirely**.

The result: your AI agent gets structured, accurate web content in **sub-second time**, at a **fraction of Chromium's memory cost**, with **zero browser dependencies** to install.

```
Traditional approach:  URL -> Chromium (1GB+) -> Render pixels -> Screenshot/DOM scrape -> LLM
Ghost Engine:          URL -> Ghost (single binary) -> Layout tree -> Structured Markdown -> LLM
```

---

## Key Features

| Feature | Description |
|---------|-------------|
| **Zero Dependencies** | Single binary. No `playwright install`, no 1.5GB browser download. |
| **Sub-Second Extraction** | 200\u2013800 ms typical page load + content extraction. |
| **Fractional Memory** | ~50\u2013150 MB per page vs. Chromium's 200\u2013500 MB. |
| **Perfect Layout Accuracy** | Reads directly from the engine's computed layout tree \u2014 no guessing. |
| **Full JavaScript Support** | SpiderMonkey (ES2023+) handles React, Vue, Svelte, and SPAs. |
| **AI-Optimized Output** | Structured Markdown or JSON with interactive element IDs. |
| **MCP Server** | 15 tools for Claude, Copilot, and custom agents via Model Context Protocol. |
| **Interactive REPL** | Step-by-step debugging with `ghost -i <url>`. |

---

## Quick Start

### Build from Source

```bash
cd ghost-engine

# Development build
cargo build -p ghost-cli -p ghost-mcp

# Production build (optimized + stripped)
./build-release.sh
```

### Extract a Web Page

```bash
# Markdown output (recommended for LLMs)
ghost --format markdown https://www.wikipedia.org/

# JSON output (structured)
ghost --format json https://news.ycombinator.com/

# Pipe to a file
ghost --format md https://example.com > page.md
```

### Interactive Mode

```bash
ghost -i https://www.wikipedia.org/

ghost> extract markdown
ghost> type 1 Servo browser engine
ghost> click 2
ghost> js document.title
ghost> quit
```

### MCP Server (for AI Agents)

```bash
# Start the MCP server (stdio mode for Claude Desktop / VS Code)
ghost-mcp

# Or HTTP mode for remote access
ghost-mcp --http --port 3100
```

**Claude Desktop** \u2014 add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "ghost": {
      "command": "/path/to/ghost-mcp"
    }
  }
}
```

**VS Code Copilot** \u2014 add to `.vscode/mcp.json`:

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

---

## How It Works

Ghost Engine takes a fundamentally different approach from every other browser automation tool:

### Everyone Else: The "Wrapper" Approach

```
+--------------------------------------+
|       Chromium / Firefox (1GB+)      |
|                                      |
|  HTML -> CSS -> JS -> Layout -> Paint|
|                              ^       |
|              Injected JS (CDP) --+   |
|                   |                  |
|                   v                  |
|            Scraped DOM / Text        |
+------------------+-------------------+
                   v
             AI Agent / LLM
```

1. Launch a massive browser binary
2. Render every pixel to a hidden screen
3. Inject JavaScript to scrape the DOM back out

### Ghost: The "Native Engine" Approach

```
+--------------------------------------+
|     Ghost Engine (Single Binary)     |
|                                      |
|  HTML -> CSS -> JS -> Layout X Paint |
|                      |    (removed)  |
|                      v               |
|              Layout Tree Traversal   |
|                      |               |
|                      v               |
|               Markdown Output        |
+------------------+-------------------+
                   v
             AI Agent / LLM
```

1. Parse HTML, compute CSS, execute JS (using Mozilla's Servo engine)
2. **Stop before painting** \u2014 skip the entire pixel-rendering pipeline
3. Traverse the engine's internal layout tree and serialize directly to Markdown

**Why this matters:**

- **No pixel rendering** -> drastically lower memory and CPU usage
- **Native layout access** -> 100% accurate element positions and visibility
- **No injected JS heuristics** -> no guessing what's visible or clickable
- **No browser binary** -> no 1.5GB dependency to download and manage

---

## Architecture

```
ghost-cli / ghost-mcp          <-- User-facing binaries
       |
   ghost-core                   <-- Servo embedding API
       |
   +---+---+-------+
   |       |        |
ghost- ghost-    ghost-         <-- Feature layers
inter- serial-   interact
ceptor izer
   |       |        |
   +---+---+--------+
       |
  Patched Servo                 <-- Engine (html5ever + Stylo + SpiderMonkey)
  (WebRender removed)
```

| Crate | Purpose |
|-------|---------|
| **ghost-core** | Servo lifecycle management, configuration, session state |
| **ghost-interceptor** | Layout tree extraction from Servo's internal structures |
| **ghost-serializer** | Token-optimized JSON / Markdown with ghost-id injection |
| **ghost-interact** | Click, type, scroll, navigate via SpiderMonkey |
| **ghost-cli** | CLI binary + interactive REPL |
| **ghost-mcp** | MCP server (15 tools, stdio + HTTP transport) |

---

## MCP Tools

Ghost Engine exposes 15 tools via the Model Context Protocol:

| Tool | Description |
|------|-------------|
| `ghost_navigate` | Load a URL, return page metadata |
| `ghost_extract` | Extract page content as Markdown or JSON |
| `ghost_screenshot` | Capture viewport as PNG (base64) |
| `ghost_click` | Click an element by ghost-id |
| `ghost_type` | Type text into an input field |
| `ghost_scroll` | Scroll an element or viewport |
| `ghost_evaluate_js` | Run arbitrary JavaScript |
| `ghost_get_cookies` | List all cookies |
| `ghost_set_cookie` | Set a cookie |
| `ghost_new_tab` | Open a new tab |
| `ghost_switch_tab` | Switch active tab |
| `ghost_close_tab` | Close a tab |
| `ghost_list_tabs` | List all open tabs |
| `ghost_block_urls` | Block URLs by pattern (ads, trackers) |
| `ghost_perf` | Performance report (timing, memory) |

---

## Performance

| Metric | Ghost Engine | Headless Chromium |
|--------|-------------|-------------------|
| **Binary size** | ~50 MB (single file) | ~1.5 GB (browser + deps) |
| **Memory (simple page)** | ~50\u201380 MB | ~200\u2013300 MB |
| **Memory (complex SPA)** | ~100\u2013150 MB | ~400\u2013500 MB |
| **Page extraction** | 200\u2013800 ms | 1\u20133 seconds |
| **Concurrent instances (8GB)** | ~50\u2013100 | ~10\u201315 |
| **Setup** | `cargo build` | `playwright install` (1.5GB download) |

---

## Documentation

| Document | Description |
|----------|-------------|
| **[User Guide](docs/USER_GUIDE.md)** | Complete usage guide \u2014 CLI, REPL, and MCP server setup |
| **[System Architecture](docs/servo_architecture_flow.md)** | Deep technical reference \u2014 component map, request lifecycle, interception pipeline |
| **[Embedding Strategy](docs/servo_embedding_strategy.md)** | Why and how Ghost Engine embeds Servo instead of wrapping Chromium |
| **[Tool Comparison](docs/COMPARISON.md)** | Head-to-head comparison with Puppeteer, Playwright, Browserbase, Skyvern, MultiOn |
| **[Web Compatibility](docs/wpt_compatibility.md)** | Feature-by-feature web platform support matrix and WPT pass rates |
| **[Implementation Roadmap](docs/phase_wise_implementation_roadmap.md)** | Phase-by-phase engineering roadmap with task tracking |

---

## Web Platform Support

Ghost Engine supports the vast majority of features AI agents encounter:

| Category | Support Level |
|----------|:------------:|
| HTML5 / DOM | \u2705 Full |
| CSS (Flexbox, Grid, Animations) | \u2705 Full |
| JavaScript (ES2023+) | \u2705 Full |
| `fetch()` / XHR | \u2705 Full |
| Cookies / localStorage | \u2705 Full |
| React / Vue / Svelte SPAs | \u2705 Full |
| Forms / Interaction | \u2705 Full |
| Service Workers | \u274c Not yet |
| WebRTC / Media | \u274c Not yet |

See the full [Web Compatibility Report](docs/wpt_compatibility.md) for detailed coverage.

---

## Project Structure

```
ai-deep/
|-- README.md                   <-- You are here
|-- docs/                       <-- Documentation
|   |-- USER_GUIDE.md
|   |-- servo_architecture_flow.md
|   |-- servo_embedding_strategy.md
|   |-- COMPARISON.md
|   |-- wpt_compatibility.md
|   |-- phase_wise_implementation_roadmap.md
|-- ghost-engine/               <-- Engine workspace
    |-- Cargo.toml              <-- Workspace root
    |-- build-release.sh        <-- Production build script
    |-- ports/
    |   |-- ghost-cli/          <-- CLI binary
    |   |-- ghost-mcp/          <-- MCP server binary
    |   |-- ghost-core/         <-- Servo embedding library
    |   |-- ghost-interceptor/  <-- Layout tree extraction
    |   |-- ghost-serializer/   <-- JSON/Markdown serialization
    |   |-- ghost-interact/     <-- DOM interaction layer
    |   |-- servoshell/         <-- Patched Servo shell
    |-- components/             <-- Servo engine components
    |-- resources/              <-- Runtime resources
```

---

## Requirements

| Requirement | Version |
|-------------|---------|
| Rust | 1.86+ (pinned via `rust-toolchain.toml`) |
| macOS | ARM64 (Apple Silicon) \u2014 primary target |
| Linux | x86_64 \u2014 supported |

No browser binary, Node.js, Python, or GStreamer required.

---

## Contributing

We welcome contributions! See [CONTRIBUTING.md](ghost-engine/CONTRIBUTING.md) for guidelines.

### Development Build

```bash
cd ghost-engine
cargo build -p ghost-cli -p ghost-mcp
```

### Run Tests

```bash
cd ghost-engine
cargo test --workspace
```

### Production Build

```bash
cd ghost-engine
./build-release.sh          # Build optimized binaries
./build-release.sh --verify # Verify build artifacts
```

---

## License

This project is licensed under the [MPL 2.0](LICENSE) license.

Ghost Engine builds upon [Mozilla Servo](https://servo.org/), which is licensed under the MPL 2.0.
