// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! `apollinaire mcp`: the artwork commands as tools for AI agents, by the Model Context
//! Protocol over stdio. Each call runs this program again with the tool's arguments, so a tool
//! does exactly what its command does. Content reaches the command on its stdin, and no tool
//! names a file or URL, so an agent cannot use the server to read files.
//!
//! The server speaks 2026-07-28, where each request carries its version in `_meta`, and the
//! versions before it, which begin with `initialize`.

use crate::Args;
use base64::Engine;
use clap::{Arg, CommandFactory};
use serde_json::{Map, Value, json};
use std::{
    io::{self, BufRead, Read, Write},
    process::{Command, Stdio},
};

/// Versions whose requests carry their version in `_meta`, newest first.
const MODERN: &[&str] = &["2026-07-28"];
/// Versions that begin with `initialize`, newest first.
const LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
/// The longest request line: base64 image data of about 24 MiB.
pub const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;
/// How long a client may cache the tools: they change only with this program.
const TTL_MS: u64 = 3_600_000;
const INSTRUCTIONS: &str = "Every result is artwork in printable ASCII: still art as text, \
animation as an ApollinaireASCII JSON project unless format asks for html or cast. Call list for \
the names that art, fonts, scenes, patterns, effects and ramps take.";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const UNSUPPORTED_VERSION: i64 = -32022;

#[derive(Clone, Copy)]
enum Kind {
    Text,
    Integer,
    Number,
    Boolean,
}

/// Text or base64 bytes for the command's stdin, standing in for its input file.
struct Content {
    name: &'static str,
    description: &'static str,
    base64: bool,
    required: bool,
    /// The command's file argument, given as `-` when the content is.
    replaces: &'static str,
}

struct Tool {
    name: &'static str,
    /// The subcommand, and its own subcommand for `generate`.
    command: &'static [&'static str],
    /// Replaces the command's description where that names files or flags.
    description: Option<&'static str>,
    /// The command's arguments the tool takes, by clap id.
    args: &'static [(&'static str, Kind)],
    /// Arguments the tool needs that the command can read from stdin instead.
    require: &'static [&'static str],
    content: Option<Content>,
    /// Takes the export options `format` and `frame`.
    export: bool,
}

use Kind::{Boolean, Integer, Number, Text};

const SIZE: [(&str, Kind); 3] = [("width", Integer), ("height", Integer), ("seed", Integer)];

