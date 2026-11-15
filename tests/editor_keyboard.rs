// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Walks the browser editor's Layers panel with the keyboard alone, in a real Chrome driven over
//! WebDriver: Tab from the top of the page reaches the panel, each control takes focus in order,
//! and Enter, Space, the arrow keys and typing operate it. `src/web/tests.rs` dispatches DOM
//! events, which cannot move focus; these keys come from the browser itself.
//!
//! It needs the built editor in `web/pkg` (see 'Build the browser editor' in the README) and
//! `CHROMEDRIVER` set to a chromedriver matching an installed Chrome or Chromium, so it is
//! ignored by default. CI's browser job runs it with
//! `cargo test --locked --test editor_keyboard -- --ignored`.
#![cfg(feature = "cli")]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const TAB: &str = "\u{E004}";
const ENTER: &str = "\u{E007}";
const SHIFT: &str = "\u{E008}";
const CONTROL: &str = "\u{E009}";
const SPACE: &str = "\u{E00D}";
const ARROW_UP: &str = "\u{E013}";

/// Serve `web/` over HTTP on a free loopback port, as the README asks; WebAssembly cannot load
/// from a `file:` URL.
fn serve(root: PathBuf) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let root = root.clone();
            std::thread::spawn(move || respond(stream, &root));
        }
    });
    port
}
fn respond(mut stream: TcpStream, root: &Path) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request = String::new();
    reader.read_line(&mut request).unwrap();
    // Read the headers so closing the connection does not reset it.
    let mut header = String::new();
    while reader.read_line(&mut header).unwrap_or(0) > 2 {
        header.clear();
    }
    let path = request
        .split(' ')
        .nth(1)
        .unwrap_or("/")
        .split(['?', '#'])
        .next()
        .unwrap();
    let path = if path == "/" { "/index.html" } else { path };
    let file = root.join(path.trim_start_matches('/'));
    let found = if path.contains("..") {
        None
    } else {
        std::fs::read(&file).ok()
    };
    let (status, body) = match found {
        Some(body) => ("200 OK", body),
        None => ("404 Not Found", Vec::new()),
    };
    let kind = match file.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        _ => "text/plain; charset=utf-8",
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
}

