# Ghost Engine — Implementation Roadmap

## Overview

**Ghost Engine** is an ultra-fast, low-memory headless browser purpose-built for AI agents. It embeds Mozilla's Servo rendering engine to deliver sub-second page loads and structured content extraction at a fraction of Chromium's resource cost.

**What it does:**
- Loads any URL and extracts a structured, token-optimized layout tree (JSON / Markdown)
- Enables agents to interact with pages — click, type, scroll, navigate — via CLI commands or MCP tool-calls
- Runs as a **single binary**: either a one-shot CLI (`ghost <url>`) or a persistent MCP server for multi-turn agent workflows

**How this roadmap is organized:**
Each phase is split into parallel **Engineering Tracks** (A: Systems, B: Backend/API, C: QA/DevOps) with individually assignable subtask IDs (`TSK-X.Y`).

> **Compatibility note:** Servo provides ~60-70% baseline web compatibility. Fallback handling and compatibility benchmarking are built into the plan at each phase.

### Core Delivery Goals
| # | Goal | Validation |
|---|------|------------|
| G1 | Page load + layout extraction in **< 500 ms** for typical pages | Benchmark suite in Phase 3 |
| G2 | Resident memory **< 150 MB** for a single-page session | RSS measurement in Phase 3 |
| G3 | **CLI** mode: `ghost <url>` → structured output to stdout | Phase 1 |
| G4 | **MCP server** mode: persistent process serving agent tool-calls | Phase 4 |
| G5 | Full action loop: load → extract → click/type/scroll → re-extract | Phase 3-4 |

---

## Phase 1: The Embedding Sandbox (Month 1)
**Focus:** Infrastructure, build systems, and basic headless embedding.
**Milestone 1:** Successfully run a headless CLI against a supported React/Vue SPA and verify no crashes via SpiderMonkey.

### Track A: Base Engine Construction (Role: Systems Engineer)
*   [x] **TSK-1.1:** Clone Servo repository, set up Rust toolchain, and compile the baseline engine.
*   [x] **TSK-1.2:** Research Servo's initialization process and identify Winit/Glutin windowing dependencies.
*   [x] **TSK-1.3:** Fork/patch Servo locally to dynamically disable Winit/Glutin initialization.
*   [x] **TSK-1.4:** Identify and strip out WebRender surface pipelines and GPU context creation.
*   [x] **TSK-1.5:** Resolve compilation errors and lifetimes broken by stripping windowing/rendering.
	Validation: `cargo build -p servoshell --features headless-shell` now succeeds again; remaining output is limited to dead-code warnings in shared headed/headless code paths.

### Track B: Rust Scaffolding & Integration (Role: Backend/CLI Engineer)
*   [x] **TSK-1.6:** Scaffold `ghost-core` crate and link it to the local patched Servo build.
*   [x] **TSK-1.7:** Write the initial `ghost-core` API wrapper to launch Servo in headless mode.
*   [x] **TSK-1.8:** Scaffold `ghost-cli` crate and set up CLI argument parsing (e.g., `--url`).
*   [x] **TSK-1.9:** Connect `ghost-cli` to `ghost-core`, passing the URL into Servo's network pipeline.
*   [x] **TSK-1.10:** Implement a basic background event loop so it doesn't terminate prematurely during JS execution.
*   [x] **TSK-1.11:** Hook into Servo's document load events to detect `DOMContentLoaded` or load completion.

### Track C: QA & Benchmarking (Role: Testing/QA Engineer)
*   [x] **TSK-1.12:** Build a test suite of HTML files (Tier 1: Static, Tier 2: Vanilla JS, Tier 3: Basic React).
*   [x] **TSK-1.13:** Implement crash recovery testing for unsupported Tier 4 sites (Heavy SPAs).

---

## Phase 2: The Interception & Layout Traversal (Month 2)
**Focus:** Hooking into the rendering pipeline and extracting raw data.
**Milestone 2:** Output a raw Rust textual dump of the computed and visible layout bounds.

