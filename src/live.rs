// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! `apollinaire mcp --live`: a bridge from the agent's MCP tools to the browser editor, which
//! connects over a WebSocket on 127.0.0.1. Nothing listens until `live_connect`, which binds a
//! port the system picks and makes a 256-bit token from the system's random source; the user
//! enters both in the editor. A connection is let in only with the editor's Origin and a
//! loopback Host, and only once it sends the token within the admission time. One signed-in
//! editor holds the bridge; a later one with the token replaces it. The editor checks and
//! applies every write itself (`src/live_edit.rs`), as one undo step. Elements (boxes, text
//! blocks and lines with ids) are drawn and changed by id the same way (`src/elements.rs`).

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

/// Capability name advertised by an editor that enforces `expected_sha256` on writes.
const CAP_EXPECTED_SHA256: &str = "expected_sha256";
const NO_SHA256_CAPABILITY: &str = "The connected editor does not advertise expected_sha256 support; reload the editor or omit expected_sha256.";
/// Capability name advertised by an editor that keeps elements.
const CAP_ELEMENTS: &str = "elements";
const NO_ELEMENTS_CAPABILITY: &str =
    "The connected editor does not support elements; reload the editor.";

type Socket = WebSocket<TcpStream>;

struct Peer {
    socket: Socket,
    supports_sha256: bool,
    supports_elements: bool,
}

/// What a request needs the editor to support.
#[derive(Clone, Copy, Default)]
struct Needs {
    sha256: bool,
    elements: bool,
}

type Slot = Arc<Mutex<Option<Peer>>>;

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

const WRITE_KEYS: &[&str] = &[
    "layer",
    "frame",
    "revision",
    "expected_sha256",
    "text",
    "x",
    "y",
];

fn write_properties(with_dry_run: bool) -> serde_json::Map<String, Value> {
    let mut props = json!({
        "layer": {"type": "integer", "minimum": 0, "description": "Target layer index, 0-based (default the active layer)."},
        "frame": {"type": "integer", "minimum": 0, "description": "Target timeline frame index, 0-based (default the frame on screen)."},
        "revision": {"type": "string", "description": "From live_read."},
        "expected_sha256": {"type": "string", "pattern": "^[0-9a-fA-F]{64}$", "description": "Optional 64-character hex SHA-256 digest of the frame text from live_read; refused on mismatch."},
        "text": {"type": "string", "description": "Rows separated by line feeds; shorter rows are padded with spaces."},
        "x": {"type": "integer", "minimum": 0, "description": "Column of the block's left edge; give y too."},
        "y": {"type": "integer", "minimum": 0, "description": "Row of the block's top edge; give x too."},
    })
    .as_object()
    .expect("object")
    .clone();
    if with_dry_run {
        props.insert(
            "dry_run".into(),
            json!({"type": "boolean", "description": "Check the write and return the would-be frame and changed cell count without editing the project or undo history."}),
        );
    }
    props
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
            "A frame in the connected editor (default active layer and shown frame), or the stacked visible frame when composite is true, as JSON with layers, active and shown. Playback pauses.",
            json!({
                "layer": {"type": "integer", "minimum": 0, "description": "Layer index, 0-based (default the active layer)."},
                "frame": {"type": "integer", "minimum": 0, "description": "Timeline frame index, 0-based (default the frame on screen)."},
                "composite": {"type": "boolean", "description": "Read the stacked visible layers through their motion keys at frame, without a layer or revision."},
            }),
            &[],
            true,
        ),
        tool(
            "live_write",
            "Replace a frame in the connected editor, or the block at x and y, with printable ASCII text, as one undo step. Refused unless revision is the one live_read gave for the frame in the current project generation, and unless optional expected_sha256 matches the frame text's SHA-256 digest. Replacing the whole frame can change its size only in a project of one frame and one layer.",
            Value::Object(write_properties(true)),
            &["revision", "text"],
            false,
        ),
        tool(
            "live_batch",
            "Apply 1 to 256 frame or block writes atomically as one undo step, checking every revision and optional expected_sha256 against the pre-batch project. Each write uses the same fields as live_write (without dry_run).",
            json!({
                "writes": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 256,
                    "description": "1 to 256 writes applied in order.",
                    "items": {
                        "type": "object",
                        "properties": Value::Object(write_properties(false)),
                        "required": ["revision", "text"],
                        "additionalProperties": false,
                    },
                },
                "dry_run": {"type": "boolean", "description": "Check every write and return the would-be frames and changed cell counts without editing the project or undo history."},
            }),
            &["writes"],
            false,
        ),
        tool(
            "live_elements",
            "The elements (boxes, text blocks and lines with ids like e3) in the connected editor's project, optionally of one layer or timeline frame: id, layer, frame, kind, place, size, style, arrow, text, revision, and drifted, the count of their cells changed by hand since they were drawn. Also the id the next element gets; ids are never reused.",
            json!({
                "layer": {"type": "integer", "minimum": 0, "description": "Only this layer's elements."},
                "frame": {"type": "integer", "minimum": 0, "description": "Only this timeline frame's elements."},
            }),
            &[],
            true,
        ),
        tool(
            "live_element_add",
            "Draw a box, text block or line in printable ASCII on top of a frame and give its id, as one undo step. The cells it covers are kept and come back when it moves or is deleted.",
            Value::Object(element_properties(true)),
            &["kind", "x", "y"],
            false,
        ),
        tool(
            "live_element_edit",
            "Move, resize, restyle or retext an element by id, as one undo step, putting back the cells it covered and redrawing any element above it. Refused when it, or an element above it, was changed by hand, unless force is true.",
            Value::Object(element_properties(false)),
            &["element"],
            false,
        ),
        tool(
            "live_element_delete",
            "Delete an element by id, as one undo step, putting back the cells it covered and redrawing any element above it. Its id is never given out again. Refused when it, or an element above it, was changed by hand, unless force is true.",
            json!({
                "element": {"type": "string", "pattern": "^e[1-9][0-9]*$", "description": "The element's id, like e3."},
                "revision": {"type": "string", "description": "Optional, from live_elements or the last element tool; refused if the element or its cells changed since."},
                "force": {"type": "boolean", "description": "Redraw over cells changed by hand."},
                "dry_run": {"type": "boolean", "description": "Return the would-be frame and changed cell count without editing the project or undo history."},
            }),
            &["element"],
            false,
        ),
    ]
}

