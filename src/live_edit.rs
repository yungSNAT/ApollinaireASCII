// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! The editor's side of `apollinaire mcp --live`: its answers to the bridge's `read`, `write`
//! and `batch` requests across any layer and timeline frame, and to its element requests
//! (`elements`, `element_add`, `element_edit`, `element_delete`). Kept apart from the browser
//! code so it is tested natively.
use crate::elements::{self, Arrow, Edit, Element, Kind, Shape, Style};
use crate::{Frame, Layered, MAX_INPUT_BYTES, Result, sha256};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Maximum cumulative cell work admitted for one live bridge request (project scans, target
/// hashes/copies and padded allocations, including dry runs).
pub const MAX_LIVE_CELL_WORK: usize = 64_000_000;
const MAX_BATCH_WRITES: usize = 256;
const READ_KEYS: &[&str] = &["op", "id", "layer", "frame", "composite"];
const WRITE_KEYS: &[&str] = &[
    "layer",
    "frame",
    "revision",
    "expected_sha256",
    "text",
    "x",
    "y",
];
const SINGLE_WRITE_KEYS: &[&str] = &[
    "op",
    "id",
    "layer",
    "frame",
    "revision",
    "expected_sha256",
    "text",
    "x",
    "y",
    "dry_run",
];
const BATCH_KEYS: &[&str] = &["op", "id", "writes", "dry_run"];

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

    /// The initial JSON sign-in message advertising `expected_sha256` and element support.
    pub fn initial_message(&self) -> String {
        json!({
            "token": self.code,
            "capabilities": ["expected_sha256", "elements"],
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

/// The editor project state visible to one live request.
#[derive(Clone, Copy)]
pub struct State<'a> {
    pub project: &'a Layered,
    pub active: usize,
    pub shown: usize,
    pub generation: u64,
}

/// Names the target's session generation, position, layer identity, project counts and cells so a
/// write based on an older read or across a delete/insert or reorder is refused.
pub fn revision(state: &State, layer: usize, frame: usize) -> String {
    let layers = state.project.layers();
    let entry = &layers[layer];
    let frames = entry.art.frames();
    frame_revision(
        state.generation,
        layer,
        &entry.name,
        layers.len(),
        frame,
        frames.len(),
        &frames[frame].frame,
    )
}

fn frame_revision(
    generation: u64,
    layer: usize,
    layer_name: &str,
    layers: usize,
    frame: usize,
    frames: usize,
    art: &Frame,
) -> String {
    let (width, height) = (art.width(), art.height());
    let place = format!(
        "{generation}:{layer}:{}:{layer_name}:{layers}:{frame}:{frames}:{width}x{height}:",
        layer_name.len()
    );
    // 64-bit FNV-1a.
    let hash = place
        .bytes()
        .chain(art.cells().iter().copied())
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    format!("{hash:016x}")
}

/// Lowercase hexadecimal SHA-256 digest of the frame's text (rows separated by line feeds).
pub fn content_sha256(art: &Frame) -> String {
    sha256::hex(art.text().as_bytes())
}

/// The reply to one request, and the updated project when a committing write or batch succeeds.
pub fn answer(state: &State, request: &Value) -> (Value, Option<Layered>) {
    let id = request["id"].clone();
    let outcome = match request["op"].as_str() {
        Some("read") => read(state, request).map(|result| (result, None)),
        Some("write") => write(state, request),
        Some("batch") => batch(state, request),
        Some("elements") => list_elements(state, request).map(|result| (result, None)),
        Some("element_add") => add_element(state, request),
        Some("element_edit") => edit_element(state, request),
        Some("element_delete") => delete_element(state, request),
        _ => Err("Unknown request".into()),
    };
    match outcome {
        Ok((result, project)) => (json!({"id": id, "result": result}), project),
        Err(error) => (json!({"id": id, "error": error}), None),
    }
}

fn check_work_budget(work: usize) -> Result<()> {
    if work > MAX_LIVE_CELL_WORK {
        return Err(format!(
            "Live edit exceeds the {MAX_LIVE_CELL_WORK}-cell work budget"
        ));
    }
    Ok(())
}

fn reject_unknown_keys(object: &serde_json::Map<String, Value>, allowed: &[&str]) -> Result<()> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("Unknown argument {key}"));
    }
    Ok(())
}

fn parse_bool(request: &Value, key: &str) -> Result<bool> {
    match request.get(key) {
        None => Ok(false),
        Some(Value::Bool(flag)) => Ok(*flag),
        Some(_) => Err(format!("{key} must be a boolean")),
    }
}

fn parse_opt_usize(request: &Value, key: &str) -> Result<Option<usize>> {
    match request.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .map(Some)
            .ok_or_else(|| format!("{key} must be a whole number")),
    }
}

fn resolve_index(index: Option<usize>, default: usize, count: usize, kind: &str) -> Result<usize> {
    let resolved = index.unwrap_or(default);
    if resolved >= count {
        return Err(if count == 1 {
            format!(
                "{kind} {resolved} is not in the project, which has 1 {} (0)",
                kind.to_ascii_lowercase()
            )
        } else {
            format!(
                "{kind} {resolved} is not in the project, which has {count} {}s (0 to {})",
                kind.to_ascii_lowercase(),
                count - 1
            )
        });
    }
    Ok(resolved)
}

fn layers_meta(project: &Layered) -> Value {
    Value::Array(
        project
            .layers()
            .iter()
            .map(|layer| {
                json!({
                    "name": layer.name,
                    "visible": layer.visible,
                    "locked": layer.locked,
                })
            })
            .collect(),
    )
}

fn read(state: &State, request: &Value) -> Result<Value> {
    let obj = request.as_object().expect("op matched on object");
    reject_unknown_keys(obj, READ_KEYS)?;
    let composite = parse_bool(request, "composite")?;
    let layer_arg = parse_opt_usize(request, "layer")?;
    let frame_arg = parse_opt_usize(request, "frame")?;
    let layers = state.project.layers();
    let frames = layers[0].art.frames().len();
    let frame = resolve_index(frame_arg, state.shown, frames, "Frame")?;

    if composite {
        if layer_arg.is_some() {
            return Err("composite reads cannot name a layer".into());
        }
        let frame_cells = layers[0].art.width() * layers[0].art.height();
        let work = layers
            .iter()
            .filter(|layer| layer.visible)
            .map(|layer| frame_cells * if layer.motion.is_empty() { 1 } else { 5 })
            .sum::<usize>()
            + frame_cells;
        check_work_budget(work)?;
        let art = state.project.frame(frame);
        return Ok(json!({
            "composite": true,
            "frame": frame,
            "frames": frames,
            "width": art.width(),
            "height": art.height(),
            "text": art.text(),
            "active": state.active,
            "shown": state.shown,
            "layers": layers_meta(state.project),
        }));
    }

    let layer = resolve_index(layer_arg, state.active, layers.len(), "Layer")?;
    let art = &layers[layer].art.frames()[frame].frame;
    check_work_budget(3 * art.cells().len())?;
    Ok(json!({
        "revision": revision(state, layer, frame),
        "sha256": content_sha256(art),
        "layer": layer,
        "frame": frame,
        "frames": frames,
        "width": art.width(),
        "height": art.height(),
        "text": art.text(),
        "active": state.active,
        "shown": state.shown,
        "layers": layers_meta(state.project),
    }))
}

