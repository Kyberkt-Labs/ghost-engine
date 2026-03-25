//! HTTP transport for Ghost MCP server.
//!
//! Supports two MCP transport modes for remote access over IP:
//!
//! - **Streamable HTTP** (`POST /mcp`): The modern MCP transport. Client sends
//!   JSON-RPC in the request body, server returns JSON-RPC response.
//!
//! - **SSE** (`GET /sse` + `POST /messages`): The older MCP transport. Client
//!   opens a persistent SSE connection, receives an endpoint URL, then sends
//!   JSON-RPC via POST. Responses arrive as SSE events.
//!
//! Both transports use the same engine queue — requests are serialized through
//! a channel to the main thread where GhostEngine lives.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::protocol::JsonRpcRequest;

/// A request queued for processing by the engine thread.
pub type EngineRequest = (JsonRpcRequest, mpsc::Sender<Option<Value>>);

/// Active SSE sessions, keyed by session ID.
type Sessions = Arc<Mutex<HashMap<String, mpsc::Sender<Value>>>>;

/// Start the HTTP server on `host:port`, forwarding all MCP requests through
/// `engine_tx` to the engine thread. This function blocks forever.
pub fn serve(host: &str, port: u16, engine_tx: mpsc::Sender<EngineRequest>) {
    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        eprintln!("[ghost-mcp] fatal: cannot bind {addr}: {e}");
        std::process::exit(1);
    });

    eprintln!("[ghost-mcp] HTTP server listening on http://{addr}");
    eprintln!("[ghost-mcp]   Streamable HTTP : POST http://{addr}/mcp");
    eprintln!("[ghost-mcp]   SSE transport   : GET  http://{addr}/sse");
    eprintln!("[ghost-mcp]   Health check    : GET  http://{addr}/health");

    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let tx = engine_tx.clone();
                let sess = Arc::clone(&sessions);
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, tx, sess) {
                        eprintln!("[ghost-mcp] connection error: {e}");
                    }
                });
            },
            Err(e) => eprintln!("[ghost-mcp] accept error: {e}"),
        }
    }
}

// ── Connection handling ─────────────────────────────────────────────────────

fn handle_connection(
    stream: TcpStream,
    engine_tx: mpsc::Sender<EngineRequest>,
    sessions: Sessions,
) -> Result<(), Box<dyn std::error::Error>> {
    let peer = stream.peer_addr().ok();
    let mut reader = BufReader::new(stream.try_clone()?);
    let (method, path, _headers, body) = parse_request(&mut reader)?;

    eprintln!(
        "[ghost-mcp] {method} {path} from {}",
        peer.map(|p| p.to_string()).unwrap_or_default()
    );

    match (method.as_str(), path.as_str()) {
        // CORS preflight
        ("OPTIONS", _) => send_cors_preflight(&stream)?,

        // Streamable HTTP transport
        ("POST", "/mcp") | ("POST", "/") => {
            handle_post_mcp(&stream, &body, &engine_tx)?;
        },

        // SSE transport: event stream
        ("GET", "/sse") => {
            handle_sse_get(stream, &sessions)?;
        },

        // SSE transport: message endpoint
        ("POST", p) if p.starts_with("/messages") => {
            let session_id = extract_query_param(p, "sessionId");
            handle_post_messages(&stream, &body, session_id.as_deref(), &engine_tx, &sessions)?;
        },

        // Health check
        ("GET", "/health") => {
            send_json_response(&stream, 200, &serde_json::json!({"status": "ok"}))?;
        },

        _ => {
            send_text_response(&stream, 404, "Not Found")?;
        },
    }

    Ok(())
}

// ── Streamable HTTP (POST /mcp) ─────────────────────────────────────────────

fn handle_post_mcp(
    stream: &TcpStream,
    body: &[u8],
    engine_tx: &mpsc::Sender<EngineRequest>,
) -> Result<(), Box<dyn std::error::Error>> {
    let request: JsonRpcRequest = serde_json::from_slice(body)
        .map_err(|e| format!("invalid JSON-RPC: {e}"))?;
    let is_notification = request.id.is_none();

    let (tx, rx) = mpsc::channel();
    engine_tx.send((request, tx))?;

    if is_notification {
        send_text_response(stream, 202, "")?;
    } else {
        match rx.recv()? {
            Some(response) => send_json_response(stream, 200, &response)?,
            None => send_text_response(stream, 202, "")?,
        }
    }

    Ok(())
}

// ── SSE transport ───────────────────────────────────────────────────────────

