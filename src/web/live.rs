// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Connect an agent: the editor's end of `apollinaire mcp --live`. It connects to the bridge on
//! 127.0.0.1, sends the token, and answers the bridge's requests through `crate::live_edit`.
use super::{App, Result, input, listen, report, state, status};
use crate::live_edit::{SignIn, State, answer};
use std::cell::RefCell;
use std::rc::Rc;
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
    let sign_in = Rc::new(RefCell::new(SignIn::new(token)));
    let initial = sign_in.borrow().initial_message();
    open_socket(port, initial, sign_in)
}

fn open_socket(port: u16, first_message: String, sign_in: Rc<RefCell<SignIn>>) -> Result<()> {
    let socket = WebSocket::new(&format!("ws://127.0.0.1:{port}"))
        .map_err(|_| "Cannot open a connection to the agent.")?;
    let sender = socket.clone();
    let open = Closure::once_into_js(move || {
        let _ = sender.send_with_str(&first_message);
    });
    socket.set_onopen(Some(open.unchecked_ref()));
    let sender = socket.clone();
    let message_state = Rc::clone(&sign_in);
    let message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        if let Some(text) = event.data().as_string()
            && let Some(reply) = reply(&text, &message_state)
        {
            let _ = sender.send_with_str(&reply);
        }
    });
    socket.set_onmessage(Some(message.as_ref().unchecked_ref()));
    // Each connection's handlers live until the page closes; there are only as many as clicks.
    message.forget();
    let close_state = Rc::clone(&sign_in);
    let close = Closure::<dyn FnMut(CloseEvent)>::new(move |event: CloseEvent| {
        SOCKET.with(|cell| cell.borrow_mut().take());
        let replaced = event.reason() == "replaced";
        let fallback = close_state
            .borrow_mut()
            .fallback_message(replaced)
            .map(str::to_owned);
        if let Some(bare_token) = fallback
            && open_socket(port, bare_token, Rc::clone(&close_state)).is_ok()
        {
            return;
        }
        if replaced {
            status(
                "Another editor connected to the agent in place of this one.",
                false,
            );
        } else {
            status(
                "The agent's connection closed. Check the port and token, or call live_connect again.",
                true,
            );
        }
    });
    socket.set_onclose(Some(close.as_ref().unchecked_ref()));
    close.forget();
    SOCKET.with(|cell| *cell.borrow_mut() = Some(socket));
    status("Connecting to the agent...", false);
    Ok(())
}

/// The reply to one message from the bridge, if it needs one.
fn reply(text: &str, sign_in: &RefCell<SignIn>) -> Option<String> {
    let request: serde_json::Value = serde_json::from_str(text).ok()?;
    if request.get("hello").is_some() {
        sign_in.borrow_mut().mark_greeted();
        status(
            "Agent connected. It can read and change frames and layers in this project.",
            false,
        );
        return None;
    }
    let outcome = state(|app| handle(app, &request));
    let reply =
        outcome.unwrap_or_else(|error| serde_json::json!({"id": request["id"], "error": error}));
    Some(reply.to_string())
}

pub(super) fn handle(app: &mut App, request: &serde_json::Value) -> Result<serde_json::Value> {
    // A playing animation would move default targeting between a read and a write based on it.
    let was_playing = app.playing;
    app.pause();
    let (reply, updated) = {
        let state = State {
            project: &app.layers,
            active: app.active,
            shown: app.selected,
            generation: app.project_generation,
        };
        answer(&state, request)
    };
    if let Some(layers) = updated {
        // Refused, as the editor's own controls are, during unapplied edits, a load or a recording.
        app.clean()?;
        app.replace_layers(layers, app.active);
        input("width")?.set_value(&app.project().width().to_string());
        input("height")?.set_value(&app.project().height().to_string());
        app.display(true)?;
        let note = if request["op"].as_str() == Some("batch") {
            "The agent changed the project. Undo reverts it."
        } else {
            "The agent changed a frame. Undo reverts it."
        };
        status(note, false);
    } else if was_playing {
        app.display(false)?;
    }
    Ok(reply)
}