struct ParsedWrite<'a> {
    layer: usize,
    frame: usize,
    text: &'a str,
    place: Option<(usize, usize)>,
    padded_cells: usize,
    has_sha256: bool,
}

struct WriteOutcome {
    layer: usize,
    frame: usize,
    revision: String,
    sha256: String,
}

struct TargetPreview {
    revision: String,
    sha256: String,
    frame: Frame,
    changed: usize,
}

struct BatchApply {
    outcomes: Vec<WriteOutcome>,
    previews: BTreeMap<(usize, usize), TargetPreview>,
    updated: Option<Layered>,
}

/// Fixed-length comparison of an ASCII hex digest (case-insensitive on `given`) with no source-level early exit on mismatch.
fn digest_matches(given: &str, actual: &str) -> bool {
    let diff = given
        .bytes()
        .zip(actual.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a.to_ascii_lowercase() ^ b));
    given.len() == actual.len() && std::hint::black_box(diff) == 0
}

fn check_expected_sha256(
    raw: Option<&Value>,
    cached_sha: &mut Option<String>,
    art: &Frame,
) -> Result<bool> {
    let Some(raw) = raw else {
        return Ok(false);
    };
    let given = raw
        .as_str()
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("expected_sha256 must be a 64-character hex SHA-256 digest")?;
    let actual = cached_sha.get_or_insert_with(|| content_sha256(art));
    if !digest_matches(given, actual) {
        return Err("The frame content digest changed since it was read: read it again".into());
    }
    Ok(true)
}

fn prefix_error(index: usize, single: bool, error: String) -> String {
    if single {
        error
    } else {
        format!("Write {}: {error}", index + 1)
    }
}

fn parse_and_precheck<'a>(
    state: &State,
    item: &'a Value,
    allowed_keys: &[&str],
    pre_checks: &mut BTreeMap<(usize, usize), (String, Option<String>)>,
) -> Result<ParsedWrite<'a>> {
    let obj = item.as_object().ok_or("must be an object")?;
    reject_unknown_keys(obj, allowed_keys)?;
    let layer_arg = parse_opt_usize(item, "layer")?;
    let frame_arg = parse_opt_usize(item, "frame")?;
    let place = match (parse_opt_usize(item, "x")?, parse_opt_usize(item, "y")?) {
        (None, None) => None,
        (Some(x), Some(y)) => Some((x, y)),
        _ => {
            return Err(
                "Give both x and y to replace a block, or neither to replace the frame".into(),
            );
        }
    };
    let given_rev = item
        .get("revision")
        .and_then(Value::as_str)
        .ok_or("revision must be a string")?;
    let expected_sha = item.get("expected_sha256");
    let text = item
        .get("text")
        .and_then(Value::as_str)
        .ok_or("text must be a string")?;

    let layers = state.project.layers();
    let frames = layers[0].art.frames().len();
    let layer = resolve_index(layer_arg, state.active, layers.len(), "Layer")?;
    let frame = resolve_index(frame_arg, state.shown, frames, "Frame")?;
    if layers[layer].locked {
        return Err(format!("Layer \"{}\" is locked", layers[layer].name));
    }

    let art = &layers[layer].art.frames()[frame].frame;
    let (expected_rev, cached_sha) = pre_checks
        .entry((layer, frame))
        .or_insert_with(|| (revision(state, layer, frame), None));
    if given_rev != expected_rev {
        return Err("The frame changed since it was read: read it again".into());
    }
    let has_sha256 = check_expected_sha256(expected_sha, cached_sha, art)?;
    let (padded_w, padded_h) = Frame::padded_dimensions(text)?;
    Ok(ParsedWrite {
        layer,
        frame,
        text,
        place,
        padded_cells: padded_w * padded_h,
        has_sha256,
    })
}