const TOOLS: &[Tool] = &[
    Tool {
        name: "list",
        command: &["list"],
        description: None,
        args: &[("tag", Text)],
        require: &[],
        content: None,
        export: false,
    },
    Tool {
        name: "art",
        command: &["art"],
        description: None,
        args: &[("name", Text), ("border", Text)],
        require: &[],
        content: None,
        export: true,
    },
    Tool {
        name: "text",
        command: &["text"],
        description: None,
        args: &[
            ("text", Text),
            ("font", Text),
            ("align", Text),
            ("width", Integer),
            ("wrap", Integer),
            ("horizontal", Text),
            ("vertical", Text),
        ],
        require: &[],
        content: None,
        export: true,
    },
    Tool {
        name: "say",
        command: &["say"],
        description: None,
        args: &[
            ("message", Text),
            ("width", Integer),
            ("style", Text),
            ("art", Text),
        ],
        require: &["message"],
        content: None,
        export: true,
    },
    Tool {
        name: "table",
        command: &["table"],
        description: None,
        args: &[("separator", Text), ("rules", Text)],
        require: &[],
        content: Some(Content {
            name: "text",
            description: "Rows of delimited text, one row per line. Blank lines are skipped.",
            base64: false,
            required: true,
            replaces: "input",
        }),
        export: true,
    },
    Tool {
        name: "tree",
        command: &["tree"],
        description: None,
        args: &[],
        require: &[],
        content: Some(Content {
            name: "text",
            description: "An outline with one node per line. Deeper indentation makes a child.",
            base64: false,
            required: true,
            replaces: "input",
        }),
        export: true,
    },
    Tool {
        name: "diagram",
        command: &["diagram"],
        description: None,
        args: &[("max_width", Integer), ("markdown", Boolean)],
        require: &[],
        content: Some(Content {
            name: "text",
            description: "A Mermaid flowchart, graph, sequence, state, class or ER diagram, Gantt chart, pie chart or timeline (optionally in one Markdown code block), or a Markdown document when markdown is true.",
            base64: false,
            required: true,
            replaces: "input",
        }),
        export: true,
    },
    Tool {
        name: "image",
        command: &["image"],
        description: Some("Convert an image or animated GIF to text."),
        args: &[
            ("width", Integer),
            ("height", Integer),
            ("ramp", Text),
            ("invert", Boolean),
            ("dither", Boolean),
            ("edges", Boolean),
            ("edge_glyphs", Boolean),
            ("gamma", Number),
            ("contrast", Number),
            ("cell_aspect", Number),
            ("background", Integer),
            ("shapes", Boolean),
        ],
        require: &[],
        content: Some(Content {
            name: "data",
            description: "The PNG, JPEG, GIF, WebP, BMP or PNM file, base64-encoded.",
            base64: true,
            required: true,
            replaces: "inputs",
        }),
        export: true,
    },
    Tool {
        name: "effect",
        command: &["effect"],
        description: None,
        args: &[
            ("effect", Text),
            ("art", Text),
            ("reverse", Boolean),
            ("duration", Integer),
            ("fps", Integer),
            ("seed", Integer),
            ("source_frame", Integer),
        ],
        require: &[],
        content: Some(Content {
            name: "text",
            description: "Text art or an ApollinaireASCII JSON project to animate, instead of \
                          art. Ragged rows are padded.",
            base64: false,
            required: false,
            replaces: "input",
        }),
        export: true,
    },
    Tool {
        name: "animate",
        command: &["animate"],
        description: None,
        args: &[
            ("scene", Text),
            ("width", Integer),
            ("height", Integer),
            ("frames", Integer),
            ("fps", Integer),
            ("seed", Integer),
            ("text", Text),
        ],
        require: &[],
        content: None,
        export: true,
    },
    Tool {
        name: "generate_maze",
        command: &["generate", "maze"],
        description: None,
        args: &[
            SIZE[0],
            SIZE[1],
            SIZE[2],
            ("algorithm", Text),
            ("style", Text),
            ("solution", Boolean),
        ],
        require: &[],
        content: None,
        export: true,
    },
    Tool {
        name: "generate_sky",
        command: &["generate", "sky"],
        description: None,
        args: &[
            SIZE[0],
            SIZE[1],
            SIZE[2],
            ("stars", Integer),
            ("big_stars", Integer),
            ("moons", Integer),
            ("phase", Text),
            ("asteroids", Integer),
        ],
        require: &[],
        content: None,
        export: true,
    },
    Tool {
        name: "generate_pattern",
        command: &["generate", "pattern"],
        description: None,
        args: &[
            ("name", Text),
            ("width", Integer),
            ("height", Integer),
            ("text", Text),
        ],
        require: &[],
        content: None,
        export: true,
    },
    Tool {
        name: "transform",
        command: &["transform"],
        description: None,
        args: &[
            ("operation", Text),
            ("x", Integer),
            ("y", Integer),
            ("width", Integer),
            ("height", Integer),
            ("style", Text),
            ("design", Text),
            ("turns", Integer),
        ],
        require: &[],
        content: Some(Content {
            name: "text",
            description: "Text art or an ApollinaireASCII JSON project.",
            base64: false,
            required: true,
            replaces: "input",
        }),
        export: true,
    },
    Tool {
        name: "check",
        command: &["check"],
        description: None,
        args: &[],
        require: &[],
        content: Some(Content {
            name: "text",
            description: "Text art or an ApollinaireASCII JSON project to validate.",
            base64: false,
            required: true,
            replaces: "input",
        }),
        export: false,
    },
];

/// The export options a tool with `export` takes.
const EXPORT: [(&str, Kind); 2] = [("format", Text), ("frame", Integer)];

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Text => "string",
            Integer => "integer",
            Number => "number",
            Boolean => "boolean",
        }
    }

    /// The argument as the command line gives it, if the JSON value has this kind.
    fn text(self, value: &Value) -> Option<String> {
        match (self, value) {
            (Text, Value::String(text)) => Some(text.clone()),
            (Integer, Value::Number(n)) if n.is_i64() || n.is_u64() => Some(n.to_string()),
            (Number, Value::Number(n)) => Some(n.to_string()),
            (Boolean, Value::Bool(set)) => Some(set.to_string()),
            _ => None,
        }
    }

    /// A clap default value as JSON.
    fn default(self, text: &str) -> Value {
        match self {
            Text => json!(text),
            Integer => json!(text.parse::<i64>().expect("integer default")),
            Number => json!(text.parse::<f64>().expect("number default")),
            Boolean => json!(text == "true"),
        }
    }
}