fn handle_sse_get(
    mut stream: TcpStream,
    sessions: &Sessions,
) -> Result<(), Box<dyn std::error::Error>> {
    let session_id = generate_session_id();

    // Send SSE response headers.
    let header = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/event-stream\r\n\
         Cache-Control: no-cache\r\n\
         Connection: keep-alive\r\n\
         Access-Control-Allow-Origin: *\r\n\
         \r\n"
    );
    stream.write_all(header.as_bytes())?;

    // First event: tell the client where to POST messages.
    write!(stream, "event: endpoint\ndata: /messages?sessionId={session_id}\n\n")?;
    stream.flush()?;

    eprintln!("[ghost-mcp] SSE session {session_id} connected");

    // Register this connection so POST /messages can push responses to it.
    let (tx, rx) = mpsc::channel::<Value>();
    sessions.lock().unwrap().insert(session_id.clone(), tx);

    // Forward engine responses as SSE events until the client disconnects.
    for response in rx {
        let data = serde_json::to_string(&response).unwrap_or_default();
        if write!(stream, "event: message\ndata: {data}\n\n").is_err() {
            break;
        }
        if stream.flush().is_err() {
            break;
        }
    }

    sessions.lock().unwrap().remove(&session_id);
    eprintln!("[ghost-mcp] SSE session {session_id} disconnected");

    Ok(())
}

fn handle_post_messages(
    stream: &TcpStream,
    body: &[u8],
    session_id: Option<&str>,
    engine_tx: &mpsc::Sender<EngineRequest>,
    sessions: &Sessions,
) -> Result<(), Box<dyn std::error::Error>> {
    let request: JsonRpcRequest = serde_json::from_slice(body)
        .map_err(|e| format!("invalid JSON-RPC: {e}"))?;
    let is_notification = request.id.is_none();

    let (tx, rx) = mpsc::channel();
    engine_tx.send((request, tx))?;

    if is_notification {
        send_text_response(stream, 202, "")?;
        return Ok(());
    }

    match rx.recv()? {
        Some(response) => {
            // Deliver to SSE stream too, if a session is active.
            if let Some(sid) = session_id {
                if let Some(sse_tx) = sessions.lock().unwrap().get(sid) {
                    sse_tx.send(response.clone()).ok();
                }
            }
            send_json_response(stream, 200, &response)?;
        },
        None => {
            send_text_response(stream, 202, "")?;
        },
    }

    Ok(())
}

// ── HTTP parsing helpers ────────────────────────────────────────────────────

fn parse_request(
    reader: &mut BufReader<TcpStream>,
) -> Result<(String, String, HashMap<String, String>, Vec<u8>), Box<dyn std::error::Error>> {
    // Request line: `METHOD /path HTTP/1.1\r\n`
    let mut request_line = String::new();
    let n = reader.read_line(&mut request_line)?;
    if n == 0 {
        return Err("empty request".into());
    }
    let parts: Vec<&str> = request_line.trim().splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err("malformed request line".into());
    }
    let method = parts[0].to_string();
    let path = parts[1].to_string();

    // Headers
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.trim().is_empty() {
            break;
        }
        if let Some((key, val)) = line.split_once(':') {
            headers.insert(key.trim().to_lowercase(), val.trim().to_string());
        }
    }

    // Body (only if Content-Length present)
    let content_length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }

    Ok((method, path, headers, body))
}

// ── HTTP response helpers ───────────────────────────────────────────────────

const CORS_HEADERS: &str = "\
    Access-Control-Allow-Origin: *\r\n\
    Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
    Access-Control-Allow-Headers: Content-Type, Mcp-Session-Id\r\n";

fn send_json_response(
    stream: &TcpStream,
    status: u16,
    body: &Value,
) -> std::io::Result<()> {
    let body_bytes = serde_json::to_vec(body).unwrap();
    let status_text = status_phrase(status);
    let mut w = stream;
    write!(
        w,
        "HTTP/1.1 {status} {status_text}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         {CORS_HEADERS}\
         \r\n",
        body_bytes.len()
    )?;
    w.write_all(&body_bytes)?;
    w.flush()
}

fn send_text_response(
    stream: &TcpStream,
    status: u16,
    text: &str,
) -> std::io::Result<()> {
    let status_text = status_phrase(status);
    let mut w = stream;
    write!(
        w,
        "HTTP/1.1 {status} {status_text}\r\n\
         Content-Type: text/plain\r\n\
         Content-Length: {}\r\n\
         {CORS_HEADERS}\
         \r\n\
         {text}",
        text.len()
    )?;
    w.flush()
}

fn send_cors_preflight(stream: &TcpStream) -> std::io::Result<()> {
    let mut w = stream;
    write!(
        w,
        "HTTP/1.1 204 No Content\r\n\
         {CORS_HEADERS}\
         Access-Control-Max-Age: 86400\r\n\
         \r\n"
    )?;
    w.flush()
}

fn status_phrase(code: u16) -> &'static str {
    match code {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        404 => "Not Found",
        _ => "OK",
    }
}

// ── Misc helpers ────────────────────────────────────────────────────────────

fn extract_query_param(path: &str, key: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn generate_session_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("gs-{ts:x}-{n:x}")
}
