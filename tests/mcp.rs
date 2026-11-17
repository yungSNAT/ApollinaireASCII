// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! `apollinaire mcp`, driven over stdio as an agent's client would.
#![cfg(feature = "cli")]

use base64::Engine;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

const BIN: &str = env!("CARGO_BIN_EXE_apollinaire");
const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;

fn cli(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

/// What the command prints, which the matching tool must return.
fn printed(args: &[&str], stdin: &[u8]) -> String {
    let output = cli(args, stdin);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// The server's replies to `input`, which is written whole before stdin closes.
fn serve_from(program: &str, input: Vec<u8>) -> Vec<Value> {
    let mut child = Command::new(program)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies = String::from_utf8(output.stdout).unwrap();
    replies
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn serve(messages: &[Value]) -> Vec<Value> {
    let mut input = Vec::new();
    for message in messages {
        writeln!(input, "{message}").unwrap();
    }
    serve_from(BIN, input)
}

/// A request in the stateless protocol, carrying its version and capabilities.
fn request(id: u64, method: &str, mut params: Value) -> Value {
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn call(id: u64, name: &str, arguments: Value) -> Value {
    request(
        id,
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    )
}

/// The text a call returned, and whether it is an error.
fn returned(reply: &Value) -> (String, bool) {
    let result = &reply["result"];
    assert_eq!(result["content"].as_array().unwrap().len(), 1, "{reply}");
    assert_eq!(result["content"][0]["type"], "text");
    let text = result["content"][0]["text"].as_str().unwrap().to_string();
    (text, result["isError"].as_bool().unwrap())
}

fn error(reply: &Value) -> (i64, &str) {
    (
        reply["error"]["code"].as_i64().unwrap(),
        reply["error"]["message"].as_str().unwrap(),
    )
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let picture = image::RgbaImage::from_fn(width, height, |x, y| {
        let shade = u8::try_from((x * 37 + y * 11) % 256).unwrap();
        image::Rgba([shade, shade, shade, 255])
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    picture
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

#[test]
fn legacy_clients_initialize_then_list_and_call_tools() {
    let replies = serve(&[
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {
            "protocolVersion": "1999-01-01"}}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "ping"}),
        json!({"jsonrpc": "2.0", "id": "five", "method": "tools/call",
            "params": {"name": "art", "arguments": {"name": "ghost"}}}),
    ]);
    // The notification gets no reply.
    assert_eq!(replies.len(), 5);
    let first = &replies[0]["result"];
    assert_eq!(first["protocolVersion"], "2025-06-18");
    assert_eq!(first["capabilities"], json!({"tools": {}}));
    assert_eq!(first["serverInfo"]["name"], "apollinaire");
    assert!(first["instructions"].as_str().unwrap().contains("list"));
    // An unknown version gets the newest one that begins with initialize.
    assert_eq!(replies[1]["result"]["protocolVersion"], "2025-11-25");
    let listed = &replies[2]["result"];
    let names: Vec<_> = listed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "list",
            "art",
            "text",
            "say",
            "table",
            "tree",
            "diagram",
            "image",
            "effect",
            "animate",
            "generate_maze",
            "generate_sky",
            "generate_pattern",
            "transform",
            "check"
        ]
    );
    // Fields of the stateless protocol stay out of the older one.
    assert!(listed.get("resultType").is_none() && listed.get("ttlMs").is_none());
    assert_eq!(replies[3], json!({"jsonrpc": "2.0", "id": 4, "result": {}}));
    assert_eq!(replies[4]["id"], "five");
    assert_eq!(
        returned(&replies[4]),
        (printed(&["art", "ghost"], b""), false)
    );
}

#[test]
fn stateless_clients_discover_and_send_their_version_each_time() {
    let unsupported = json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list", "params": {
        "_meta": {"io.modelcontextprotocol/protocolVersion": "2025-11-25",
            "io.modelcontextprotocol/clientCapabilities": {}}}});
    let no_capabilities = json!({"jsonrpc": "2.0", "id": 5, "method": "tools/list", "params": {
        "_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}}});
    let replies = serve(&[
        json!({"jsonrpc": "2.0", "id": 1, "method": "server/discover"}),
        request(2, "server/discover", json!({})),
        request(3, "tools/list", json!({"cursor": "ignored"})),
        unsupported,
        no_capabilities,
        json!({"jsonrpc": "2.0", "id": 6, "method": "tools/list"}),
        request(7, "resources/list", json!({})),
        request(8, "ping", json!({})),
    ]);
    for discovered in &replies[..2] {
        let result = &discovered["result"];
        assert_eq!(result["supportedVersions"], json!(["2026-07-28"]));
        assert_eq!(result["capabilities"], json!({"tools": {}}));
        assert_eq!(result["resultType"], "complete");
        assert_eq!(result["cacheScope"], "public");
        assert!(result["ttlMs"].as_u64().unwrap() > 0);
        assert_eq!(
            result["_meta"]["io.modelcontextprotocol/serverInfo"]["version"],
            env!("CARGO_PKG_VERSION")
        );
    }
    let listed = &replies[2]["result"];
    assert_eq!(listed["tools"].as_array().unwrap().len(), 15);
    assert_eq!(listed["resultType"], "complete");
    assert_eq!(listed["cacheScope"], "public");
    assert_eq!(
        replies[3]["error"],
        json!({"code": -32022, "message": "Unsupported protocol version",
            "data": {"supported": ["2026-07-28"], "requested": "2025-11-25"}})
    );
    let (code, message) = error(&replies[4]);
    assert_eq!(code, -32602);
    assert!(message.contains("clientCapabilities"), "{message}");
    // Without initialize, a request needs its version.
    let (code, message) = error(&replies[5]);
    assert_eq!(code, -32602);
    assert!(
        message.contains("protocolVersion") && message.contains("initialize"),
        "{message}"
    );
    assert_eq!(
        error(&replies[6]),
        (-32601, "Unknown method resources/list")
    );
    assert_eq!(replies[7]["result"]["resultType"], "complete");
}

#[test]
fn tool_schemas_come_from_the_command_line() {
    let replies = serve(&[request(1, "tools/list", json!({}))]);
    let tools = replies[0]["result"]["tools"].as_array().unwrap();
    let tool = |name: &str| tools.iter().find(|tool| tool["name"] == name).unwrap();
    let schema = |name: &str| tool(name)["inputSchema"].clone();
    for tool in tools {
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["annotations"]["openWorldHint"], false);
        assert!(!tool["description"].as_str().unwrap().is_empty());
    }
    let art = schema("art");
    assert_eq!(art["required"], json!(["name"]));
    assert_eq!(
        art["properties"]["border"]["enum"],
        json!(["plain", "rounded", "dotted", "heavy"])
    );
    assert_eq!(
        art["properties"]["format"]["enum"],
        json!([
            "text",
            "json",
            "html",
            "cast",
            "react",
            "ink",
            "opentui",
            "bubbletea"
        ])
    );
    assert_eq!(art["properties"]["frame"]["type"], "integer");
    let image = schema("image");
    assert_eq!(image["required"], json!(["data"]));
    assert_eq!(image["properties"]["width"]["default"], 80);
    assert_eq!(image["properties"]["gamma"]["default"], 1.0);
    assert_eq!(image["properties"]["invert"]["type"], "boolean");
    assert_eq!(image["properties"]["invert"]["default"], false);
    assert_eq!(image["properties"]["ramp"]["default"], "standard");
    assert!(image["properties"].get("output_dir").is_none());
    assert_eq!(
        image["properties"]["contrast"]["description"],
        "Contrast around mid-grey, 0.1 to 5.0"
    );
    // The command's own description names files and URLs, which the tool does not take.
    assert_eq!(
        tool("image")["description"],
        "Convert an image or animated GIF to text."
    );
    assert_eq!(schema("say")["required"], json!(["message"]));
    assert_eq!(
        schema("transform")["required"],
        json!(["text", "operation"])
    );
    assert!(schema("effect")["required"].as_array().unwrap() == &[json!("effect")]);
    // Commands that do not export artwork take no export options.
    for name in ["list", "check"] {
        assert!(schema(name)["properties"].get("format").is_none());
    }
    // No tool takes a file or directory.
    for tool in tools {
        for name in [
            "output",
            "output_dir",
            "font_file",
            "art_file",
            "input",
            "inputs",
        ] {
            assert!(tool["inputSchema"]["properties"].get(name).is_none());
        }
    }
}

#[test]
fn tools_return_what_their_commands_print() {
    let picture = png(24, 12);
    let data = base64::engine::general_purpose::STANDARD.encode(&picture);
    let rows = "name,size\nghost,small\n";
    let outline = "root\n  leaf\n  branch\n    twig\n";
    let calls = [
        (
            call(1, "art", json!({"name": "cat", "border": "rounded"})),
            printed(&["art", "cat", "--border=rounded"], b""),
        ),
        (
            call(
                2,
                "text",
                json!({"text": "-Hi", "font": "small", "width": 30, "align": "right"}),
            ),
            printed(
                &[
                    "text", "--font", "small", "--width", "30", "--align", "right", "--", "-Hi",
                ],
                b"",
            ),
        ),
        (
            call(
                3,
                "say",
                json!({"message": "Moo", "art": "cat", "style": "heavy"}),
            ),
            printed(&["say", "Moo", "--art", "cat", "--style", "heavy"], b""),
        ),
        (
            call(
                4,
                "table",
                json!({"text": rows, "separator": ",", "rules": "every"}),
            ),
            printed(
                &["table", "-", "--separator", ",", "--rules", "every"],
                rows.as_bytes(),
            ),
        ),
        (
            call(5, "tree", json!({"text": outline})),
            printed(&["tree", "-"], outline.as_bytes()),
        ),
        (
            call(
                6,
                "image",
                json!({"data": data, "width": 20, "invert": true,
                "dither": false, "gamma": 1.5}),
            ),
            printed(
                &["image", "-", "--width", "20", "--invert", "--gamma", "1.5"],
                &picture,
            ),
        ),
        (
            call(
                7,
                "effect",
                json!({"effect": "scatter", "text": "ab\nc", "fps": 5}),
            ),
            printed(&["effect", "scatter", "-", "--fps", "5"], b"ab\nc"),
        ),
        (
            call(
                8,
                "effect",
                json!({"effect": "wave", "art": "cat", "reverse": true}),
            ),
            printed(&["effect", "wave", "--art", "cat", "--reverse"], b""),
        ),
        (
            call(
                9,
                "animate",
                json!({"scene": "donut", "frames": 3, "width": 20,
                "height": 10, "format": "text", "frame": 2}),
            ),
            printed(
                &[
                    "animate", "donut", "--frames", "3", "--width", "20", "--height", "10",
                    "--frame", "2",
                ],
                b"",
            ),
        ),
        (
            call(
                10,
                "generate_maze",
                json!({"width": 11, "height": 7, "seed": 3,
                "solution": true, "style": "lines"}),
            ),
            printed(
                &[
                    "generate",
                    "maze",
                    "--width",
                    "11",
                    "--height",
                    "7",
                    "--seed",
                    "3",
                    "--solution",
                    "--style",
                    "lines",
                ],
                b"",
            ),
        ),
        (
            call(
                11,
                "generate_sky",
                json!({"width": 30, "height": 10, "stars": 9,
                "big_stars": 1, "phase": "waning"}),
            ),
            printed(
                &[
                    "generate",
                    "sky",
                    "--width",
                    "30",
                    "--height",
                    "10",
                    "--stars",
                    "9",
                    "--big-stars",
                    "1",
                    "--phase",
                    "waning",
                ],
                b"",
            ),
        ),
        (
            call(
                12,
                "generate_pattern",
                json!({"name": "checker", "width": 8, "height": 3}),
            ),
            printed(
                &[
                    "generate", "pattern", "checker", "--width", "8", "--height", "3",
                ],
                b"",
            ),
        ),
        (
            call(
                13,
                "transform",
                json!({"text": "ab\ncd", "operation": "crop", "x": 1,
                "width": 1, "height": 2}),
            ),
            printed(
                &[
                    "transform",
                    "-",
                    "crop",
                    "--x",
                    "1",
                    "--width",
                    "1",
                    "--height",
                    "2",
                ],
                b"ab\ncd",
            ),
        ),
        (
            call(14, "list", json!({"tag": "cat"})),
            printed(&["list", "--tag", "cat"], b""),
        ),
        (call(15, "list", Value::Null), printed(&["list"], b"")),
        (
            call(16, "check", json!({"text": "ab\ncd\n"})),
            printed(&["check", "-"], b"ab\ncd\n"),
        ),
        (
            call(17, "art", json!({"name": "ghost", "format": "json"})),
            printed(&["art", "ghost", "--format", "json"], b""),
        ),
        (
            call(
                18,
                "transform",
                json!({"text": "ab/\ncd-", "operation": "rotate", "turns": 3}),
            ),
            printed(&["transform", "-", "rotate", "--turns", "3"], b"ab/\ncd-"),
        ),
        (
            call(20, "diagram", json!({"text": "graph LR\nA --> B"})),
            printed(&["diagram", "-"], b"graph LR\nA --> B"),
        ),
        (
            call(
                19,
                "transform",
                json!({"text": "Hi", "operation": "border", "design": "curly"}),
            ),
            printed(&["transform", "-", "border", "--design", "curly"], b"Hi"),
        ),
    ];
    let messages: Vec<_> = calls.iter().map(|(message, _)| message.clone()).collect();
    let replies = serve(&messages);
    assert_eq!(replies.len(), calls.len());
    for ((message, expected), reply) in calls.iter().zip(&replies) {
        assert_eq!(reply["id"], message["id"]);
        assert_eq!(reply["result"]["resultType"], "complete");
        assert_eq!(returned(reply), (expected.clone(), false), "{message}");
    }
}

#[test]
fn command_errors_return_to_the_agent_as_tool_errors() {
    let replies = serve(&[
        call(1, "check", json!({"text": "a\tb\n"})),
        call(2, "art", json!({"name": "no-such-piece"})),
        call(3, "image", json!({"data": "aGVsbG8="})),
        call(4, "art", json!({"name": "ghost", "border": "wavy"})),
    ]);
    let (report, failed) = returned(&replies[0]);
    assert!(failed);
    assert!(report.starts_with("1 problem in stdin:"), "{report}");
    assert!(report.contains("line 1, column 2"), "{report}");
    for reply in &replies[1..] {
        let (text, failed) = returned(reply);
        assert!(failed, "{text}");
        assert!(!text.starts_with("apollinaire:"), "{text}");
        assert!(!text.is_empty());
    }
    assert!(returned(&replies[3]).0.contains("wavy"));
}

#[test]
fn calls_outside_a_tool_schema_are_refused() {
    let refusals = [
        (call(1, "draw", json!({})), "Unknown tool: draw"),
        (
            request(2, "tools/call", json!({"name": "art", "arguments": [1]})),
            "arguments must be an object",
        ),
        (request(3, "tools/call", json!({})), "Unknown tool: "),
        (
            call(4, "image", json!({"data": "", "output_dir": "/tmp"})),
            "Invalid arguments for tool image: it has no argument output_dir",
        ),
        (
            call(5, "say", json!({"art": "cat"})),
            "Invalid arguments for tool say: message is required",
        ),
        (
            call(6, "tree", json!({})),
            "Invalid arguments for tool tree: text is required",
        ),
        (
            call(7, "tree", json!({"text": 5})),
            "Invalid arguments for tool tree: text must be a string",
        ),
        (
            call(8, "image", json!({"data": "not base64!"})),
            "Invalid arguments for tool image: data is not base64",
        ),
        (
            call(9, "image", json!({"data": "", "width": "wide"})),
            "Invalid arguments for tool image: width must be of type integer",
        ),
        (
            call(10, "image", json!({"data": "", "width": 1.5})),
            "Invalid arguments for tool image: width must be of type integer",
        ),
        (
            call(11, "image", json!({"data": "", "invert": "yes"})),
            "Invalid arguments for tool image: invert must be of type boolean",
        ),
        (
            call(12, "image", json!({"data": "", "gamma": "high"})),
            "Invalid arguments for tool image: gamma must be of type number",
        ),
        (
            call(13, "art", json!({"name": 7})),
            "Invalid arguments for tool art: name must be of type string",
        ),
    ];
    let messages: Vec<_> = refusals
        .iter()
        .map(|(message, _)| message.clone())
        .collect();
    for ((_, expected), reply) in refusals.iter().zip(serve(&messages)) {
        let (code, message) = error(&reply);
        assert_eq!(code, -32602);
        assert!(message.starts_with(expected), "{message}");
    }
}

#[test]
fn messages_that_are_not_requests_get_json_rpc_errors_or_nothing() {
    let mut input = b"not json\n[1, 2]\n\n   \n".to_vec();
    let mut missing = call(6, "say", json!({"message": "hi"}));
    missing.as_object_mut().unwrap().remove("jsonrpc");
    let mut old = call(7, "say", json!({"message": "hi"}));
    old["jsonrpc"] = json!("1.0");
    let mut number = call(8, "say", json!({"message": "hi"}));
    number["jsonrpc"] = json!(2);
    for message in [
        json!({"jsonrpc": "2.0", "id": {"nested": 1}, "method": "ping"}),
        json!({"jsonrpc": "2.0", "id": 3}),
        json!({"jsonrpc": "2.0", "id": 4, "result": {}}),
        json!({"jsonrpc": "2.0", "id": 5, "error": {"code": 1, "message": "no"}}),
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled"}),
        missing,
        old,
        number,
    ] {
        writeln!(input, "{message}").unwrap();
    }
    let replies = serve_from(BIN, input);
    assert_eq!(replies.len(), 7, "{replies:?}");
    // A call without JSON-RPC 2.0's version is refused and its tool never runs.
    for (reply, id) in replies[4..].iter().zip(6..) {
        assert_eq!(reply["id"], id);
        assert_eq!(
            error(reply),
            (-32600, r#"A message needs "jsonrpc": "2.0""#)
        );
    }
    assert_eq!(
        replies[0],
        json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "Not JSON"}})
    );
    assert_eq!(error(&replies[1]), (-32600, "Expected one JSON-RPC object"));
    assert_eq!(replies[2]["id"], json!({"nested": 1}));
    assert_eq!(error(&replies[2]).0, -32600);
    assert_eq!(replies[3]["id"], 3);
    assert_eq!(error(&replies[3]).0, -32600);
}

