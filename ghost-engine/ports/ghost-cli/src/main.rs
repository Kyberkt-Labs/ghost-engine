use std::io::{BufRead, Write};
use std::process::ExitCode;

use bpaf::Bpaf;
use ghost_core::{GhostEngine, GhostEngineConfig, GhostWebView, LoadStatus, VERSION};
use ghost_interact::{Action, ActionResult, SpecialKey, execute, extract_and_stamp};
use ghost_serializer::{to_json, to_markdown};

/// Ghost Engine — a headless browser for AI agents, powered by Servo.
#[derive(Debug, Clone, Bpaf)]
#[bpaf(options, version(VERSION))]
pub struct CliArgs {
    /// URL to load in the headless browser.
    #[bpaf(positional("URL"))]
    pub url: String,

    /// Viewport width in device pixels.
    #[bpaf(long("width"), fallback(1920))]
    pub width: u32,

    /// Viewport height in device pixels.
    #[bpaf(long("height"), fallback(1080))]
    pub height: u32,

    /// Timeout in seconds for page load.
    #[bpaf(long("timeout"), fallback(30))]
    pub timeout: u64,

    /// Seconds to keep spinning after load for async JS to settle.
    #[bpaf(long("settle"), fallback(2))]
    pub settle: u64,

    /// Quiet-period in milliseconds: if no engine activity for this long
    /// during the settle phase, assume JS is idle and stop early.
    #[bpaf(long("quiet"), fallback(500))]
    pub quiet: u64,

    /// Output format: "json", "markdown", or "tree" (default: "tree").
    #[bpaf(long("format"), argument::<String>("FORMAT"), fallback("tree".to_string()))]
    pub format: String,

    /// Enter interactive REPL mode after initial page load.
    #[bpaf(long("interactive"), short('i'))]
    pub interactive: bool,

    /// Tracing/log filter directive (e.g. "servo=debug").
    #[bpaf(long("log-filter"), argument::<String>("FILTER"), optional)]
    pub log_filter: Option<String>,
}

fn main() -> ExitCode {
    let args = cli_args().run();

    let config = GhostEngineConfig {
        viewport_width: args.width,
        viewport_height: args.height,
        tracing_filter: args.log_filter,
        load_timeout: std::time::Duration::from_secs(args.timeout),
        settle_timeout: std::time::Duration::from_secs(args.settle),
        quiet_period: std::time::Duration::from_millis(args.quiet),
        user_agent: None,
        ..Default::default()
    };

    eprintln!("Ghost Engine v{VERSION}");
    eprintln!(
        "Viewport: {}x{}, timeout: {}s",
        config.viewport_width, config.viewport_height, args.timeout
    );

    let engine = match GhostEngine::new(config) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("fatal: {err}");
            return ExitCode::FAILURE;
        },
    };

    eprintln!("Loading: {}", args.url);
    let webview = match engine.new_webview(&args.url) {
        Ok(wv) => wv,
        Err(err) => {
            eprintln!("fatal: {err}");
            return ExitCode::FAILURE;
        },
    };

    // Log each load-status transition as it happens.
    webview.set_on_load_status(Some(Box::new(|status, progress| {
        let elapsed = progress.initiated_at.elapsed();
        match status {
            LoadStatus::Started => {
                eprintln!("[{elapsed:.2?}] load started");
            },
            LoadStatus::HeadParsed => {
                eprintln!("[{elapsed:.2?}] <head> parsed, <body> available");
            },
            LoadStatus::Complete => {
                eprintln!("[{elapsed:.2?}] page complete (all sub-resources loaded)");
            },
        }
    })));

    let load_result = engine.load_and_wait(&webview);
    if let Err(err) = &load_result {
        match err {
            ghost_core::GhostError::Crashed { backtrace, .. } => {
                eprintln!("fatal: {err}");
                if let Some(bt) = backtrace {
                    eprintln!("backtrace:\n{bt}");
                }
                return ExitCode::FAILURE;
            },
            _ => {
                // Timeouts and other non-fatal errors: warn but continue
                // (the page may still be partially usable).
                eprintln!("warning: {err} — continuing with partial page");
            },
        }
    }

    let progress = webview.load_progress();
    eprintln!(
        "Page loaded — title: {:?}, url: {:?}",
        webview.page_title().as_deref().unwrap_or("<none>"),
        webview.url().map(|u| u.to_string()).unwrap_or_default(),
    );
    if let (Some(start), Some(end)) = (progress.started_at, progress.complete_at) {
        eprintln!("Load duration: {:.2?}", end.duration_since(start));
    }

    // Extract the layout tree via JS injection.
    let tree = match extract_and_stamp(&engine, &webview) {
        Ok(tree) => tree,
        Err(err) => {
            eprintln!("layout extraction failed: {err}");
            return ExitCode::FAILURE;
        },
    };

    eprintln!("Extracted {} visible nodes", tree.len());

    let fmt = args.format.as_str();
    print_tree(&tree, fmt);

    if args.interactive {
        run_repl(&engine, &webview, fmt);
    }

    ExitCode::SUCCESS
}

