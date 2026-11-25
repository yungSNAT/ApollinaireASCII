// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Zero-Node Claude Code hook and CLI helper for inline Mermaid ASCII rendering.
//!
//! Reads workspace-contained Markdown or Claude Code `PostToolUse` JSON on stdin and renders
//! fenced `mermaid` blocks as pure printable ASCII via [`apollinaire_ascii::diagram`].

mod safe_io;

use apollinaire_ascii::{MAX_INPUT_BYTES, MAX_WIDTH, Result, diagram};
use serde_json::{Map, Value, json};
use std::{
    borrow::Cow,
    io::{self, Read},
    path::PathBuf,
};

use safe_io::{has_nul, is_markdown_path, read_contained_utf8};

const DEFAULT_MAX_WIDTH: usize = 80;
const MIN_WIDTH: usize = 1;
const MAX_CONTEXT_BYTES: usize = 1024 * 1024;
const EDIT_TOOLS: &[&str] = &["Write", "Edit", "MultiEdit"];

#[derive(clap::Args)]
pub struct ClaudeHookArgs {
    /// Read raw Markdown from stdin (or --file) and print rendered Markdown to stdout.
    #[arg(long)]
    pub markdown: bool,
    /// Markdown file inside the current working directory to render when --markdown is set.
    #[arg(long)]
    pub file: Option<String>,
    /// Maximum diagram width in columns (1..=512; default 80, or APOLLINAIRE_MAX_WIDTH).
    #[arg(long, allow_hyphen_values = true)]
    pub max_width: Option<String>,
}

fn parse_max_width(raw: Option<&str>) -> Result<usize> {
    let Some(trimmed) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(DEFAULT_MAX_WIDTH);
    };
    if trimmed.len() > 4 || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!(
            "max-width must be an ASCII integer between {MIN_WIDTH} and {MAX_WIDTH}: {raw:?}"
        ));
    }
    let value: usize = trimmed
        .parse()
        .map_err(|_| format!("max-width must be between {MIN_WIDTH} and {MAX_WIDTH}: {raw:?}"))?;
    if !(MIN_WIDTH..=MAX_WIDTH).contains(&value) {
        return Err(format!(
            "max-width must be between {MIN_WIDTH} and {MAX_WIDTH}, got {value}"
        ));
    }
    Ok(value)
}

fn resolve_max_width(cli_width: Option<&str>) -> Result<usize> {
    if cli_width.is_some() {
        return parse_max_width(cli_width);
    }
    let env_width = match std::env::var_os("APOLLINAIRE_MAX_WIDTH") {
        None => None,
        Some(val) => Some(Cow::Owned(
            val.into_string()
                .map_err(|_| "APOLLINAIRE_MAX_WIDTH is not valid UTF-8")?,
        )),
    };
    parse_max_width(env_width.as_deref())
}

pub(crate) fn read_bounded_utf8(
    reader: impl Read,
    max_bytes: usize,
    too_large: &str,
    bad_utf8: &str,
) -> Result<String> {
    let mut bytes = Vec::new();
    reader
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Read failed: {e}"))?;
    if bytes.len() > max_bytes {
        return Err(too_large.to_owned());
    }
    String::from_utf8(bytes).map_err(|e| format!("{bad_utf8}: {e}"))
}

fn read_bounded_stdin() -> Result<String> {
    read_bounded_utf8(
        io::stdin().lock(),
        MAX_INPUT_BYTES,
        "Input exceeds the 16 MiB limit.",
        "Input is not valid UTF-8",
    )
}

fn extract_file_path(payload: &Map<String, Value>) -> Result<Option<&str>> {
    let tool_name = match payload.get("tool_name") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) => {
            if !EDIT_TOOLS.contains(&name.as_str()) {
                return Ok(None);
            }
            Some(name.as_str())
        }
        Some(_) => return Err("Hook payload tool_name must be a string.".into()),
    };
    let tool_input = match payload.get("tool_input") {
        None | Some(Value::Null) => match tool_name {
            Some(name) => return Err(format!("Hook payload for {name} is missing tool_input.")),
            None => return Ok(None),
        },
        Some(Value::Object(obj)) => obj,
        Some(_) => return Err("Hook payload tool_input must be a JSON object.".into()),
    };
    match tool_input.get("file_path") {
        None | Some(Value::Null) => match tool_name {
            Some(name) => Err(format!(
                "Hook payload for {name} is missing tool_input.file_path."
            )),
            None => Ok(None),
        },
        Some(Value::String(path)) if !path.trim().is_empty() => Ok(Some(path.as_str())),
        Some(_) => Err("Hook payload tool_input.file_path must be a non-empty string.".into()),
    }
}