/// The program's commands with their global arguments, ready to read.
fn commands() -> clap::Command {
    let mut root = Args::command();
    root.build();
    root
}

impl Tool {
    fn subcommand<'a>(&self, root: &'a clap::Command) -> &'a clap::Command {
        self.command.iter().fold(root, |command, name| {
            command.find_subcommand(name).expect("tool command exists")
        })
    }

    /// The tool's arguments with their kinds and clap definitions: the command's, in its order,
    /// then the export options.
    fn arguments<'a>(&self, root: &'a clap::Command) -> Vec<(&'a Arg, Kind)> {
        let export: &[(&str, Kind)] = if self.export { &EXPORT } else { &[] };
        let pick = |command: &'a clap::Command, known: &'static [(&'static str, Kind)]| {
            command.get_arguments().filter_map(move |arg| {
                let id = arg.get_id().as_str();
                let kind = known.iter().find(|(name, _)| *name == id);
                kind.map(|&(_, kind)| (arg, kind))
            })
        };
        pick(self.subcommand(root), self.args)
            .chain(pick(root, export))
            .collect()
    }

    fn definition(&self, root: &clap::Command) -> Value {
        let command = self.subcommand(root);
        let arguments = self.arguments(root);
        assert_eq!(
            arguments.len(),
            self.args.len() + if self.export { EXPORT.len() } else { 0 },
            "every argument of {} exists",
            self.name
        );
        let mut properties = Map::new();
        let mut required = Vec::new();
        if let Some(content) = &self.content {
            properties.insert(
                content.name.into(),
                json!({"type": "string", "description": content.description}),
            );
            if content.required {
                required.push(content.name);
            }
        }
        for (arg, kind) in arguments {
            let id = arg.get_id().as_str();
            let mut property = Map::new();
            property.insert("type".into(), json!(kind.name()));
            if let Some(help) = arg.get_help() {
                property.insert("description".into(), json!(help.to_string()));
            }
            let values: Vec<_> = arg
                .get_possible_values()
                .into_iter()
                .filter(|value| !value.is_hide_set())
                .map(|value| value.get_name().to_string())
                .collect();
            if matches!(kind, Text) && !values.is_empty() {
                property.insert("enum".into(), json!(values));
            }
            if let Some(default) = arg.get_default_values().first() {
                let default = default.to_str().expect("defaults are UTF-8");
                property.insert("default".into(), kind.default(default));
            }
            if arg.is_required_set() || self.require.contains(&id) {
                required.push(id);
            }
            properties.insert(id.into(), Value::Object(property));
        }
        let about = command.get_about().map(ToString::to_string);
        json!({
            "name": self.name,
            "description": self.description.map(Into::into).or(about),
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false,
            },
        })
    }

    /// The command line and stdin for a call, or why the arguments do not fit the schema.
    fn invocation(
        &self,
        root: &clap::Command,
        given: &Map<String, Value>,
    ) -> Result<(Vec<String>, Option<Vec<u8>>), String> {
        let arguments = self.arguments(root);
        let content = self.content.as_ref();
        if let Some(name) = given.keys().find(|name| {
            content.is_none_or(|content| content.name != name.as_str())
                && !arguments
                    .iter()
                    .any(|(arg, _)| arg.get_id() == name.as_str())
        }) {
            return Err(format!("it has no argument {name}"));
        }
        let required = arguments
            .iter()
            .map(|(arg, _)| arg.get_id().as_str())
            .filter(|id| self.require.iter().any(|name| name == id))
            .chain(content.filter(|content| content.required).map(|c| c.name));
        if let Some(missing) = required.into_iter().find(|id| !given.contains_key(*id)) {
            return Err(format!("{missing} is required"));
        }
        let mut stdin = None;
        if let Some(content) = content
            && let Some(value) = given.get(content.name)
        {
            let text = value
                .as_str()
                .ok_or_else(|| format!("{} must be a string", content.name))?;
            stdin = Some(if content.base64 {
                base64::engine::general_purpose::STANDARD
                    .decode(text)
                    .map_err(|e| format!("{} is not base64: {e}", content.name))?
            } else {
                text.as_bytes().to_vec()
            });
        }
        let mut line: Vec<String> = self.command.iter().map(ToString::to_string).collect();
        let mut positional = Vec::new();
        let replaced = self
            .subcommand(root)
            .get_arguments()
            .filter(|arg| content.is_some_and(|content| content.replaces == arg.get_id()));
        // Positional arguments in the command's order, the replaced file among them.
        let mut ordered: Vec<_> = replaced.map(|arg| (arg, None)).collect();
        ordered.extend(arguments.into_iter().map(|(arg, kind)| (arg, Some(kind))));
        ordered.sort_by_key(|(arg, _)| arg.get_index().unwrap_or(usize::MAX));
        for (arg, kind) in ordered {
            let id = arg.get_id().as_str();
            let Some(kind) = kind else {
                if stdin.is_some() {
                    positional.push("-".to_string());
                }
                continue;
            };
            let Some(value) = given.get(id) else {
                continue;
            };
            let text = kind
                .text(value)
                .ok_or_else(|| format!("{id} must be of type {}", kind.name()))?;
            if arg.is_positional() {
                positional.push(text);
                continue;
            }
            let long = arg.get_long().expect("options have long names");
            if !matches!(kind, Boolean) {
                line.push(format!("--{long}={text}"));
            } else if value == &Value::Bool(true) {
                line.push(format!("--{long}"));
            }
        }
        line.push("--".into());
        line.extend(positional);
        Ok((line, stdin))
    }
}

