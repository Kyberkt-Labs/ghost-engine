# Ghost Engine — Web Platform Compatibility Report

> Comprehensive compatibility reference based on Servo's current implementation
> status and Ghost Engine's headless embedding layer. Use this document to
> understand what works, what doesn't, and what to expect when your AI agent
> visits real-world websites.

---

## Table of Contents

- [Compatibility Summary](#compatibility-summary)
- [Legend](#legend)
- [Core HTML & DOM](#core-html--dom)
- [CSS](#css)
- [JavaScript Engine (SpiderMonkey)](#javascript-engine-spidermonkey)
- [Web APIs](#web-apis)
- [Storage](#storage)
- [Canvas & Graphics](#canvas--graphics)
- [Media](#media)
- [Forms & Input](#forms--input)
- [Network & Security](#network--security)
- [Accessibility](#accessibility)
- [WPT Pass Rates](#wpt-pass-rates-servo-ci-approximate)
- [Known Servo Limitations for AI Agents](#known-servo-limitations-for-ai-agents)
- [Real-World Site Compatibility](#real-world-site-compatibility)
- [Workarounds & Best Practices](#workarounds--best-practices)

---

## Compatibility Summary

Ghost Engine inherits Servo's web platform support, which covers the vast majority of features AI agents encounter on real-world websites:

| Category | Full Support | Partial | Unsupported |
|----------|:----------:|:-------:|:-----------:|
| HTML & DOM | 8 | 3 | 0 |
| CSS | 10 | 3 | 0 |
| JavaScript | 7 | 1 | 0 |
| Web APIs | 9 | 1 | 5 |
| Storage | 3 | 1 | 1 |
| Canvas & Graphics | 3 | 2 | 1 |
| Media | 1 | 1 | 3 |
| Forms | 5 | 2 | 1 |
| Network & Security | 4 | 2 | 1 |
| Accessibility | 3 | 0 | 1 |
| **Total** | **53** | **16** | **13** |

**Bottom line:** ~65% of tracked web platform features are fully supported. ~85% are at least partially functional. The unsupported features (Service Workers, WebRTC, media playback, WebGPU) rarely impact AI agent workflows.

---

## Legend

| Level | Meaning |
|-------|---------|
| \u2705 Full | Feature works reliably in Ghost Engine |
| \u26a0\ufe0f Partial | Feature exists but has known limitations or gaps |
| \u274c Unsupported | Feature is missing or non-functional in Servo |

---

## Core HTML & DOM

| Feature | Status | Notes |
|---------|--------|-------|
| HTML5 parsing | \u2705 Full | html5ever parser, spec-compliant |
| DOM Level 3 Core | \u2705 Full | `createElement`, `querySelector`, etc. |
| Shadow DOM v1 | \u26a0\ufe0f Partial | Declarative shadow DOM supported; some edge cases in slotting |
| Custom Elements v1 | \u26a0\ufe0f Partial | `define()` and lifecycle callbacks work; some upgrade timing issues |
| `<template>` / `<slot>` | \u2705 Full | |
| `<dialog>` | \u26a0\ufe0f Partial | `showModal()` supported but no `::backdrop` rendering |
| `<details>` / `<summary>` | \u2705 Full | |
| `contentEditable` | \u26a0\ufe0f Partial | Basic editing works; `execCommand` has gaps |
| `MutationObserver` | \u2705 Full | |
| `IntersectionObserver` | \u2705 Full | |
| `ResizeObserver` | \u2705 Full | |

---

## CSS

| Feature | Status | Notes |
|---------|--------|-------|
| CSS Flexbox | \u2705 Full | Stylo engine, full spec coverage |
| CSS Grid | \u2705 Full | |
| CSS Animations | \u2705 Full | `@keyframes`, `animation-*` properties |
| CSS Transitions | \u2705 Full | |
| CSS Variables (custom properties) | \u2705 Full | |
| CSS `calc()` | \u2705 Full | |
| `position: sticky` | \u2705 Full | |
| CSS Containment | \u26a0\ufe0f Partial | `contain: layout\|paint` works; `content-visibility` incomplete |
| `@layer` | \u2705 Full | |
| `@container` queries | \u26a0\ufe0f Partial | Size queries work; style queries not yet |
| `:has()` selector | \u2705 Full | |
| `@media` queries | \u2705 Full | Viewport-based; no media device queries in headless |
| `@font-face` | \u26a0\ufe0f Partial | Works with system fonts; WOFF2 remote loading can fail in headless |

---

## JavaScript Engine (SpiderMonkey)

| Feature | Status | Notes |
|---------|--------|-------|
| ES2023+ syntax | \u2705 Full | SpiderMonkey 128+ provides full ES coverage |
| `async` / `await` | \u2705 Full | |
| Promises | \u2705 Full | |
| `import()` (dynamic) | \u2705 Full | |
| ES Modules (`<script type="module">`) | \u2705 Full | |
| `WeakRef` / `FinalizationRegistry` | \u2705 Full | |
| `Atomics` / `SharedArrayBuffer` | \u26a0\ufe0f Partial | Requires cross-origin isolation headers |
| Web Workers | \u2705 Full | Dedicated workers; shared workers partial |
| Service Workers | \u274c Unsupported | Not implemented in Servo |

---

## Web APIs

| Feature | Status | Notes |
|---------|--------|-------|
| `fetch()` | \u2705 Full | HTTP/1.1 and HTTP/2; CORS enforced |
| `XMLHttpRequest` | \u2705 Full | |
| `setTimeout` / `setInterval` | \u2705 Full | |
| `requestAnimationFrame` | \u2705 Full | Fires in headless but at variable rate |
| `requestIdleCallback` | \u26a0\ufe0f Partial | Basic support; deadline estimation may differ |
| `navigator.clipboard` | \u274c Unsupported | No system clipboard in headless |
| `navigator.geolocation` | \u274c Unsupported | |
| `navigator.permissions` | \u274c Unsupported | |
| Notifications API | \u274c Unsupported | |
| `window.open()` / popups | \u274c Unsupported | Ghost Engine is single-webview |
| `history.pushState` / `popState` | \u2705 Full | |
| `URLSearchParams` | \u2705 Full | |
| `URL` / `URL.createObjectURL` | \u2705 Full | |
| `FormData` | \u2705 Full | |
| `Blob` / `File` | \u2705 Full | |

---

## Storage

| Feature | Status | Notes |
|---------|--------|-------|
| `document.cookie` | \u2705 Full | Ghost Engine reads/writes via JS. `httpOnly` cookies not accessible. |
| `localStorage` | \u2705 Full | In-memory (not persisted across engine restarts) |
| `sessionStorage` | \u2705 Full | |
| IndexedDB | \u26a0\ufe0f Partial | Basic operations work; complex transactions may have gaps |
| Cache API | \u274c Unsupported | Tied to Service Workers |

---

## Canvas & Graphics

| Feature | Status | Notes |
|---------|--------|-------|
| Canvas 2D | \u2705 Full | |
| WebGL 1.0 | \u26a0\ufe0f Partial | Software rendering in headless; some extensions missing |
| WebGL 2.0 | \u26a0\ufe0f Partial | Basic support; compute shaders not available |
| WebGPU | \u274c Unsupported | |
| SVG (inline) | \u2705 Full | |
| SVG (via `<img>`) | \u2705 Full | |

---

## Media

| Feature | Status | Notes |
|---------|--------|-------|
| `<img>` (PNG, JPEG, GIF, WebP) | \u2705 Full | |
| `<img>` (AVIF) | \u26a0\ufe0f Partial | Depends on build flags |
| `<video>` / `<audio>` | \u274c Unsupported | Ghost Engine uses `media-stack=dummy` |
| MediaStream / WebRTC | \u274c Unsupported | |
| Web Audio API | \u274c Unsupported | Dummy media stack |

---

## Forms & Input

| Feature | Status | Notes |
|---------|--------|-------|
| `<input>` (text, password, email, etc.) | \u2705 Full | |
| `<input type="date">` | \u26a0\ufe0f Partial | No native date picker; value settable via JS |
| `<input type="file">` | \u274c Unsupported | No file dialog in headless |
| `<textarea>` | \u2705 Full | |
| `<select>` / `<option>` | \u2705 Full | Ghost Engine selects via JS |
| `<input type="checkbox">` / `<radio>` | \u2705 Full | |
| Form validation (`:valid`, `:invalid`) | \u2705 Full | |
| `<form>` submission | \u26a0\ufe0f Partial | GET submissions work; multipart POST may have gaps |

---

## Network & Security

| Feature | Status | Notes |
|---------|--------|-------|
| HTTPS / TLS | \u2705 Full | |
| CORS | \u2705 Full | |
| CSP (Content Security Policy) | \u26a0\ufe0f Partial | Basic directives enforced; `report-uri` not functional |
| HSTS | \u2705 Full | |
| Cookies (same-site, secure, httpOnly) | \u26a0\ufe0f Partial | Enforced at network layer; Ghost reads via `document.cookie` only |
| Custom request headers | \u274c Unsupported | No Servo API for arbitrary header injection |
| Custom User-Agent | \u2705 Full | Via `GhostEngineConfig::user_agent` |

---

## Accessibility

| Feature | Status | Notes |
|---------|--------|-------|
| ARIA roles | \u2705 Full | Extracted by ghost-interceptor |
| `aria-label` / `aria-*` | \u2705 Full | Extracted by ghost-interceptor |
| Semantic HTML mapping | \u2705 Full | ghost-serializer Markdown preserves structure |
| Screen reader tree | \u274c Unsupported | No AT-SPI / accessibility tree export |

---

## WPT Pass Rates (Servo CI, approximate)

These numbers are from Servo's upstream CI and may differ for Ghost Engine's
`media-stack=dummy` headless builds. They give a rough idea of baseline
spec compliance.

| WPT Suite | Approx. Pass Rate | Notes |
|-----------|-------------------|-------|
| `dom/` | ~92% | Core DOM operations |
| `html/` | ~85% | HTML parsing, semantics |
| `css/` | ~88% | Stylo engine, strong coverage |
| `fetch/` | ~80% | Networking, CORS |
| `url/` | ~98% | URL parsing |
| `xhr/` | ~85% | XMLHttpRequest |
| `workers/` | ~70% | Dedicated workers OK, shared partial |
| `canvas/` | ~75% | 2D good, WebGL varies |
| `webgl/` | ~50% | Software rendering, conformance gaps |
| `wasm/` | ~95% | SpiderMonkey WASM support |

---

## Known Servo Limitations for AI Agents

These are the most impactful limitations when using Ghost Engine for real-world AI agent workflows:

### 1. No Service Workers

PWAs that depend on Service Workers for routing or caching won't work. Pages may show "offline" fallbacks.

**Agent impact:** Low. Most sites work without SW. If a site requires SW, the agent receives a clean error.

### 2. No `window.open()`

Popups, OAuth redirect flows that open new windows, and `target="_blank"` links won't spawn new views.

**Agent impact:** Medium. OAuth flows that use popups need a workaround (direct URL navigation instead).

### 3. Media Disabled

Audio/video elements are non-functional. `HTMLMediaElement.play()` will reject.

**Agent impact:** Low. AI agents rarely need to play media. Video metadata (poster, duration) may still be accessible via DOM attributes.

### 4. No File Uploads

`<input type="file">` cannot be used. Workaround: set `files` property via JS if the site's CSP allows it.

**Agent impact:** Low for browsing tasks. Blocking for file-upload workflows.

### 5. Cookie Visibility

`httpOnly` cookies are enforced at the network layer but cannot be read or set from `document.cookie`. Ghost Engine's cookie helpers only see non-`httpOnly` cookies.

**Agent impact:** Low. Auth cookies are typically sent automatically by the network stack.

### 6. WebGL in Software

WebGL works but renders via software. Performance is significantly lower than GPU-accelerated browsers. Canvas fingerprinting results will differ from Chrome/Firefox.

**Agent impact:** Low. Agents rarely need WebGL. Sites that fingerprint via canvas will see unusual results.

### 7. Panics on Exotic APIs

Some rarely-used DOM APIs may trigger Servo panics. Ghost Engine wraps `evaluate_js` with `catch_unwind` to convert these to `GhostError::Panic` instead of crashing the process.

**Agent impact:** Low. The panic is caught and reported; the agent can skip the page and continue.

---

## Real-World Site Compatibility

Based on testing against popular websites:

### Tier 1 — Full Support (Works Out of the Box)

Sites that load correctly, extract clean content, and support interaction:

- Static content sites (Wikipedia, blogs, news articles)
- Search engines (Google search results page, DuckDuckGo)
- E-commerce product pages (basic product info extraction)
- Documentation sites (MDN, ReadTheDocs, GitHub READMEs)
- Government / institutional sites

### Tier 2 — Partial Support (Works with Caveats)

Sites that load but may have minor rendering or interaction issues:

- Complex SPAs (React/Vue/Svelte apps) — usually work, may need longer settle time
- Sites with heavy animations — animations compute but don't render visually
- Sites with complex auth flows — may need manual cookie/header setup

### Tier 3 — Limited Support

Sites that load but with significant functionality gaps:

- Sites heavily dependent on Service Workers
- Sites requiring WebRTC (video calling, screen sharing)
- Sites with mandatory media playback
- Sites using WebGPU for core functionality

---

## Workarounds & Best Practices

### For SPA Content Loading

```bash
# Increase settle time for heavy SPAs
ghost --settle 5 --quiet 1000 https://heavy-spa.example.com/
```

### For Authentication

```bash
# Use custom User-Agent
ghost --format markdown https://auth-site.example.com/

# Then in interactive mode, set cookies:
ghost> js document.cookie = "session=abc123; path=/"
```

### For Pages That Don't Fully Load

```bash
# Increase timeout for slow servers
ghost --timeout 60 --settle 10 https://slow-site.example.com/
```

### For Extracting Specific Content

```bash
# Use JS evaluation to check specific elements
ghost> js document.querySelector('.product-price')?.textContent
```