fn escape_ascii_label(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        let code = ch as u32;
        if (32..=126).contains(&code) {
            out.push(ch);
        } else if code < 0x100 {
            out.push_str(&format!("\\x{code:02x}"));
        } else {
            out.push_str(&format!("\\u{code:04x}"));
        }
    }
    out
}

fn check_printable_ascii(text: &str) -> Result<()> {
    if let Some((index, byte)) = text
        .bytes()
        .enumerate()
        .find(|(_, b)| *b != b'\n' && !(32..=126).contains(b))
    {
        return Err(format!(
            "Rendered context contains non-ASCII byte at offset {index}: 0x{byte:02x}"
        ));
    }
    Ok(())
}

fn check_block_count(source_len: usize, rendered_len: usize) -> Result<()> {
    if rendered_len != source_len {
        return Err("Rendered Markdown block count did not match source Markdown.".into());
    }
    Ok(())
}

fn render_err(error: String) -> String {
    format!("apollinaire diagram failed: {error}")
}

fn build_context(
    leaf_name: &str,
    max_width: usize,
    ascii_blocks: &[String],
    max_context_bytes: usize,
) -> Result<String> {
    let count = ascii_blocks.len();
    let noun = if count == 1 { "diagram" } else { "diagrams" };
    let safe_name = escape_ascii_label(leaf_name);
    let context = format!(
        "ApollinaireASCII rendered {count} fenced Mermaid {noun} in {safe_name} \
         (--max-width {max_width}, pure printable ASCII):\n\n{}",
        ascii_blocks.join("\n\n")
    );
    if context.len() > max_context_bytes {
        return Err("Rendered additionalContext exceeds the 1 MiB limit.".into());
    }
    check_printable_ascii(&context)?;
    Ok(context)
}

fn render_post_tool_reply(
    leaf_name: &str,
    markdown: &str,
    max_width: usize,
) -> Result<Option<String>> {
    // Diverges from Python `splitlines()`: splits on `\n` only, matching `diagram::render_markdown`.
    let source_blocks = diagram::fenced_blocks(markdown);
    let mermaid_indices: Vec<usize> = source_blocks
        .iter()
        .enumerate()
        .filter_map(|(idx, (is_m, _))| is_m.then_some(idx))
        .collect();
    if mermaid_indices.is_empty() {
        return Ok(None);
    }
    let rendered = diagram::render_markdown(markdown, Some(max_width)).map_err(render_err)?;
    let rendered_blocks = diagram::fenced_blocks(&rendered);
    check_block_count(source_blocks.len(), rendered_blocks.len())?;
    let ascii_blocks: Vec<String> = mermaid_indices
        .into_iter()
        .map(|idx| {
            rendered_blocks[idx]
                .1
                .replace("\r\n", "\n")
                .replace('\r', "\n")
        })
        .collect();
    let context = build_context(leaf_name, max_width, &ascii_blocks, MAX_CONTEXT_BYTES)?;
    let reply = json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": context,
        }
    });
    Ok(Some(reply.to_string()))
}

fn handle_post_tool_use_with(
    payload_text: &str,
    max_width: usize,
    resolve_fallback_cwd: impl FnOnce() -> Result<PathBuf>,
) -> Result<Option<String>> {
    if payload_text.trim().is_empty() {
        return Err("Expected Claude Code hook JSON on stdin, got empty input.".into());
    }
    let value: Value = serde_json::from_str(payload_text)
        .map_err(|e| format!("Invalid hook JSON on stdin: {e}"))?;
    let Some(payload) = value.as_object() else {
        return Err("Hook payload must be a JSON object.".into());
    };
    let Some(file_path) = extract_file_path(payload)? else {
        return Ok(None);
    };
    has_nul(file_path, "file_path")?;
    if !is_markdown_path(file_path) {
        return Ok(None);
    }
    let base_dir = match payload.get("cwd") {
        None | Some(Value::Null) => resolve_fallback_cwd()?,
        Some(Value::String(cwd)) if !cwd.trim().is_empty() => PathBuf::from(cwd),
        Some(_) => return Err("Hook payload cwd must be a non-empty string.".into()),
    };
    let (leaf_name, markdown) = read_contained_utf8(file_path, &base_dir, MAX_INPUT_BYTES)?;
    render_post_tool_reply(&leaf_name, &markdown, max_width)
}

fn current_dir_result() -> Result<PathBuf> {
    std::env::current_dir().map_err(|e| e.to_string())
}

