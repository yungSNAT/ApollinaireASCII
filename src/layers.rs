// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Layers: animations drawn separately on one timeline and flattened into one.
//!
//! ```
//! use apollinaire_ascii::{Animation, Frame, layers::{Layer, Layered}};
//! let sky = Animation::still(Frame::from_text("* *")?);
//! let bird = Animation::still(Frame::from_text(" v ")?);
//! let project = Layered::new(vec![
//!     Layer { name: "Sky".into(), visible: true, locked: false, art: sky, motion: Default::default(), group: None },
//!     Layer { name: "Bird".into(), visible: true, locked: false, art: bird, motion: Default::default(), group: None },
//! ])?;
//! assert_eq!(project.flatten().frames()[0].frame.text(), "*v*");
//! assert_eq!(Layered::from_json(&project.to_json())?, project);
//! # Ok::<(), String>(())
//! ```

use crate::motion::{Motion, Property};
use crate::{Animation, Frame, MAX_INPUT_BYTES, MAX_PROJECT_CELLS, Result, TimedFrame};
use serde_json::{Map, Value, json};
use std::ops::Range;

pub const MAX_LAYERS: usize = 16;
/// A one-layer project with this name, visible, is written as a version 1 project.
pub const DEFAULT_NAME: &str = "Layer 1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    /// 1 to 32 printable ASCII characters.
    pub name: String,
    pub visible: bool,
    /// The browser editor refuses to change a locked layer's characters, keys or name.
    pub locked: bool,
    pub art: Animation,
    /// Keys that move, scale and turn the layer over the frames; empty for none.
    pub motion: Motion,
    /// The name of the group the layer is in, 1 to 32 printable ASCII characters, or `None`.
    /// A group is a label: the layers in one are next to each other in the list, and the
    /// browser editor shows, hides, locks, moves and deletes them together. Drawing ignores it.
    pub group: Option<String>,
}

impl Layer {
    /// Frame `index` of this layer as it is shown, through its motion.
    pub fn drawn(&self, index: usize) -> std::borrow::Cow<'_, Frame> {
        let frame = &self.art.frames()[index].frame;
        match self.motion.is_empty() {
            true => std::borrow::Cow::Borrowed(frame),
            false => std::borrow::Cow::Owned(self.motion.draw(frame, index)),
        }
    }
}

/// Layers from bottom to top. Every layer has the same dimensions, frame count, frame
/// durations and looping, and a space in a layer shows the layers below it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layered {
    layers: Vec<Layer>,
}

