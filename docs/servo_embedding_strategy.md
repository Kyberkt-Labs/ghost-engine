# Ghost Engine — Servo Embedding Strategy

> How and why Ghost Engine embeds Mozilla's Servo engine instead of wrapping
> Chromium, and the technical decisions behind each layer of the integration.

---

## Table of Contents

- [Executive Summary](#executive-summary)
- [Why Servo?](#why-servo)
- [The Embedding Model](#the-embedding-model)
- [Engine Modifications](#engine-modifications)
- [The Interception Point](#the-interception-point)
- [SpiderMonkey Integration](#spidermonkey-integration)
- [Headless Execution Model](#headless-execution-model)
- [Dependency Management](#dependency-management)
- [Compatibility Trade-offs](#compatibility-trade-offs)
- [Future Embedding Directions](#future-embedding-directions)

---

## Executive Summary

Building a browser engine from scratch — network stack, HTML parser, CSS engine, JavaScript runtime — is a multi-year, multi-million dollar effort. Ghost Engine takes a fundamentally different approach: it **embeds and surgically modifies** Mozilla's Servo engine to create a headless browser purpose-built for AI agents.

The core insight: AI agents don't need pixels. They need **structured, accurate representations of web content**. By removing Servo's rendering pipeline and replacing it with a data extraction layer, Ghost Engine delivers:

- **Sub-second page extraction** (200–800 ms typical)
- **Fractional memory footprint** (~50–150 MB vs. Chromium's 200–500 MB)
- **Zero external dependencies** (single binary, no browser download)
- **Perfect layout accuracy** (data comes from the engine's own calculations)

---

## Why Servo?

### The Selection Criteria

When evaluating existing browser engines for embedding, four criteria mattered:

| Criterion | Chromium | Firefox (Gecko) | WebKit | **Servo** |
|-----------|----------|-----------------|--------|-----------|
| **Language** | C++ | C++ | C++ | **Rust** |
| **Designed for embedding** | No (monolithic) | No (tightly coupled) | Partial (WebKitGTK) | **Yes (crate-based)** |
| **Rendering removable** | Extremely difficult | Extremely difficult | Difficult | **Straightforward** |
| **Build complexity** | 30+ GB, custom toolchain | 20+ GB, Mozilla build system | 10+ GB, CMake | **Standard cargo build** |
| **Memory safety** | Manual C++ | Manual C++ | Manual C++ | **Rust ownership model** |
| **JS engine** | V8 | SpiderMonkey | JavaScriptCore | **SpiderMonkey** |
| **CSS engine** | Blink | Gecko | WebKit | **Stylo** (same as Firefox) |

### Why Not Just Use Chromium?

Chromium is designed as a **monolithic application**, not a library. Headless Chrome still:

1. Initializes the GPU process (even if unused)
2. Allocates full rendering pipelines per tab
3. Runs the compositor thread continuously
4. Requires the complete 1GB+ binary distribution

Removing these components from Chromium would require forking millions of lines of C++ with tightly coupled dependencies. This is not viable.

### Servo's Unique Advantages

**Servo was designed from the ground up as a collection of independent, embeddable Rust crates.** This is not a happy accident — it was Mozilla's explicit research goal when they created the project.

Key Servo components that Ghost Engine embeds:

| Component | Crate | What It Does |
|-----------|-------|-------------|
| HTML Parser | `html5ever` | Spec-compliant HTML tokenizer and tree builder |
| CSS Engine | `style` (Stylo) | Selector matching, cascade, computed values — **same engine used in Firefox** |
| JavaScript | SpiderMonkey (via FFI) | Full ES2023+, JIT compilation, async/await |
| Layout | `layout` | Flexbox, Grid, block/inline flow, absolute positions |
| Networking | `net` | HTTP/1.1, HTTP/2, TLS, CORS, cookie management |
| DOM | `script` | Full DOM API implementation with Rust<->JS bindings |

---

## The Embedding Model

Ghost Engine doesn't use Servo as-is. It restructures the dependency chain:

```
                Traditional Servo
                -----------------

     servoshell (binary)
         |
         |-- Winit (windowing)
         |-- Glutin (OpenGL context)
         |-- WebRender (GPU painting)
         |-- surfman (GPU surfaces)
         |-- constellation (pipeline manager)
         |-- script + SpiderMonkey (JS)
         |-- style/Stylo (CSS)
         |-- layout (geometry)
         |-- net (HTTP)
         |-- html5ever (parsing)


                Ghost Engine
                ------------

     ghost-cli / ghost-mcp (binaries)
         |
         |-- ghost-core (embedding API)
         |   |-- servoshell --features headless-shell
         |       |-- constellation [KEPT]
         |       |-- script + SpiderMonkey [KEPT]
         |       |-- style/Stylo [KEPT]
         |       |-- layout [KEPT]
         |       |-- net [KEPT]
         |       |-- html5ever [KEPT]
         |       |-- Winit [REMOVED]
         |       |-- Glutin [REMOVED]
         |       |-- WebRender [REMOVED]
         |       |-- surfman [REMOVED]
         |
         |-- ghost-interceptor (layout extraction)
         |-- ghost-serializer (JSON/Markdown)
         |-- ghost-interact (DOM interaction)
```

### The `headless-shell` Feature Flag

The core engineering mechanism is a Cargo feature flag (`headless-shell`) applied to the patched `servoshell`:

```toml
[features]
headless-shell = []  # Routes through headless path, excludes GPU code
```

When this feature is active:

1. **Winit window creation** is replaced with a no-op virtual viewport
2. **Glutin OpenGL context** initialization is skipped entirely
3. **WebRender surface creation** is disabled — no GPU buffers allocated
4. **surfman** surface management is bypassed
5. The event loop uses a **headless compositor** that dispatches DOM events without rendering

This means the entire GPU pipeline — which accounts for ~40% of Chromium's memory usage — simply doesn't exist in Ghost Engine's address space.

---

## Engine Modifications

### Patches Applied to Servo

Ghost Engine maintains a **patched fork** of Servo with the following modifications:

| Patch | Purpose | Impact |
|-------|---------|--------|
| `SERVOSHELL_FORCE_HEADLESS` | Runtime env var to force headless mode | Development convenience |
| `headless-shell` feature | Compile-time exclusion of GPU paths | Smaller binary, less memory |
| Winit dependency gating | `#[cfg(not(feature = "headless-shell"))]` on windowing code | Clean separation |
| WebRender surface init skip | No `create_surface()` calls in headless mode | No GPU context needed |
| Compositor adaptation | Headless compositor handles events without rendering | Event loop works without pixels |
| `media-stack = dummy` | Stub media backend | No GStreamer dependency |

### What Is NOT Modified

Ghost Engine deliberately preserves Servo's core correctness:

- **html5ever** — untouched, spec-compliant HTML parsing
- **Stylo** — untouched, identical CSS computation as Firefox
- **SpiderMonkey** — untouched, full JavaScript execution
- **DOM implementation** — untouched, complete Web API surface
- **Network stack** — untouched, full HTTP/TLS/CORS
- **Cookie handling** — untouched, spec-compliant cookie jar

This is critical: Ghost Engine's layout accuracy comes from using **unmodified, battle-tested engine components**. Only the output layer (pixels -> data) is changed.

---

## The Interception Point

### Where in the Pipeline?

The rendering pipeline has a clear boundary where layout is complete but painting hasn't started:

```
DOM Tree -> Style Computation -> Layout -> | -> Paint -> Composite -> Display
                                           |
                                 Ghost intercepts here
```

After Stylo completes layout computation, every DOM node has:

- **Computed styles** — the final, resolved CSS values
- **Fragment geometry** — absolute x, y, width, height in the coordinate space
- **Box model** — margin, border, padding, content dimensions
- **Stacking context** — z-index ordering and visibility

Ghost Engine's `ghost-interceptor` reads this data **before** it would normally flow into WebRender for pixel rasterization.

### Why This Point?

| Interception Point | Accuracy | Performance | Complexity |
|-------------------|----------|-------------|-----------|
| Raw HTML (before CSS) | Low | Fast | Simple |
| After style computation | Medium | Fast | Medium |
| **After layout** | **Full geometry** | **Fast** | **Medium** |
| After painting | Pixel-perfect | Slow (GPU needed) | Complex |

Post-layout is the **optimal point**: you have complete geometric accuracy without paying the cost of pixel rendering.

---

## SpiderMonkey Integration

Ghost Engine uses SpiderMonkey (Mozilla's JavaScript engine) through Servo's existing Rust<->JS FFI bindings.

### JS Evaluation API

```rust
// ghost-core exposes this API
engine.evaluate_js("document.title")  // -> Ok(JsValue::String("Page Title"))
engine.evaluate_js("document.querySelectorAll('a').length")  // -> Ok(JsValue::Number(42))
```

This capability is used by:

1. **`ghost-interceptor`** — traverses the DOM via JS to extract layout data
2. **`ghost-interact`** — fires synthetic events (click, type, scroll) via JS
3. **`ghost-mcp`** — exposes `ghost_evaluate_js` tool for arbitrary JS execution

### Panic Safety

SpiderMonkey can panic on unsupported or exotic DOM APIs. Ghost Engine wraps all JS evaluation in `catch_unwind`:

```rust
match std::panic::catch_unwind(|| engine.evaluate_js(code)) {
    Ok(result) => result,
    Err(_) => Err(GhostError::Panic("SpiderMonkey panic caught".into()))
}
```

This ensures that a single bad page never crashes the process — the error is reported cleanly to the agent.

---

## Headless Execution Model

### Event Loop Architecture

Ghost Engine runs Servo's event loop in headless mode:

```
+------------------------------------------+
|              Event Loop                   |
|                                           |
|  +-------------+    +-----------------+   |
|  | Network     |    | Timer           |   |
|  | Events      |    | Events          |   |
|  | (fetch done)|    | (setTimeout)    |   |
|  +------+------+    +-------+---------+   |
|         |                   |             |
|         +---------+---------+             |
|                   |                       |
|         +---------v----------+            |
|         |  Constellation     |            |
|         |  (event dispatch)  |            |
|         +---------+----------+            |
|                   |                       |
|         +---------v----------+            |
|         |  Script Thread     |            |
|         |  (SpiderMonkey)    |            |
|         |  Process DOM events|            |
|         |  Run JS handlers   |            |
|         +---------+----------+            |
|                   |                       |
|         +---------v----------+            |
|         |  Layout (Stylo)    |            |
|         |  Reflow if dirty   |            |
|         +---------+----------+            |
|                   |                       |
|         +---------v----------+            |
|         |  Headless          |            |
|         |  Compositor        |            |
|         |  (no-op paint)     |            |
|         +--------------------+            |
+------------------------------------------+
```

The headless compositor is a no-op — it acknowledges paint requests without doing any work. This keeps Servo's pipeline state machines happy while avoiding all GPU operations.

### Page Load Detection

Ghost Engine detects page completion through multiple signals:

| Signal | What It Means |
|--------|--------------|
| `DOMContentLoaded` | HTML parsed, DOM ready |
| `load` | All sub-resources (CSS, images) loaded |
| Network idle | No pending HTTP requests for N ms |
| Layout stable | No pending reflows for N ms |

The `--settle` and `--quiet` CLI options control how long Ghost Engine waits after initial load for async JS to finish executing.

---

## Dependency Management

### Build Dependencies

| Dependency | Purpose | Source |
|------------|---------|--------|
| Rust toolchain | Compilation | `rust-toolchain.toml` (pinned version) |
| SpiderMonkey | JS engine | Built from source as part of Servo build |
| OpenSSL / LibreSSL | TLS | System library |
| fontconfig (Linux) | Font discovery | System library |
| CoreText (macOS) | Font discovery | System framework |

### Runtime Dependencies

| Dependency | Required? | Notes |
|------------|-----------|-------|
| System libc | Yes | Standard on all platforms |
| System TLS library | Yes | OpenSSL/LibreSSL on Linux, Security.framework on macOS |
| GStreamer | **No** | Disabled via `media-stack=dummy` |
| GPU drivers | **No** | No rendering pipeline |
| X11 / Wayland | **No** | No windowing |
| Browser binary | **No** | Ghost Engine **is** the engine |

---

## Compatibility Trade-offs

Embedding Servo instead of Chromium comes with known trade-offs:

### What Works Well

- **Standard HTML/CSS** — Stylo (Firefox's CSS engine) handles modern CSS excellently
- **Modern JavaScript** — SpiderMonkey supports full ES2023+
- **SPAs** (React, Vue, Svelte) — JS execution + DOM mutation works correctly
- **Forms and interaction** — Standard form elements, events, validation
- **Cookies and auth** — Full cookie jar, CORS, TLS

### Known Limitations

| Feature | Status | Impact on AI Agents |
|---------|--------|-------------------|
| Service Workers | Not implemented | PWA offline fallbacks won't work |
| WebRTC | Not implemented | Video calling sites won't function |
| Web Audio | Disabled (dummy media) | Audio-dependent features unavailable |
| WebGPU | Not implemented | GPU compute sites won't work |
| `window.open()` | Not implemented | OAuth popup flows need workarounds |
| File uploads | No dialog in headless | Must set via JS if CSP allows |

### Mitigation Strategy

For sites that fall outside Servo's compatibility:

1. **Detection** — Ghost Engine reports clean errors (not crashes) for unsupported APIs
2. **Graceful degradation** — Agents receive structured error messages they can reason about
3. **Fallback** — For critical sites, agents can defer to a Playwright-based fallback (not included in Ghost Engine, but easy to integrate at the agent level)

---

## Future Embedding Directions

### Upstream Servo Improvements

As Servo development continues under the Linux Foundation, Ghost Engine benefits from upstream improvements:

- **Service Worker support** — in progress upstream
- **CSS `content-visibility`** — partial support, expanding
- **WebGPU** — early research stage in Servo

### Ghost Engine Roadmap

| Direction | Description |
|-----------|-------------|
| **WASM embedding** | Compile Ghost Engine to WebAssembly for browser-hosted agents |
| **Multi-process** | Leverage Servo's constellation for process isolation per tab |
| **Custom protocols** | Add `ghost://` protocol for agent-to-agent communication |
| **Streaming extraction** | Stream layout nodes as they're computed, before full page load |
| **Plugin API** | Allow custom Rust plugins to transform the layout tree |

---

## Summary

Ghost Engine's embedding strategy can be summarized in one sentence:

> **Take Servo's spec-compliant HTML, CSS, and JS engines; remove the pixel renderer; replace it with a data extractor.**

This gives AI agents the accuracy of a real browser engine, the speed of skipping unnecessary rendering, and the simplicity of a single self-contained binary — all built on battle-tested Mozilla technology.
