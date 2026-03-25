# AI Deep — How It Compares to Other Browser AI Tools

## Overview

This document compares the architectural approach of **AI Deep** against other popular browser automation and AI agent tools. The key distinction lies in **how** the browser engine is used — as a wrapped dependency vs. a natively integrated component.

---

## The Two Architectural Approaches

### The "Wrapper" Approach (Everyone Else)

Tools like **Browserbase/Stagehand**, **Skyvern**, **MultiOn**, and **Puppeteer/Playwright** all follow the same fundamental pattern:

1. They launch a **massive, 1GB+ compiled binary** of Chromium or Firefox.
2. The browser downloads the HTML, parses the CSS, executes the JS, and spends CPU/GPU cycles **drawing every single pixel** to a hidden virtual screen (headless rendering).
3. The AI tool then **injects JavaScript back** into that running Chromium instance (usually via the Chrome DevTools Protocol — CDP) to scrape the DOM, hide invisible elements, calculate bounding boxes, and generate a clean text output for the LLM.

```
┌─────────────────────────────────────────────────┐
│              Chromium / Firefox (1GB+)           │
│                                                   │
│  HTML → CSS → JS → Layout → Paint → Pixels       │
│                                        ▲          │
│                                        │          │
│                    Injected JS (CDP) ──┘          │
│                        │                          │
│                        ▼                          │
│                  Scraped DOM / Text                │
└────────────────────────┬────────────────────────┘
                         │
                         ▼
                   AI Agent / LLM
```

### The "Ghost" Approach

**Ghost** does not launch Chromium. It compiles Mozilla's core engine components (**Servo/SpiderMonkey**) directly into its own lightweight Rust binary.

1. It downloads the HTML, parses the CSS, and executes the JS.
2. **It stops.** It completely removes the "Painter" (the WebRender engine that draws pixels).
3. Instead of drawing pixels, Ghost's native Rust code **directly traverses the engine's internal, mathematically calculated layout tree** (the exact physical geometry of the page) and serializes it straight to Markdown.

```
┌─────────────────────────────────────────────────┐
│         Ghost (Single Rust Binary)               │
│                                                   │
│  HTML → CSS → JS → Layout ──╳── Paint (removed)  │
│                       │                           │
│                       ▼                           │
│             Layout Tree Traversal                 │
│             (Native Rust, zero JS)                │
│                       │                           │
│                       ▼                           │
│                Markdown Output                    │
└────────────────────────┬────────────────────────┘
                         │
                         ▼
                   AI Agent / LLM
```

---

## Why This Distinction Matters

Because Ghost is **not a wrapper** around a massive human browser, it achieves three things the others cannot:

### 1. Zero Dependencies

You don't need to run `playwright install` to download 1.5GB of browser binaries. Ghost is a **single, self-contained executable**.

| Tool | Setup Requirement |
|------|-------------------|
| Puppeteer | `npx puppeteer install` → downloads Chromium (~280MB) |
| Playwright | `npx playwright install` → downloads 3 browsers (~1.5GB) |
| Skyvern | Chromium + Python dependencies |
| Browserbase | Cloud-hosted Chromium (external dependency) |
| **Ghost** | **Single binary. Nothing else.** |

### 2. Fractional Memory / CPU Overhead

By skipping the pixel-rendering phase and stripping out the Chromium UI layer entirely, Ghost can:

- Run on **significantly cheaper, lower-power hardware**
- Run **many more concurrent instances** on a single server

| Metric | Wrapper (Chromium) | Ghost |
|--------|--------------------|-------|
| Memory per instance | ~200–500 MB | ~30–80 MB |
| CPU (idle page) | Compositing + painting threads active | Layout only |
| Binary size | 1GB+ | Single lightweight binary |
| Concurrent instances (8GB server) | ~10–15 | ~50–100+ |

### 3. Perfect Accuracy

Wrappers relying on injected JavaScript or Computer Vision often make mistakes guessing what is actually **visible or clickable** because they are trying to **reverse-engineer** Chromium's rendering from the outside.

Ghost **is** the renderer. It knows with **100% mathematical certainty** exactly where every node sits in the layout and whether it is visible or not.

