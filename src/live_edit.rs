// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! The editor's side of `apollinaire mcp --live`: its answers to the bridge's `read` and `write`
//! requests about the frame on screen. Kept apart from the browser code so it is tested natively.
use crate::{Canvas, Frame, Result, sha256};
use serde_json::{Value, json};

/// Sign-in handshake state for the browser editor: sends the JSON capability envelope first,
/// and falls back once to the bare token if a legacy bridge closes the socket before greeting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignIn {
    code: String,
    greeted: bool,
    fell_back: bool,
}

impl SignIn {
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            greeted: false,
            fell_back: false,
        }
    }

    /// The initial JSON sign-in message advertising `expected_sha256` support.
    pub fn initial_message(&self) -> String {
        json!({
            "token": self.code,
            "capabilities": ["expected_sha256"],
        })
        .to_string()
    }

    /// Record that the bridge greeted this connection; disables any legacy fallback.
    pub fn mark_greeted(&mut self) {
        self.greeted = true;
    }

    /// On socket close: returns the bare token once if the bridge closed before greeting
    /// and this editor was not replaced; otherwise `None`.
    pub fn fallback_message(&mut self, replaced: bool) -> Option<&str> {
        if self.greeted || self.fell_back || replaced {
            return None;
        }
        self.fell_back = true;
        Some(&self.code)
    }
}

/// The frame on screen: the active layer's selected frame.
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub layer: usize,
    pub frame: usize,
    pub frames: usize,
    pub art: &'a Frame,
}

/// Names the frame's position and content, so a write based on an older read is refused.
pub fn revision(view: &View) -> String {
    let (width, height) = (view.art.width(), view.art.height());
    let place = format!("{}:{}:{width}x{height}:", view.layer, view.frame);
    // 64-bit FNV-1a.
    let hash = place
        .bytes()
        .chain(view.art.cells().iter().copied())
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    format!("{hash:016x}")
}

/// Lowercase hexadecimal SHA-256 digest of the frame's text (rows separated by line feeds).
pub fn content_sha256(art: &Frame) -> String {
    sha256::hex(art.text().as_bytes())
}

/// The reply to one request, and the frame a write puts in place of the one on screen.
pub fn answer(view: &View, request: &Value) -> (Value, Option<Frame>) {
    let id = request["id"].clone();
    let outcome = match request["op"].as_str() {
        Some("read") => Ok((read(view), None)),
        Some("write") => write(view, request).map(|art| {
            let revision = revision(&View { art: &art, ..*view });
            let sha256 = content_sha256(&art);
            (json!({"revision": revision, "sha256": sha256}), Some(art))
        }),
        _ => Err("Unknown request".into()),
    };
    match outcome {
        Ok((result, art)) => (json!({"id": id, "result": result}), art),
        Err(error) => (json!({"id": id, "error": error}), None),
    }
}

fn read(view: &View) -> Value {
    json!({
        "revision": revision(view),
        "sha256": content_sha256(view.art),
        "layer": view.layer,
        "frame": view.frame,
        "frames": view.frames,
        "width": view.art.width(),
        "height": view.art.height(),
        "text": view.art.text(),
    })
}

/// Fixed-length comparison of an ASCII hex digest (case-insensitive on `given`) with no source-level early exit on mismatch.
fn digest_matches(given: &str, actual: &str) -> bool {
    let diff = given
        .bytes()
        .zip(actual.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a.to_ascii_lowercase() ^ b));
    given.len() == actual.len() && std::hint::black_box(diff) == 0
}

fn check_expected_sha256(art: &Frame, request: &Value) -> Result<()> {
    let Some(raw) = request.get("expected_sha256") else {
        return Ok(());
    };
    let given = raw
        .as_str()
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("expected_sha256 must be a 64-character hex SHA-256 digest")?;
    if !digest_matches(given, &content_sha256(art)) {
        return Err("The frame content digest changed since it was read: read it again".into());
    }
    Ok(())
}

