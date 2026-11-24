// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! `apollinaire mcp --live`: an agent's client on stdio and a stand-in for the browser editor
//! on the loopback WebSocket.
#![cfg(feature = "cli")]

mod live;

use live::{Session, answer, capable_editor, closed, editor};
use serde_json::{Value, json};
use std::thread;
use tungstenite::Message;

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
fn a_second_signed_in_editor_replaces_the_first() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, token) = session.connect();
    let mut first = editor(port, &token);
    let mut second = editor(port, &token);
    match first.read() {
        Ok(Message::Close(Some(frame))) => assert_eq!(frame.reason.as_str(), "replaced"),
        other => panic!("expected a close frame, got {other:?}"),
    }
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

#[test]
fn live_mode_keeps_every_other_tool_as_it_was() {
    let list = |args: &[&str]| -> Vec<Value> {
        let mut session = Session::start(args);
        session.request("tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .clone()
    };
    let plain = list(&["mcp"]);
    let live: Vec<Value> = list(&["mcp", "--live"])
        .into_iter()
        .filter(|tool| !tool["name"].as_str().unwrap().starts_with("live_"))
        .collect();
    assert_eq!(live, plain);
    let mut session = Session::start(&["mcp", "--live"]);
    for (name, arguments, error) in [
        (
            "live_connect",
            json!({"port": 80}),
            "live_connect takes no arguments",
        ),
        (
            "live_read",
            json!({"frame": 1}),
            "live_read takes no arguments",
        ),
        (
            "live_write",
            json!({"revision": "0", "text": "a", "z": 1}),
            "Unknown argument z",
        ),
        ("live_delete", json!({}), "Unknown tool: live_delete"),
    ] {
        let reply = session.request("tools/call", json!({"name": name, "arguments": arguments}));
        let text = reply["result"]["content"][0]["text"]
            .as_str()
            .or(reply["error"]["message"].as_str());
        assert_eq!(text, Some(error), "{reply}");
    }
    // Other tools still work in live mode, and a live tool may leave out its arguments.
    let (text, failed) = session.call("text", json!({"text": "A"}));
    assert!(!failed, "{text}");
    let reply = session.request("tools/call", json!({"name": "live_read"}));
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    let reply = session.request(
        "tools/call",
        json!({"name": "live_connect", "arguments": "x"}),
    );
    assert_eq!(reply["error"]["message"], "arguments must be an object");
}

#[test]
fn the_bridge_skips_what_is_not_an_answer_and_notices_a_dropped_editor() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, token) = session.connect();
    let mut socket = editor(port, &token);
    let browser = thread::spawn(move || {
        let request: Value =
            serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
        socket.send(Message::binary(vec![1, 2, 3])).unwrap();
        socket.send(Message::text("not JSON")).unwrap();
        let reply = json!({"id": request["id"], "result": {"text": "after"}});
        socket.send(Message::text(reply.to_string())).unwrap();
        // The next request, then gone without a closing handshake.
        socket.read().unwrap();
        drop(socket);
    });
    let (text, failed) = session.call("live_read", json!({}));
    assert!(!failed, "{text}");
    assert!(text.contains("after"), "{text}");
    let (text, failed) = session.call("live_read", json!({}));
    assert!(failed);
    assert!(text.starts_with("The editor disconnected"), "{text}");
    browser.join().unwrap();
}

#[test]
fn an_unanswered_request_keeps_the_editor_and_its_late_answer_is_skipped() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, token) = session.connect();
    let mut socket = editor(port, &token);
    let browser = thread::spawn(move || {
        let first: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
        let second: Value =
            serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
        // The late answer to the first request, then the answer to the second.
        for (request, text) in [(&first, "late"), (&second, "current")] {
            let reply = json!({"id": request["id"], "result": {"text": text}});
            socket.send(Message::text(reply.to_string())).unwrap();
        }
        socket
    });
    let (text, failed) = session.call("live_read", json!({}));
    assert!(failed);
    assert!(text.contains("did not answer"), "{text}");
    let (text, failed) = session.call("live_read", json!({}));
    assert!(!failed, "{text}");
    assert!(text.contains("current"), "{text}");
    let _socket = browser.join().unwrap();
}

