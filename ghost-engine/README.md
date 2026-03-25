# Ghost Engine - Headless Embedded Servo Browser for AI Agents

## Executive Summary
Building a brand new browser engine from scratch (Network, HTML, JS, CSS) to avoid using Chrome/Playwright is a multi-year, multi-million dollar undertaking. The "Cheat Code" strategy allows a startup to bypass 10 years of systems engineering by **strategically embedding and modifying an existing, highly modular browser engine: Mozilla's Servo.**

By leveraging Servo (written in Rust), you can build a blazing-fast, zero-dependency, ultra-lightweight headless browser optimized purely for AI Agents in 3-6 months.

## Why Mozilla's Servo?
Servo was originally created by Mozilla to research a next-generation, memory-safe, highly concurrent browser engine in Rust. It is now maintained by the Linux Foundation.
*   **Written in Rust:** Matches modern high-performance tooling trends
*   **Designed for Embedding:** Built as a collection of decoupled crates designed to be embedded into other applications.
*   **SpiderMonkey & Stylo:** Handles executing JavaScript and calculating complex CSS layouts seamlessly.

## The Core Strategy: The "Interception"
1.  **Keep the Engine:** Compile Servo as a Rust library and feed it a URL.
2.  **Delete the Painter:** Strip out or disable \`WebRender\`. AI agents do not need pixels.
3.  **The Interception:** Intercept the DOM data structure right after CSS Layout computes physical geometry.
4.  **The Translation:** Write custom Rust code to traverse Servo's Layout Tree, filter out invisible/irrelevant nodes, assign interactivity IDs, and serialize to an LLM-optimized payload.

## Engineering Roadmap (3-Month Sprint)
*   **Month 1 (ghost-cli & ghost-core):** The Embedding Sandbox (Winit/Glutin removal, headless JS execution).
*   **Month 2 (ghost-interceptor):** The Interception & Layout Traversal (Stylo hooks, visibility filtering).
*   **Month 3 (ghost-serializer & ghost-interact):** LLM Serialization & Interaction API (JSON/Markdown payload, synthetic clicks mapping to JS via SpiderMonkey).

## Project Structure

Ghost Engine **is** a forked Servo workspace — ghost crates live directly inside `ports/` alongside `servoshell`. This means the workspace root (`ghost-engine/`) hosts both Servo's build infrastructure (`.cargo/config.toml`, `rust-toolchain.toml`, `mach`, etc.) and our custom crates. This is the standard Rust workspace layout and ensures ghost crates share Servo's build config, feature resolution, and `mozjs_sys` pre-built archive.

```
ghost-engine/                  # Cargo workspace root (forked Servo)
  README.md                    # this file
  Cargo.toml                   # workspace: ports/servoshell, ports/ghost-*
  .cargo/config.toml           # build env (RUSTC_BOOTSTRAP, MACOSX_DEPLOYMENT_TARGET)
  .servobuild                  # Servo build config (media-stack = "dummy")
  rust-toolchain.toml          # pinned Rust 1.92.0
  mach                         # Servo's build orchestrator
  components/                  # Servo engine crates (script, layout, net, etc.)
  ports/
    servoshell/                # Servo's default shell (patched for headless)
    ghost-core/                # core embedding layer (lib crate)
    ghost-cli/                 # CLI binary (future)
    ghost-interceptor/         # layout interception hooks (future)
    ghost-serializer/          # LLM serialization pipeline (future)
    ghost-interact/            # synthetic interaction API (future)
  docs/                        # Servo documentation
  resources/                   # Servo runtime resources
  tests/                       # Servo test suites
```

### Building

```bash
cd ghost-engine

# Full Servo dev build (first time)
./mach build -d -j 8

# Check ghost-core only
cargo check -p ghost-core

# Build servoshell in headless mode
cargo build -p servoshell --features headless-shell
```
