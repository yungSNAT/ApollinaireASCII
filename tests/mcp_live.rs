// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! `apollinaire mcp --live`: an agent's client on stdio and a stand-in for the browser editor
//! on the loopback WebSocket.
#![cfg(feature = "cli")]

mod live;

use apollinaire_ascii::live_edit::SignIn;
use live::{EDITOR, Session, answer, capable_editor, closed, current_editor, editor, open};
use serde_json::{Value, json};
use std::thread;
use tungstenite::Message;

const LIVE: [&str; 8] = [
    "live_connect",
    "live_read",
    "live_write",
    "live_batch",
    "live_elements",
    "live_element_add",
    "live_element_edit",
    "live_element_delete",
];

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
            json!({"unknown": 1}),
            "Unknown argument unknown",
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

#[test]
fn updated_browser_sign_in_falls_back_once_to_bare_token_for_legacy_bridge() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let token = "c".repeat(64);
    let expected = token.clone();
    // Simulate a pre-PR legacy bridge that only accepts the bare 64-char token.
    let legacy_bridge = thread::spawn(move || {
        let mut seen = Vec::new();
        for _ in 0..2 {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            let given = ws.read().unwrap().into_text().unwrap().to_string();
            seen.push(given.clone());
            if given == expected {
                ws.send(Message::text(r#"{"hello":"apollinaire"}"#))
                    .unwrap();
                let _ = ws.close(None);
                let _ = ws.flush();
            } else {
                let _ = ws.close(None);
                let _ = ws.flush();
            }
        }
        seen
    });

    let mut sign_in = SignIn::new(token.clone());
    // First attempt sends the JSON capability envelope; legacy bridge closes before hello.
    let mut first = open(port, Some(EDITOR), None).unwrap();
    first
        .send(Message::text(sign_in.initial_message()))
        .unwrap();
    assert!(closed(&mut first));

    // Bounded single fallback retries with the bare token and succeeds.
    let bare = sign_in.fallback_message(false).unwrap().to_owned();
    assert_eq!(bare, token);
    let mut second = open(port, Some(EDITOR), None).unwrap();
    second.send(Message::text(bare)).unwrap();
    let hello: Value = serde_json::from_str(second.read().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(hello["hello"], "apollinaire");
    sign_in.mark_greeted();
    assert_eq!(sign_in.fallback_message(false), None);

    let seen = legacy_bridge.join().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].starts_with('{'));
    assert_eq!(seen[1], token);
}

#[test]
fn live_read_and_write_advertise_and_forward_layer_frame_composite_and_dry_run() {
    let mut session = Session::start(&["mcp", "--live"]);
    let tools = session.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .unwrap()
        .clone();
    let read_tool = tools.iter().find(|t| t["name"] == "live_read").unwrap();
    for key in ["layer", "frame", "composite"] {
        assert!(
            read_tool["inputSchema"]["properties"].get(key).is_some(),
            "{read_tool}"
        );
    }
    let write_tool = tools.iter().find(|t| t["name"] == "live_write").unwrap();
    for key in ["layer", "frame", "dry_run"] {
        assert!(
            write_tool["inputSchema"]["properties"].get(key).is_some(),
            "{write_tool}"
        );
    }

    let (port, token) = session.connect();
    let mut socket = capable_editor(port, &token);
    let browser = thread::spawn(move || {
        let targeted_read = answer(
            &mut socket,
            json!({"result": {"text": "ab", "layer": 1, "frame": 2}}),
        );
        let composite_read = answer(
            &mut socket,
            json!({"result": {"composite": true, "frame": 0, "text": "xy"}}),
        );
        let dry_write = answer(
            &mut socket,
            json!({"result": {"dry_run": true, "text": "cd", "changed": 2, "revision": "0200"}}),
        );
        (targeted_read, composite_read, dry_write)
    });

    let (text, failed) = session.call("live_read", json!({"layer": 1, "frame": 2}));
    assert!(!failed, "{text}");
    assert!(text.contains("\"layer\":1"), "{text}");
    let (text, failed) = session.call("live_read", json!({"composite": true, "frame": 0}));
    assert!(!failed, "{text}");
    assert!(text.contains("\"composite\":true"), "{text}");
    let (text, failed) = session.call(
        "live_write",
        json!({"layer": 1, "frame": 2, "revision": "00ff", "text": "cd", "dry_run": true}),
    );
    assert!(!failed, "{text}");
    assert!(text.contains("\"changed\":2"), "{text}");

    let (targeted_read, composite_read, dry_write) = browser.join().unwrap();
    assert_eq!(
        (
            &targeted_read["op"],
            &targeted_read["layer"],
            &targeted_read["frame"]
        ),
        (&json!("read"), &json!(1), &json!(2))
    );
    assert_eq!(
        (
            &composite_read["op"],
            &composite_read["composite"],
            &composite_read["frame"]
        ),
        (&json!("read"), &json!(true), &json!(0))
    );
    assert_eq!(
        (
            &dry_write["op"],
            &dry_write["layer"],
            &dry_write["frame"],
            &dry_write["dry_run"]
        ),
        (&json!("write"), &json!(1), &json!(2), &json!(true))
    );
}