### Track A: Interception Hooks (Role: Systems Engineer)
*   [x] **TSK-2.1:** Scaffold the `ghost-interceptor` crate.
*   [x] **TSK-2.2:** Find the exact pipeline hook where Stylo finishes layout computations.
*   [x] **TSK-2.3:** Implement a Rust trait/callback in `ghost-core` attached to the post-layout phase.
*   [x] **TSK-2.4:** Integrate the compiled interceptor callback back into the `ghost-cli` run loop.

### Track B: Traversal & Extraction (Role: Algorithm Engineer)
*   [x] **TSK-2.5:** Define the Rust enum/struct to represent a simplified Layout Node (Tag, ID, X, Y, Width, Height, Text).
*   [x] **TSK-2.6:** Write the recursive function to walk Servo's internal Layout Tree.
*   [x] **TSK-2.7:** Extract tag names, text content, and DOM attributes during traversal.
*   [x] **TSK-2.8:** Extract computed geometry (X, Y, Box Rectangles) during traversal.
*   [x] **TSK-2.8b:** Implement arbitrary JavaScript evaluation via `WebView` (`evaluate_js(code) -> Result<JsValue>`), required for advanced selectors and agent-side DOM queries.

### Track C: Rules & Filtering (Role: Backend/QA Engineer)
*   [x] **TSK-2.9:** Implement visibility filtering part 1: `display: none` and `visibility: hidden`.
*   [x] **TSK-2.10:** Implement visibility filtering part 2: `width == 0` or `height == 0`.
*   [x] **TSK-2.11:** Implement visibility filtering part 3: `opacity: 0` or off-screen nodes.
*   [x] **TSK-2.11b:** Implement `wait_for_selector(css, timeout)` — Playwright-style API to block until a matching element appears in the layout tree (needed for SPAs that lazy-render).
*   [x] **TSK-2.12:** Write snapshot tests validating correct layout extraction on Phase 1 test sites.

---

## Phase 3: Serialization, Interaction & Compatibility (Month 3)
**Focus:** AI-Agent UX, graceful fallbacks, and dynamic control.
**Milestone 3:** Full loop demonstration: load site -> export tree -> issue `click(id)` -> export updated tree.

### Track A: Serialization Pipeline (Role: Backend Engineer)
*   [x] **TSK-3.1:** Scaffold the `ghost-serializer` crate.
*   [x] **TSK-3.2:** Compress the raw layout tree into strict, minimal LLM JSON format.
*   [x] **TSK-3.3:** Compress the raw layout tree into semantic Markdown format.
*   [x] **TSK-3.4:** Inject mutation: identify interactive elements and map them with sequential `ghost-id`s.

### Track B: API & SpiderMonkey Interaction (Role: Systems Engineer)
*   [x] **TSK-3.5:** Scaffold the `ghost-interact` crate and define API commands (`Click(id)`, `Type(id, text)`, `Scroll(id, direction)`, `Hover(id)`, `Select(id, value)`, `Focus(id)`).
*   [x] **TSK-3.6:** Bridge Rust→SpiderMonkey: FFI logic to fire synthetic `MouseEvent` on targeted nodes (click, hover, mousedown/up).
*   [x] **TSK-3.7:** Bridge Rust→SpiderMonkey: FFI logic to fire synthetic `KeyboardEvent` and `InputEvent` triggers (type text, press keys).
*   [x] **TSK-3.7b:** Implement `scroll_to(id)` / `scroll_by(dx, dy)` — fire `ScrollEvent` and re-trigger layout for lazy-loaded content.
*   [x] **TSK-3.7c:** Implement `select_option(id, value)` for `<select>` dropdowns and `check(id)` / `uncheck(id)` for checkboxes/radios.
*   [x] **TSK-3.8:** Implement lookup cache mapping `ghost-id` back to the internal Servo DOM pointers.
*   [x] **TSK-3.8b:** Implement page navigation support: `navigate(url)`, `go_back()`, `go_forward()`, `reload()` — monitor `LoadStatus` transitions across navigations and re-extract layout after each.
*   [x] **TSK-3.8c:** Implement cookie and session state management: `get_cookies()`, `set_cookie(name, value, domain)`, `clear_cookies()` — required for authenticated agent workflows.
*   [x] **TSK-3.8d:** Implement HTTP header injection: custom `User-Agent`, `Authorization`, and arbitrary request headers set per-session — needed for API-gated sites.
*   [x] **TSK-3.9:** Add interactive stdin command loop to `ghost-cli` for real-time testing.

