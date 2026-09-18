// Phase 0a spike — the Rust half of the Atrium bridge.
//
// DESIGN.md §2 / DECISIONS.md §Bridge: local socket, JSON messages, not FFI.
//
// Deliberately boring, single-client TCP server on 127.0.0.1.
// Not a concurrency subsystem: exactly one client is expected (the Flutter
// app). This is plumbing, not the lock manager.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

/// Envelope for every message on the bridge (both directions).
/// serde enforces the shape: {"type":"echo","text":"..."} or {"type":"ping"}.
/// Unknown fields are rejected; missing "text" defaults to None.
#[derive(serde::Deserialize, serde::Serialize)]
struct Envelope {
    /// "echo" or "ping" from the client; "pong", "echo" or "app_error" back.
    r#type: String,
    /// Payload; only meaningful for "echo" and error replies.
    #[serde(default)]
    text: Option<String>,
}

fn main() {
    // 127.0.0.1 only: never binds to an interface reachable from the network.
    // Port hardcoded so the Flutter side has a fixed target to dial.
    let listener = match TcpListener::bind("127.0.0.1:42321") {
        Ok(l) => l,
        Err(e) => {
            eprintln!("FATAL: cannot bind 127.0.0.1:42321: {e}");
            std::process::exit(1);
        }
    };

    println!("listening on 127.0.0.1:42321");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let peer = stream
                    .peer_addr()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|e| format!("<no peer addr: {e}>"));
                println!("client connected: {peer}");
                handle_client(stream, &peer);
            }
            Err(e) => eprintln!("accept error: {e}"),
        }
    }
}

/// Serves one client until it disconnects.
///
/// NOTE: this blocks the accept loop — a second client waits until the first
/// disconnects. That is deliberate: the spike expects exactly one client.
/// Do not "fix" this into a thread pool; concurrency belongs to later phases.
fn handle_client(stream: TcpStream, peer: &str) {
    let mut reader = BufReader::new(stream.try_clone().expect("stream clone for reader"));
    let mut writer = stream;
    loop {
        // Newline-delimited JSON: one line = one request envelope.
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                println!("[{peer}] client disconnected");
                return;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue; // tolerate blank separator lines
                }
                println!("[{peer}] recv: {trimmed}");
                let response: Envelope = match serde_json::from_str(trimmed) {
                    Ok(env) => dispatch(env),
                    // Malformed JSON: reply with a reason, keep the connection.
                    Err(e) => Envelope {
                        r#type: "app_error".into(),
                        text: Some(format!("malformed JSON: {e}")),
                    },
                };
                // to_string cannot fail on Envelope (no map keys, no cycles).
                let mut out = serde_json::to_string(&response).expect("serialize envelope");
                out.push('\n');
                if let Err(e) = writer.write_all(out.as_bytes()) {
                    eprintln!("[{peer}] write error: {e}");
                    return;
                }
                println!("[{peer}] sent: {}", out.trim_end());
            }
            Err(e) => {
                eprintln!("[{peer}] read error: {e}");
                return;
            }
        }
    }
}

fn dispatch(env: Envelope) -> Envelope {
    match env.r#type.as_str() {
        "ping" => Envelope { r#type: "pong".into(), text: None },
        "echo" => Envelope { r#type: "echo".into(), text: Some(env.text.unwrap_or_default()) },
        other => Envelope {
            r#type: "app_error".into(),
            // Unknown message type: report rather than silently drop.
            text: Some(format!("unknown message type: {other}")),
        },
    }
}