// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! `apollinaire mcp --live`: a bridge from the agent's MCP tools to the browser editor, which
//! connects over a WebSocket on 127.0.0.1. Nothing listens until `live_connect`, which binds a
//! port the system picks and makes a 256-bit token from the system's random source; the user
//! enters both in the editor. A connection is let in only with the editor's Origin and a
//! loopback Host, and only once it sends the token within the admission time. One signed-in
//! editor holds the bridge; a later one with the token replaces it. The editor checks and
//! applies every write itself (`src/live_edit.rs`), as one undo step.

use serde_json::{Value, json};
use std::{
    hash::{BuildHasher, RandomState},
    io,
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};
use tungstenite::{
    Message, WebSocket,
    handshake::server::{ErrorResponse, Request, Response},
    http::StatusCode,
    protocol::{CloseFrame, WebSocketConfig, frame::coding::CloseCode},
};

/// The published editor; local builds are served from 127.0.0.1 or localhost on any port.
const EDITOR: &str = "https://yungsnat.github.io";
const EDITOR_PAGE: &str = "https://yungsnat.github.io/ApollinaireASCII/";
/// From accepting a connection to receiving its token.
const ADMISSION: Duration = Duration::from_secs(5);
/// Connections being admitted at once; a newer one closes one of them at random.
const ADMITTING: usize = 32;
/// What a signed-in editor is sent first.
const HELLO: &str = concat!(
    r#"{"hello":"apollinaire","version":""#,
    env!("CARGO_PKG_VERSION"),
    r#""}"#
);
/// A connection being admitted: its number, a copy of its stream and its admission's flag.
type Pending = (u64, TcpStream, Arc<AtomicBool>);
/// The longest message read before the token is checked; the token is 64 characters.
const TOKEN_LIMIT: usize = 128;
/// How long the editor has to answer a request.
const ANSWER: Duration = Duration::from_secs(10);
const NOT_OPEN: &str = "The bridge is not open: call live_connect first, then enter its port and token in the editor's Connect an agent box.";
const NO_EDITOR: &str = "No editor is connected: enter the port and token from live_connect in the editor's Connect an agent box.";

type Socket = WebSocket<TcpStream>;
type Slot = Arc<Mutex<Option<Socket>>>;

#[derive(Default)]
pub struct Bridge {
    open: Option<Open>,
    next: u64,
}

struct Open {
    port: u16,
    /// The token, kept in this process only.
    code: Arc<str>,
    slot: Slot,
}

/// The tools `--live` adds, as `tools/list` gives them.
pub fn tools() -> Vec<Value> {
    let tool = |name: &str, description: &str, properties: Value, required: &[&str], read: bool| {
        json!({
            "name": name,
            "description": description,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            },
            "annotations": {
                "readOnlyHint": read,
                "destructiveHint": false,
                "idempotentHint": read,
                "openWorldHint": false,
            },
        })
    };
    vec![
        tool(
            "live_connect",
            "Open the bridge to the browser editor and give its port and token, which the user enters in the editor's Connect an agent box. Calling it again gives the same port and token.",
            json!({}),
            &[],
            false,
        ),
        tool(
            "live_read",
            "The active layer's frame on screen in the connected editor, as JSON: revision, layer, frame, frames, width, height and text. Playback pauses.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "live_write",
            "Replace the active layer's frame on screen in the editor, or the block at x and y, with printable ASCII text, as one undo step. Refused unless revision is the one live_read gave for the frame as it is now. Replacing the whole frame can change its size only in a project of one frame and one layer.",
            json!({
                "revision": {"type": "string", "description": "From live_read."},
                "text": {"type": "string", "description": "Rows separated by line feeds; shorter rows are padded with spaces."},
                "x": {"type": "integer", "minimum": 0, "description": "Column of the block's left edge; give y too."},
                "y": {"type": "integer", "minimum": 0, "description": "Row of the block's top edge; give x too."},
            }),
            &["revision", "text"],
            false,
        ),
    ]
}