### Track C: Release & Compatibility Benchmarking (Role: Full-Stack / DevOps)
*   [x] **TSK-3.10:** Build final deployment pipeline (LTO optimizations, stripping binaries).
*   [x] **TSK-3.10b:** Performance benchmark suite: measure page-load latency (target G1: < 500 ms), peak RSS memory (target G2: < 150 MB), and layout-extraction throughput across Tier 1-4 fixtures. Record baselines.
*   [x] **TSK-3.10c:** Startup-time optimization: measure and reduce cold-start overhead (Servo init, SpiderMonkey JIT warm-up). Target: < 200 ms to first `spin_event_loop`.
*   [x] **TSK-3.11:** Automate Rust thread panic handling so unsupported DOM APIs return clean semantic errors to LLMs.
*   [x] **TSK-3.12:** Extensive WPT (Web Platform Tests) evaluation: Finalize the "Supported/Unsupported" documentation.

---

## Phase 4: MCP Server & Agent Integration (Month 4)
**Focus:** Expose Ghost Engine as a persistent MCP tool-server so AI agents (Claude, GPT, custom) can browse, extract, and interact with websites over the Model Context Protocol.
**Milestone 4:** An AI agent loads a real website via MCP, reads structured content, clicks a button, and reads the updated page — end-to-end in a single conversation turn.

### Track A: MCP Server Core (Role: Backend Engineer)
*   [x] **TSK-4.1:** Design the MCP tool schema: define tools (`ghost_navigate`, `ghost_extract`, `ghost_click`, `ghost_type`, `ghost_scroll`, `ghost_screenshot`, `ghost_evaluate_js`, `ghost_get_cookies`, `ghost_set_cookie`) with typed input/output schemas.
*   [x] **TSK-4.2:** Scaffold MCP server binary (e.g. `ghost-mcp`) using `stdio` transport (JSON-RPC over stdin/stdout). Depend on a lightweight MCP SDK or implement the small protocol surface directly.
*   [x] **TSK-4.3:** Implement `ghost_navigate` tool: accepts URL, returns load status + page title + URL + timing.
*   [x] **TSK-4.4:** Implement `ghost_extract` tool: returns the serialized layout tree (JSON or Markdown, agent-selectable format).
*   [x] **TSK-4.5:** Implement `ghost_click` / `ghost_type` / `ghost_scroll` action tools: accept `ghost-id`, perform interaction, re-extract layout, return updated tree.
*   [x] **TSK-4.6:** Implement `ghost_evaluate_js` tool: execute arbitrary JS in page context and return serialized result (with sandboxing and timeout).
*   [x] **TSK-4.7:** Implement `ghost_screenshot` tool: capture current viewport as PNG via `SoftwareRenderingContext` and return base64-encoded image.

### Track B: Session & State Management (Role: Systems Engineer)
*   [x] **TSK-4.8:** Implement persistent browser session: a single `GhostEngine` instance keeps DOM, cookies, and JS state alive across multiple MCP tool calls within one agent conversation.
*   [x] **TSK-4.9:** Implement multi-tab support: `ghost_new_tab(url)` and `ghost_switch_tab(id)` so agents can work with multiple pages simultaneously.
*   [x] **TSK-4.10:** Implement session isolation and cleanup: each MCP connection gets an independent browser context; resources are freed when the connection closes.
*   [x] **TSK-4.11:** Add network request interception: `ghost_block_urls(patterns)` to block ads, trackers, or unnecessary resources — reduces load time and memory for agent workflows.