/// Run this program with a tool's command line, giving it `stdin`.
fn run(line: &[String], stdin: Option<Vec<u8>>) -> Value {
    // nosemgrep: rust.lang.security.current-exe.current-exe -- the server reruns its own program for a tool call; no trust decision rests on the path
    let child = std::env::current_exe().and_then(|program| {
        let mut child = Command::new(program)
            .args(line)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // Written from another thread, so a command writing a large result cannot block on a
        // full pipe whilst its input is still being written.
        let writer = child.stdin.take().zip(stdin).map(|(mut pipe, bytes)| {
            std::thread::spawn(move || {
                // A command that refuses its input stops reading; its error says why.
                let _ = pipe.write_all(&bytes);
            })
        });
        let output = child.wait_with_output()?;
        if let Some(writer) = writer {
            writer.join().expect("the stdin writer does not panic");
        }
        Ok(output)
    });
    let (text, failed) = match child {
        Ok(output) if output.status.success() => {
            (String::from_utf8_lossy(&output.stdout).into_owned(), false)
        }
        Ok(output) => {
            let error = String::from_utf8_lossy(&output.stderr);
            let error = error.trim();
            (
                error.strip_prefix("apollinaire: ").unwrap_or(error).into(),
                true,
            )
        }
        Err(e) => (format!("Cannot run apollinaire: {e}"), true),
    };
    json!({"content": [{"type": "text", "text": text}], "isError": failed})
}

fn error(code: i64, message: impl Into<String>) -> Value {
    json!({"code": code, "message": message.into()})
}

fn info() -> Value {
    json!({"name": "apollinaire", "title": "ApollinaireASCII", "version": env!("CARGO_PKG_VERSION")})
}

#[derive(Default)]
struct Server {
    /// A client has sent `initialize`, so requests without `_meta` use its version.
    initialized: bool,
    /// The bridge to the browser editor, with `--live`.
    live: Option<crate::live::Bridge>,
}