fn preflight_work(state: &State, parsed: &[ParsedWrite<'_>], dry_run: bool) -> Result<()> {
    let layers = state.project.layers();
    let project_cells = layers.len()
        * layers[0].art.frames().len()
        * layers[0].art.width()
        * layers[0].art.height();
    let mut work = if dry_run {
        project_cells
    } else {
        2usize.saturating_mul(project_cells)
    };
    let mut targets: BTreeMap<(usize, usize), (usize, bool, usize)> = BTreeMap::new();
    for entry in parsed {
        work = work.saturating_add(2usize.saturating_mul(entry.padded_cells));
        let orig_cells = layers[entry.layer].art.frames()[entry.frame]
            .frame
            .cells()
            .len();
        let target = targets
            .entry((entry.layer, entry.frame))
            .or_insert((orig_cells, false, orig_cells));
        target.1 |= entry.has_sha256;
        if entry.place.is_none() {
            target.2 = entry.padded_cells;
        }
    }
    for (orig_cells, checked_sha, final_cells) in targets.into_values() {
        let pre_mult = 1 + usize::from(checked_sha) + usize::from(dry_run);
        let post_mult = 2 + usize::from(dry_run);
        work = work
            .saturating_add(pre_mult.saturating_mul(orig_cells))
            .saturating_add(post_mult.saturating_mul(final_cells));
    }
    check_work_budget(work)
}

fn changed_cells(old: &Frame, new: &Frame) -> usize {
    let (ow, oh) = (old.width(), old.height());
    let (nw, nh) = (new.width(), new.height());
    let (min_w, min_h) = (ow.min(nw), oh.min(nh));
    let mut same = 0usize;
    for row in 0..min_h {
        let a = &old.cells()[row * ow..row * ow + min_w];
        let b = &new.cells()[row * nw..row * nw + min_w];
        same += a.iter().zip(b).filter(|(x, y)| x == y).count();
    }
    ow * oh + nw * nh - min_w * min_h - same
}

fn frame_json_payload_len(frame: &Frame) -> isize {
    let cells = frame.cells();
    let escaped = cells.iter().filter(|&&b| b == b'"' || b == b'\\').count();
    (cells.len() + escaped + 2 * (frame.height() - 1)) as isize
}

fn apply_writes(
    state: &State,
    items: &[&Value],
    allowed_keys: &[&str],
    single: bool,
    dry_run: bool,
) -> Result<BatchApply> {
    let mut pre_checks = BTreeMap::new();
    let mut parsed = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let entry = parse_and_precheck(state, item, allowed_keys, &mut pre_checks)
            .map_err(|error| prefix_error(index, single, error))?;
        parsed.push(entry);
    }
    preflight_work(state, &parsed, dry_run)?;

    let layers = state.project.layers();
    let one_by_one = layers.len() == 1 && layers[0].art.frames().len() == 1;
    let mut buffers: BTreeMap<(usize, usize), (usize, usize, Vec<u8>)> = BTreeMap::new();

    for (index, entry) in parsed.iter().enumerate() {
        let block = Frame::from_text_padded(entry.text).expect("validated in precheck");
        let (cur_w, cur_h, cur_cells) =
            buffers
                .entry((entry.layer, entry.frame))
                .or_insert_with(|| {
                    let orig = &layers[entry.layer].art.frames()[entry.frame].frame;
                    (orig.width(), orig.height(), orig.cells().to_vec())
                });
        match entry.place {
            None => {
                if !one_by_one && (block.width() != *cur_w || block.height() != *cur_h) {
                    return Err(prefix_error(
                        index,
                        single,
                        "Replacing the whole frame can change its size only in a project of one frame and one layer".into(),
                    ));
                }
                *cur_w = block.width();
                *cur_h = block.height();
                *cur_cells = block.cells().to_vec();
            }
            Some((x, y)) => {
                if x.saturating_add(block.width()) > *cur_w
                    || y.saturating_add(block.height()) > *cur_h
                {
                    return Err(prefix_error(
                        index,
                        single,
                        format!(
                            "A {}x{} block at {x},{y} does not fit the {cur_w}x{cur_h} frame",
                            block.width(),
                            block.height()
                        ),
                    ));
                }
                let (bw, bh) = (block.width(), block.height());
                let src = block.cells();
                for row in 0..bh {
                    let dst_start = (y + row) * *cur_w + x;
                    let src_start = row * bw;
                    cur_cells[dst_start..dst_start + bw]
                        .copy_from_slice(&src[src_start..src_start + bw]);
                }
            }
        }
    }

    let next_gen = state.generation.wrapping_add(1);
    let frames_len = layers[0].art.frames().len();
    let mut previews = BTreeMap::new();
    let mut delta_json = 0isize;

    for ((layer, frame), (w, h, cells)) in buffers {
        let orig = &layers[layer].art.frames()[frame].frame;
        let new_frame = Frame::new(w, h, cells).expect("validated ASCII cells");
        delta_json += frame_json_payload_len(&new_frame) - frame_json_payload_len(orig);
        let rev = frame_revision(
            next_gen,
            layer,
            &layers[layer].name,
            layers.len(),
            frame,
            frames_len,
            &new_frame,
        );
        let sha = content_sha256(&new_frame);
        let changed = if dry_run {
            changed_cells(orig, &new_frame)
        } else {
            0
        };
        previews.insert(
            (layer, frame),
            TargetPreview {
                revision: rev,
                sha256: sha,
                frame: new_frame,
                changed,
            },
        );
    }

    let updated = if dry_run {
        for preview in previews.values() {
            state.project.check_resize(&preview.frame)?;
        }
        if state.project.json_bound() as isize + delta_json > MAX_INPUT_BYTES as isize {
            let mut scratch = state.project.clone();
            for (&(layer, frame), preview) in &previews {
                scratch.swap_frame(layer, frame, preview.frame.clone())?;
            }
            scratch.fits()?;
        }
        None
    } else {
        let mut next_project = state.project.clone();
        for (&(layer, frame), preview) in &previews {
            next_project.swap_frame(layer, frame, preview.frame.clone())?;
        }
        next_project.fits()?;
        Some(next_project)
    };

    let outcomes = parsed
        .iter()
        .map(|entry| {
            let preview = &previews[&(entry.layer, entry.frame)];
            WriteOutcome {
                layer: entry.layer,
                frame: entry.frame,
                revision: preview.revision.clone(),
                sha256: preview.sha256.clone(),
            }
        })
        .collect();
    Ok(BatchApply {
        outcomes,
        previews,
        updated,
    })
}

fn write(state: &State, request: &Value) -> Result<(Value, Option<Layered>)> {
    let dry_run = parse_bool(request, "dry_run")?;
    let mut applied = apply_writes(state, &[request], SINGLE_WRITE_KEYS, true, dry_run)?;
    let out = &applied.outcomes[0];
    let result = if dry_run {
        let preview = applied
            .previews
            .remove(&(out.layer, out.frame))
            .expect("target preview exists");
        json!({
            "dry_run": true,
            "revision": preview.revision,
            "sha256": preview.sha256,
            "layer": out.layer,
            "frame": out.frame,
            "width": preview.frame.width(),
            "height": preview.frame.height(),
            "text": preview.frame.text(),
            "changed": preview.changed,
        })
    } else {
        json!({
            "revision": out.revision,
            "sha256": out.sha256,
            "layer": out.layer,
            "frame": out.frame,
        })
    };
    Ok((result, applied.updated))
}

fn batch(state: &State, request: &Value) -> Result<(Value, Option<Layered>)> {
    let obj = request.as_object().expect("op matched on object");
    reject_unknown_keys(obj, BATCH_KEYS)?;
    let dry_run = parse_bool(request, "dry_run")?;
    let writes = request
        .get("writes")
        .and_then(Value::as_array)
        .filter(|items| (1..=MAX_BATCH_WRITES).contains(&items.len()))
        .ok_or_else(|| {
            format!("writes must be an array of 1 to {MAX_BATCH_WRITES} write objects")
        })?;
    let refs: Vec<&Value> = writes.iter().collect();
    let applied = apply_writes(state, &refs, WRITE_KEYS, false, dry_run)?;
    let revisions: Vec<Value> = applied
        .outcomes
        .into_iter()
        .map(|out| {
            json!({
                "layer": out.layer,
                "frame": out.frame,
                "revision": out.revision,
                "sha256": out.sha256,
            })
        })
        .collect();
    let result = if dry_run {
        let mut total_changed = 0usize;
        let frames: Vec<Value> = applied
            .previews
            .into_iter()
            .map(|((layer, frame), preview)| {
                total_changed += preview.changed;
                json!({
                    "layer": layer,
                    "frame": frame,
                    "revision": preview.revision,
                    "sha256": preview.sha256,
                    "width": preview.frame.width(),
                    "height": preview.frame.height(),
                    "text": preview.frame.text(),
                    "changed": preview.changed,
                })
            })
            .collect();
        json!({
            "dry_run": true,
            "revisions": revisions,
            "frames": frames,
            "changed": total_changed,
        })
    } else {
        json!({"revisions": revisions})
    };
    Ok((result, applied.updated))
}

const ELEMENTS_KEYS: &[&str] = &["op", "id", "layer", "frame"];
const ADD_KEYS: &[&str] = &[
    "op", "id", "kind", "layer", "frame", "x", "y", "width", "height", "text", "style", "arrow",
    "dry_run",
];
const EDIT_KEYS: &[&str] = &[
    "op", "id", "element", "revision", "x", "y", "width", "height", "text", "style", "arrow",
    "force", "dry_run",
];
const DELETE_KEYS: &[&str] = &["op", "id", "element", "revision", "force", "dry_run"];

