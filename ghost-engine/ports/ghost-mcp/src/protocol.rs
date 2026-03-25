//! JSON-RPC 2.0 framing for MCP stdio transport.
//!
//! Uses Content-Length header framing (same as LSP), which is what the
//! official MCP SDKs expect.

use std::io::{self, BufRead, Write};

use serde::Deserialize;
use serde_json::Value;

/// An incoming JSON-RPC request or notification.
#[derive(Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    /// `None` for notifications (no response expected).
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Read one Content-Length–framed JSON-RPC message from `reader`.
///
/// Returns `Ok(None)` on EOF.
pub fn read_message(reader: &mut impl BufRead) -> io::Result<Option<JsonRpcRequest>> {
    let mut content_length: Option<usize> = None;

    // Read headers until blank line.
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            return Ok(None); // EOF
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break; // end of headers
        }
        if let Some(val) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                val.trim()
                    .parse()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?,
            );
        }
        // Ignore unrecognised headers (Content-Type, etc.)
    }

    let len = content_length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;

    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;

    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Write a Content-Length–framed JSON-RPC response to `writer`.
pub fn send_response(writer: &mut impl Write, response: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(response)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}
