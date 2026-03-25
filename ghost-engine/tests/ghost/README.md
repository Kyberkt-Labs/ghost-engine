# Ghost Engine Test Fixtures

HTML test pages for validating the Ghost Engine headless browser across
increasing complexity tiers.

## Tiers

| Tier | Directory | What it tests |
|------|-----------|---------------|
| 1 | `tier1_static/` | Pure HTML+CSS — no JavaScript. Verifies basic DOM parsing, title extraction, and page-load lifecycle. |
| 2 | `tier2_vanilla_js/` | Vanilla JavaScript — DOM manipulation, timers, fetch simulation, Promises. Verifies SpiderMonkey execution and the settle loop. |
| 3 | `tier3_react/` | Minimal React SPA (via CDN ESM or inline bundle). Verifies JSX/virtual-DOM rendering and client-side routing basics. |

## Conventions

- Every test page sets `<title>` to a known string so the Rust test
  harness can assert `webview.page_title()`.
- Pages that modify the DOM via JS write a `<meta name="ghost-status">`
  tag with `content="ready"` when finished, giving the harness a
  secondary completion signal.
- All files are self-contained (inline styles/scripts, no external
  fetches except Tier 3's React CDN).