fn print_tree(tree: &ghost_interceptor::LayoutTree, fmt: &str) {
    match fmt {
        "json" => println!("{}", to_json(tree)),
        "markdown" | "md" => print!("{}", to_markdown(tree)),
        _ => print_layout_tree(tree),
    }
}

/// Print a compact text representation of the extracted layout tree to stdout.
fn print_layout_tree(tree: &ghost_interceptor::LayoutTree) {
    if let Some(url) = &tree.url {
        println!("url: {url}");
    }
    if let Some(title) = &tree.title {
        println!("title: {title}");
    }
    println!("nodes: {}", tree.len());
    println!();

    if let Some(root_idx) = tree.root_index() {
        print_node(tree, root_idx, 0);
    }
}

fn print_node(tree: &ghost_interceptor::LayoutTree, idx: usize, depth: usize) {
    let node = &tree.nodes[idx];
    let indent = "  ".repeat(depth);
    let mut label = node.tag.clone();

    if let Some(id) = &node.id {
        label.push_str(&format!("#{id}"));
    }
    if let Some(cls) = &node.class {
        for c in cls.split_whitespace() {
            label.push_str(&format!(".{c}"));
        }
    }

    let geo = format!(
        "[{},{} {}x{}]",
        node.rect.x, node.rect.y, node.rect.w, node.rect.h
    );

    let mut extra = String::new();
    if node.interactive {
        extra.push_str(" *interactive*");
    }
    if let Some(text) = &node.text {
        let truncated: String = text.chars().take(60).collect();
        extra.push_str(&format!(" \"{truncated}\""));
    }
    if let Some(href) = &node.href {
        extra.push_str(&format!(" href={href}"));
    }
    if let Some(role) = &node.role {
        extra.push_str(&format!(" role={role}"));
    }

    println!("{indent}{label} {geo}{extra}");

    for &child_idx in &node.children {
        print_node(tree, child_idx, depth + 1);
    }
}

// ── Interactive REPL ────────────────────────────────────────────────────────