#[test]
fn live_batch_validates_enforces_sha256_capability_and_forwards() {
    let mut session = Session::start(&["mcp", "--live"]);
    let tools = session.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .unwrap()
        .clone();
    let batch_tool = tools.iter().find(|t| t["name"] == "live_batch").unwrap();
    let item_props = &batch_tool["inputSchema"]["properties"]["writes"]["items"]["properties"];
    assert!(item_props.get("dry_run").is_none());
    assert!(item_props.get("writes").is_none());
    assert!(item_props.get("expected_sha256").is_some());

    for (args, needle) in [
        (json!({"writes": [], "extra": 1}), "Unknown argument extra"),
        (json!({"writes": "nope"}), "1 to 256"),
        (json!({"writes": []}), "1 to 256"),
        (
            json!({"writes": vec![json!({"revision": "0", "text": "a"}); 257]}),
            "1 to 256",
        ),
        (json!({"writes": [42]}), "Write 1: must be an object"),
        (
            json!({"writes": [{"revision": "0", "text": "a", "dry_run": true}]}),
            "Write 1: Unknown argument dry_run",
        ),
        (
            json!({"writes": [{"revision": "0", "text": "a", "writes": []}]}),
            "Write 1: Unknown argument writes",
        ),
    ] {
        let (text, failed) = session.call("live_batch", args);
        assert!(failed, "{text}");
        assert!(text.contains(needle), "{text}");
    }

    let (port, token) = session.connect();
    let mut legacy = editor(port, &token);
    let legacy_thread = thread::spawn(move || {
        let seen = answer(
            &mut legacy,
            json!({"result": {"revisions": [{"layer": 0, "frame": 0, "revision": "0100", "sha256": "b".repeat(64)}]}}),
        );
        (legacy, seen)
    });
    let (text, failed) = session.call(
        "live_batch",
        json!({"writes": [{"revision": "00ff", "expected_sha256": "a".repeat(64), "text": "no"}]}),
    );
    assert!(failed, "{text}");
    assert!(text.contains("expected_sha256"), "{text}");

    let (text, failed) = session.call(
        "live_batch",
        json!({"writes": [{"revision": "00ff", "text": "ok"}]}),
    );
    assert!(!failed, "{text}");
    let (mut legacy, seen) = legacy_thread.join().unwrap();
    assert_eq!(seen["op"], "batch");

    let mut modern = capable_editor(port, &token);
    assert!(closed(&mut legacy));
    let modern_thread = thread::spawn(move || {
        answer(
            &mut modern,
            json!({"result": {"dry_run": true, "revisions": [{"layer": 0, "frame": 0, "revision": "0100", "sha256": "b".repeat(64)}], "frames": [{"layer": 0, "frame": 0, "revision": "0100", "sha256": "b".repeat(64), "width": 2, "height": 1, "text": "ok", "changed": 2}], "changed": 2}}),
        )
    });
    let (text, failed) = session.call(
        "live_batch",
        json!({"dry_run": true, "writes": [{"layer": 0, "frame": 0, "revision": "00ff", "expected_sha256": "a".repeat(64), "text": "ok"}]}),
    );
    assert!(!failed, "{text}");
    assert!(text.contains("\"changed\":2"), "{text}");
    let req = modern_thread.join().unwrap();
    assert_eq!(req["op"], "batch");
    assert_eq!(req["dry_run"], json!(true));
    assert_eq!(req["writes"][0]["expected_sha256"], json!("a".repeat(64)));
}

#[test]
fn element_tools_check_their_arguments_need_an_editor_that_keeps_elements_and_forward() {
    let mut session = Session::start(&["mcp", "--live"]);
    for (tool, args) in [
        ("live_elements", json!({"kind": "box"})),
        (
            "live_element_add",
            json!({"kind": "box", "x": 0, "y": 0, "force": true}),
        ),
        (
            "live_element_edit",
            json!({"element": "e1", "kind": "line"}),
        ),
        ("live_element_delete", json!({"element": "e1", "x": 1})),
    ] {
        let (text, failed) = session.call(tool, args);
        assert!(failed, "{tool}: {text}");
        assert!(text.starts_with("Unknown argument"), "{tool}: {text}");
    }

    // An editor from before elements is refused before anything is sent to it.
    let (port, token) = session.connect();
    let mut older = capable_editor(port, &token);
    let (text, failed) = session.call("live_elements", json!({}));
    assert!(failed, "{text}");
    assert!(text.contains("does not support elements"), "{text}");

    let mut current = current_editor(port, &token);
    assert!(closed(&mut older));
    let browser = thread::spawn(move || {
        let added = answer(
            &mut current,
            json!({"result": {"element": "e1", "layer": 0, "frame": 0, "revision": "00aa", "frame_revision": "00bb", "changed": 6}}),
        );
        let refused = answer(
            &mut current,
            json!({"error": "Element e1 was changed by hand in 2 cells; pass force: true to redraw over them"}),
        );
        (added, refused)
    });
    let (text, failed) = session.call(
        "live_element_add",
        json!({"kind": "box", "x": 1, "y": 2, "width": 3, "height": 2, "style": "heavy"}),
    );
    assert!(!failed, "{text}");
    assert!(text.contains("\"element\":\"e1\""), "{text}");
    let (text, failed) = session.call("live_element_delete", json!({"element": "e1"}));
    assert!(failed, "{text}");
    assert!(text.contains("force: true"), "{text}");
    let (added, refused) = browser.join().unwrap();
    assert_eq!(added["op"], "element_add");
    assert_eq!(
        (
            added["kind"].clone(),
            added["width"].clone(),
            added["style"].clone()
        ),
        (json!("box"), json!(3), json!("heavy"))
    );
    assert_eq!(refused["op"], "element_delete");
    assert_eq!(refused["element"], "e1");
}