/// A tool result in the shape `mcp.rs` gives every tool.
fn result(text: impl Into<String>, failed: bool) -> Value {
    json!({"content": [{"type": "text", "text": text.into()}], "isError": failed})
}

impl Bridge {
    /// The result of a live tool, or `None` for another tool's name.
    pub fn call(
        &mut self,
        name: &str,
        arguments: &serde_json::Map<String, Value>,
    ) -> Option<Value> {
        let outcome = match name {
            "live_connect" if arguments.is_empty() => self.connect(),
            "live_connect" => Err("live_connect takes no arguments".into()),
            "live_read" if arguments.is_empty() => self.ask(json!({"op": "read"})),
            "live_read" => Err("live_read takes no arguments".into()),
            "live_write" => write_request(arguments).and_then(|request| self.ask(request)),
            _ => return None,
        };
        Some(match outcome {
            Ok(text) => result(text, false),
            Err(error) => result(error, true),
        })
    }

    fn connect(&mut self) -> Result<String, String> {
        if self.open.is_none() {
            let listener = TcpListener::bind(("127.0.0.1", 0))
                .map_err(|e| format!("Cannot open the bridge: {e}"))?;
            let port = listener
                .local_addr()
                .map_err(|e| format!("Cannot open the bridge: {e}"))?
                .port();
            let open = Open {
                port,
                code: fresh_code()?.into(),
                slot: Slot::default(),
            };
            let (code, slot) = (open.code.clone(), open.slot.clone());
            thread::spawn(move || accept(&listener, port, &code, &slot));
            self.open = Some(open);
        }
        let open = self.open.as_ref().expect("opened above");
        Ok(format!(
            "Bridge open on port {}, token {}. Open the editor at {EDITOR_PAGE} or a local build, choose Connect an agent and enter both; then use live_read and live_write.",
            open.port, open.code
        ))
    }

    /// Send a request to the editor and give its result as JSON text.
    fn ask(&mut self, mut request: Value) -> Result<String, String> {
        let open = self.open.as_ref().ok_or(NOT_OPEN)?;
        let mut slot = open.slot.lock().unwrap_or_else(PoisonError::into_inner);
        let socket = slot.as_mut().ok_or(NO_EDITOR)?;
        self.next += 1;
        request["id"] = json!(self.next);
        match exchange(socket, &request, self.next) {
            Ok(result) => Ok(result.to_string()),
            Err(Failure::Answer(error)) => Err(error),
            Err(Failure::Lost(error)) => {
                *slot = None;
                Err(format!("The editor disconnected: {error}"))
            }
        }
    }
}

fn write_request(arguments: &serde_json::Map<String, Value>) -> Result<Value, String> {
    if let Some(key) = arguments
        .keys()
        .find(|key| !["revision", "text", "x", "y"].contains(&key.as_str()))
    {
        return Err(format!("Unknown argument {key}"));
    }
    let mut request = json!({"op": "write"});
    for (key, value) in arguments {
        request[key] = value.clone();
    }
    Ok(request)
}

/// 32 bytes from the system's random source, in hexadecimal.
fn fresh_code() -> Result<String, String> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("No random source: {e}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Equal, in time that depends only on the lengths.
fn same(given: &[u8], token: &[u8]) -> bool {
    let differ = given.iter().zip(token).fold(0, |acc, (a, b)| acc | (a ^ b));
    given.len() == token.len() && std::hint::black_box(differ) == 0
}

enum Failure {
    /// The editor answered with an error, or not in time; the connection stays.
    Answer(String),
    /// The connection is gone.
    Lost(String),
}

fn exchange(socket: &mut Socket, request: &Value, id: u64) -> Result<Value, Failure> {
    let lost = |e: tungstenite::Error| Failure::Lost(e.to_string());
    socket
        .send(Message::text(request.to_string()))
        .map_err(lost)?;
    let deadline = Instant::now() + ANSWER;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Failure::Answer(format!(
                "The editor did not answer within {} seconds; a write may still apply, so read again before trying it again",
                ANSWER.as_secs()
            )));
        }
        socket
            .get_mut()
            .set_read_timeout(Some(left))
            .map_err(|e| Failure::Lost(e.to_string()))?;
        match socket.read() {
            Ok(Message::Text(text)) => {
                // An answer to an earlier request that timed out is skipped.
                let Ok(reply) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if reply["id"] != json!(id) {
                    continue;
                }
                return match reply.get("error") {
                    Some(error) => Err(Failure::Answer(
                        error.as_str().unwrap_or("The editor refused").to_owned(),
                    )),
                    None => Ok(reply["result"].clone()),
                };
            }
            Ok(Message::Close(_)) => return Err(Failure::Lost("closed".into())),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if timed_out(&e) => {}
            Err(e) => return Err(lost(e)),
        }
    }
}

