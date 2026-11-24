// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! `apollinaire mcp --live`: an agent's client on stdio and a stand-in for the browser editor
//! on the loopback WebSocket.
#![cfg(feature = "cli")]

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, client::IntoClientRequest, http::HeaderValue};

const BIN: &str = env!("CARGO_BIN_EXE_apollinaire");
const EDITOR: &str = "https://yungsnat.github.io";

/// An MCP session with one server process.
struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    id: u64,
}

impl Session {
    fn start(args: &[&str]) -> Self {
        let mut child = Command::new(BIN)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            id: 0,
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let mut params = params;
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {},
        });
        let message = json!({"jsonrpc": "2.0", "id": self.id, "method": method, "params": params});
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["id"], self.id, "{reply}");
        reply
    }

    fn tools(&mut self) -> Vec<String> {
        self.request("tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect()
    }

    /// A tool's text and whether it failed.
    fn call(&mut self, name: &str, arguments: Value) -> (String, bool) {
        let reply = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        let result = &reply["result"];
        (
            result["content"][0]["text"].as_str().unwrap().to_owned(),
            result["isError"].as_bool().unwrap(),
        )
    }

    /// The port and token `live_connect` gives.
    fn connect(&mut self) -> (u16, String) {
        let (text, failed) = self.call("live_connect", json!({}));
        assert!(!failed, "{text}");
        let field = |name: &str| {
            text.split_whitespace()
                .skip_while(|word| *word != name)
                .nth(1)
                .unwrap_or_else(|| panic!("no {name} in {text}"))
                .trim_end_matches([',', '.'])
                .to_owned()
        };
        (field("port").parse().unwrap(), field("token"))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A WebSocket handshake to the bridge with the given headers; `None` leaves one out.
fn open(
    port: u16,
    origin: Option<&str>,
    host: Option<&str>,
) -> tungstenite::Result<WebSocket<TcpStream>> {
    let mut request = format!("ws://127.0.0.1:{port}").into_client_request()?;
    let headers = request.headers_mut();
    match origin {
        Some(origin) => headers.insert("Origin", HeaderValue::from_str(origin).unwrap()),
        None => headers.remove("Origin"),
    };
    if let Some(host) = host {
        headers.insert("Host", HeaderValue::from_str(host).unwrap());
    }
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    tungstenite::client(request, stream)
        .map(|(socket, _)| socket)
        .map_err(|error| match error {
            tungstenite::HandshakeError::Failure(error) => error,
            tungstenite::HandshakeError::Interrupted(_) => panic!("blocking handshake"),
        })
}

/// The editor at its own address, signed in with `token`.
fn editor(port: u16, token: &str) -> WebSocket<TcpStream> {
    let mut socket = open(port, Some(EDITOR), None).unwrap();
    socket.send(Message::text(token)).unwrap();
    let hello: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(hello["hello"], "apollinaire", "{hello}");
    socket
}

/// Whether the server closes the connection without answering.
fn closed(socket: &mut WebSocket<TcpStream>) -> bool {
    loop {
        match socket.read() {
            Ok(Message::Close(_)) => {}
            Ok(message) => panic!("unexpected {message:?}"),
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return false;
            }
            Err(_) => return true,
        }
    }
}

/// Answer one request from the bridge with `reply` (its id is filled in) and return the request.
fn answer(socket: &mut WebSocket<TcpStream>, mut reply: Value) -> Value {
    let request: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
    reply["id"] = request["id"].clone();
    socket.send(Message::text(reply.to_string())).unwrap();
    request
}

const LIVE: [&str; 3] = ["live_connect", "live_read", "live_write"];

#[test]
fn only_live_mode_lists_the_live_tools() {
    let plain = Session::start(&["mcp"]).tools();
    assert!(
        plain.iter().all(|name| !name.starts_with("live_")),
        "{plain:?}"
    );
    let live = Session::start(&["mcp", "--live"]).tools();
    for name in LIVE {
        assert!(live.contains(&name.to_owned()), "{live:?}");
    }
    assert_eq!(live.len(), plain.len() + LIVE.len());
}

#[test]
fn live_connect_gives_a_loopback_port_and_one_token() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, token) = session.connect();
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()), "{token}");
    assert_eq!(session.connect(), (port, token.clone()));
    let other = Session::start(&["mcp", "--live"]).connect().1;
    assert_ne!(other, token);
}

#[test]
fn live_tools_need_a_connected_editor() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (text, failed) = session.call("live_read", json!({}));
    assert!(failed);
    assert!(text.contains("live_connect"), "{text}");
    session.connect();
    let (text, failed) = session.call("live_read", json!({}));
    assert!(failed);
    assert!(text.contains("No editor is connected"), "{text}");
}