pub fn run(args: &ClaudeHookArgs) -> Result<()> {
    let max_width = resolve_max_width(args.max_width.as_deref())?;
    if args.markdown {
        let markdown = match args.file.as_deref().filter(|s| !s.is_empty()) {
            Some(path) => read_contained_utf8(path, &current_dir_result()?, MAX_INPUT_BYTES)?.1,
            None => read_bounded_stdin()?,
        };
        let rendered = diagram::render_markdown(&markdown, Some(max_width)).map_err(render_err)?;
        return crate::emit(rendered.as_bytes());
    }
    let payload_text = read_bounded_stdin()?;
    if let Some(reply) = handle_post_tool_use_with(&payload_text, max_width, current_dir_result)? {
        crate::emit(format!("{reply}\n").as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::Path;

    fn handle_post_tool_use(
        payload_text: &str,
        max_width: usize,
        fallback_cwd: &Path,
    ) -> Result<Option<String>> {
        handle_post_tool_use_with(payload_text, max_width, || Ok(fallback_cwd.to_path_buf()))
    }

    struct FailingReader;
    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("broken stream"))
        }
    }

    fn temp_workspace(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp")
            .join(format!("claude-hook-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn width_escaping_and_guards_enforce_bounds() {
        assert_eq!(parse_max_width(None).unwrap(), 80);
        assert_eq!(parse_max_width(Some("   ")).unwrap(), 80);
        assert_eq!(parse_max_width(Some(" 36 ")).unwrap(), 36);
        assert!(parse_max_width(Some("0")).is_err());
        assert!(parse_max_width(Some("513")).is_err());
        assert!(parse_max_width(Some("-5")).is_err());
        assert!(parse_max_width(Some("99999")).is_err());

        assert_eq!(escape_ascii_label("café.md"), r"caf\xe9.md");
        assert_eq!(
            escape_ascii_label("a\x1bb—👻.md"),
            r"a\x1bb\u2014\u1f47b.md"
        );
        assert!(check_printable_ascii("ok\nline").is_ok());
        assert!(check_printable_ascii("bad\x1b").is_err());
        assert!(check_block_count(2, 2).is_ok());
        assert!(check_block_count(2, 1).is_err());
        assert!(build_context("a.md", 80, &["x".repeat(128)], 64).is_err());
    }

    #[test]
    fn bounded_reader_rejects_io_errors_overflow_and_invalid_utf8() {
        assert!(
            read_bounded_utf8(FailingReader, 64, "big", "utf8")
                .unwrap_err()
                .contains("Read failed")
        );
        assert_eq!(
            read_bounded_utf8(Cursor::new(b"x".repeat(20)), 10, "too large", "utf8").unwrap_err(),
            "too large"
        );
        assert!(
            read_bounded_utf8(Cursor::new(vec![0xff]), 10, "big", "bad utf8")
                .unwrap_err()
                .contains("bad utf8")
        );
    }

    #[test]
    fn workspace_containment_handles_symlinks_prefixes_and_permissions() {
        let root = temp_workspace("contain");
        let real_ws = root.join("real");
        std::fs::create_dir_all(&real_ws).unwrap();
        let doc = real_ws.join("doc.md");
        std::fs::write(&doc, "# Ok\n").unwrap();
        std::fs::write(real_ws.join("bad.md"), [0xff]).unwrap();
        std::fs::write(real_ws.join("big.md"), vec![b'a'; 2048]).unwrap();
        std::fs::create_dir_all(real_ws.join("dir.md")).unwrap();

        assert!(read_contained_utf8("a\0.md", &real_ws, 1024).is_err());
        assert!(read_contained_utf8("doc.md", Path::new("bad\0dir"), 1024).is_err());
        assert!(read_contained_utf8("doc.md", &root.join("missing"), 1024).is_err());
        assert!(read_contained_utf8(".", &real_ws, 1024).is_err());
        assert!(read_contained_utf8("doc.md/child.md", &real_ws, 1024).is_err());
        assert!(read_contained_utf8("dir.md", &real_ws, 1024).is_err());
        assert!(read_contained_utf8("bad.md", &real_ws, 1024).is_err());
        assert!(
            read_contained_utf8("big.md", &real_ws, 1024)
                .unwrap_err()
                .contains("exceeds")
        );
        assert_eq!(
            read_contained_utf8("./doc.md", &real_ws, 1024).unwrap().0,
            "doc.md"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn post_tool_use_handles_edge_payloads() {
        let root = temp_workspace("payloads");
        assert!(handle_post_tool_use("   ", 80, &root).is_err());
        assert!(handle_post_tool_use("[]", 80, &root).is_err());
        assert!(
            handle_post_tool_use(r#"{"tool_input":{}}"#, 80, &root)
                .unwrap()
                .is_none()
        );
        let nul_payload = r#"{"tool_name":"Write","tool_input":{"file_path":"a\u0000.md"}}"#;
        assert!(handle_post_tool_use(nul_payload, 80, &root).is_err());
        let bad_cwd = r#"{"tool_name":"Write","cwd":"  ","tool_input":{"file_path":"a.md"}}"#;
        assert!(handle_post_tool_use(bad_cwd, 80, &root).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