/// Names the element's layer, place, shape and the cells under its rectangle, so an edit based
/// on an older list is refused once the element or its cells change.
pub fn element_revision(layer: usize, layer_name: &str, element: &Element, art: &Frame) -> String {
    let shape = element.shape();
    let place = format!(
        "{layer}:{}:{layer_name}:{}:{}:{},{}:{}x{}:{}:{}:{}:{}:",
        layer_name.len(),
        element.id(),
        element.frame(),
        element.x(),
        element.y(),
        shape.width,
        shape.height,
        shape.kind.name(),
        shape.style.name(),
        shape.arrow.name(),
        shape.text.len(),
    );
    let width = art.width();
    let cells = (0..shape.height).flat_map(|row| {
        let at = (element.y() + row) * width + element.x();
        art.cells()[at..at + shape.width].iter().copied()
    });
    // 64-bit FNV-1a, as frame revisions use.
    let hash = place
        .bytes()
        .chain(shape.text.bytes())
        .chain(cells)
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    format!("{hash:016x}")
}

fn parse_opt_str<'a>(request: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match request.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn element_json(layer: usize, element: &Element, art: &Frame, layer_name: &str) -> Value {
    let shape = element.shape();
    let mut entry = json!({
        "element": element.name(),
        "layer": layer,
        "frame": element.frame(),
        "kind": shape.kind.name(),
        "x": element.x(),
        "y": element.y(),
        "width": shape.width,
        "height": shape.height,
        "revision": element_revision(layer, layer_name, element, art),
    });
    if shape.kind != Kind::Text {
        entry["style"] = json!(shape.style.name());
    }
    if shape.kind == Kind::Line {
        entry["arrow"] = json!(shape.arrow.name());
    }
    if !shape.text.is_empty() {
        entry["text"] = json!(shape.text);
    }
    entry
}

fn list_elements(state: &State, request: &Value) -> Result<Value> {
    reject_unknown_keys(
        request.as_object().expect("op matched on object"),
        ELEMENTS_KEYS,
    )?;
    let layers = state.project.layers();
    let frames = layers[0].art.frames().len();
    let layer_arg = parse_opt_usize(request, "layer")?
        .map(|index| resolve_index(Some(index), 0, layers.len(), "Layer"))
        .transpose()?;
    let frame_arg = parse_opt_usize(request, "frame")?
        .map(|index| resolve_index(Some(index), 0, frames, "Frame"))
        .transpose()?;
    let mut found = Vec::new();
    let mut work = 0usize;
    for (index, layer) in layers.iter().enumerate() {
        if layer_arg.is_some_and(|wanted| wanted != index) {
            continue;
        }
        let mut on: Vec<usize> = layer.elements.iter().map(Element::frame).collect();
        on.sort_unstable();
        on.dedup();
        for frame in on
            .into_iter()
            .filter(|&f| frame_arg.is_none_or(|wanted| wanted == f))
        {
            let art = &layer.art.frames()[frame].frame;
            work += 2 * art.cells().len();
            work += layer
                .elements
                .iter()
                .filter(|e| e.frame() == frame)
                .map(|e| 4 * e.shape().width * e.shape().height)
                .sum::<usize>();
            check_work_budget(work)?;
            let drift: BTreeMap<u32, usize> = elements::drift(art, &layer.elements, frame)
                .into_iter()
                .collect();
            for element in layer.elements.iter().filter(|e| e.frame() == frame) {
                let mut entry = element_json(index, element, art, &layer.name);
                entry["drifted"] = json!(drift[&element.id()]);
                found.push(entry);
            }
        }
    }
    Ok(json!({
        "elements": found,
        "next_element": format!("e{}", state.project.next_element()),
    }))
}

/// The layer and position in its list of the element `request["element"]` names.
fn find_element(state: &State, request: &Value) -> Result<(usize, usize)> {
    let name = request
        .get("element")
        .and_then(Value::as_str)
        .ok_or("element must be an id like e1")?;
    let id = elements::parse_id(name)?;
    state
        .project
        .layers()
        .iter()
        .enumerate()
        .find_map(|(layer, entry)| {
            let index = entry.elements.iter().position(|e| e.id() == id)?;
            Some((layer, index))
        })
        .ok_or_else(|| {
            format!(
                "No element {name}; live_elements lists the current ones (ids are never reused)"
            )
        })
}

/// The shape a request asks for, starting from `base` (an edit) or from nothing (an add).
fn requested_shape(request: &Value, base: Option<&Shape>) -> Result<Shape> {
    let kind = match (base, parse_opt_str(request, "kind")?) {
        (Some(shape), _) => shape.kind,
        (None, Some(kind)) => Kind::from_name(kind)?,
        (None, None) => return Err("kind must be box, text or line".into()),
    };
    let text = match parse_opt_str(request, "text")? {
        Some(text) => text.to_owned(),
        None => base.map_or_else(String::new, |shape| shape.text.clone()),
    };
    let style = match parse_opt_str(request, "style")? {
        Some(style) => Style::from_name(style)?,
        None => base.map_or(Style::Plain, |shape| shape.style),
    };
    let arrow = match parse_opt_str(request, "arrow")? {
        Some(arrow) => Arrow::from_name(arrow)?,
        None => base.map_or(Arrow::None, |shape| shape.arrow),
    };
    let (width, height) = (
        parse_opt_usize(request, "width")?,
        parse_opt_usize(request, "height")?,
    );
    let (width, height) = match kind {
        Kind::Text => {
            if width.is_some() || height.is_some() {
                return Err("A text element's size is its text's; omit width and height".into());
            }
            Frame::padded_dimensions(&text)?
        }
        Kind::Box => match (
            width.or(base.map(|s| s.width)),
            height.or(base.map(|s| s.height)),
        ) {
            (Some(width), Some(height)) => (width, height),
            _ => return Err("A box needs width and height".into()),
        },
        Kind::Line => (
            width.or(base.map(|s| s.width)).unwrap_or(1),
            height.or(base.map(|s| s.height)).unwrap_or(1),
        ),
    };
    let shape = Shape {
        kind,
        width,
        height,
        style,
        arrow,
        text,
    };
    shape.check()?;
    Ok(shape)
}

/// Apply an element edit to frame `frame` of layer `layer` and give the reply and project.
fn commit_element(
    state: &State,
    request: &Value,
    (layer, frame): (usize, usize),
    edit: Edit,
    id: u32,
) -> Result<(Value, Option<Layered>)> {
    let dry_run = parse_bool(request, "dry_run")?;
    let force = parse_bool(request, "force")?;
    let layers = state.project.layers();
    let entry = &layers[layer];
    if entry.locked {
        return Err(format!("Layer \"{}\" is locked", entry.name));
    }
    let project_cells =
        layers.len() * entry.art.frames().len() * entry.art.width() * entry.art.height();
    // Each element of the frame may be lifted and redrawn: its drawing made, compared and pasted.
    let element_cells: usize = entry
        .elements
        .iter()
        .filter(|element| element.frame() == frame)
        .map(|element| element.shape().width * element.shape().height)
        .sum();
    check_work_budget(2 * project_cells + 6 * element_cells)?;
    let art = &entry.art.frames()[frame].frame;
    let (drawn, elements) = elements::apply(art, &entry.elements, frame, edit, force)?;
    let changed = changed_cells(art, &drawn);
    let next_gen = state.generation.wrapping_add(1);
    let frame_rev = frame_revision(
        next_gen,
        layer,
        &entry.name,
        layers.len(),
        frame,
        entry.art.frames().len(),
        &drawn,
    );
    let mut reply = json!({
        "element": format!("e{id}"),
        "layer": layer,
        "frame": frame,
        "frame_revision": frame_rev,
        "changed": changed,
    });
    if let Some(element) = elements.iter().find(|e| e.id() == id) {
        reply["revision"] = json!(element_revision(layer, &entry.name, element, &drawn));
    }
    let text = drawn.text();
    let project = state.project.with_elements(layer, frame, drawn, elements)?;
    if dry_run {
        reply["dry_run"] = json!(true);
        reply["text"] = json!(text);
        return Ok((reply, None));
    }
    Ok((reply, Some(project)))
}