/// The arguments of `live_element_add` (`adding`) or `live_element_edit`.
fn element_properties(adding: bool) -> serde_json::Map<String, Value> {
    let mut props = json!({
        "x": {"type": "integer", "minimum": 0, "description": "Column of the element's left edge."},
        "y": {"type": "integer", "minimum": 0, "description": "Row of the element's top edge."},
        "width": {"type": "integer", "minimum": 1, "description": "A box's or horizontal line's width (box at least 2). Not for text, whose size is its text's."},
        "height": {"type": "integer", "minimum": 1, "description": "A box's or vertical line's height (box at least 2). Not for text."},
        "text": {"type": "string", "description": "A text element's rows, separated by line feeds, or a box's text from its top-left inner cell; printable ASCII."},
        "style": {"type": "string", "enum": ["plain", "rounded", "dotted", "heavy"], "description": "A box's border or a line's stroke (lines are not rounded); default plain."},
        "arrow": {"type": "string", "enum": ["none", "start", "end", "both"], "description": "A line's arrowheads; default none."},
        "dry_run": {"type": "boolean", "description": "Return the would-be frame and changed cell count without editing the project or undo history."},
    })
    .as_object()
    .expect("object")
    .clone();
    if adding {
        props.insert(
            "kind".into(),
            json!({"type": "string", "enum": ["box", "text", "line"]}),
        );
        props.insert("layer".into(), json!({"type": "integer", "minimum": 0, "description": "Layer index, 0-based (default the active layer)."}));
        props.insert("frame".into(), json!({"type": "integer", "minimum": 0, "description": "Timeline frame index, 0-based (default the frame on screen)."}));
    } else {
        props.insert("element".into(), json!({"type": "string", "pattern": "^e[1-9][0-9]*$", "description": "The element's id, like e3."}));
        props.insert("revision".into(), json!({"type": "string", "description": "Optional, from live_elements or the last element tool; refused if the element or its cells changed since."}));
        props.insert(
            "force".into(),
            json!({"type": "boolean", "description": "Redraw over cells changed by hand."}),
        );
    }
    props
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
            "live_read" => {
                read_request(arguments).and_then(|request| self.ask(request, Needs::default()))
            }
            "live_write" => write_request(arguments).and_then(|(request, sha256)| {
                self.ask(
                    request,
                    Needs {
                        sha256,
                        ..Needs::default()
                    },
                )
            }),
            "live_batch" => batch_request(arguments).and_then(|(request, sha256)| {
                self.ask(
                    request,
                    Needs {
                        sha256,
                        ..Needs::default()
                    },
                )
            }),
            "live_elements" | "live_element_add" | "live_element_edit" | "live_element_delete" => {
                element_request(name, arguments).and_then(|request| {
                    self.ask(
                        request,
                        Needs {
                            elements: true,
                            ..Needs::default()
                        },
                    )
                })
            }
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
            "Bridge open on port {}, token {}. Open the editor at {EDITOR_PAGE} or a local build, choose Connect an agent and enter both; then use live_read, live_write, live_batch and the element tools.",
            open.port, open.code
        ))
    }

    /// Send a request to the editor and give its result as JSON text.
    fn ask(&mut self, mut request: Value, needs: Needs) -> Result<String, String> {
        let open = self.open.as_ref().ok_or(NOT_OPEN)?;
        let mut slot = open.slot.lock().unwrap_or_else(PoisonError::into_inner);
        let peer = slot.as_mut().ok_or(NO_EDITOR)?;
        if needs.sha256 && !peer.supports_sha256 {
            return Err(NO_SHA256_CAPABILITY.into());
        }
        if needs.elements && !peer.supports_elements {
            return Err(NO_ELEMENTS_CAPABILITY.into());
        }
        self.next += 1;
        request["id"] = json!(self.next);
        match exchange(&mut peer.socket, &request, self.next) {
            Ok(result) => Ok(result.to_string()),
            Err(Failure::Answer(error)) => Err(error),
            Err(Failure::Lost(error)) => {
                *slot = None;
                Err(format!("The editor disconnected: {error}"))
            }
        }
    }
}