#[test]
fn messages_over_32_mib_are_refused_and_the_next_one_is_read() {
    // A message of exactly the limit is read.
    let ping = request(1, "ping", json!({"pad": ""})).to_string();
    let pad = MAX_MESSAGE_BYTES - ping.len();
    let exact = ping.replace("\"pad\":\"\"", &format!("\"pad\":\"{}\"", "x".repeat(pad)));
    assert_eq!(exact.len(), MAX_MESSAGE_BYTES);
    let mut input = format!("{exact}\n").into_bytes();
    input.extend(vec![b'x'; MAX_MESSAGE_BYTES + 100]);
    input.push(b'\n');
    input.extend(format!("{}\n", request(2, "ping", json!({}))).bytes());
    // The last line is too long and never ends.
    input.extend(vec![b'y'; MAX_MESSAGE_BYTES * 2]);
    let replies = serve_from(BIN, input);
    assert_eq!(replies.len(), 4);
    assert_eq!(replies[0]["id"], 1);
    assert!(replies[0].get("result").is_some());
    let too_long = json!({"jsonrpc": "2.0", "id": null,
        "error": {"code": -32600, "message": "A message can be at most 32 MiB"}});
    assert_eq!(replies[1], too_long);
    // The rest of the long line is skipped, so the request after it is answered.
    assert_eq!(replies[2]["id"], 2);
    assert_eq!(replies[2]["result"]["resultType"], "complete");
    assert_eq!(replies[3], too_long);
}

#[cfg(unix)]
#[test]
fn a_server_whose_program_is_gone_reports_tool_errors() {
    // A hard link beside the program, not a copy: a file still open for writing when another
    // test forks cannot be run (ETXTBSY).
    let bin = std::path::Path::new(BIN);
    let dir = bin
        .parent()
        .unwrap()
        .join(format!("mcp-gone-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let copy = dir.join("apollinaire");
    std::fs::hard_link(bin, &copy).unwrap();
    let mut child = Command::new(&copy)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", call(1, "art", json!({"name": "ghost"}))).unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    let reply: Value = serde_json::from_slice(&output.stdout).unwrap();
    let (text, failed) = returned(&reply);
    assert!(failed);
    assert!(text.starts_with("Cannot run apollinaire: "), "{text}");
}