impl Server {
    /// The reply to one message, if it needs one.
    fn message(&mut self, bytes: &[u8]) -> Option<Value> {
        let reply = |id: Value, outcome: Result<Value, Value>| {
            Some(match outcome {
                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
            })
        };
        let Ok(message) = serde_json::from_slice::<Value>(bytes) else {
            return reply(Value::Null, Err(error(PARSE_ERROR, "Not JSON")));
        };
        let Some(message) = message.as_object() else {
            return reply(
                Value::Null,
                Err(error(INVALID_REQUEST, "Expected one JSON-RPC object")),
            );
        };
        let id = message.get("id");
        if message.get("jsonrpc") != Some(&json!("2.0")) {
            return reply(
                id.cloned().unwrap_or(Value::Null),
                Err(error(
                    INVALID_REQUEST,
                    r#"A message needs "jsonrpc": "2.0""#,
                )),
            );
        }
        match (message.get("method").and_then(Value::as_str), id) {
            // Notifications need no reply, and the server sends no requests to be answered.
            (Some(_), None) => None,
            (None, _) if message.contains_key("result") || message.contains_key("error") => None,
            (Some(method), Some(id)) if id.is_string() || id.is_number() => {
                let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
                reply(id.clone(), self.request(method, &params))
            }
            _ => reply(
                id.cloned().unwrap_or(Value::Null),
                Err(error(
                    INVALID_REQUEST,
                    "A request needs a method and a string or number id",
                )),
            ),
        }
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, Value> {
        if method == "initialize" {
            let requested = params["protocolVersion"].as_str();
            self.initialized = true;
            return Ok(json!({
                "protocolVersion": requested.filter(|v| LEGACY.contains(v)).unwrap_or(LEGACY[0]),
                "capabilities": {"tools": {}},
                "serverInfo": info(),
                "instructions": INSTRUCTIONS,
            }));
        }
        let meta = &params["_meta"];
        let modern = match meta.get(VERSION) {
            Some(version) => {
                if !version.as_str().is_some_and(|v| MODERN.contains(&v)) {
                    return Err(json!({
                        "code": UNSUPPORTED_VERSION,
                        "message": "Unsupported protocol version",
                        "data": {"supported": MODERN, "requested": version},
                    }));
                }
                if method != "server/discover" && !meta[CAPABILITIES].is_object() {
                    return Err(error(INVALID_PARAMS, format!("_meta needs {CAPABILITIES}")));
                }
                true
            }
            None if method == "server/discover" => true,
            None if self.initialized => false,
            None => {
                return Err(error(
                    INVALID_PARAMS,
                    format!(
                        "_meta needs {VERSION}: {}, or initialize first for {}",
                        MODERN.join(", "),
                        LEGACY.join(", ")
                    ),
                ));
            }
        };
        let mut result = match method {
            "server/discover" => json!({
                "supportedVersions": MODERN,
                "capabilities": {"tools": {}},
                "instructions": INSTRUCTIONS,
                "ttlMs": TTL_MS,
                "cacheScope": "public",
            }),
            "ping" => json!({}),
            "tools/list" => {
                let root = commands();
                let mut tools: Vec<_> = TOOLS.iter().map(|tool| tool.definition(&root)).collect();
                if self.live.is_some() {
                    tools.extend(crate::live::tools());
                }
                let mut result = json!({"tools": tools});
                if modern {
                    result["ttlMs"] = json!(TTL_MS);
                    result["cacheScope"] = json!("public");
                }
                result
            }
            "tools/call" => match self.live.as_mut().map(|bridge| live_call(bridge, params)) {
                Some(Some(result)) => result?,
                _ => call(params)?,
            },
            _ => return Err(error(METHOD_NOT_FOUND, format!("Unknown method {method}"))),
        };
        if modern {
            result["resultType"] = json!("complete");
            result["_meta"] = json!({"io.modelcontextprotocol/serverInfo": info()});
        }
        Ok(result)
    }
}

/// A live tool's result, or `None` for another tool.
fn live_call(bridge: &mut crate::live::Bridge, params: &Value) -> Option<Result<Value, Value>> {
    let name = params["name"]
        .as_str()
        .filter(|name| name.starts_with("live_"))?;
    let empty = Map::new();
    let given = match &params["arguments"] {
        Value::Null => &empty,
        Value::Object(given) => given,
        _ => return Some(Err(error(INVALID_PARAMS, "arguments must be an object"))),
    };
    bridge.call(name, given).map(Ok)
}

fn call(params: &Value) -> Result<Value, Value> {
    let name = params["name"].as_str().unwrap_or_default();
    let tool = TOOLS
        .iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| error(INVALID_PARAMS, format!("Unknown tool: {name}")))?;
    let empty = Map::new();
    let given = match &params["arguments"] {
        Value::Null => &empty,
        Value::Object(given) => given,
        _ => return Err(error(INVALID_PARAMS, "arguments must be an object")),
    };
    let (line, stdin) = tool.invocation(&commands(), given).map_err(|e| {
        error(
            INVALID_PARAMS,
            format!("Invalid arguments for tool {name}: {e}"),
        )
    })?;
    Ok(run(&line, stdin))
}