fn timed_out(error: &io::Error) -> bool {
    [io::ErrorKind::WouldBlock, io::ErrorKind::TimedOut].contains(&error.kind())
}

fn accept(listener: &TcpListener, port: u16, token: &Arc<str>, slot: &Slot) {
    let admitting: Arc<Mutex<Vec<Pending>>> = Arc::default();
    // Picks which admission to close when the limit is reached; randomly keyed for each run.
    let pick = RandomState::new();
    // Out of file descriptors, say: wait rather than spin.
    let wait = |_| thread::sleep(Duration::from_millis(50));
    let pair = |s: TcpStream| Ok((s.try_clone()?, s));
    let streams = listener
        .incoming()
        .filter_map(|s| s.and_then(pair).map_err(wait).ok());
    for (id, (copy, stream)) in (0..).zip(streams) {
        let done = Arc::new(AtomicBool::new(false));
        {
            let mut queue = admitting.lock().unwrap_or_else(PoisonError::into_inner);
            // At the limit a random admission is closed, so a flood of connections that never
            // sign in cannot reliably close a newer one, such as the editor's.
            if queue.len() >= ADMITTING {
                let (_, evicted, signed_in) =
                    queue.swap_remove(pick.hash_one(id) as usize % ADMITTING);
                // Whichever of eviction, the watchdog and the admission sets the flag first decides.
                if !signed_in.swap(true, Ordering::SeqCst) {
                    let _ = evicted.shutdown(Shutdown::Both);
                }
            }
            queue.push((id, copy, done.clone()));
        }
        let (queue, token, slot) = (admitting.clone(), token.clone(), slot.clone());
        let admission = move || {
            let admitted = admit(stream, port, &token, &done);
            let mut pending = queue.lock().unwrap_or_else(PoisonError::into_inner);
            pending.retain(|(each, ..)| *each != id);
            drop(pending);
            // The editor is greeted whilst the slot is held, so no request can reach it first and
            // a request sent after the greeting finds it in the slot.
            let take = |mut socket: Socket| {
                let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
                socket.send(Message::text(HELLO.to_string())).ok()?;
                slot.replace(socket)
            };
            if let Some(mut old) = admitted.and_then(take) {
                let _ = old.close(Some(CloseFrame {
                    code: CloseCode::Normal,
                    reason: "replaced".into(),
                }));
                let _ = old.flush();
            }
        };
        // Out of threads: the connection is dropped (and its queue entry left for eviction).
        let _ = thread::Builder::new().spawn(admission);
    }
}

