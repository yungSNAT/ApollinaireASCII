// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Layers: animations drawn separately on one timeline and flattened into one.
//!
//! ```
//! use apollinaire_ascii::{Animation, Frame, layers::{Layer, Layered}};
//! let sky = Animation::still(Frame::from_text("* *")?);
//! let bird = Animation::still(Frame::from_text(" v ")?);
//! let project = Layered::new(vec![
//!     Layer { name: "Sky".into(), visible: true, art: sky },
//!     Layer { name: "Bird".into(), visible: true, art: bird },
//! ])?;
//! assert_eq!(project.flatten().frames()[0].frame.text(), "*v*");
//! assert_eq!(Layered::from_json(&project.to_json())?, project);
//! # Ok::<(), String>(())
//! ```

use crate::{Animation, Frame, MAX_INPUT_BYTES, MAX_PROJECT_CELLS, Result, TimedFrame};
use serde_json::json;

pub const MAX_LAYERS: usize = 16;
/// A one-layer project with this name, visible, is written as a version 1 project.
pub const DEFAULT_NAME: &str = "Layer 1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    /// 1 to 32 printable ASCII characters.
    pub name: String,
    pub visible: bool,
    pub art: Animation,
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
        for layer in &layers {
            check_name(&layer.name)?;
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
            .map(|layer| 2 * layer.name.len() + 32)
            .sum();
        // Per frame: {"duration_ms":60000,"layers":[]}, and the header with its brackets.
        texts + names + 40 * self.layers[0].art.frames().len() + 128
    }
    pub fn single(art: Animation) -> Self {
        Self {
            layers: vec![Layer {
                name: DEFAULT_NAME.into(),
                visible: true,
                art,
            }],
        }
    }
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }
    /// The animation, when the project is one visible layer named [`DEFAULT_NAME`]: the
    /// project a version 1 file holds.
    pub fn plain(&self) -> Option<&Animation> {
        match &self.layers[..] {
            [layer] if layer.visible && layer.name == DEFAULT_NAME => Some(&layer.art),
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
    /// One frame as shown: the visible layers stacked.
    pub fn frame(&self, index: usize) -> Frame {
        let art = &self.layers[0].art;
        stack(
            art.width(),
            art.height(),
            self.layers
                .iter()
                .filter(|layer| layer.visible)
                .map(|layer| &layer.art.frames()[index].frame),
        )
    }
    /// [`flatten`](Self::flatten), moving rather than copying a lone visible layer.
    pub(crate) fn into_flat(mut self) -> Animation {
        match &self.layers[..] {
            [layer] if layer.visible => self.layers.swap_remove(0).art,
            _ => self.flatten(),
        }
    }
    /// The project as shown, for export and every command that reads one animation.
    pub fn flatten(&self) -> Animation {
        let first = &self.layers[0].art;
        if let [layer] = &self.layers[..]
            && layer.visible
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
    /// Version 1 for a [`plain`](Self::plain) project, otherwise version 2.
    pub fn to_json(&self) -> String {
        if let Some(art) = self.plain() {
            return art.to_json();
        }
        let first = &self.layers[0].art;
        json!({"format":"asciiforge","version":2,"width":first.width(),"height":first.height(),
            "loop":first.looping(),
            "layers":self.layers.iter().map(|l| json!({"name":l.name,"visible":l.visible}))
                .collect::<Vec<_>>(),
            "frames":(0..first.frames().len()).map(|index| json!({
                "duration_ms":first.frames()[index].duration_ms,
                "layers":self.layers.iter().map(|l| l.art.frames()[index].frame.text())
                    .collect::<Vec<_>>()})).collect::<Vec<_>>()
        })
        .to_string()
    }
    /// Read a version 1 or version 2 project.
    pub fn from_json(input: &str) -> Result<Self> {
        if input.len() > MAX_INPUT_BYTES {
            return Err("Project exceeds the 16 MiB input limit".into());
        }
        crate::project_json::parse(input)
    }
}

fn check_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 32 || !name.bytes().all(|b| (32..=126).contains(&b)) {
        return Err("A layer name needs 1 to 32 printable ASCII characters".into());
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