### Track C: Agent UX & Reliability (Role: QA / Integration Engineer)
*   [x] **TSK-4.12:** Write end-to-end MCP integration tests: simulate an agent conversation that navigates, extracts, clicks, and re-extracts using the MCP tools.
*   [x] **TSK-4.13:** Implement graceful error reporting: every MCP tool returns structured errors (timeout, crash, element-not-found, JS error) that agents can reason about.
*   [x] **TSK-4.14:** Add an MCP resource for enumerating supported capabilities and limitations, so agents can self-discover what the browser can and cannot do.
*   [x] **TSK-4.15:** Write user-facing documentation: MCP server setup guide, `claude_desktop_config.json` / `mcp.json` examples, and supported tool reference.

---

## Phase 5: Hardening, Performance & Production (Month 5)
**Focus:** Production-grade reliability, real-world compatibility, and performance validation against stated goals.
**Milestone 5:** Ghost Engine runs reliably on top-100 websites with sub-second extraction, < 150 MB RSS, and zero unhandled panics.

### Track A: Real-World Compatibility (Role: Systems Engineer)
*   [x] **TSK-5.1:** Test against the top-25 most-visited websites and categorize results: full support / partial / unsupported.
*   [x] **TSK-5.2:** Implement iframe traversal: extract layout from nested iframes and merge into the parent layout tree with scoped `ghost-id` ranges.
*   [x] **TSK-5.3:** Handle `<shadow-dom>` / Web Components: traverse shadow roots during layout extraction.
*   [x] **TSK-5.4:** Improve SPA support: detect client-side route changes (history.pushState / popstate) and re-trigger layout extraction without full reload.

### Track B: Performance & Memory (Role: Performance Engineer)
*   [x] **TSK-5.5:** Profile and optimize page-load pipeline: `LoadTiming` struct with nav/head/sub/total breakdown, `PerfReport` snapshot, `ghost_perf` MCP tool, `perf_bench` example binary.
*   [x] **TSK-5.6:** Profile and optimize memory: `current_rss_bytes()` via platform syscalls (macOS `task_info`, Linux `/proc/self/statm`), RSS included in `PerfReport`.
*   [x] **TSK-5.7:** Implement resource budgeting: `ResourceBudget` struct with skip_images/fonts/media/stylesheets and max_resource_bytes; Accept-header + URL-extension filtering in `load_web_resource()`; atomic blocked/saved counters.
*   [x] **TSK-5.8:** Implement connection pooling and DNS caching: `connection_timeout`, `http_cache_enabled`, `http_cache_size` plumbed to Servo `Preferences`; Hyper built-in Keep-Alive + HTTP/2 multiplexing; OS-level DNS caching.

### Track C: Packaging & Distribution (Role: DevOps)
*   [x] **TSK-5.9:** Produce statically-linked release binaries for macOS ARM64: enhanced `build-release.sh` builds both `ghost` and `ghost-mcp` with `production-stripped` profile (LTO, codegen-units=1, opt-level=s, strip=true). `--verify` checks Mach-O architecture, stripping, dylib deps, and smoke test. `--package` creates versioned `dist/` tarball.
*   [x] **TSK-5.10:** Ghost Engine CI workflow (`.github/workflows/ghost-ci.yml`): 5-job pipeline — build check, unit tests (nextest), Tier 1-3 fixture integration tests, performance benchmarks with regression thresholds (init < 2s, Tier 1 load < 3s, RSS < 300 MB), and release build + deployment verification. Artifacts uploaded for all jobs.