fn add_element(state: &State, request: &Value) -> Result<(Value, Option<Layered>)> {
    reject_unknown_keys(request.as_object().expect("op matched on object"), ADD_KEYS)?;
    let layers = state.project.layers();
    let layer = resolve_index(
        parse_opt_usize(request, "layer")?,
        state.active,
        layers.len(),
        "Layer",
    )?;
    let frames = layers[0].art.frames().len();
    let frame = resolve_index(
        parse_opt_usize(request, "frame")?,
        state.shown,
        frames,
        "Frame",
    )?;
    let at = match (
        parse_opt_usize(request, "x")?,
        parse_opt_usize(request, "y")?,
    ) {
        (Some(x), Some(y)) => (x, y),
        _ => return Err("Give x and y, the element's top-left cell".into()),
    };
    let shape = requested_shape(request, None)?;
    let id = state.project.next_element();
    commit_element(
        state,
        request,
        (layer, frame),
        Edit::Add { id, at, shape },
        id,
    )
}

/// The element an edit or delete names, after its optional revision check.
fn named_element<'a>(state: &'a State, request: &Value) -> Result<(usize, &'a Element)> {
    let (layer, index) = find_element(state, request)?;
    let entry = &state.project.layers()[layer];
    let element = &entry.elements[index];
    if let Some(given) = parse_opt_str(request, "revision")? {
        let art = &entry.art.frames()[element.frame()].frame;
        if given != element_revision(layer, &entry.name, element, art) {
            return Err(format!(
                "Element {} changed since it was listed: call live_elements again",
                element.name()
            ));
        }
    }
    Ok((layer, element))
}

fn edit_element(state: &State, request: &Value) -> Result<(Value, Option<Layered>)> {
    reject_unknown_keys(
        request.as_object().expect("op matched on object"),
        EDIT_KEYS,
    )?;
    if !["x", "y", "width", "height", "text", "style", "arrow"]
        .iter()
        .any(|key| request.get(key).is_some_and(|value| !value.is_null()))
    {
        return Err("Give at least one of x, y, width, height, text, style and arrow".into());
    }
    let (layer, element) = named_element(state, request)?;
    let at = (
        parse_opt_usize(request, "x")?.unwrap_or(element.x()),
        parse_opt_usize(request, "y")?.unwrap_or(element.y()),
    );
    let shape = requested_shape(request, Some(element.shape()))?;
    let id = element.id();
    let edit = Edit::Change { id, at, shape };
    commit_element(state, request, (layer, element.frame()), edit, id)
}