/// A chromedriver process and one headless browser session, both ended on drop.
struct Browser {
    driver: Child,
    agent: ureq::Agent,
    session: String,
}
impl Browser {
    fn start() -> Self {
        let program = std::env::var("CHROMEDRIVER")
            .expect("set CHROMEDRIVER to a chromedriver matching an installed Chrome or Chromium");
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let driver = Command::new(program)
            .arg(format!("--port={port}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("chromedriver starts");
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .into();
        let mut browser = Browser {
            driver,
            agent,
            session: format!("http://127.0.0.1:{port}"),
        };
        let started = Instant::now();
        while browser.agent.get(browser.session.as_str()).call().is_err() {
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "chromedriver did not answer"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        // Reduced motion starts the editor paused, so playback does not redraw the panel.
        let created = browser.post(
            "/session",
            &json!({"capabilities": {"alwaysMatch": {"goog:chromeOptions": {"args": [
                "--headless=new",
                "--force-prefers-reduced-motion",
                "--window-size=1280,1600"
            ]}}}}),
        );
        let id = created["sessionId"].as_str().expect("a WebDriver session");
        browser.session = format!("{}/session/{id}", browser.session);
        browser
    }
    /// POST a WebDriver command relative to the session and return its `value`.
    fn post(&self, path: &str, body: &Value) -> Value {
        let mut response = self
            .agent
            .post(format!("{}{path}", self.session))
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .expect("WebDriver answers");
        let ok = response.status().is_success();
        let reply: Value =
            serde_json::from_str(&response.body_mut().read_to_string().unwrap()).unwrap();
        assert!(ok, "WebDriver {path} failed: {reply}");
        reply["value"].clone()
    }
    fn script(&self, source: &str) -> Value {
        self.post("/execute/sync", &json!({"script": source, "args": []}))
    }
    /// Press the keys together, then release them in reverse order.
    fn chord(&self, keys: &[&str]) {
        let mut actions: Vec<Value> = keys
            .iter()
            .map(|k| json!({"type": "keyDown", "value": k}))
            .collect();
        actions.extend(
            keys.iter()
                .rev()
                .map(|k| json!({"type": "keyUp", "value": k})),
        );
        self.post(
            "/actions",
            &json!({"actions": [{"type": "key", "id": "keyboard", "actions": actions}]}),
        );
    }
    fn press(&self, key: &str, times: usize) {
        for _ in 0..times {
            self.chord(&[key]);
        }
    }
    fn type_text(&self, text: &str) {
        for c in text.chars() {
            self.chord(&[&c.to_string()]);
        }
    }
    /// The focused element's id, `summary` for the Layers summary, or its tag name.
    fn focused(&self) -> String {
        self.script(
            "const e = document.activeElement;
             if (e === document.querySelector('#layers-panel > summary')) return 'summary';
             return e.id || e.tagName.toLowerCase();",
        )
        .as_str()
        .unwrap()
        .to_string()
    }
    /// Wait up to 5 seconds for `read` to give `expected`: the browser runs a key's default
    /// action, such as opening a summary, after WebDriver has delivered the key.
    fn settles<T: PartialEq + std::fmt::Debug>(&self, expected: T, read: impl Fn(&Self) -> T) {
        let started = Instant::now();
        loop {
            let value = read(self);
            if value == expected {
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "expected {expected:?}, still {value:?}; focus on {}",
                self.focused()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    /// Press Tab (or Shift+Tab) once for each id and check where focus lands.
    fn tab_through(&self, ids: &[&str], back: bool) {
        for id in ids {
            if back {
                self.chord(&[SHIFT, TAB]);
            } else {
                self.chord(&[TAB]);
            }
            self.settles(id.to_string(), Self::focused);
        }
    }
    /// The Layers list as shown, top layer first, and the name field.
    fn layers(&self) -> (Vec<String>, String) {
        let value = self.script(
            "return [[...document.getElementById('layer').options].map(o => o.text),
                     document.getElementById('layer-name').value];",
        );
        let list = value[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        (list, value[1].as_str().unwrap().to_string())
    }
    fn status(&self) -> String {
        self.script("return document.getElementById('status').textContent;")
            .as_str()
            .unwrap()
            .to_string()
    }
}
impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.agent.delete(self.session.as_str()).call();
        let _ = self.driver.kill();
        let _ = self.driver.wait();
    }
}

#[test]
#[ignore = "needs the built editor in web/pkg and CHROMEDRIVER; CI's browser job runs it"]
fn layers_panel_works_from_the_keyboard_alone() {
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
    assert!(
        web.join("pkg/apollinaire_ascii_bg.wasm").is_file(),
        "build the editor into web/pkg first (README: Build the browser editor)"
    );
    let port = serve(web);
    let browser = Browser::start();
    browser.post(
        "/url",
        &json!({"url": format!("http://127.0.0.1:{port}/index.html")}),
    );
    let started = Instant::now();
    while browser
        .script("return document.getElementById('art').textContent.length;")
        .as_u64()
        == Some(0)
    {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the editor did not load: {}",
            browser.status()
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    // Tab from the top of the page reaches the folded panel, and Enter opens it.
    let mut presses = 0;
    while browser.focused() != "summary" {
        presses += 1;
        assert!(presses < 300, "Tab never reached the Layers panel");
        browser.chord(&[TAB]);
    }
    let open = "return document.getElementById('layers-panel').open;";
    assert_eq!(browser.script(open), false);
    browser.press(ENTER, 1);
    browser.settles(json!(true), |b| b.script(open));

    // With one layer, only the list, name, visibility and Add take focus.
    browser.tab_through(
        &["layer", "layer-name", "layer-visible", "layer-add"],
        false,
    );
    browser.press(ENTER, 1);
    browser.settles(
        (vec!["Layer 2".into(), "Layer 1".into()], "Layer 2".into()),
        Browser::layers,
    );
    browser.settles("layer-add".to_string(), Browser::focused);

    // The new top layer can move down, be deleted or merged; Move up stays out of the order.
    browser.tab_through(&["layer-down", "layer-delete", "layer-merge"], false);
    browser.tab_through(&["layer-delete", "layer-down"], true);
    browser.press(ENTER, 1);
    browser.settles(
        (vec!["Layer 1".into(), "Layer 2".into()], "Layer 2".into()),
        Browser::layers,
    );
    // Move down disabled itself at the bottom; focus stays in the panel on the list.
    browser.settles("layer".to_string(), Browser::focused);

    // The arrow keys choose a layer; a typed name and Space on the checkbox apply to it.
    browser.press(ARROW_UP, 1);
    browser.settles("Layer 1".to_string(), |b| b.layers().1);
    browser.tab_through(&["layer-name"], false);
    browser.chord(&[CONTROL, "a"]);
    browser.type_text("Sky");
    browser.tab_through(&["layer-visible"], false);
    browser.settles(["Sky", "Layer 2"].map(String::from).to_vec(), |b| {
        b.layers().0
    });
    browser.press(SPACE, 1);
    browser.settles(
        ["Sky (hidden)", "Layer 2"].map(String::from).to_vec(),
        |b| b.layers().0,
    );
    browser.press(SPACE, 1);
    browser.settles(["Sky", "Layer 2"].map(String::from).to_vec(), |b| {
        b.layers().0
    });

    // Sky is on top, so Move down is next after Add; Move up disables itself in turn.
    browser.tab_through(&["layer-add", "layer-down"], false);
    browser.press(ENTER, 1);
    browser.settles(["Layer 2", "Sky"].map(String::from).to_vec(), |b| {
        b.layers().0
    });
    browser.settles("layer".to_string(), Browser::focused);
    browser.tab_through(
        &["layer-name", "layer-visible", "layer-add", "layer-up"],
        false,
    );
    browser.press(ENTER, 1);
    browser.settles(["Sky", "Layer 2"].map(String::from).to_vec(), |b| {
        b.layers().0
    });
    browser.settles("layer".to_string(), Browser::focused);

    // Delete leaves one layer and disables itself; focus again stays on the list.
    browser.tab_through(
        &[
            "layer-name",
            "layer-visible",
            "layer-add",
            "layer-down",
            "layer-delete",
        ],
        false,
    );
    browser.press(ENTER, 1);
    browser.settles((vec!["Layer 2".into()], "Layer 2".into()), Browser::layers);
    browser.settles("layer".to_string(), Browser::focused);

    // Merge does the same.
    browser.tab_through(&["layer-name", "layer-visible", "layer-add"], false);
    browser.press(ENTER, 1);
    browser.tab_through(&["layer-down", "layer-delete", "layer-merge"], false);
    browser.press(ENTER, 1);
    browser.settles(["Layer 1"].map(String::from).to_vec(), |b| b.layers().0);
    browser.settles("Layers merged into one.".to_string(), Browser::status);
    browser.settles("layer".to_string(), Browser::focused);
}
