//! Performance benchmark suite for Ghost Engine.
//!
//! Measures cold-start latency, page-load times across all four fixture
//! tiers, layout-extraction throughput, and peak RSS memory.
//!
//! Run with:
//! ```sh
//! cargo test -p ghost-core --test perf_bench -- --nocapture --ignored
//! ```
//!
//! All benchmarks are `#[ignore]`d so they don't run in normal CI.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ghost_core::{GhostEngine, GhostEngineConfig};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn fixtures_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let dir = workspace_root.join("tests").join("ghost");
    assert!(dir.is_dir(), "fixtures dir missing: {}", dir.display());
    dir
}

fn fixture_url(relative: &str) -> String {
    let path = fixtures_dir().join(relative);
    assert!(path.is_file(), "fixture not found: {}", path.display());
    format!("file://{}", path.display())
}

/// Current process RSS in bytes (macOS via mach_task_info, Linux via /proc).
fn rss_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        use std::mem;
        unsafe extern "C" {
            fn mach_task_self() -> u32;
            fn task_info(
                target_task: u32,
                flavor: u32,
                task_info_out: *mut libc_task_basic_info,
                task_info_count: *mut u32,
            ) -> i32;
        }
        #[repr(C)]
        struct libc_task_basic_info {
            virtual_size: u64,
            resident_size: u64,
            resident_size_max: u64,
            user_time: [u32; 2],
            system_time: [u32; 2],
            policy: i32,
            suspend_count: i32,
        }
        const MACH_TASK_BASIC_INFO: u32 = 20;
        unsafe {
            let mut info: libc_task_basic_info = mem::zeroed();
            let mut count = (mem::size_of::<libc_task_basic_info>() / mem::size_of::<u32>()) as u32;
            let kr = task_info(
                mach_task_self(),
                MACH_TASK_BASIC_INFO,
                &mut info as *mut _ as *mut _,
                &mut count,
            );
            if kr == 0 {
                return Some(info.resident_size);
            }
        }
        None
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
            let rss_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
            let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
            return Some(rss_pages * page_size);
        }
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

fn rss_mb() -> f64 {
    rss_bytes().unwrap_or(0) as f64 / (1024.0 * 1024.0)
}

fn fast_config() -> GhostEngineConfig {
    GhostEngineConfig {
        settle_timeout: Duration::from_millis(500),
        quiet_period: Duration::from_millis(200),
        load_timeout: Duration::from_secs(15),
        ..Default::default()
    }
}

// All fixture paths grouped by tier.
const TIER1: &[&str] = &[
    "tier1_static/hello.html",
    "tier1_static/semantic_structure.html",
    "tier1_static/table_and_list.html",
];
const TIER2: &[&str] = &[
    "tier2_vanilla_js/dom_manipulation.html",
    "tier2_vanilla_js/async_timers.html",
    "tier2_vanilla_js/promises.html",
    "tier2_vanilla_js/event_listeners.html",
    "tier2_vanilla_js/json_render.html",
];
const TIER3: &[&str] = &[
    "tier3_react/counter.html",
    "tier3_react/todo_app.html",
    "tier3_react/async_fetch.html",
];
const TIER4: &[&str] = &[
    "tier4_heavy/deep_dom.html",
    "tier4_heavy/css_animations.html",
    "tier4_heavy/web_worker.html",
    "tier4_heavy/webgl_canvas.html",
    "tier4_heavy/error_recovery.html",
];

// ── TSK-3.10c: Cold-start benchmark ────────────────────────────────────────

#[test]
#[ignore]
fn bench_engine_cold_start() {
    println!("\n=== Ghost Engine Cold-Start Benchmark ===\n");

    let rss_before = rss_mb();
    let t0 = Instant::now();
    let engine = GhostEngine::new(fast_config()).expect("engine init failed");
    let init_dur = t0.elapsed();

    // Create one webview to force full initialization
    let t1 = Instant::now();
    let wv = engine
        .new_webview(&fixture_url("tier1_static/hello.html"))
        .expect("webview creation failed");
    let wv_dur = t1.elapsed();

    // Load first page
    let t2 = Instant::now();
    engine.load_and_wait(&wv).unwrap();
    let load_dur = t2.elapsed();

    let rss_after = rss_mb();

    println!("  GhostEngine::new()     : {:>8.1} ms", init_dur.as_secs_f64() * 1000.0);
    println!("  new_webview()          : {:>8.1} ms", wv_dur.as_secs_f64() * 1000.0);
    println!("  First page load        : {:>8.1} ms", load_dur.as_secs_f64() * 1000.0);
    println!("  ──────────────────────────────────");
    println!(
        "  Total cold→ready       : {:>8.1} ms  (target: < 200 ms + settle)",
        (init_dur + wv_dur + load_dur).as_secs_f64() * 1000.0
    );
    println!("  RSS before init        : {:>8.1} MB", rss_before);
    println!("  RSS after first load   : {:>8.1} MB", rss_after);
    println!("  RSS delta              : {:>8.1} MB  (target: < 150 MB)", rss_after - rss_before);
}

// ── TSK-3.10b: Page-load latency per tier ───────────────────────────────────