impl Layered {
    /// Also refuses a project whose JSON would pass the 16 MiB input limit, so every project
    /// that can be built can be loaded again.
    pub fn new(layers: Vec<Layer>) -> Result<Self> {
        let project = Self::parsed(layers)?;
        project.fits()?;
        Ok(project)
    }
    /// [`new`](Self::new) without the size check, for the parser: a parsed project's JSON is
    /// no longer than the input it came from.
    pub(crate) fn parsed(layers: Vec<Layer>) -> Result<Self> {
        if layers.is_empty() || layers.len() > MAX_LAYERS {
            return Err(format!("A project needs 1..={MAX_LAYERS} layers"));
        }
        let first = &layers[0].art;
        for (index, layer) in layers.iter().enumerate() {
            check_name(&layer.name, "layer")?;
            if let Some(group) = &layer.group {
                check_name(group, "group")?;
                let next = layers.get(index + 1).and_then(|l| l.group.as_ref());
                if next != Some(group)
                    && layers[index + 1..]
                        .iter()
                        .any(|l| l.group.as_ref() == Some(group))
                {
                    return Err(format!(
                        "The layers in group {group} must be next to each other"
                    ));
                }
            }
            if layer
                .motion
                .last_frame()
                .is_some_and(|frame| frame >= layer.art.frames().len())
            {
                return Err("A key is on a frame the project does not have".into());
            }
            let art = &layer.art;
            if art.width() != first.width()
                || art.height() != first.height()
                || art.looping() != first.looping()
                || art.frames().len() != first.frames().len()
                || art
                    .frames()
                    .iter()
                    .zip(first.frames())
                    .any(|(a, b)| a.duration_ms != b.duration_ms)
            {
                return Err(
                    "Every layer must have the same dimensions, frames, durations and looping"
                        .into(),
                );
            }
        }
        // Each factor is bounded by a single animation's limits, so this cannot overflow.
        if first.width() * first.height() * first.frames().len() * layers.len() > MAX_PROJECT_CELLS
        {
            return Err("Project exceeds 8,000,000 cells".into());
        }
        Ok(Self { layers })
    }
    fn fits(&self) -> Result<()> {
        // The bound is cheap and close; only a project near the limit is serialised to measure.
        if self.json_bound() > MAX_INPUT_BYTES && self.to_json().len() > MAX_INPUT_BYTES {
            return Err("Project JSON would exceed the 16 MiB input limit".into());
        }
        Ok(())
    }
    /// At least the length of [`to_json`](Self::to_json). In printable ASCII JSON escapes only
    /// `"` and `\`, and each row break is written as `\n`.
    fn json_bound(&self) -> usize {
        let texts: usize = self
            .layers
            .iter()
            .flat_map(|layer| layer.art.frames())
            .map(|entry| {
                let cells = entry.frame.cells();
                let escaped = cells.iter().filter(|&&b| b == b'"' || b == b'\\').count();
                // Quotes, a comma and an escaped line feed between rows.
                cells.len() + escaped + 3 + 2 * (entry.frame.height() - 1)
            })
            .sum();
        let names: usize = self
            .layers
            .iter()
            // A key is at most 158 bytes (a 3-digit frame, and a value and 4 curve points of
            // up to 24 characters each); the layer's other fields and track names fit in 160.
            .map(|layer| {
                let group = layer.group.as_ref().map_or(0, |group| 2 * group.len() + 12);
                2 * layer.name.len() + group + 160 + 160 * layer.motion.len()
            })
            .sum();
        // Per frame: {"duration_ms":60000,"layers":[]}, and the header with its brackets.
        texts + names + 40 * self.layers[0].art.frames().len() + 128
    }
    pub fn single(art: Animation) -> Self {
        Self {
            layers: vec![Layer {
                name: DEFAULT_NAME.into(),
                visible: true,
                locked: false,
                art,
                motion: Motion::default(),
                group: None,
            }],
        }
    }
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }
    /// The layers that move together with layer `index`: its group, or the layer alone. Panics
    /// when `index` is not a layer of the project.
    pub fn block(&self, index: usize) -> Range<usize> {
        let Some(group) = &self.layers[index].group else {
            return index..index + 1;
        };
        let same = |layer: &Layer| layer.group.as_ref() == Some(group);
        let start = self.layers[..index]
            .iter()
            .rposition(|layer| !same(layer))
            .map_or(0, |at| at + 1);
        let end = self.layers[index..]
            .iter()
            .position(|layer| !same(layer))
            .map_or(self.layers.len(), |at| index + at);
        start..end
    }
    /// The animation, when the project is one visible, unlocked layer named [`DEFAULT_NAME`]
    /// with no keys and no group: the project a version 1 file holds.
    pub fn plain(&self) -> Option<&Animation> {
        match &self.layers[..] {
            [layer]
                if layer.visible
                    && !layer.locked
                    && layer.name == DEFAULT_NAME
                    && layer.motion.is_empty()
                    && layer.group.is_none() =>
            {
                Some(&layer.art)
            }
            _ => None,
        }
    }
    /// Replace one frame of one layer, keeping the frame's duration. Nothing changes on error.
    pub fn replace_frame(&mut self, layer: usize, index: usize, frame: Frame) -> Result<()> {
        let art = &mut self
            .layers
            .get_mut(layer)
            .ok_or("Layer index is outside the project")?
            .art;
        let old = art
            .frames()
            .get(index)
            .ok_or("Frame index is outside the project")?
            .clone();
        let duration_ms = old.duration_ms;
        art.replace_frame(index, TimedFrame { frame, duration_ms })?;
        self.fits().inspect_err(|_| {
            self.layers[layer]
                .art
                .replace_frame(index, old)
                .expect("The old frame fitted");
        })
    }
    /// One frame as shown: the visible layers, each drawn through its motion, stacked.
    pub fn frame(&self, index: usize) -> Frame {
        let art = &self.layers[0].art;
        let shown: Vec<std::borrow::Cow<Frame>> = self
            .layers
            .iter()
            .filter(|layer| layer.visible)
            .map(|layer| layer.drawn(index))
            .collect();
        stack(art.width(), art.height(), shown.iter().map(AsRef::as_ref))
    }
    /// [`flatten`](Self::flatten), moving rather than copying a lone visible layer.
    pub(crate) fn into_flat(mut self) -> Animation {
        match &self.layers[..] {
            [layer] if layer.visible && layer.motion.is_empty() => self.layers.swap_remove(0).art,
            _ => self.flatten(),
        }
    }
    /// The project as shown, for export and every command that reads one animation.
    pub fn flatten(&self) -> Animation {
        let first = &self.layers[0].art;
        if let [layer] = &self.layers[..]
            && layer.visible
            && layer.motion.is_empty()
        {
            return layer.art.clone();
        }
        let frames = (0..first.frames().len())
            .map(|index| TimedFrame {
                frame: self.frame(index),
                duration_ms: first.frames()[index].duration_ms,
            })
            .collect();
        Animation::new(frames, first.looping()).expect("Layers share a valid timeline")
    }
    /// Version 1 for a [`plain`](Self::plain) project, version 3 when a layer has keys,
    /// otherwise version 2.
    pub fn to_json(&self) -> String {
        if let Some(art) = self.plain() {
            return art.to_json();
        }
        let first = &self.layers[0].art;
        let keyed = self.layers.iter().any(|layer| !layer.motion.is_empty());
        json!({"format":"asciiforge","version":if keyed { 3 } else { 2 },
            "width":first.width(),"height":first.height(),"loop":first.looping(),
            "layers":self.layers.iter().map(layer_json).collect::<Vec<_>>(),
            "frames":(0..first.frames().len()).map(|index| json!({
                "duration_ms":first.frames()[index].duration_ms,
                "layers":self.layers.iter().map(|l| l.art.frames()[index].frame.text())
                    .collect::<Vec<_>>()})).collect::<Vec<_>>()
        })
        .to_string()
    }
    /// Insert a blank frame at `index` in every layer; keys from `index` on move with their
    /// frames. Nothing changes on error.
    pub fn insert_blank_frame(&mut self, index: usize, duration_ms: u32) -> Result<()> {
        self.restructure(
            |art| {
                let frame = Frame::blank(art.width(), art.height())?;
                art.insert_frame(index, TimedFrame { frame, duration_ms })
            },
            |key| Some(key + usize::from(key >= index)),
        )
    }
    /// Copy a range of frames in every layer to just after it. Keys stay on the original
    /// frames, and later keys move with their frames. Nothing changes on error.
    pub fn duplicate_frames(&mut self, range: Range<usize>) -> Result<()> {
        let (end, count) = (range.end, range.len());
        self.restructure(
            |art| art.duplicate_frames(range.clone()),
            |key| Some(if key >= end { key + count } else { key }),
        )
    }
    /// Remove a range of frames from every layer, with the keys on them. Nothing changes on
    /// error.
    pub fn remove_frames(&mut self, range: Range<usize>) -> Result<()> {
        let (start, end) = (range.start, range.end);
        self.restructure(
            |art| art.remove_frames(range.clone()),
            |key| match key {
                _ if key < start => Some(key),
                _ if key < end => None,
                _ => Some(key - (end - start)),
            },
        )
    }
    /// Move a range of frames in every layer to `destination`, as
    /// [`Animation::move_frames`] does; keys move with their frames. Nothing changes on error.
    pub fn move_frames(&mut self, range: Range<usize>, destination: usize) -> Result<()> {
        let (start, count) = (range.start, range.len());
        self.restructure(
            |art| art.move_frames(range.clone(), destination),
            |key| {
                // The frame's position with the range taken out, then with it put back at
                // `destination`.
                Some(if range.contains(&key) {
                    key - start + destination
                } else {
                    let rest = if key >= range.end { key - count } else { key };
                    if rest >= destination {
                        rest + count
                    } else {
                        rest
                    }
                })
            },
        )
    }
    fn restructure(
        &mut self,
        edit: impl Fn(&mut Animation) -> Result<()>,
        map: impl Fn(usize) -> Option<usize>,
    ) -> Result<()> {
        let layers = self
            .layers
            .iter()
            .map(|layer| {
                let mut art = layer.art.clone();
                edit(&mut art)?;
                let mut motion = layer.motion.clone();
                motion.remap(&map);
                Ok(Layer {
                    name: layer.name.clone(),
                    visible: layer.visible,
                    locked: layer.locked,
                    art,
                    motion,
                    group: layer.group.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        *self = Self::new(layers)?;
        Ok(())
    }
    /// Read a version 1, 2 or 3 project.
    pub fn from_json(input: &str) -> Result<Self> {
        if input.len() > MAX_INPUT_BYTES {
            return Err("Project exceeds the 16 MiB input limit".into());
        }
        crate::project_json::parse(input)
    }
}

/// A layer's entry in the `layers` list: `locked` only when true, `group` only when set, and
/// its keys in version 3.
fn layer_json(layer: &Layer) -> Value {
    let mut entry = json!({"name":layer.name,"visible":layer.visible});
    if layer.locked {
        entry["locked"] = Value::Bool(true);
    }
    if let Some(group) = &layer.group {
        entry["group"] = json!(group);
    }
    if !layer.motion.is_empty() {
        let tracks: Map<String, Value> = Property::ALL
            .into_iter()
            .filter(|&property| !layer.motion.keys(property).is_empty())
            .map(|property| {
                let keys = layer.motion.keys(property).iter().map(|key| {
                    let easing = match key.easing.name() {
                        Some(name) => json!(name),
                        None => json!(key.easing.points()),
                    };
                    json!({"frame":key.frame,"value":key.value,"easing":easing})
                });
                (property.name().to_string(), keys.collect())
            })
            .collect();
        entry["motion"] = Value::Object(tracks);
    }
    entry
}

fn check_name(name: &str, of: &str) -> Result<()> {
    if name.is_empty() || name.len() > 32 || !name.bytes().all(|b| (32..=126).contains(&b)) {
        return Err(format!(
            "A {of} name needs 1 to 32 printable ASCII characters"
        ));
    }
    Ok(())
}

/// Stack frames of one size from bottom to top; spaces are transparent.
pub(crate) fn stack<'a>(
    width: usize,
    height: usize,
    frames: impl IntoIterator<Item = &'a Frame>,
) -> Frame {
    let mut cells = vec![b' '; width * height];
    for frame in frames {
        for (cell, &value) in cells.iter_mut().zip(frame.cells()) {
            if value != b' ' {
                *cell = value;
            }
        }
    }
    Frame::new(width, height, cells).expect("Stacked frames stay printable ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MAX_FRAMES,
        motion::{Easing, Key},
    };

    /// The longest numbers serde_json writes in range, a key on every frame of every track.
    #[test]
    fn the_json_bound_covers_the_longest_keys() {
        let frames = vec![
            TimedFrame {
                frame: Frame::blank(1, 1).unwrap(),
                duration_ms: 6_000,
            };
            MAX_FRAMES
        ];
        let art = Animation::new(frames, true).unwrap();
        let tiny = 1.234_567_890_123_456_7e-300;
        let mut motion = Motion::default();
        for property in Property::ALL {
            let value = if property.range().contains(&-tiny) {
                -tiny
            } else {
                tiny
            };
            for frame in 0..MAX_FRAMES {
                let easing = Easing::curve([tiny, -tiny, tiny, -tiny]).unwrap();
                motion
                    .set_key(
                        property,
                        Key {
                            frame,
                            value,
                            easing,
                        },
                    )
                    .unwrap();
            }
        }
        let layer = Layer {
            name: "\"".repeat(32),
            visible: false,
            locked: true,
            art,
            motion,
            group: Some("\\".repeat(32)),
        };
        let project = Layered::new(vec![layer.clone(), layer]).unwrap();
        let json = project.to_json();
        let motion = &json[json.find("\"motion\"").unwrap()..][..300];
        assert!(motion.contains("-1.2345678901234568e-300"), "{motion}");
        let (bound, len) = (project.json_bound(), json.len());
        assert!(bound >= len, "{bound} < {len}");
    }
}