#[test]
fn live_write_forwards_expected_sha256_and_advertises_it() {
    let mut session = Session::start(&["mcp", "--live"]);
    let tools = session.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .unwrap()
        .clone();
    let write_tool = tools
        .iter()
        .find(|tool| tool["name"] == "live_write")
        .unwrap();
    assert_eq!(
        write_tool["inputSchema"]["properties"]["expected_sha256"]["type"],
        "string"
    );

    let (port, token) = session.connect();
    let mut socket = capable_editor(port, &token);
    let digest = "a".repeat(64);
    let browser = thread::spawn(move || {
        let accepted = answer(
            &mut socket,
            json!({"result": {"revision": "0100", "sha256": "b".repeat(64)}}),
        );
        let rejected = answer(
            &mut socket,
            json!({"error": "The frame content digest changed since it was read: read it again"}),
        );
        (accepted, rejected)
    });
    let (text, failed) = session.call(
        "live_write",
        json!({"revision": "00ff", "expected_sha256": digest, "text": "ok"}),
    );
    assert!(!failed, "{text}");
    assert!(text.contains(&"b".repeat(64)), "{text}");
    let (text, failed) = session.call(
        "live_write",
        json!({"revision": "0100", "expected_sha256": "0".repeat(64), "text": "no"}),
    );
    assert!(failed);
    assert_eq!(
        text,
        "The frame content digest changed since it was read: read it again"
    );
    let (accepted, rejected) = browser.join().unwrap();
    assert_eq!(accepted["expected_sha256"], json!("a".repeat(64)));
    assert_eq!(rejected["expected_sha256"], json!("0".repeat(64)));
}

#[test]
fn legacy_editor_without_sha256_capability_refuses_guarded_writes_before_forwarding() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, token) = session.connect();
    let mut legacy = editor(port, &token);
    let browser = thread::spawn(move || {
        // The guarded write must never reach the legacy editor; the very first frame it sees
        // must be the follow-up live_read.
        let seen = answer(&mut legacy, json!({"result": {"text": "untouched"}}));
        (legacy, seen)
    });
    let (text, failed) = session.call(
        "live_write",
        json!({"revision": "00ff", "expected_sha256": "a".repeat(64), "text": "overwrite"}),
    );
    assert!(failed, "{text}");
    assert!(text.contains("expected_sha256"), "{text}");

    let (text, failed) = session.call("live_read", json!({}));
    assert!(!failed, "{text}");
    assert!(text.contains("untouched"), "{text}");
    let (mut legacy, seen) = browser.join().unwrap();
    assert_eq!(seen["op"], "read");

    // Replacing with a capable editor enables expected_sha256, and reconnecting a legacy editor
    // resets the capability back to false.
    let mut modern = capable_editor(port, &token);
    assert!(closed(&mut legacy));
    let modern_thread = thread::spawn(move || {
        let write = answer(
            &mut modern,
            json!({"result": {"revision": "0100", "sha256": "b".repeat(64)}}),
        );
        (modern, write)
    });
    let (text, failed) = session.call(
        "live_write",
        json!({"revision": "00ff", "expected_sha256": "a".repeat(64), "text": "ok"}),
    );
    assert!(!failed, "{text}");
    let (mut modern, write) = modern_thread.join().unwrap();
    assert_eq!(write["op"], "write");

    let legacy_again = editor(port, &token);
    assert!(closed(&mut modern));
    let (text, failed) = session.call(
        "live_write",
        json!({"revision": "0100", "expected_sha256": "b".repeat(64), "text": "blocked"}),
    );
    assert!(failed, "{text}");
    assert!(text.contains("expected_sha256"), "{text}");
    drop(legacy_again);
}