#[test]
#[ignore]
fn bench_page_load_latency() {
    println!("\n=== Page-Load Latency Benchmark ===\n");

    let engine = GhostEngine::new(fast_config()).expect("engine init failed");

    let tiers: &[(&str, &[&str])] = &[
        ("Tier 1 (static)", TIER1),
        ("Tier 2 (vanilla JS)", TIER2),
        ("Tier 3 (React)", TIER3),
        ("Tier 4 (heavy)", TIER4),
    ];

    println!("  {:<50} {:>10} {:>10}", "Fixture", "Load (ms)", "RSS (MB)");
    println!("  {}", "─".repeat(72));

    for (tier_name, fixtures) in tiers {
        let mut tier_total = Duration::ZERO;

        for fixture in *fixtures {
            let url = fixture_url(fixture);
            let wv = engine.new_webview(&url).expect("webview creation failed");

            let t0 = Instant::now();
            let result = engine.load_and_wait(&wv);
            let elapsed = t0.elapsed();

            let status = if result.is_ok() { "" } else { " [FAIL]" };
            let rss = rss_mb();

            println!(
                "  {:<50} {:>9.1} {:>9.1}{}",
                fixture,
                elapsed.as_secs_f64() * 1000.0,
                rss,
                status,
            );
            tier_total += elapsed;
        }

        let avg = tier_total.as_secs_f64() * 1000.0 / fixtures.len() as f64;
        println!(
            "  {:<50} {:>9.1}",
            format!("  ▸ {} avg", tier_name),
            avg,
        );
        println!();
    }
}

// ── TSK-3.10b: Layout extraction throughput ─────────────────────────────────

#[test]
#[ignore]
fn bench_extraction_throughput() {
    println!("\n=== Layout Extraction Throughput Benchmark ===\n");

    let engine = GhostEngine::new(fast_config()).expect("engine init failed");

    // Use a representative subset: one per tier.
    let fixtures = &[
        "tier1_static/semantic_structure.html",
        "tier2_vanilla_js/json_render.html",
        "tier3_react/todo_app.html",
        "tier4_heavy/deep_dom.html",
    ];

    println!(
        "  {:<50} {:>10} {:>10} {:>10}",
        "Fixture", "Nodes", "Extract", "ops/s"
    );
    println!("  {}", "─".repeat(82));

    for fixture in fixtures {
        let url = fixture_url(fixture);
        let wv = engine.new_webview(&url).expect("webview creation failed");
        engine.load_and_wait(&wv).unwrap();

        // Extract layout once to warm up, then measure 5 iterations.
        let js = ghost_interceptor::EXTRACT_LAYOUT_JS;
        let _ = engine.evaluate_js(&wv, js);

        let iterations = 5;
        let mut total = Duration::ZERO;
        let mut node_count = 0;

        for _ in 0..iterations {
            let t0 = Instant::now();
            let result = engine.evaluate_js(&wv, js);
            total += t0.elapsed();

            if let Ok(ghost_core::JSValue::String(ref s)) = result {
                // Count "tag" occurrences as a rough node count proxy.
                node_count = s.matches("\"tag\"").count();
            }
        }

        let avg_ms = total.as_secs_f64() * 1000.0 / iterations as f64;
        let ops_per_sec = if avg_ms > 0.0 {
            1000.0 / avg_ms
        } else {
            f64::INFINITY
        };

        println!(
            "  {:<50} {:>10} {:>9.1}ms {:>9.1}",
            fixture, node_count, avg_ms, ops_per_sec,
        );
    }
}

// ── TSK-3.10b: Combined summary with JSON output ───────────────────────────

#[test]
#[ignore]
fn bench_summary_json() {
    println!("\n=== Ghost Engine Benchmark Summary (JSON) ===\n");

    let rss_before = rss_mb();

    // Cold start
    let t0 = Instant::now();
    let engine = GhostEngine::new(fast_config()).expect("engine init failed");
    let init_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let all_fixtures: Vec<&str> = TIER1
        .iter()
        .chain(TIER2)
        .chain(TIER3)
        .chain(TIER4)
        .copied()
        .collect();

    let mut results = Vec::new();
    let mut peak_rss = rss_before;

    for fixture in &all_fixtures {
        let url = fixture_url(fixture);
        let wv = engine.new_webview(&url).expect("webview creation failed");

        let t0 = Instant::now();
        let load_ok = engine.load_and_wait(&wv).is_ok();
        let load_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let current_rss = rss_mb();
        if current_rss > peak_rss {
            peak_rss = current_rss;
        }

        results.push(format!(
            "    {{\"fixture\":\"{}\",\"load_ms\":{:.1},\"ok\":{}}}",
            fixture, load_ms, load_ok
        ));
    }

    let rss_after = rss_mb();

    // Emit machine-readable JSON.
    println!("{{");
    println!("  \"engine_init_ms\": {:.1},", init_ms);
    println!("  \"rss_before_mb\": {:.1},", rss_before);
    println!("  \"rss_after_mb\": {:.1},", rss_after);
    println!("  \"peak_rss_mb\": {:.1},", peak_rss);
    println!("  \"fixture_count\": {},", results.len());
    println!("  \"fixtures\": [");
    for (i, r) in results.iter().enumerate() {
        let comma = if i + 1 < results.len() { "," } else { "" };
        println!("{}{}", r, comma);
    }
    println!("  ]");
    println!("}}");
}
