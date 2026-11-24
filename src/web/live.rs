// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Connect an agent: the editor's end of `apollinaire mcp --live`. It connects to the bridge on
//! 127.0.0.1, sends the token, and answers the bridge's requests through `crate::live_edit`.
use super::{App, Result, input, listen, report, state, status};
use crate::live_edit::{View, answer};
use std::cell::RefCell;
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{CloseEvent, MessageEvent, WebSocket};

thread_local! {static SOCKET: RefCell<Option<WebSocket>> = const {RefCell::new(None)};}

pub fn start() -> std::result::Result<(), JsValue> {
    listen("live-connect", "click", |_| report(connect()))?;
    listen("live-disconnect", "click", |_| {
        disconnect();
        status("Agent disconnected.", false);
    })
}

fn disconnect() {
    if let Some(socket) = SOCKET.with(|cell| cell.borrow_mut().take()) {
        socket.set_onmessage(None);
        socket.set_onclose(None);
        let _ = socket.close();
    }
}

fn connect() -> Result<()> {
    let port: u16 = input("live-port")?
        .value()
        .trim()
        .parse()
        .map_err(|_| "Enter the port live_connect gave.")?;
    let token = input("live-token")?.value().trim().to_owned();
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Enter the 64-character token live_connect gave.".into());
    }
    disconnect();
    let socket = WebSocket::new(&format!("ws://127.0.0.1:{port}"))
        .map_err(|_| "Cannot open a connection to the agent.")?;
    let sender = socket.clone();
    let open = Closure::once_into_js(move || {
        let _ = sender.send_with_str(&token);
    });
    socket.set_onopen(Some(open.unchecked_ref()));
    let sender = socket.clone();
    let message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        if let Some(text) = event.data().as_string()
            && let Some(reply) = reply(&text)
        {
            let _ = sender.send_with_str(&reply);
        }
    });
    socket.set_onmessage(Some(message.as_ref().unchecked_ref()));
    // Each connection's handlers live until the page closes; there are only as many as clicks.
    message.forget();
    let close = Closure::<dyn FnMut(CloseEvent)>::new(|_: CloseEvent| {
        SOCKET.with(|cell| cell.borrow_mut().take());
        status(
            "The agent's connection closed. Check the port and token, or call live_connect again.",
            true,
        );
    });
    socket.set_onclose(Some(close.as_ref().unchecked_ref()));
    close.forget();
    SOCKET.with(|cell| *cell.borrow_mut() = Some(socket));
    status("Connecting to the agent...", false);
    Ok(())
}

/// The reply to one message from the bridge, if it needs one.
fn reply(text: &str) -> Option<String> {
    let request: serde_json::Value = serde_json::from_str(text).ok()?;
    if request.get("hello").is_some() {
        status(
            "Agent connected. It can read and change the frame on screen.",
            false,
        );
        return None;
    }
    let outcome = state(|app| handle(app, &request));
    let reply =
        outcome.unwrap_or_else(|error| serde_json::json!({"id": request["id"], "error": error}));
    Some(reply.to_string())
}

fn handle(app: &mut App, request: &serde_json::Value) -> Result<serde_json::Value> {
    let (reply, written, duration_ms) = {
        let project = app.project();
        let shown = &project.frames()[app.selected];
        let view = View {
            layer: app.active,
            frame: app.selected,
            frames: project.frames().len(),
            art: &shown.frame,
        };
        let (reply, written) = answer(&view, request);
        (reply, written, shown.duration_ms)
    };
    if let Some(frame) = written {
        if app.dirty {
            return Err("The editor has unapplied edits: apply or discard them first.".into());
        }
        app.update_frame(frame, duration_ms)?;
        status("The agent changed this frame. Undo reverts it.", false);
    }
    Ok(reply)
}
