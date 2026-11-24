// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! `apollinaire mcp --live`: the bridge's admission of editors, and of connections that are not.
#![cfg(feature = "cli")]

mod live;

use live::{EDITOR, Session, answer, closed, editor, open};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tungstenite::Message;

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
fn past_the_admission_limit_one_connection_is_closed_for_each_new_one() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, _) = session.connect();
    let streams: Vec<TcpStream> = (0..40)
        .map(|_| TcpStream::connect(("127.0.0.1", port)).unwrap())
        .collect();
    // Well before the 5 second deadline, 8 are closed (a reset on Windows) and 32 still wait.
    thread::sleep(Duration::from_secs(1));
    let still_open = |mut stream: &TcpStream| {
        stream.set_nonblocking(true).unwrap();
        matches!(stream.read(&mut [0; 16]), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock)
    };
    assert_eq!(streams.iter().filter(|s| still_open(s)).count(), 32);
}

#[test]
fn connections_that_never_sign_in_cannot_keep_the_editor_out() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, token) = session.connect();
    // Hold far more connections than the bridge admits at once and keep opening more.
    let stop = Arc::new(AtomicBool::new(false));
    let flood = {
        let stop = stop.clone();
        thread::spawn(move || {
            let mut held = Vec::new();
            while !stop.load(Ordering::SeqCst) {
                held.extend(TcpStream::connect(("127.0.0.1", port)).ok());
                thread::sleep(Duration::from_millis(5));
            }
            held.len()
        })
    };
    thread::sleep(Duration::from_secs(1));
    let mut socket = editor(port, &token);
    let browser = thread::spawn(move || answer(&mut socket, json!({"result": {"text": "in"}})));
    let (text, failed) = session.call("live_read", json!({}));
    stop.store(true, Ordering::SeqCst);
    assert!(!failed, "{text}");
    assert!(flood.join().unwrap() > 64);
    browser.join().unwrap();
}

#[test]
fn a_connection_trickling_its_handshake_is_closed_at_the_deadline() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, _) = session.connect();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let start = Instant::now();
    // One byte a second never ends the request, and keeps each read within its timeout.
    let request = format!(
        "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Slow: {}",
        "a".repeat(30)
    );
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut closed_after = None;
    for byte in request.bytes() {
        let sent = stream.write_all(&[byte]);
        // The bridge sends nothing before the request ends, so a read returns only at its close.
        let ended = match stream.read(&mut [0; 16]) {
            Ok(_) => true,
            Err(e) => !matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ),
        };
        if sent.is_err() || ended {
            closed_after = Some(start.elapsed());
            break;
        }
    }
    let closed_after = closed_after.expect("the bridge kept a trickling connection open");
    assert!(closed_after < Duration::from_secs(8), "{closed_after:?}");
}

#[test]
fn a_message_longer_than_a_token_is_refused_before_it_is_read() {
    let mut session = Session::start(&["mcp", "--live"]);
    let (port, _) = session.connect();
    let mut socket = open(port, Some(EDITOR), None).unwrap();
    // Only the header of a masked text frame that declares 1 MiB: a bridge that waited for the
    // payload would hold the connection until the admission deadline.
    let mut header = vec![0x81, 0xFF];
    header.extend_from_slice(&(1u64 << 20).to_be_bytes());
    header.extend_from_slice(&[1, 2, 3, 4]);
    let start = Instant::now();
    socket.get_mut().write_all(&header).unwrap();
    assert!(closed(&mut socket));
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "{:?}",
        start.elapsed()
    );
}