/// The handshake and the token, within the admission time, or `None`.
fn admit(stream: TcpStream, port: u16, token: &str, done: &Arc<AtomicBool>) -> Option<Socket> {
    let start = Instant::now();
    // Closes a connection still not admitted at the deadline, whatever it is waiting on. It
    // ends, with its copy of the stream, as soon as this function returns and drops `_stop`.
    let (_stop, stopped) = mpsc::channel::<()>();
    let watch = (stream.try_clone().ok()?, done.clone());
    thread::Builder::new()
        .spawn(move || {
            // Whichever of the watch and the admission sets the flag first decides.
            if stopped.recv_timeout(ADMISSION) == Err(RecvTimeoutError::Timeout)
                && !watch.1.swap(true, Ordering::SeqCst)
            {
                let _ = watch.0.shutdown(Shutdown::Both);
            }
        })
        .ok()?;
    stream.set_read_timeout(Some(ADMISSION)).ok()?;
    stream.set_write_timeout(Some(ADMISSION)).ok()?;
    // Nothing longer than a token is read before the token is checked.
    let config = WebSocketConfig::default()
        .max_message_size(Some(TOKEN_LIMIT))
        .max_frame_size(Some(TOKEN_LIMIT));
    // tungstenite's callback type fixes the error as an HTTP response.
    #[allow(clippy::result_large_err)]
    let check = |request: &Request, response: Response| {
        let header = |name| request.headers().get(name)?.to_str().ok();
        if allowed_host(header("host"), port) && allowed_origin(header("origin")) {
            Ok(response)
        } else {
            let mut refusal = ErrorResponse::new(None);
            *refusal.status_mut() = StatusCode::FORBIDDEN;
            Err(refusal)
        }
    };
    let mut socket = tungstenite::accept_hdr_with_config(stream, check, Some(config)).ok()?;
    let signed_in = matches!(socket.read(), Ok(Message::Text(given)) if same(given.as_bytes(), token.as_bytes()));
    if !signed_in || start.elapsed() >= ADMISSION || done.swap(true, Ordering::SeqCst) {
        let _ = socket.close(None);
        let _ = socket.flush();
        return None;
    }
    let limit = crate::mcp::MAX_MESSAGE_BYTES;
    socket.set_config(|config| {
        config.max_message_size = Some(limit);
        config.max_frame_size = Some(limit);
    });
    Some(socket)
}

/// The bridge's own address, by number or as localhost, so a page on another name that
/// resolves to 127.0.0.1 is refused.
fn allowed_host(host: Option<&str>, port: u16) -> bool {
    host.and_then(|host| host.rsplit_once(':'))
        .is_some_and(|(name, given)| {
            ["127.0.0.1", "localhost"].contains(&name) && given == port.to_string()
        })
}

/// The published editor, or a local build on 127.0.0.1 or localhost over http.
fn allowed_origin(origin: Option<&str>) -> bool {
    let Some(origin) = origin else { return false };
    if origin == EDITOR {
        return true;
    }
    let Some(rest) = ["http://127.0.0.1", "http://localhost"]
        .iter()
        .find_map(|base| origin.strip_prefix(base))
    else {
        return false;
    };
    rest.is_empty()
        || rest
            .strip_prefix(':')
            .is_some_and(|port| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_editor_and_local_builds_are_allowed_origins() {
        for origin in [
            EDITOR,
            "http://127.0.0.1",
            "http://127.0.0.1:8000",
            "http://localhost:1",
        ] {
            assert!(allowed_origin(Some(origin)), "{origin}");
        }
        for origin in [
            "https://yungsnat.github.io.attacker.example",
            "https://yungsnat.github.io:444",
            "http://yungsnat.github.io",
            "http://localhost.attacker.example",
            "http://127.0.0.1:",
            "http://127.0.0.1:80/",
            "https://localhost:8000",
            "null",
            "",
        ] {
            assert!(!allowed_origin(Some(origin)), "{origin}");
        }
        assert!(!allowed_origin(None));
    }

    #[test]
    fn only_the_bridge_s_own_address_is_an_allowed_host() {
        assert!(allowed_host(Some("127.0.0.1:4000"), 4000));
        assert!(allowed_host(Some("localhost:4000"), 4000));
        for host in [
            "127.0.0.1:4001",
            "127.0.0.1",
            "attacker.example:4000",
            "[::1]:4000",
        ] {
            assert!(!allowed_host(Some(host), 4000), "{host}");
        }
        assert!(!allowed_host(None, 4000));
    }

    #[test]
    fn tokens_compare_whole() {
        assert!(same(b"abcd", b"abcd"));
        for given in [&b"abce"[..], b"abc", b"abcde", b""] {
            assert!(!same(given, b"abcd"));
        }
    }

    #[test]
    fn tokens_are_256_bits_and_differ() {
        let (a, b) = (fresh_code().unwrap(), fresh_code().unwrap());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }
}