#[test]
fn the_editor_answers_reads_and_writes() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, token) = session.connect();
    let mut socket = editor(port, &token);
    let frame = json!({"revision": "00ff", "layer": 0, "frame": 0, "frames": 1,
        "width": 2, "height": 1, "text": "ab"});
    let browser = thread::spawn(move || {
        let read = answer(&mut socket, json!({"result": frame}));
        let write = answer(&mut socket, json!({"result": {"revision": "0100"}}));
        let refused = answer(
            &mut socket,
            json!({"error": "The frame changed since it was read: read it again"}),
        );
        (read, write, refused)
    });
    let (text, failed) = session.call("live_read", json!({}));
    assert!(!failed, "{text}");
    let read: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(read["text"], "ab");
    assert_eq!(read["revision"], "00ff");
    let (text, failed) = session.call(
        "live_write",
        json!({"revision": "00ff", "text": "x", "x": 1, "y": 0}),
    );
    assert!(!failed, "{text}");
    assert!(text.contains("0100"), "{text}");
    let (text, failed) = session.call("live_write", json!({"revision": "00ff", "text": "y"}));
    assert!(failed);
    assert_eq!(text, "The frame changed since it was read: read it again");
    let (read, write, refused) = browser.join().unwrap();
    assert_eq!(read["op"], "read");
    assert_eq!(
        (
            &write["op"],
            &write["revision"],
            &write["text"],
            &write["x"],
            &write["y"]
        ),
        (
            &json!("write"),
            &json!("00ff"),
            &json!("x"),
            &json!(1),
            &json!(0)
        )
    );
    assert_eq!(refused["x"], Value::Null);
}

#[test]
fn other_origins_and_hosts_are_refused() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, _) = session.connect();
    for (origin, host) in [
        (None, None),
        (Some("https://attacker.example"), None),
        (Some("https://yungsnat.github.io.attacker.example"), None),
        (Some("http://localhost.attacker.example"), None),
        (Some("null"), None),
        (Some(EDITOR), Some("attacker.example")),
        (Some(EDITOR), Some(&*format!("localhost:{}", port + 1))),
    ] {
        let refused = open(port, origin, host);
        assert!(refused.is_err(), "{origin:?} {host:?} was let in");
    }
    for origin in ["http://127.0.0.1:8000", "http://localhost:8000", EDITOR] {
        assert!(open(port, Some(origin), None).is_ok(), "{origin}");
    }
    let localhost = format!("localhost:{port}");
    assert!(open(port, Some(EDITOR), Some(&localhost)).is_ok());
}

#[test]
fn a_wrong_token_is_refused() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, token) = session.connect();
    let last = if token.ends_with('0') { '1' } else { '0' };
    let wrong = format!("{}{last}", &token[..63]);
    for guess in [wrong.as_str(), "", &token[..63], &format!("{token}0")] {
        let mut socket = open(port, Some(EDITOR), None).unwrap();
        socket.send(Message::text(guess)).unwrap();
        assert!(closed(&mut socket), "{guess:?} was let in");
    }
    let (text, failed) = session.call("live_read", json!({}));
    assert!(failed);
    assert!(text.contains("No editor is connected"), "{text}");
}

#[test]
fn a_silent_connection_is_closed_and_takes_no_slot() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, token) = session.connect();
    // Connections that never finish the handshake or never send the token.
    let mut idle: Vec<TcpStream> = (0..8)
        .map(|_| TcpStream::connect(("127.0.0.1", port)).unwrap())
        .collect();
    let mut silent = open(port, Some(EDITOR), None).unwrap();
    // The editor still gets in, and works, whilst they wait.
    let mut socket = editor(port, &token);
    let browser = thread::spawn(move || {
        answer(&mut socket, json!({"result": {"text": "ok"}}));
        socket
    });
    let (text, failed) = session.call("live_read", json!({}));
    assert!(!failed, "{text}");
    let _socket = browser.join().unwrap();
    let start = Instant::now();
    assert!(closed(&mut silent));
    let stream = &mut idle[0];
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    assert_eq!(stream.read(&mut [0; 16]).unwrap_or(0), 0);
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_second_signed_in_editor_replaces_the_first() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, token) = session.connect();
    let mut first = editor(port, &token);
    let mut second = editor(port, &token);
    assert!(closed(&mut first));
    let browser = thread::spawn(move || answer(&mut second, json!({"result": {"text": "second"}})));
    let (text, failed) = session.call("live_read", json!({}));
    assert!(!failed, "{text}");
    assert!(text.contains("second"), "{text}");
    browser.join().unwrap();
}

#[test]
fn a_closed_editor_frees_the_bridge() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, token) = session.connect();
    let mut socket = editor(port, &token);
    socket.close(None).unwrap();
    let _ = socket.flush();
    drop(socket);
    let (text, failed) = session.call("live_read", json!({}));
    assert!(failed);
    let (text2, failed2) = session.call("live_read", json!({}));
    assert!(failed2);
    assert!(text2.contains("No editor is connected"), "{text} / {text2}");
}