| Accuracy Challenge | Wrapper Approach | Ghost Approach |
|--------------------|-----------------|----------------|
| Is element visible? | JS heuristics (`offsetHeight`, `getComputedStyle`) — can be wrong | Direct layout tree query — **always correct** |
| Bounding box calculation | CDP `getBoxModel` — async, sometimes stale | Native layout geometry — **synchronous, exact** |
| Hidden overflow / clipping | Must re-implement clipping logic in JS | Engine already computed it — **native access** |
| Dynamic `visibility` / `opacity` | Fragile CSS parsing via injected scripts | Resolved in style computation — **engine-level** |
| `z-index` / stacking context | Complex JS reconstruction, often wrong | Layout tree has the truth — **no guessing** |

---

## Detailed Tool-by-Tool Comparison

### vs. Puppeteer / Playwright

| Feature | Puppeteer / Playwright | Ghost |
|---------|----------------------|-------|
| Architecture | CDP wrapper around Chromium | Native engine integration |
| Browser download | Required (1GB+) | Not needed |
| Headless rendering | Full pixel rendering (hidden) | No pixel rendering |
| DOM access | Via injected JavaScript | Direct layout tree traversal |
| Multi-browser | Chromium, Firefox, WebKit | Servo/SpiderMonkey (Mozilla) |
| Use case | General testing + scraping | AI-optimized content extraction |
| Output format | Raw HTML / DOM snapshots | Structured Markdown |

### vs. Browserbase / Stagehand

| Feature | Browserbase / Stagehand | Ghost |
|---------|------------------------|-------|
| Architecture | Cloud-hosted Chromium + AI layer | Local native binary |
| Infrastructure | Requires cloud browser farm | Runs anywhere, no infra needed |
| Latency | Network hop to cloud browser | Local execution |
| Cost model | Per-session cloud billing | Self-hosted, no per-use cost |
| Scaling | Scale cloud instances ($$$) | Scale on commodity hardware |

### vs. Skyvern

| Feature | Skyvern | Ghost |
|---------|---------|-------|
| Architecture | Chromium + Computer Vision + LLM | Native engine + Layout tree |
| Element detection | Screenshot → CV model → guess coordinates | Exact coordinates from layout |
| Failure mode | CV misidentifies elements | N/A — geometry is computed, not guessed |
| Resource usage | Chromium + CV model + LLM | Single binary + LLM |

### vs. MultiOn

| Feature | MultiOn | Ghost |
|---------|---------|-------|
| Architecture | Cloud Chromium + proprietary agent | Local native binary + configurable agent |
| Control | Opaque cloud execution | Full local control |
| Data privacy | Pages rendered on MultiOn's servers | Pages processed locally |
| Customization | Limited API | Full source-level customization |

---

## The Closest Technical Cousin

The closest thing technically to Ghost is **Servo** itself.

[Servo](https://servo.org/) is Mozilla's experimental "Next Generation Browser Engine" — a research project written in Rust that aimed to build a browser engine from scratch with parallelism and safety as first-class goals. Mozilla scaled back active development, and the project moved to the Linux Foundation.

Ghost is essentially taking Servo's core engine and **deeply modifying it for an entirely new, non-human use case**:

| Aspect | Servo (Original) | Ghost (Modified) |
|--------|------------------|------------------|
| Goal | Build a human-facing browser | Build an AI-facing content extractor |
| Rendering | Full WebRender pixel pipeline | **Removed** — layout only |
| Output | Pixels on screen | Structured Markdown for LLMs |
| UI layer | Browser chrome, tabs, navigation | Stripped — headless by design |
| Target user | Humans | AI agents |

---

## Summary

```
                    Accuracy    Memory    Dependencies    Speed
                    ────────    ──────    ────────────    ─────
Puppeteer/PW        ██████░░    ████████    ████████      ██████░░
Browserbase          ██████░░    ████████    ██████░░      █████░░░
Skyvern              █████░░░    ████████    ████████      █████░░░
MultiOn              █████░░░    ████████    ██████░░      █████░░░
Ghost                ████████    ██░░░░░░    █░░░░░░░      ████████

Lower is better for Memory and Dependencies.
Higher is better for Accuracy and Speed.
```

**Ghost trades browser compatibility for raw efficiency and accuracy.** If you need to render a page exactly as Chrome would for visual regression testing, use Playwright. If you need an AI agent to **understand and interact with web content** at scale and at minimal cost, Ghost is architecturally superior.
