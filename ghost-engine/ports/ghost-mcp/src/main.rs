//! Ghost MCP Server — exposes Ghost Engine as MCP tools for AI agents.
//!
//! ## Stdio mode (default)
//!
//! Communicates via JSON-RPC 2.0 over stdin/stdout with Content-Length
//! framing. Designed for use with Claude Desktop, VS Code Copilot, and
//! other MCP-compatible AI agents.
//!
//! ```sh
//! ghost-mcp
//! ```
//!
//! ## HTTP mode (remote)
//!
//! Start an HTTP server so any MCP client on the network can connect:
//!
//! ```sh
//! ghost-mcp --http                        # localhost:3100
//! ghost-mcp --http --port 8080            # localhost:8080
//! ghost-mcp --http --host 0.0.0.0         # all interfaces
//! ```
//!
//! Supports MCP Streamable HTTP (`POST /mcp`) and SSE (`GET /sse`).

mod http;
mod protocol;
mod schema;
mod server;

struct Args {
    http: bool,
    host: String,
    port: u16,
}

fn parse_args() -> Args {
    let mut args = Args {
        http: false,
        host: "127.0.0.1".to_string(),
        port: 3100,
    };

    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--http" => args.http = true,
            "--host" => {
                i += 1;
                if let Some(v) = raw.get(i) {
                    args.host = v.clone();
                }
            },
            "--port" => {
                i += 1;
                if let Some(v) = raw.get(i) {
                    args.port = v.parse().unwrap_or(3100);
                }
            },
            "--help" | "-h" => {
                eprintln!("ghost-mcp — Ghost Engine MCP Server\n");
                eprintln!("USAGE:");
                eprintln!("  ghost-mcp                     stdio mode (default)");
                eprintln!("  ghost-mcp --http              HTTP mode on 127.0.0.1:3100");
                eprintln!("  ghost-mcp --http --port 8080  custom port");
                eprintln!("  ghost-mcp --http --host 0.0.0.0  listen on all interfaces\n");
                eprintln!("OPTIONS:");
                eprintln!("  --http          Start HTTP server instead of stdio");
                eprintln!("  --host <ADDR>   Bind address (default: 127.0.0.1)");
                eprintln!("  --port <PORT>   Listen port (default: 3100)");
                eprintln!("  -h, --help      Show this help");
                std::process::exit(0);
            },
            _ => {},
        }
        i += 1;
    }

    args
}

fn main() {
    let args = parse_args();

    eprintln!("[ghost-mcp] initializing Ghost Engine…");

    let mut srv = match server::McpServer::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[ghost-mcp] fatal: engine init failed: {e}");
            std::process::exit(1);
        },
    };

    if args.http {
        // HTTP mode: start the HTTP server on a background thread,
        // process engine requests on the main thread.
        let (tx, rx) = std::sync::mpsc::channel();
        let host = args.host;
        let port = args.port;
        std::thread::spawn(move || http::serve(&host, port, tx));
        srv.run_channel(rx);
    } else {
        eprintln!("[ghost-mcp] ready — waiting for MCP messages on stdin");
        srv.run();
    }
}