fn write(view: &View, request: &Value) -> Result<Frame> {
    if request["revision"].as_str() != Some(revision(view).as_str()) {
        return Err("The frame changed since it was read: read it again".into());
    }
    check_expected_sha256(view.art, request)?;
    let text = request["text"].as_str().ok_or("text must be a string")?;
    let block = Frame::from_text_padded(text)?;
    let place = |name: &str| match &request[name] {
        Value::Null => Ok(None),
        value => value
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .map(Some)
            .ok_or(format!("{name} must be a whole number")),
    };
    match (place("x")?, place("y")?) {
        (None, None) => Ok(block),
        (Some(x), Some(y)) => {
            let (width, height) = (view.art.width(), view.art.height());
            if x.saturating_add(block.width()) > width || y.saturating_add(block.height()) > height
            {
                return Err(format!(
                    "A {}x{} block at {x},{y} does not fit the {width}x{height} frame",
                    block.width(),
                    block.height()
                ));
            }
            let mut canvas = Canvas::from_frame(view.art);
            // Both fit in i32, as the frame's own size does.
            canvas.overlay(&block, x as i32, y as i32, false)?;
            Ok(canvas.finish())
        }
        _ => Err("Give both x and y to replace a block, or neither to replace the frame".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(art: &Frame) -> View<'_> {
        View {
            layer: 0,
            frame: 1,
            frames: 3,
            art,
        }
    }

    fn write(art: &Frame, fields: Value) -> (Value, Option<Frame>) {
        let mut request = json!({"id": 7, "op": "write", "revision": revision(&view(art))});
        for (key, value) in fields.as_object().unwrap() {
            request[key] = value.clone();
        }
        answer(&view(art), &request)
    }

    #[test]
    fn a_read_gives_the_frame_and_its_revision() {
        let art = Frame::from_text("ab\ncd").unwrap();
        let (reply, changed) = answer(&view(&art), &json!({"id": 3, "op": "read"}));
        assert_eq!(changed, None);
        assert_eq!(
            reply,
            json!({"id": 3, "result": {
                "revision": revision(&view(&art)),
                "sha256": content_sha256(&art),
                "layer": 0, "frame": 1, "frames": 3, "width": 2, "height": 2, "text": "ab\ncd",
            }})
        );
    }

    #[test]
    fn the_revision_follows_content_and_position() {
        let art = Frame::from_text("ab\ncd").unwrap();
        let other = Frame::from_text("ab\nce").unwrap();
        let base = revision(&view(&art));
        assert_eq!(base.len(), 16);
        assert_eq!(base, revision(&view(&art.clone())));
        assert_ne!(base, revision(&view(&other)));
        assert_ne!(
            base,
            revision(&View {
                frame: 2,
                ..view(&art)
            })
        );
        assert_ne!(
            base,
            revision(&View {
                layer: 1,
                ..view(&art)
            })
        );
    }

    #[test]
    fn a_write_at_the_read_revision_replaces_a_block() {
        let art = Frame::from_text("....\n....\n....").unwrap();
        let (reply, changed) = write(&art, json!({"text": "xy\nz", "x": 1, "y": 1}));
        let changed = changed.unwrap();
        assert_eq!(changed.text(), "....\n.xy.\n.z .");
        assert_eq!(
            reply,
            json!({"id": 7, "result": {
                "revision": revision(&view(&changed)),
                "sha256": content_sha256(&changed),
            }})
        );
    }

    #[test]
    fn a_write_without_a_place_replaces_the_frame() {
        let art = Frame::from_text("....").unwrap();
        let (_, changed) = write(&art, json!({"text": "ab\nc"}));
        assert_eq!(changed.unwrap().text(), "ab\nc ");
    }

    #[test]
    fn a_write_at_an_older_revision_is_refused() {
        let art = Frame::from_text("ab").unwrap();
        let request = json!({"id": 1, "op": "write", "revision": "0", "text": "xy"});
        let (reply, changed) = answer(&view(&art), &request);
        assert_eq!(changed, None);
        assert_eq!(
            reply,
            json!({"id": 1, "error": "The frame changed since it was read: read it again"})
        );
    }

    #[test]
    fn expected_sha256_accepts_a_matching_digest_and_fails_closed_otherwise() {
        let art = Frame::from_text("ab\ncd").unwrap();
        let digest = content_sha256(&art);
        assert_eq!(digest.len(), 64);

        let (reply, changed) = write(&art, json!({"text": "xy", "expected_sha256": digest}));
        let changed = changed.unwrap();
        assert_eq!(changed.text(), "xy");
        assert_eq!(reply["result"]["sha256"], content_sha256(&changed));

        // Uppercase hex is accepted.
        let (_, upper) = write(
            &art,
            json!({"text": "zz", "expected_sha256": digest.to_ascii_uppercase()}),
        );
        assert_eq!(upper.unwrap().text(), "zz");

        // Mismatched digest fails closed even when revision matches.
        let wrong = content_sha256(&Frame::from_text("ab\nce").unwrap());
        let (reply, changed) = write(&art, json!({"text": "xy", "expected_sha256": wrong}));
        assert_eq!(changed, None);
        assert_eq!(
            reply["error"],
            "The frame content digest changed since it was read: read it again"
        );

        // Null, non-string, wrong length or non-hex values fail closed.
        for bad in [
            Value::Null,
            json!(42),
            json!(""),
            json!("abcd"),
            json!("g".repeat(64)),
        ] {
            let (reply, changed) = write(&art, json!({"text": "xy", "expected_sha256": bad}));
            assert_eq!(changed, None);
            assert_eq!(
                reply["error"],
                "expected_sha256 must be a 64-character hex SHA-256 digest"
            );
        }
    }

    #[test]
    fn bad_writes_are_refused_with_the_reason() {
        let art = Frame::from_text("....\n....").unwrap();
        for (fields, error) in [
            (
                json!({"text": "abc", "x": 2, "y": 0}),
                "A 3x1 block at 2,0 does not fit the 4x2 frame",
            ),
            (
                json!({"text": "a", "x": 0}),
                "Give both x and y to replace a block, or neither to replace the frame",
            ),
            (
                json!({"text": "a", "x": -1, "y": 0}),
                "x must be a whole number",
            ),
            (json!({"x": 0, "y": 0}), "text must be a string"),
        ] {
            let (reply, changed) = write(&art, fields);
            assert_eq!(changed, None);
            assert_eq!(reply["error"], error);
        }
        let (reply, changed) = write(&art, json!({"text": "a\u{7}"}));
        assert_eq!(changed, None);
        assert!(reply["error"].is_string(), "{reply}");
        let (reply, _) = answer(&view(&art), &json!({"id": 2, "op": "delete"}));
        assert_eq!(reply, json!({"id": 2, "error": "Unknown request"}));
    }

    #[test]
    fn sign_in_falls_back_once_to_bare_token_before_greeting() {
        let token = "a".repeat(64);
        let mut sign_in = SignIn::new(token.clone());
        let initial: Value = serde_json::from_str(&sign_in.initial_message()).unwrap();
        assert_eq!(initial["token"], token);
        assert_eq!(initial["capabilities"], json!(["expected_sha256"]));
        // Replaced connections never fall back.
        assert_eq!(sign_in.fallback_message(true), None);
        // Pre-greeting close falls back once, never a second time.
        assert_eq!(sign_in.fallback_message(false), Some(token.as_str()));
        assert_eq!(sign_in.fallback_message(false), None);
        // Once greeted, closing never triggers fallback.
        let mut greeted = SignIn::new(token);
        greeted.mark_greeted();
        assert_eq!(greeted.fallback_message(false), None);
    }
}