fn read_request(arguments: &serde_json::Map<String, Value>) -> Result<Value, String> {
    if let Some(key) = arguments
        .keys()
        .find(|key| !["layer", "frame", "composite"].contains(&key.as_str()))
    {
        return Err(format!("Unknown argument {key}"));
    }
    let mut request = json!({"op": "read"});
    for (key, value) in arguments {
        request[key] = value.clone();
    }
    Ok(request)
}

fn write_request(arguments: &serde_json::Map<String, Value>) -> Result<(Value, bool), String> {
    if let Some(key) = arguments
        .keys()
        .find(|key| !WRITE_KEYS.contains(&key.as_str()) && key.as_str() != "dry_run")
    {
        return Err(format!("Unknown argument {key}"));
    }
    let mut request = json!({"op": "write"});
    for (key, value) in arguments {
        request[key] = value.clone();
    }
    let needs_sha256 = arguments.contains_key("expected_sha256");
    Ok((request, needs_sha256))
}

fn batch_request(arguments: &serde_json::Map<String, Value>) -> Result<(Value, bool), String> {
    if let Some(key) = arguments
        .keys()
        .find(|key| !["writes", "dry_run"].contains(&key.as_str()))
    {
        return Err(format!("Unknown argument {key}"));
    }
    let writes = arguments
        .get("writes")
        .and_then(Value::as_array)
        .filter(|items| (1..=256).contains(&items.len()))
        .ok_or("writes must be an array of 1 to 256 write objects")?;
    let mut needs_sha256 = false;
    for (index, item) in writes.iter().enumerate() {
        let obj = item
            .as_object()
            .ok_or_else(|| format!("Write {}: must be an object", index + 1))?;
        if let Some(key) = obj.keys().find(|key| !WRITE_KEYS.contains(&key.as_str())) {
            return Err(format!("Write {}: Unknown argument {key}", index + 1));
        }
        needs_sha256 |= obj.contains_key("expected_sha256");
    }
    let mut request = json!({"op": "batch"});
    for (key, value) in arguments {
        request[key] = value.clone();
    }
    Ok((request, needs_sha256))
}

