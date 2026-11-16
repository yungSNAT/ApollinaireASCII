// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Keyframes: a layer's position, scale, rotation and anchor keyed at frames and eased between
//! them. A layer draws through its [`Motion`] in every export and command that reads it.
//!
//! ```
//! use apollinaire_ascii::{Frame, motion::{Easing, Key, Motion, Property}};
//! let mut motion = Motion::default();
//! motion.set_key(Property::X, Key { frame: 0, value: 0.0, easing: Easing::Linear })?;
//! motion.set_key(Property::X, Key { frame: 4, value: 4.0, easing: Easing::Linear })?;
//! assert_eq!(motion.value_at(Property::X, 2), Some(2.0));
//! let bird = Frame::from_text("v    ")?;
//! assert_eq!(motion.draw(&bird, 2).text(), "  v  ");
//! # Ok::<(), String>(())
//! ```

use crate::{Frame, Result};
use std::ops::RangeInclusive;

/// What a key sets. Positions and anchors are in columns across and rows down, and may hold
/// fractions of a cell; the anchor is a point, so `(0, 0)` is the top-left corner of the
/// first cell. Scales are in percent and rotation in degrees, clockwise as seen on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Property {
    X,
    Y,
    ScaleX,
    ScaleY,
    Rotation,
    AnchorX,
    AnchorY,
}

impl Property {
    pub const ALL: [Self; 7] = [
        Self::X,
        Self::Y,
        Self::ScaleX,
        Self::ScaleY,
        Self::Rotation,
        Self::AnchorX,
        Self::AnchorY,
    ];
    /// The name in project JSON.
    pub fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::ScaleX => "scale_x",
            Self::ScaleY => "scale_y",
            Self::Rotation => "rotation",
            Self::AnchorX => "anchor_x",
            Self::AnchorY => "anchor_y",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
    /// The value without keys, for art `width` by `height`: no move, full size, no turn and
    /// the anchor at the centre.
    pub fn default_value(self, width: usize, height: usize) -> f64 {
        match self {
            Self::ScaleX | Self::ScaleY => 100.0,
            Self::AnchorX => width as f64 / 2.0,
            Self::AnchorY => height as f64 / 2.0,
            _ => 0.0,
        }
    }
    /// The values a key may hold.
    pub fn range(self) -> RangeInclusive<f64> {
        match self {
            Self::ScaleX | Self::ScaleY => 0.0..=1000.0,
            Self::Rotation => -3600.0..=3600.0,
            _ => -1024.0..=1024.0,
        }
    }
}

/// How a value moves from a key to the next one. The presets are cubic bezier curves, named
/// and shaped as in CSS and common animation tools; `Hold` keeps the value until the next key.
#[derive(Clone, Copy, Debug)]
pub enum Easing {
    Linear,
    Hold,
    EaseIn,
    EaseOut,
    EaseInOut,
    EaseInBack,
    EaseOutBack,
    Bounce,
    /// Control points `[x1, y1, x2, y2]` of a cubic bezier from (0, 0) to (1, 1); made by
    /// [`Easing::curve`], which keeps x in 0 to 1 and y in -5 to 5.
    Curve([f64; 4]),
}

impl PartialEq for Easing {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            // Curves hold finite numbers only, so comparing bits is equality.
            (Self::Curve(a), Self::Curve(b)) => a.map(f64::to_bits) == b.map(f64::to_bits),
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}
impl Eq for Easing {}

impl Easing {
    pub const PRESETS: [Self; 8] = [
        Self::Linear,
        Self::Hold,
        Self::EaseIn,
        Self::EaseOut,
        Self::EaseInOut,
        Self::EaseInBack,
        Self::EaseOutBack,
        Self::Bounce,
    ];
    /// A custom curve; x values must be 0 to 1 and y values -5 to 5.
    pub fn curve(points: [f64; 4]) -> Result<Self> {
        let [x1, y1, x2, y2] = points;
        if [x1, x2].iter().all(|x| (0.0..=1.0).contains(x))
            && [y1, y2].iter().all(|y| (-5.0..=5.0).contains(y))
        {
            Ok(Self::Curve(points))
        } else {
            Err("An easing curve needs x1 and x2 from 0 to 1 and y1 and y2 from -5 to 5".into())
        }
    }
    /// The preset's name in project JSON, or `None` for a custom curve.
    pub fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::Linear => "linear",
            Self::Hold => "hold",
            Self::EaseIn => "ease-in",
            Self::EaseOut => "ease-out",
            Self::EaseInOut => "ease-in-out",
            Self::EaseInBack => "ease-in-back",
            Self::EaseOutBack => "ease-out-back",
            Self::Bounce => "bounce",
            Self::Curve(_) => return None,
        })
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::PRESETS.into_iter().find(|e| e.name() == Some(name))
    }
    /// The bezier control points, or `None` for `Hold`.
    pub fn points(self) -> Option<[f64; 4]> {
        Some(match self {
            Self::Linear => [0.0, 0.0, 1.0, 1.0],
            Self::Hold => return None,
            Self::EaseIn => [0.42, 0.0, 1.0, 1.0],
            Self::EaseOut => [0.0, 0.0, 0.58, 1.0],
            Self::EaseInOut => [0.42, 0.0, 0.58, 1.0],
            Self::EaseInBack => [0.36, 0.0, 0.66, -0.56],
            Self::EaseOutBack => [0.34, 1.56, 0.64, 1.0],
            Self::Bounce => [0.34, 1.4, 0.64, 1.0],
            Self::Curve(points) => points,
        })
    }
    /// Progress from 0 to 1 through a segment, eased; it may leave 0 to 1 to overshoot.
    pub fn ease(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self.points() {
            None => 0.0,
            Some(_) if self == Self::Linear => t,
            Some([x1, y1, x2, y2]) => bezier(solve(t, x1, x2), y1, y2),
        }
    }
}