/// Serve requests from stdin until it closes, one JSON-RPC message per line.
pub fn serve(live: bool) -> crate::Result<()> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut server = Server {
        live: live.then(Default::default),
        ..Server::default()
    };
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = input
            .by_ref()
            .take(MAX_MESSAGE_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| format!("Read failed: {e}"))?;
        if read == 0 {
            return Ok(());
        }
        let ended = line.last() == Some(&b'\n');
        let reply = if line.len() - usize::from(ended) > MAX_MESSAGE_BYTES {
            // At most the limit and one more byte were read, so no line feed yet: skip the rest
            // of the line, so the next message starts clean.
            skip_line(&mut input)?;
            Some(json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": error(INVALID_REQUEST, format!(
                    "A message can be at most {} MiB",
                    MAX_MESSAGE_BYTES / (1024 * 1024)
                )),
            }))
        } else if line.trim_ascii().is_empty() {
            None
        } else {
            server.message(&line)
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")
                .and_then(|()| output.flush())
                .map_err(|e| format!("Output failed: {e}"))?;
        }
    }
}

fn skip_line(input: &mut impl BufRead) -> crate::Result<()> {
    loop {
        let buffer = input.fill_buf().map_err(|e| format!("Read failed: {e}"))?;
        if buffer.is_empty() {
            return Ok(());
        }
        match buffer.iter().position(|&b| b == b'\n') {
            Some(end) => {
                input.consume(end + 1);
                return Ok(());
            }
            None => {
                let all = buffer.len();
                input.consume(all);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Commands that are no tool: interactive ones, ones that print no artwork, and ones that
    /// need a file name (whose extension picks the format) or 2 files, which a tool cannot pass,
    /// plus `claude-hook` (reads workspace files for Claude Code hooks; `diagram` serves Markdown).
    const NOT_TOOLS: &[&str] = &[
        "import",
        "model",
        "compose",
        "play",
        "edit",
        "completions",
        "man",
        "mcp",
        "claude-hook",
    ];
    /// Arguments no tool takes: files and directories, which the server never names.
    const NOT_TAKEN: &[&str] = &[
        "output",
        "output_dir",
        "font_file",
        "art_file",
        "design_file",
    ];
    /// The image tool's picture comes as data, never a video file, so it takes no video options.
    const VIDEO_ONLY: &[&str] = &["fps", "start", "end", "hwaccel"];

    #[test]
    fn every_command_and_argument_is_a_tool_or_left_out_on_purpose() {
        let root = commands();
        // clap adds a help command beside each set of commands.
        for command in root.get_subcommands().filter(|c| c.get_name() != "help") {
            let name = command.get_name();
            let paths: Vec<Vec<&str>> = if command.has_subcommands() {
                command
                    .get_subcommands()
                    .filter(|sub| sub.get_name() != "help")
                    .map(|sub| vec![name, sub.get_name()])
                    .collect()
            } else {
                vec![vec![name]]
            };
            for path in paths {
                let Some(tool) = TOOLS.iter().find(|tool| tool.command == path) else {
                    assert!(NOT_TOOLS.contains(&name), "{path:?} is no tool");
                    continue;
                };
                for arg in tool.subcommand(&root).get_arguments() {
                    let id = arg.get_id().as_str();
                    let taken = tool
                        .args
                        .iter()
                        .chain(&EXPORT)
                        .any(|(known, _)| *known == id)
                        || tool.content.as_ref().is_some_and(|c| c.replaces == id)
                        || NOT_TAKEN.contains(&id)
                        || (tool.name == "image" && VIDEO_ONLY.contains(&id))
                        || matches!(id, "help" | "version");
                    assert!(taken, "the {} tool does not take {id}", tool.name);
                }
            }
        }
    }
}
