# ghost-core

`ghost-core` is the Rust embedding layer that links Ghost Engine to the local patched Servo build.

## Location
This crate lives inside the Servo workspace at `ports/ghost-core/` — a sibling of `ports/servoshell/`. This ensures it shares Servo's `.cargo/config.toml` environment, build cache, and workspace feature resolution — all required for `mozjs_sys` to compile correctly on macOS.

## Current Scope
- depends directly on the sibling `servoshell` crate at `../servoshell`
- exposes runtime bootstrap helpers for Servo crypto and tracing initialization
- provides a small metadata surface so downstream crates can confirm which local Servo build they are linked against

## Validation
From `ghost-engine/`:

```bash
cargo check -p ghost-core
```

## Next Responsibilities
- wrap headless Servo startup behind a stable Ghost Engine API
- own shared runtime/session state for `ghost-cli`
- become the integration point for later interception and serialization hooks