fn run_repl(engine: &GhostEngine, webview: &GhostWebView, fmt: &str) {
    eprintln!("\nInteractive mode. Commands:");
    eprintln!("  click <id>              — click element");
    eprintln!("  hover <id>              — hover element");
    eprintln!("  focus <id>              — focus element");
    eprintln!("  type <id> <text>        — type text into element");
    eprintln!("  key <id> <key>          — press special key (Enter, Tab, Escape, ...)");
    eprintln!("  scroll <id>             — scroll element into view");
    eprintln!("  scrollby <dx> <dy>      — scroll viewport by pixels");
    eprintln!("  select <id> <value>     — select option by value");
    eprintln!("  check <id>              — check checkbox/radio");
    eprintln!("  uncheck <id>            — uncheck checkbox/radio");
    eprintln!("  nav <url>               — navigate to URL");
    eprintln!("  back                    — go back");
    eprintln!("  forward                 — go forward");
    eprintln!("  reload                  — reload page");
    eprintln!("  cookies                 — list cookies");
    eprintln!("  extract                 — re-extract and print layout");
    eprintln!("  js <code>               — evaluate JavaScript");
    eprintln!("  help                    — show this help");
    eprintln!("  quit                    — exit");

    let stdin = std::io::stdin();

    // Show initial prompt
    eprint!("\nghost> ");
    let _ = std::io::stderr().flush();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            eprint!("ghost> ");
            let _ = std::io::stderr().flush();
            continue;
        }

        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        let cmd = parts[0];

        let result = match cmd {
            "quit" | "exit" | "q" => break,

            "help" | "?" => {
                eprintln!("  click <id>     hover <id>      focus <id>");
                eprintln!("  type <id> <t>  key <id> <key>  select <id> <val>");
                eprintln!("  check <id>     uncheck <id>    scroll <id>");
                eprintln!("  scrollby <dx> <dy>             extract [json|markdown]");
                eprintln!("  nav <url>      back   forward  reload");
                eprintln!("  cookies        js <code>       help   quit");
                eprint!("ghost> ");
                let _ = std::io::stderr().flush();
                continue;
            },

            "click" => parse_id(&parts).and_then(|id| execute(engine, webview, &Action::Click(id))),
            "hover" => parse_id(&parts).and_then(|id| execute(engine, webview, &Action::Hover(id))),
            "focus" => parse_id(&parts).and_then(|id| execute(engine, webview, &Action::Focus(id))),

            "type" => {
                if parts.len() < 3 {
                    Err(ghost_core::GhostError::JavaScript("usage: type <id> <text>".into()))
                } else {
                    parse_id(&parts).and_then(|id| {
                        execute(engine, webview, &Action::Type(id, parts[2].to_string()))
                    })
                }
            },

            "key" => {
                if parts.len() < 3 {
                    Err(ghost_core::GhostError::JavaScript("usage: key <id> <keyname>".into()))
                } else {
                    parse_id(&parts).and_then(|id| {
                        match parse_special_key(parts[2]) {
                            Some(key) => execute(engine, webview, &Action::PressKey(id, key)),
                            None => Err(ghost_core::GhostError::JavaScript(
                                format!("unknown key: {}. Use Enter, Tab, Escape, Backspace, Delete, ArrowUp/Down/Left/Right, Home, End, PageUp, PageDown", parts[2])
                            )),
                        }
                    })
                }
            },

            "scroll" => parse_id(&parts).and_then(|id| execute(engine, webview, &Action::ScrollTo(id))),

            "scrollby" => {
                if parts.len() < 3 {
                    Err(ghost_core::GhostError::JavaScript("usage: scrollby <dx> <dy>".into()))
                } else {
                    let dx = parts[1].parse::<i32>().unwrap_or(0);
                    let dy = parts[2].parse::<i32>().unwrap_or(0);
                    execute(engine, webview, &Action::ScrollBy(dx, dy))
                }
            },

            "select" => {
                if parts.len() < 3 {
                    Err(ghost_core::GhostError::JavaScript("usage: select <id> <value>".into()))
                } else {
                    parse_id(&parts).and_then(|id| {
                        execute(engine, webview, &Action::SelectOption(id, parts[2].to_string()))
                    })
                }
            },

            "check" => parse_id(&parts).and_then(|id| execute(engine, webview, &Action::Check(id))),
            "uncheck" => parse_id(&parts).and_then(|id| execute(engine, webview, &Action::Uncheck(id))),

            "nav" | "navigate" => {
                if parts.len() < 2 {
                    Err(ghost_core::GhostError::JavaScript("usage: nav <url>".into()))
                } else {
                    execute(engine, webview, &Action::Navigate(parts[1].to_string()))
                }
            },

            "back" => execute(engine, webview, &Action::GoBack),
            "forward" => execute(engine, webview, &Action::GoForward),
            "reload" => execute(engine, webview, &Action::Reload),

            "cookies" => execute(engine, webview, &Action::GetCookies),

            "extract" => {
                let extract_fmt = if parts.len() >= 2 {
                    match parts[1] {
                        "json" => "json",
                        "markdown" | "md" => "markdown",
                        _ => fmt,
                    }
                } else {
                    fmt
                };
                match extract_and_stamp(engine, webview) {
                    Ok(tree) => {
                        eprintln!("Extracted {} visible nodes", tree.len());
                        print_tree(&tree, extract_fmt);
                        Ok(ActionResult::Ok)
                    },
                    Err(e) => Err(e),
                }
            },

            "js" => {
                if parts.len() < 2 {
                    Err(ghost_core::GhostError::JavaScript("usage: js <code>".into()))
                } else {
                    // Rejoin from the original line after "js "
                    let code = &line[cmd.len()..].trim_start();
                    match engine.evaluate_js(webview, code) {
                        Ok(val) => {
                            println!("{val:?}");
                            Ok(ActionResult::Ok)
                        },
                        Err(e) => Err(e),
                    }
                }
            },

            other => {
                eprintln!("unknown command: {other}. Type 'help' for available commands.");
                eprint!("ghost> ");
                let _ = std::io::stderr().flush();
                continue;
            },
        };

        match result {
            Ok(ActionResult::Ok) => eprintln!("ok"),
            Ok(ActionResult::Navigated) => {
                eprintln!("navigated — re-extracting...");
                match extract_and_stamp(engine, webview) {
                    Ok(tree) => {
                        eprintln!("Extracted {} visible nodes", tree.len());
                        print_tree(&tree, fmt);
                    },
                    Err(e) => eprintln!("extraction failed: {e}"),
                }
            },
            Ok(ActionResult::Cookies(cookies)) => {
                for c in &cookies {
                    println!("{}={}", c.name, c.value);
                }
                eprintln!("{} cookie(s)", cookies.len());
            },
            Err(e) => eprintln!("error: {e}"),
        }

        eprint!("ghost> ");
        let _ = std::io::stderr().flush();
    }

    eprintln!("bye");
}

fn parse_id(parts: &[&str]) -> Result<u32, ghost_core::GhostError> {
    if parts.len() < 2 {
        return Err(ghost_core::GhostError::JavaScript("missing ghost-id argument".into()));
    }
    parts[1]
        .parse::<u32>()
        .map_err(|_| ghost_core::GhostError::JavaScript(format!("invalid ghost-id: {}", parts[1])))
}

fn parse_special_key(s: &str) -> Option<SpecialKey> {
    match s.to_lowercase().as_str() {
        "enter" | "return" => Some(SpecialKey::Enter),
        "escape" | "esc" => Some(SpecialKey::Escape),
        "tab" => Some(SpecialKey::Tab),
        "backspace" => Some(SpecialKey::Backspace),
        "delete" | "del" => Some(SpecialKey::Delete),
        "arrowup" | "up" => Some(SpecialKey::ArrowUp),
        "arrowdown" | "down" => Some(SpecialKey::ArrowDown),
        "arrowleft" | "left" => Some(SpecialKey::ArrowLeft),
        "arrowright" | "right" => Some(SpecialKey::ArrowRight),
        "home" => Some(SpecialKey::Home),
        "end" => Some(SpecialKey::End),
        "pageup" => Some(SpecialKey::PageUp),
        "pagedown" => Some(SpecialKey::PageDown),
        _ => None,
    }
}