fn delete_element(state: &State, request: &Value) -> Result<(Value, Option<Layered>)> {
    reject_unknown_keys(
        request.as_object().expect("op matched on object"),
        DELETE_KEYS,
    )?;
    let (layer, element) = named_element(state, request)?;
    let id = element.id();
    commit_element(
        state,
        request,
        (layer, element.frame()),
        Edit::Delete { id },
        id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Animation, Layer, TimedFrame,
        motion::{Easing, Key, Motion, Property},
    };

    fn make_layered(layers: Vec<(&str, bool, bool, Vec<&str>)>) -> Layered {
        let built = layers
            .into_iter()
            .map(|(name, visible, locked, frames)| Layer {
                name: name.into(),
                visible,
                locked,
                art: Animation::new(
                    frames
                        .into_iter()
                        .map(|t| TimedFrame {
                            frame: Frame::from_text(t).unwrap(),
                            duration_ms: 100,
                        })
                        .collect(),
                    false,
                )
                .unwrap(),
                motion: Motion::default(),
                group: None,
                elements: Vec::new(),
            })
            .collect();
        Layered::new(built).unwrap()
    }

    fn state<'a>(project: &'a Layered, active: usize, shown: usize, generation: u64) -> State<'a> {
        State {
            project,
            active,
            shown,
            generation,
        }
    }

    #[test]
    fn reads_target_frames_and_composites_with_layers_metadata() {
        let project = make_layered(vec![
            ("Sky", true, false, vec!["....\n....", "~~~~\n~~~~"]),
            ("Zihuatanejo", true, true, vec!["  * \n    ", "    \n *  "]),
            ("Hidden", false, false, vec!["####\n####", "####\n####"]),
        ]);
        let mut layers = project.layers().to_vec();
        layers[1]
            .motion
            .set_key(
                Property::X,
                Key {
                    frame: 1,
                    value: 1.0,
                    easing: Easing::Linear,
                },
            )
            .unwrap();
        let project = Layered::new(layers).unwrap();

        let s = state(&project, 0, 0, 77);
        let (reply, changed) = answer(&s, &json!({"id": 3, "op": "read", "layer": 1, "frame": 0}));
        assert!(changed.is_none());
        let expected_art = &project.layers()[1].art.frames()[0].frame;
        assert_eq!(
            reply,
            json!({"id": 3, "result": {
                "revision": revision(&s, 1, 0),
                "sha256": content_sha256(expected_art),
                "layer": 1,
                "frame": 0,
                "frames": 2,
                "width": 4,
                "height": 2,
                "text": "  * \n    ",
                "active": 0,
                "shown": 0,
                "layers": [
                    {"name": "Sky", "visible": true, "locked": false},
                    {"name": "Zihuatanejo", "visible": true, "locked": true},
                    {"name": "Hidden", "visible": false, "locked": false},
                ],
            }})
        );

        // Composite read at frame 1 skips hidden layer, applies motion key, and omits revision/sha256/layer.
        let (comp_reply, changed) = answer(
            &s,
            &json!({"id": 4, "op": "read", "composite": true, "frame": 1}),
        );
        assert!(changed.is_none());
        let res = &comp_reply["result"];
        assert_eq!(res["composite"], true);
        assert!(res.get("revision").is_none());
        assert!(res.get("sha256").is_none());
        assert!(res.get("layer").is_none());
        assert_eq!(res["frame"], 1);
        assert_eq!(res["text"], project.frame(1).text());
        assert_eq!(res["active"], 0);
        assert_eq!(res["shown"], 0);

        // Composite cannot name a layer.
        let (err, _) = answer(
            &s,
            &json!({"id": 5, "op": "read", "composite": true, "layer": 0}),
        );
        assert_eq!(err["error"], "composite reads cannot name a layer");
    }

    #[test]
    fn f1_generation_invalidates_reads_after_delete_insert_reorder_and_undo() {
        let base_proj = make_layered(vec![
            ("Sky", true, false, vec!["aa", "bb"]),
            ("Bird", true, false, vec!["cc", "dd"]),
        ]);
        let s0 = state(&base_proj, 0, 0, 100);
        let rev_f0 = revision(&s0, 0, 0);
        let rev_f1 = revision(&s0, 0, 1);
        assert_eq!(rev_f0.len(), 16);

        // Delete frame 0, then insert "aa" back at frame 0: counts and cells match, generation moved.
        let mut edited = base_proj.clone();
        edited.remove_frames(0..1).unwrap();
        edited.insert_blank_frame(0, 100).unwrap();
        edited
            .replace_frame(0, 0, Frame::from_text("aa").unwrap())
            .unwrap();
        let s_after = state(&edited, 0, 0, 103);
        assert_ne!(revision(&s_after, 0, 0), rev_f0);
        let (reply, next) = answer(
            &s_after,
            &json!({"id": 1, "op": "write", "layer": 0, "frame": 0, "revision": rev_f0, "text": "zz"}),
        );
        assert!(next.is_none());
        assert_eq!(
            reply["error"],
            "The frame changed since it was read: read it again"
        );

        // Reordering identical frames still invalidates because project generation advanced.
        let identical = make_layered(vec![
            ("Sky", true, false, vec!["aa", "aa"]),
            ("Bird", true, false, vec!["cc", "dd"]),
        ]);
        let s_ident = state(&identical, 0, 0, 50);
        let rev_ident = revision(&s_ident, 0, 0);
        let mut reordered = identical.clone();
        reordered.move_frames(0..1, 1).unwrap();
        let s_reordered = state(&reordered, 0, 0, 51);
        let (reply, next) = answer(
            &s_reordered,
            &json!({"id": 2, "op": "write", "layer": 0, "frame": 0, "revision": rev_ident, "text": "xx"}),
        );
        assert!(next.is_none());
        assert_eq!(
            reply["error"],
            "The frame changed since it was read: read it again"
        );

        // Even an unrelated edit or undo/redo advances generation and invalidates s0's read on frame 1.
        let s_undone = state(&base_proj, 0, 0, 102);
        let (reply, next) = answer(
            &s_undone,
            &json!({"id": 3, "op": "write", "layer": 0, "frame": 1, "revision": rev_f1, "text": "yy"}),
        );
        assert!(next.is_none());
        assert!(reply["error"].is_string());

        // Moving default active/shown invalidates a write without explicit layer/frame.
        let s_moved = state(&base_proj, 1, 0, 100);
        let (reply, next) = answer(
            &s_moved,
            &json!({"id": 4, "op": "write", "revision": rev_f0, "text": "yy"}),
        );
        assert!(next.is_none());
        assert!(reply["error"].is_string());
    }

    #[test]
    fn write_dry_run_resize_lock_and_expected_sha256_contract() {
        let proj = make_layered(vec![
            ("Sky", true, false, vec!["....\n...."]),
            ("Locked", true, true, vec!["xxxx\nxxxx"]),
        ]);
        let s = state(&proj, 0, 0, 10);
        let rev = revision(&s, 0, 0);
        let sha = content_sha256(&proj.layers()[0].art.frames()[0].frame);

        // Dry run returns would-be text, revision (at gen+1), sha256, and changed count, leaving project None.
        let (reply, updated) = answer(
            &s,
            &json!({
                "id": 1, "op": "write", "layer": 0, "frame": 0,
                "revision": rev, "expected_sha256": sha.to_ascii_uppercase(),
                "text": "xy", "x": 1, "y": 1, "dry_run": true
            }),
        );
        assert!(updated.is_none());
        assert_eq!(reply["result"]["dry_run"], true);
        assert_eq!(reply["result"]["text"], "....\n.xy.");
        assert_eq!(reply["result"]["changed"], 2);

        // Actual write matches the dry run's revision and sha256.
        let (real_reply, real_proj) = answer(
            &s,
            &json!({
                "id": 2, "op": "write", "layer": 0, "frame": 0,
                "revision": rev, "expected_sha256": sha,
                "text": "xy", "x": 1, "y": 1
            }),
        );
        let real_proj = real_proj.unwrap();
        assert_eq!(
            real_reply["result"]["revision"],
            reply["result"]["revision"]
        );
        assert_eq!(real_reply["result"]["sha256"], reply["result"]["sha256"]);
        assert_eq!(
            real_proj.layers()[0].art.frames()[0].frame.text(),
            "....\n.xy."
        );

        // Locked layer refusal names the layer.
        let locked_rev = revision(&s, 1, 0);
        let (reply, updated) = answer(
            &s,
            &json!({"id": 3, "op": "write", "layer": 1, "frame": 0, "revision": locked_rev, "text": "aaaa\naaaa"}),
        );
        assert!(updated.is_none());
        assert_eq!(reply["error"], "Layer \"Locked\" is locked");

        // Size change refused in multi-layer project, allowed in 1x1 project.
        let (reply, updated) = answer(
            &s,
            &json!({"id": 4, "op": "write", "revision": rev, "text": "ab"}),
        );
        assert!(updated.is_none());
        assert!(
            reply["error"]
                .as_str()
                .unwrap()
                .contains("one frame and one layer")
        );

        let single_proj = make_layered(vec![("Layer 1", true, false, vec!["ab\ncd"])]);
        let s1 = state(&single_proj, 0, 0, 5);
        let r1 = revision(&s1, 0, 0);
        let (dry, dry_proj) = answer(
            &s1,
            &json!({"id": 5, "op": "write", "revision": r1, "text": "ab \ncd ", "dry_run": true}),
        );
        assert!(dry_proj.is_none());
        assert_eq!(dry["result"]["changed"], 2);
        let (res, resized) = answer(
            &s1,
            &json!({"id": 6, "op": "write", "revision": r1, "text": "abc\ndef"}),
        );
        let resized = resized.unwrap();
        assert_eq!(resized.layers()[0].art.frames()[0].frame.text(), "abc\ndef");
        assert!(res["result"]["revision"].is_string());

        // Mismatched and malformed expected_sha256 fail closed.
        let wrong = content_sha256(&Frame::from_text("zzzz\nzzzz").unwrap());
        let (reply, _) = answer(
            &s,
            &json!({"id": 7, "op": "write", "revision": rev, "expected_sha256": wrong, "text": "xy"}),
        );
        assert_eq!(
            reply["error"],
            "The frame content digest changed since it was read: read it again"
        );
        for bad in [Value::Null, json!(42), json!("abcd"), json!("g".repeat(64))] {
            let (reply, _) = answer(
                &s,
                &json!({"id": 8, "op": "write", "revision": rev, "expected_sha256": bad, "text": "xy"}),
            );
            assert_eq!(
                reply["error"],
                "expected_sha256 must be a 64-character hex SHA-256 digest"
            );
        }
    }

    #[test]
    fn f2_batch_prechecks_composes_checks_successive_geometry_and_rolls_back() {
        let proj = make_layered(vec![
            ("Sky", true, false, vec!["....\n....", "aaaa\naaaa"]),
            ("Bird", true, false, vec!["1111\n1111", "2222\n2222"]),
        ]);
        let s = state(&proj, 0, 0, 9);
        let rev00 = revision(&s, 0, 0);
        let sha00 = content_sha256(&proj.layers()[0].art.frames()[0].frame);
        let rev11 = revision(&s, 1, 1);

        // Two overlapping writes sharing the original revision & sha256: later wins, both return final revision.
        let (reply, updated) = answer(
            &s,
            &json!({
                "id": 1,
                "op": "batch",
                "writes": [
                    {"revision": rev00, "expected_sha256": sha00, "text": "AA\nAA", "x": 0, "y": 0},
                    {"revision": rev00, "expected_sha256": sha00, "text": "BB", "x": 1, "y": 0},
                    {"layer": 1, "frame": 1, "revision": rev11, "text": "ZZ", "x": 2, "y": 1},
                ]
            }),
        );
        let updated = updated.unwrap();
        assert_eq!(
            updated.layers()[0].art.frames()[0].frame.text(),
            "ABB.\nAA.."
        );
        assert_eq!(
            updated.layers()[1].art.frames()[1].frame.text(),
            "2222\n22ZZ"
        );
        let revs = reply["result"]["revisions"].as_array().unwrap();
        assert_eq!(revs.len(), 3);
        assert_eq!(revs[0], revs[1]);
        assert_ne!(revs[0]["revision"], rev00);

        // Dry run batch reports per-frame changed count and leaves project untouched.
        let (dry, none) = answer(
            &s,
            &json!({
                "id": 2,
                "op": "batch",
                "dry_run": true,
                "writes": [
                    {"revision": rev00, "text": "AA", "x": 0, "y": 0},
                    {"revision": rev00, "text": "BB", "x": 1, "y": 0},
                ]
            }),
        );
        assert!(none.is_none());
        assert_eq!(dry["result"]["dry_run"], true);
        assert_eq!(dry["result"]["changed"], 3);
        assert_eq!(dry["result"]["frames"][0]["text"], "ABB.\n....");

        // Stale final write rolls everything back.
        let (err, none) = answer(
            &s,
            &json!({
                "id": 3,
                "op": "batch",
                "writes": [
                    {"revision": rev00, "text": "AA", "x": 0, "y": 0},
                    {"layer": 1, "frame": 1, "revision": "bad", "text": "ZZ", "x": 0, "y": 0},
                ]
            }),
        );
        assert!(none.is_none());
        assert_eq!(
            err["error"],
            "Write 2: The frame changed since it was read: read it again"
        );

        // Geometry checked against successive scratch state in a 1x1 project.
        let one = make_layered(vec![("Layer 1", true, false, vec!["ab\ncd"])]);
        let s1 = state(&one, 0, 0, 1);
        let r1 = revision(&s1, 0, 0);
        let (ok, resized) = answer(
            &s1,
            &json!({
                "id": 4,
                "op": "batch",
                "writes": [
                    {"revision": r1, "text": "abcdef\nghijkl"},
                    {"revision": r1, "text": "ZZ", "x": 4, "y": 0},
                ]
            }),
        );
        assert!(ok["result"].is_object());
        assert_eq!(
            resized.unwrap().layers()[0].art.frames()[0].frame.text(),
            "abcdZZ\nghijkl"
        );

        let (err, none) = answer(
            &s1,
            &json!({
                "id": 5,
                "op": "batch",
                "writes": [
                    {"revision": r1, "text": "ab"},
                    {"revision": r1, "text": "Z", "x": 0, "y": 1},
                ]
            }),
        );
        assert!(none.is_none());
        assert_eq!(
            err["error"],
            "Write 2: A 1x1 block at 0,1 does not fit the 2x1 frame"
        );

        // Multi-layer batch cannot resize.
        let (err, _) = answer(
            &s,
            &json!({"id": 6, "op": "batch", "writes": [{"revision": rev00, "text": "ab"}]}),
        );
        assert!(
            err["error"]
                .as_str()
                .unwrap()
                .starts_with("Write 1: Replacing the whole frame")
        );
    }

    #[test]
    fn f3_work_budget_admits_8m_batches_and_rejects_oversized_padded_batches_before_alloc() {
        // 8,000,000-cell project: 16 layers x 4 frames x 500x250.
        let blank = Frame::blank(500, 250).unwrap();
        let art = Animation::new(
            vec![
                TimedFrame {
                    frame: blank,
                    duration_ms: 100
                };
                4
            ],
            false,
        )
        .unwrap();
        let layers = (0..16)
            .map(|i| Layer {
                name: format!("L{i}"),
                visible: true,
                locked: false,
                art: art.clone(),
                motion: Motion::default(),
                group: None,
                elements: Vec::new(),
            })
            .collect();
        let big = Layered::new(layers).unwrap();
        let s = state(&big, 0, 0, 42);
        let rev = revision(&s, 0, 0);
        let sha = content_sha256(&big.layers()[0].art.frames()[0].frame);

        // 256 small writes on an 8M-cell project succeed cleanly within budget.
        let writes: Vec<Value> = (0..256)
            .map(|i| json!({"revision": rev, "expected_sha256": sha, "text": "x", "x": i, "y": 0}))
            .collect();
        let (reply, updated) = answer(&s, &json!({"id": 1, "op": "batch", "writes": writes}));
        let updated = updated.expect("256 small writes on 8M cells succeed");
        assert_eq!(reply["result"]["revisions"].as_array().unwrap().len(), 256);
        assert_eq!(
            &updated.layers()[0].art.frames()[0].frame.text()[..256],
            &"x".repeat(256)
        );

        // 256 full-size 512x256 padded writes (only ~200 KiB JSON!) exceed MAX_LIVE_CELL_WORK, even on dry_run.
        let small = make_layered(vec![("Layer 1", true, false, vec!["...."])]);
        let s_small = state(&small, 0, 0, 1);
        let r_small = revision(&s_small, 0, 0);
        let padded_512x256 = format!("{}\n{}", "x".repeat(512), "\n".repeat(255));
        assert_eq!(
            Frame::padded_dimensions(&padded_512x256).unwrap(),
            (512, 256)
        );
        let over_writes = vec![json!({"revision": r_small, "text": padded_512x256}); 256];
        let (err, none) = answer(
            &s_small,
            &json!({"id": 2, "op": "batch", "dry_run": true, "writes": over_writes}),
        );
        assert!(none.is_none());
        assert!(
            err["error"].as_str().unwrap().contains("work budget"),
            "{err}"
        );
    }

    #[test]
    fn dry_run_checks_16_mib_json_limit_and_validation_rejects_bad_inputs() {
        // Test the 16 MiB JSON limit branch on dry_run.
        let col3_slash = Frame::new(3, 256, vec![b'\\'; 3 * 256]).unwrap();
        let col3_space = Frame::blank(3, 256).unwrap();
        let slash_art = Animation::new(
            vec![
                TimedFrame {
                    frame: col3_slash.clone(),
                    duration_ms: 100,
                };
                595
            ],
            false,
        )
        .unwrap();
        let space_art = Animation::new(
            vec![
                TimedFrame {
                    frame: col3_space,
                    duration_ms: 100,
                };
                595
            ],
            false,
        )
        .unwrap();
        let mut p_layers: Vec<Layer> = (0..13)
            .map(|i| Layer {
                name: format!("S{i}"),
                visible: true,
                locked: false,
                art: slash_art.clone(),
                motion: Motion::default(),
                group: None,
                elements: Vec::new(),
            })
            .collect();
        p_layers.push(Layer {
            name: "Edit".into(),
            visible: true,
            locked: false,
            art: space_art,
            motion: Motion::default(),
            group: None,
            elements: Vec::new(),
        });
        let proj_near = Layered::new(p_layers).unwrap();
        let s_near = state(&proj_near, 13, 0, 1);
        let slash_text = col3_slash.text();
        let near_writes: Vec<Value> = (0..256)
            .map(|f| {
                json!({
                    "layer": 13,
                    "frame": f,
                    "revision": revision(&s_near, 13, f),
                    "text": slash_text,
                })
            })
            .collect();
        let (err, none) = answer(
            &s_near,
            &json!({"id": 9, "op": "batch", "dry_run": true, "writes": near_writes}),
        );
        assert!(none.is_none());
        assert!(err["error"].as_str().unwrap().contains("16 MiB"), "{err}");

        // Exercise argument and shape validation branches cleanly.
        let tiny = make_layered(vec![("L", true, false, vec!["ab"])]);
        let st = state(&tiny, 0, 0, 1);
        let rt = revision(&st, 0, 0);
        assert_eq!(
            answer(&st, &json!({"id": 1, "op": "nope"})).0["error"],
            "Unknown request"
        );
        assert_eq!(answer(&st, &json!("bad")).0["error"], "Unknown request");
        for (req, expected) in [
            (
                json!({"id": 1, "op": "read", "unknown": 1}),
                "Unknown argument unknown",
            ),
            (
                json!({"id": 1, "op": "read", "composite": "yes"}),
                "composite must be a boolean",
            ),
            (
                json!({"id": 1, "op": "read", "layer": -1}),
                "layer must be a whole number",
            ),
            (
                json!({"id": 1, "op": "read", "layer": 1}),
                "Layer 1 is not in the project, which has 1 layer (0)",
            ),
            (
                json!({"id": 1, "op": "read", "frame": 1}),
                "Frame 1 is not in the project, which has 1 frame (0)",
            ),
            (
                json!({"id": 1, "op": "write", "dry_run": 1, "revision": rt, "text": "ab"}),
                "dry_run must be a boolean",
            ),
            (
                json!({"id": 1, "op": "write", "extra": 1, "revision": rt, "text": "ab"}),
                "Unknown argument extra",
            ),
            (
                json!({"id": 1, "op": "write", "revision": 1, "text": "ab"}),
                "revision must be a string",
            ),
            (
                json!({"id": 1, "op": "write", "revision": rt, "text": "ab", "x": 0}),
                "Give both x and y to replace a block, or neither to replace the frame",
            ),
            (
                json!({"id": 1, "op": "write", "revision": rt, "x": 0, "y": 0}),
                "text must be a string",
            ),
            (
                json!({"id": 1, "op": "write", "revision": rt, "text": "a".repeat(MAX_INPUT_BYTES + 1)}),
                "Text exceeds the 16 MiB input limit",
            ),
            (
                json!({"id": 1, "op": "write", "revision": rt, "text": "\n".repeat(257)}),
                "Dimensions must be 1..=512 columns and 1..=256 rows",
            ),
            (
                json!({"id": 1, "op": "write", "revision": rt, "text": "a\u{7}"}),
                "Only printable ASCII is allowed: byte 1 is 0x07",
            ),
            (
                json!({"id": 1, "op": "write", "revision": rt, "text": "abcd", "x": 0, "y": 0}),
                "A 4x1 block at 0,0 does not fit the 2x1 frame",
            ),
            (
                json!({"id": 1, "op": "batch", "writes": []}),
                "writes must be an array of 1 to 256 write objects",
            ),
            (
                json!({"id": 1, "op": "batch", "writes": [1]}),
                "Write 1: must be an object",
            ),
            (
                json!({"id": 1, "op": "batch", "writes": [{"revision": rt, "text": "ab", "dry_run": true}]}),
                "Write 1: Unknown argument dry_run",
            ),
            (
                json!({"id": 1, "op": "batch", "writes": [{"revision": rt, "text": "ab", "writes": []}]}),
                "Write 1: Unknown argument writes",
            ),
        ] {
            let (reply, _) = answer(&st, &req);
            assert_eq!(reply["error"], expected, "{req}");
        }
        // Mixed L-shaped resize and shrink changed-cell counts.
        let rect = make_layered(vec![("L", true, false, vec!["abc\ndef"])]);
        let sr = state(&rect, 0, 0, 1);
        let rr = revision(&sr, 0, 0);
        let (l_resize, _) = answer(
            &sr,
            &json!({"id": 1, "op": "write", "revision": rr, "text": "ax\nyz\n12", "dry_run": true}),
        );
        assert_eq!(l_resize["result"]["changed"], 7);
        let (shrink, _) = answer(
            &sr,
            &json!({"id": 2, "op": "write", "revision": rr, "text": "ab", "dry_run": true}),
        );
        assert_eq!(shrink["result"]["changed"], 4);

        // Length-prefixed layer.name keeps colons and digits collision-free.
        let colon_a = make_layered(vec![
            ("L:0:1", true, false, vec!["a"]),
            ("1:b", true, false, vec!["b"]),
        ]);
        let colon_b = make_layered(vec![
            ("L:0", true, false, vec!["a"]),
            ("1:1:b", true, false, vec!["b"]),
        ]);
        assert_ne!(
            revision(&state(&colon_a, 0, 0, 1), 0, 0),
            revision(&state(&colon_b, 0, 0, 1), 0, 0)
        );
        // Multi-layer/multi-frame out-of-range wording too.
        let multi = make_layered(vec![
            ("A", true, false, vec!["ab", "cd"]),
            ("B", true, false, vec!["ef", "gh"]),
        ]);
        let sm = state(&multi, 0, 0, 1);
        assert_eq!(
            answer(&sm, &json!({"id": 1, "op": "read", "layer": 2})).0["error"],
            "Layer 2 is not in the project, which has 2 layers (0 to 1)"
        );
        assert_eq!(
            answer(&sm, &json!({"id": 1, "op": "read", "frame": 2})).0["error"],
            "Frame 2 is not in the project, which has 2 frames (0 to 1)"
        );
    }

    #[test]
    fn sign_in_falls_back_once_to_bare_token_before_greeting() {
        let token = "a".repeat(64);
        let mut sign_in = SignIn::new(token.clone());
        let initial: Value = serde_json::from_str(&sign_in.initial_message()).unwrap();
        assert_eq!(initial["token"], token);
        assert_eq!(
            initial["capabilities"],
            json!(["expected_sha256", "elements"])
        );
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