/// One coordinate of a cubic bezier from 0 to 1 with inner control values `a` and `b`.
fn bezier(u: f64, a: f64, b: f64) -> f64 {
    let v = 1.0 - u;
    3.0 * v * v * u * a + 3.0 * v * u * u * b + u * u * u
}
/// The curve parameter whose x is `x`. With control x values in 0 to 1 the x coordinate never
/// falls, so bisection always converges; Newton steps reach it faster where the slope allows.
fn solve(x: f64, x1: f64, x2: f64) -> f64 {
    let mut u = x;
    for _ in 0..8 {
        let error = bezier(u, x1, x2) - x;
        let v = 1.0 - u;
        let slope = 3.0 * v * v * x1 + 6.0 * v * u * (x2 - x1) + 3.0 * u * u * (1.0 - x2);
        if error.abs() < 1e-9 {
            return u;
        }
        if slope.abs() < 1e-6 {
            break;
        }
        u = (u - error / slope).clamp(0.0, 1.0);
    }
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..60 {
        u = (low + high) / 2.0;
        if bezier(u, x1, x2) < x {
            low = u;
        } else {
            high = u;
        }
    }
    u
}

#[derive(Clone, Copy, Debug)]
pub struct Key {
    /// Zero-based frame index.
    pub frame: usize,
    /// Inside the property's [`range`](Property::range).
    pub value: f64,
    /// How the value moves from this key to the next one.
    pub easing: Easing,
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        // Stored values are finite and never -0.0, so comparing bits is equality.
        self.frame == other.frame
            && self.value.to_bits() == other.value.to_bits()
            && self.easing == other.easing
    }
}
impl Eq for Key {}

/// A layer's keys, one track per [`Property`], each sorted by frame with one key per frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Motion {
    tracks: [Vec<Key>; 7],
}

/// Rows are drawn about twice as tall as columns are wide, so rotation works in a space where a
/// cell is 1 wide and 2 tall, and a rotated square stays square on screen.
const CELL_ASPECT: f64 = 2.0;

