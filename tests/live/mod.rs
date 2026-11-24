// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! An agent's client on stdio and a stand-in for the browser editor on the loopback WebSocket,
//! shared by the `mcp --live` tests.
// Each test file uses only some of these.
#![allow(dead_code)]

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::Duration,
};
use tungstenite::{Message, WebSocket, client::IntoClientRequest, http::HeaderValue};

const BIN: &str = env!("CARGO_BIN_EXE_apollinaire");
pub const EDITOR: &str = "https://yungsnat.github.io";

/// An MCP session with one server process.
pub struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    id: u64,
}

impl Session {
    pub fn start(args: &[&str]) -> Self {
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
            stdin: Some(stdin),
            stdout,
            id: 0,
        }
    }

    pub fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let mut params = params;
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {},
        });
        let message = json!({"jsonrpc": "2.0", "id": self.id, "method": method, "params": params});
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["id"], self.id, "{reply}");
        reply
    }

    pub fn tools(&mut self) -> Vec<String> {
        self.request("tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect()
    }

    /// A tool's text and whether it failed.
    pub fn call(&mut self, name: &str, arguments: Value) -> (String, bool) {
        let reply = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        let result = &reply["result"];
        (
            result["content"][0]["text"].as_str().unwrap().to_owned(),
            result["isError"].as_bool().unwrap(),
        )
    }

    /// The port and token `live_connect` gives.
    pub fn connect(&mut self) -> (u16, String) {
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
        // Closing stdin ends the server normally, so a coverage run records what it ran.
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

/// A WebSocket handshake to the bridge with the given headers; `None` leaves one out.
pub fn open(
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

/// A legacy editor at its own address, signed in with the bare `token` (no capabilities).
pub fn editor(port: u16, token: &str) -> WebSocket<TcpStream> {
    sign_in(port, token)
}

/// A modern editor advertising `expected_sha256` support during sign-in.
pub fn capable_editor(port: u16, token: &str) -> WebSocket<TcpStream> {
    let handshake = json!({"token": token, "capabilities": ["expected_sha256"]}).to_string();
    sign_in(port, &handshake)
}

fn sign_in(port: u16, message: &str) -> WebSocket<TcpStream> {
    let mut socket = open(port, Some(EDITOR), None).unwrap();
    socket.send(Message::text(message)).unwrap();
    let hello: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(hello["hello"], "apollinaire", "{hello}");
    socket
}

/// Whether the server closes the connection without answering.
pub fn closed(socket: &mut WebSocket<TcpStream>) -> bool {
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
pub fn answer(socket: &mut WebSocket<TcpStream>, mut reply: Value) -> Value {
    let request: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
    reply["id"] = request["id"].clone();
    socket.send(Message::text(reply.to_string())).unwrap();
    request
}
