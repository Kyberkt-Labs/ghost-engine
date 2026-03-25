//! TSK-5.5: Performance benchmark example.
//!
//! Run with:
//! ```sh
//! cargo run --example perf_bench -p ghost-cli
//! ```
//!
//! Loads a set of sites in two modes — **full** (all resources) and **lean**
//! (images, fonts, and media blocked) — then prints a comparative performance
//! report.

use std::time::Duration;

use ghost_core::{GhostEngine, GhostEngineConfig, ResourceBudget};

const BENCH_SITES: &[&str] = &[
    "https://www.wikipedia.org",
    "https://news.ycombinator.com",
    "https://www.example.com",
];

fn main() {
    println!("# Ghost Engine — Performance Benchmark\n");

    // ── Full mode (all resources allowed) ───────────────────────────────
    println!("## Full Mode (all resources)\n");
    let full_config = GhostEngineConfig {
        load_timeout: Duration::from_secs(30),
        settle_timeout: Duration::from_secs(3),
        quiet_period: Duration::from_millis(500),
        ..Default::default()
    };

    let engine = match GhostEngine::new(full_config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to initialise Ghost Engine: {e}");
            std::process::exit(1);
        },
    };

    println!(
        "Engine init: {:.1} ms\n",
        engine.init_duration().as_secs_f64() * 1000.0
    );

    println!("| Site | Total (ms) | Nav (ms) | Head (ms) | Sub (ms) | Blocked | Saved (KB) |");
    println!("|------|-----------|----------|-----------|----------|---------|------------|");

    for url in BENCH_SITES {
        bench_site(&engine, url);
    }

    // ── Lean mode (skip images, fonts, media) ───────────────────────────
    println!("\n## Lean Mode (skip images + fonts + media)\n");
    let lean_config = GhostEngineConfig {
        load_timeout: Duration::from_secs(30),
        settle_timeout: Duration::from_secs(3),
        quiet_period: Duration::from_millis(500),
        resource_budget: ResourceBudget {
            skip_images: true,
            skip_fonts: true,
            skip_media: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let lean_engine = match GhostEngine::new(lean_config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to initialise Ghost Engine (lean): {e}");
            std::process::exit(1);
        },
    };

    println!("| Site | Total (ms) | Nav (ms) | Head (ms) | Sub (ms) | Blocked | Saved (KB) |");
    println!("|------|-----------|----------|-----------|----------|---------|------------|");

    for url in BENCH_SITES {
        bench_site(&lean_engine, url);
    }

    // ── Summary ─────────────────────────────────────────────────────────
    println!("\n## Memory\n");
    let report = engine.perf_report(
        // Use a dummy load just to read RSS; no webview needed for RSS.
        &engine.new_webview("about:blank").expect("about:blank"),
    );
    println!(
        "Process RSS: {:.1} MB",
        report.rss_bytes as f64 / (1024.0 * 1024.0)
    );
}

fn bench_site(engine: &GhostEngine, url: &str) {
    let webview = match engine.new_webview(url) {
        Ok(wv) => wv,
        Err(e) => {
            println!("| {url} | ERROR | - | - | - | - | - | ({e})");
            return;
        },
    };

    if let Err(e) = engine.load_and_wait(&webview) {
        println!("| {url} | TIMEOUT | - | - | - | - | - | ({e})");
        return;
    }

    let report = engine.perf_report(&webview);
    let lt = webview.load_timing();

    fn ms(d: Option<Duration>) -> String {
        d.map(|d| format!("{:.1}", d.as_secs_f64() * 1000.0))
            .unwrap_or_else(|| "-".into())
    }

    println!(
        "| {url} | {} | {} | {} | {} | {} | {:.1} |",
        ms(lt.total),
        ms(lt.navigation),
        ms(lt.head_parse),
        ms(lt.subresources),
        report.resources_blocked,
        report.bytes_saved as f64 / 1024.0,
    );
}
