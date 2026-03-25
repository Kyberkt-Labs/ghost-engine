//! TSK-5.1: Top-25 website compatibility test.
//!
//! Run with:
//! ```sh
//! cargo run --example compat_test -p ghost-cli
//! ```
//!
//! Navigates to each site, attempts layout extraction, and categorises
//! the result as **full** (≥50 nodes), **partial** (1–49 nodes), or
//! **unsupported** (0 nodes / error). Produces a Markdown report on stdout.

use std::time::Duration;

use ghost_core::{GhostEngine, GhostEngineConfig};
use ghost_interceptor::extract_layout;

/// Top-25 websites by global traffic (Similarweb / Cloudflare Radar — 2025).
const TOP_SITES: &[&str] = &[
    "https://www.google.com",
    "https://www.youtube.com",
    "https://www.facebook.com",
    "https://www.twitter.com",
    "https://www.instagram.com",
    "https://www.wikipedia.org",
    "https://www.reddit.com",
    "https://www.amazon.com",
    "https://www.yahoo.com",
    "https://www.linkedin.com",
    "https://www.netflix.com",
    "https://www.bing.com",
    "https://www.twitch.tv",
    "https://www.microsoft.com",
    "https://www.apple.com",
    "https://www.github.com",
    "https://www.stackoverflow.com",
    "https://www.discord.com",
    "https://www.pinterest.com",
    "https://www.cnn.com",
    "https://www.bbc.com",
    "https://www.ebay.com",
    "https://www.quora.com",
    "https://news.ycombinator.com",
    "https://www.nytimes.com",
];

#[derive(Debug)]
enum SupportLevel {
    Full,
    Partial,
    Unsupported,
    Error(String),
}

impl std::fmt::Display for SupportLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SupportLevel::Full => write!(f, "✅ Full"),
            SupportLevel::Partial => write!(f, "⚠️  Partial"),
            SupportLevel::Unsupported => write!(f, "❌ Unsupported"),
            SupportLevel::Error(e) => write!(f, "💥 Error: {e}"),
        }
    }
}

fn main() {
    let config = GhostEngineConfig {
        load_timeout: Duration::from_secs(30),
        settle_timeout: Duration::from_secs(3),
        quiet_period: Duration::from_millis(500),
        ..Default::default()
    };

    let engine = match GhostEngine::new(config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to initialise Ghost Engine: {e}");
            std::process::exit(1);
        },
    };

    println!("# Ghost Engine — Top-25 Site Compatibility Report\n");
    println!("| # | Site | Nodes | Interactive | Title | Status |");
    println!("|---|------|-------|-------------|-------|--------|");

    let mut full = 0u32;
    let mut partial = 0u32;
    let mut unsupported = 0u32;
    let mut errors = 0u32;

    for (i, url) in TOP_SITES.iter().enumerate() {
        let result = test_site(&engine, url);
        match &result {
            (SupportLevel::Full, _, _, _) => full += 1,
            (SupportLevel::Partial, _, _, _) => partial += 1,
            (SupportLevel::Unsupported, _, _, _) => unsupported += 1,
            (SupportLevel::Error(_), _, _, _) => errors += 1,
        }

        let (level, nodes, interactive, title) = result;
        println!(
            "| {} | {} | {} | {} | {} | {} |",
            i + 1,
            url,
            nodes,
            interactive,
            title.as_deref().unwrap_or("-"),
            level,
        );
    }

    let total = TOP_SITES.len() as u32;
    println!("\n## Summary\n");
    println!("- **Full**: {full}/{total}");
    println!("- **Partial**: {partial}/{total}");
    println!("- **Unsupported**: {unsupported}/{total}");
    println!("- **Errors**: {errors}/{total}");
}

fn test_site(
    engine: &GhostEngine,
    url: &str,
) -> (SupportLevel, usize, usize, Option<String>) {
    let webview = match engine.new_webview(url) {
        Ok(wv) => wv,
        Err(e) => return (SupportLevel::Error(e.to_string()), 0, 0, None),
    };

    if let Err(e) = engine.load_and_wait(&webview) {
        return (SupportLevel::Error(e.to_string()), 0, 0, None);
    }

    let title = webview.page_title();

    match extract_layout(engine, &webview) {
        Ok(tree) => {
            let node_count = tree.nodes.len();
            let interactive_count = tree.nodes.iter().filter(|n| n.interactive).count();

            let level = if node_count >= 50 {
                SupportLevel::Full
            } else if node_count > 0 {
                SupportLevel::Partial
            } else {
                SupportLevel::Unsupported
            };

            (level, node_count, interactive_count, title)
        },
        Err(e) => (SupportLevel::Error(e.to_string()), 0, 0, title),
    }
}