impl Motion {
    pub fn is_empty(&self) -> bool {
        self.tracks.iter().all(Vec::is_empty)
    }
    pub fn keys(&self, property: Property) -> &[Key] {
        &self.tracks[property as usize]
    }
    /// The number of keys on every track.
    pub fn len(&self) -> usize {
        self.tracks.iter().map(Vec::len).sum()
    }
    /// Add a key, replacing any key on the same frame. The project checks the frame.
    pub fn set_key(&mut self, property: Property, mut key: Key) -> Result<()> {
        let range = property.range();
        if !range.contains(&key.value) {
            return Err(format!(
                "A {} key needs a value from {} to {}",
                property.name(),
                range.start(),
                range.end()
            ));
        }
        if let Easing::Curve(points) = key.easing {
            Easing::curve(points)?;
        }
        // -0.0 becomes 0.0, so equal keys compare equal.
        key.value += 0.0;
        let track = &mut self.tracks[property as usize];
        match track.binary_search_by_key(&key.frame, |k| k.frame) {
            Ok(index) => track[index] = key,
            Err(index) => track.insert(index, key),
        }
        Ok(())
    }
    /// Remove the key on `frame`, returning whether there was one.
    pub fn remove_key(&mut self, property: Property, frame: usize) -> bool {
        let track = &mut self.tracks[property as usize];
        match track.binary_search_by_key(&frame, |k| k.frame) {
            Ok(index) => {
                track.remove(index);
                true
            }
            Err(_) => false,
        }
    }
    /// The last frame any key is on.
    pub(crate) fn last_frame(&self) -> Option<usize> {
        self.tracks
            .iter()
            .filter_map(|t| t.last())
            .map(|k| k.frame)
            .max()
    }
    /// Move every key to `map(frame)`, dropping keys it maps to `None`, then sort each track.
    /// `map` must send distinct frames to distinct frames, as moving whole frames does.
    pub(crate) fn remap(&mut self, map: impl Fn(usize) -> Option<usize>) {
        for track in &mut self.tracks {
            *track = track
                .iter()
                .filter_map(|key| map(key.frame).map(|frame| Key { frame, ..*key }))
                .collect();
            track.sort_by_key(|key| key.frame);
        }
    }
    /// The property's value at a frame, or `None` when the property has no keys. Before the first
    /// key it holds the first key's value and after the last key the last one's; between 2 keys
    /// it moves by the earlier key's easing.
    pub fn value_at(&self, property: Property, frame: usize) -> Option<f64> {
        let track = self.keys(property);
        let first = track.first()?;
        let next = track.partition_point(|key| key.frame <= frame);
        let Some(key) = next.checked_sub(1).map(|index| track[index]) else {
            return Some(first.value);
        };
        let Some(end) = track.get(next) else {
            return Some(key.value);
        };
        let t = (frame - key.frame) as f64 / (end.frame - key.frame) as f64;
        Some(key.value + (end.value - key.value) * key.easing.ease(t))
    }
    /// The frame drawn through the motion at frame index `index`: each cell shows the cell of
    /// `art` that the transform puts there, spaces elsewhere. Rotation turns the line characters
    /// `-`, `\`, `|` and `/` in 45 degree steps so lines keep their direction; a turn halfway
    /// between 2 steps, such as 22.5 degrees, rounds away from zero. Without keys the frame is
    /// returned unchanged.
    pub fn draw(&self, art: &Frame, index: usize) -> Frame {
        if self.is_empty() {
            return art.clone();
        }
        let (width, height) = (art.width(), art.height());
        let value = |property: Property| {
            self.value_at(property, index)
                .unwrap_or_else(|| property.default_value(width, height))
        };
        let (x, y) = (value(Property::X), value(Property::Y));
        let scale_x = value(Property::ScaleX) / 100.0;
        let scale_y = value(Property::ScaleY) / 100.0;
        let degrees = value(Property::Rotation);
        let anchor_x = value(Property::AnchorX);
        let anchor_y = value(Property::AnchorY);
        let mut cells = vec![b' '; width * height];
        if scale_x > 0.0 && scale_y > 0.0 {
            let (sin, cos) = turn(degrees);
            let steps = (degrees / 45.0).round().rem_euclid(4.0) as usize;
            for (row, line) in cells.chunks_mut(width).enumerate() {
                let dy = (row as f64 + 0.5 - y - anchor_y) * CELL_ASPECT;
                for (column, cell) in line.iter_mut().enumerate() {
                    let dx = column as f64 + 0.5 - x - anchor_x;
                    // Undo the rotation, then the scale, to find the source cell.
                    let source_x = (dx * cos + dy * sin) / scale_x + anchor_x;
                    let source_y = (dy * cos - dx * sin) / (scale_y * CELL_ASPECT) + anchor_y;
                    if (0.0..width as f64).contains(&source_x)
                        && (0.0..height as f64).contains(&source_y)
                    {
                        let glyph = art.cells()[source_y as usize * width + source_x as usize];
                        *cell = turned(glyph, steps, false);
                    }
                }
            }
        }
        Frame::new(width, height, cells).expect("Cells come from a valid frame")
    }
}

/// The sine and cosine of a clockwise turn, exact at multiples of 90 degrees so those turns
/// sample the same cells on every platform.
fn turn(degrees: f64) -> (f64, f64) {
    if degrees % 90.0 == 0.0 {
        match (degrees / 90.0).rem_euclid(4.0) as u8 {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        }
    } else {
        degrees.to_radians().sin_cos()
    }
}

/// A line character turned clockwise by `steps` of 45 degrees and, with `arrows`, an
/// arrowhead turned by whole quarter turns (`steps` rounded down to a multiple of 2).
pub(crate) fn turned(glyph: u8, steps: usize, arrows: bool) -> u8 {
    const LINES: &[u8; 4] = b"-\\|/";
    const ARROWS: &[u8; 4] = b"<^>v";
    if let Some(index) = LINES.iter().position(|&line| line == glyph) {
        return LINES[(index + steps) % 4];
    }
    match ARROWS.iter().position(|&arrow| arrow == glyph) {
        Some(index) if arrows => ARROWS[(index + steps / 2) % 4],
        _ => glyph,
    }
}