/// An element tool's request, after the names its schema allows; the editor checks the rest.
fn element_request(
    name: &str,
    arguments: &serde_json::Map<String, Value>,
) -> Result<Value, String> {
    let (op, allowed): (&str, &[&str]) = match name {
        "live_elements" => ("elements", &["layer", "frame"]),
        "live_element_add" => (
            "element_add",
            &[
                "kind", "layer", "frame", "x", "y", "width", "height", "text", "style", "arrow",
                "dry_run",
            ],
        ),
        "live_element_edit" => (
            "element_edit",
            &[
                "element", "revision", "x", "y", "width", "height", "text", "style", "arrow",
                "force", "dry_run",
            ],
        ),
        _ => (
            "element_delete",
            &["element", "revision", "force", "dry_run"],
        ),
    };
    if let Some(key) = arguments
        .keys()
        .find(|key| !allowed.contains(&key.as_str()))
    {
        return Err(format!("Unknown argument {key}"));
    }
    let mut request = json!({"op": op});
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
            let take = |mut peer: Peer| {
                let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
                peer.socket.send(Message::text(HELLO.to_string())).ok()?;
                slot.replace(peer)
            };
            if let Some(mut old) = admitted.and_then(take) {
                let _ = old.socket.close(Some(CloseFrame {
                    code: CloseCode::Normal,
                    reason: "replaced".into(),
                }));
                let _ = old.socket.flush();
            }
        };
        // Out of threads: the connection is dropped (and its queue entry left for eviction).
        let _ = thread::Builder::new().spawn(admission);
    }
}

/// Parse the editor's sign-in message: either the bare 64-character token (legacy editor, no
/// capabilities) or JSON `{"token": "...", "capabilities": [...]}`. Gives what the editor
/// supports.
fn parse_sign_in(given: &str, token: &str) -> Option<Needs> {
    if same(given.as_bytes(), token.as_bytes()) {
        return Some(Needs::default());
    }
    let payload: Value = serde_json::from_str(given).ok()?;
    let supplied = payload.get("token")?.as_str()?;
    if !same(supplied.as_bytes(), token.as_bytes()) {
        return None;
    }
    let caps = payload.get("capabilities").and_then(Value::as_array);
    let has =
        |name: &str| caps.is_some_and(|caps| caps.iter().any(|cap| cap.as_str() == Some(name)));
    Some(Needs {
        sha256: has(CAP_EXPECTED_SHA256),
        elements: has(CAP_ELEMENTS),
    })
}

/// The handshake and the token, within the admission time, or `None`.
fn admit(stream: TcpStream, port: u16, token: &str, done: &Arc<AtomicBool>) -> Option<Peer> {
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
    let supports = match socket.read() {
        Ok(Message::Text(given)) => parse_sign_in(&given, token),
        _ => None,
    };
    let Some(supports) =
        supports.filter(|_| start.elapsed() < ADMISSION && !done.swap(true, Ordering::SeqCst))
    else {
        let _ = socket.close(None);
        let _ = socket.flush();
        return None;
    };
    let limit = crate::mcp::MAX_MESSAGE_BYTES;
    socket.set_config(|config| {
        config.max_message_size = Some(limit);
        config.max_frame_size = Some(limit);
    });
    Some(Peer {
        socket,
        supports_sha256: supports.sha256,
        supports_elements: supports.elements,
    })
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

    #[test]
    fn sign_in_parses_legacy_tokens_and_capability_handshakes() {
        let token = "a".repeat(64);
        let caps = |given: &str| parse_sign_in(given, &token).map(|n| (n.sha256, n.elements));
        assert_eq!(caps(&token), Some((false, false)));
        let modern = json!({"token": token, "capabilities": ["expected_sha256"]}).to_string();
        assert!(modern.len() <= TOKEN_LIMIT);
        assert_eq!(caps(&modern), Some((true, false)));
        // The editor's own sign-in, with every capability, fits before the token is checked.
        let editor = apollinaire_ascii::live_edit::SignIn::new(token.clone()).initial_message();
        assert!(editor.len() <= TOKEN_LIMIT, "{}", editor.len());
        assert_eq!(caps(&editor), Some((true, true)));
        let no_caps = json!({"token": token, "capabilities": []}).to_string();
        assert_eq!(caps(&no_caps), Some((false, false)));
        let wrong =
            json!({"token": "b".repeat(64), "capabilities": ["expected_sha256"]}).to_string();
        assert_eq!(caps(&wrong), None);
        assert_eq!(caps("{bad"), None);
        assert_eq!(caps("{}"), None);
    }
}
