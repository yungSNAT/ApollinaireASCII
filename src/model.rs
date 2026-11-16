// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! 3D models (STL, binary or text, and Wavefront OBJ) drawn as a turning ASCII animation.
//!
//! The model is centred and scaled so it stays inside the frame at every angle, turned a full
//! circle over the frames, and drawn without perspective: each cell shows the nearest surface
//! under its centre, shaded by how directly it faces the light, through a ramp from light to
//! dense. Cells are taken as twice as tall as they are wide. GLTF and GLB are not read.
//!
//! ```
//! use apollinaire_ascii::model::{self, ModelOptions};
//! let solid = "solid t\nfacet normal 0 0 1\nouter loop\nvertex -1 -1 0\nvertex 1 -1 0\n\
//!              vertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
//! let mesh = model::read("triangle.stl", solid.as_bytes())?;
//! let options = ModelOptions { width: 20, height: 10, frames: 4, ..Default::default() };
//! let turning = model::render(&mesh, &options)?;
//! assert_eq!(turning.frames().len(), 4);
//! # Ok::<(), String>(())
//! ```
use crate::{Animation, Frame, MAX_FRAMES, MAX_PROJECT_CELLS, Result, TimedFrame};
use crate::{dimensions, printable};

/// The most triangles a model may have.
pub const MAX_TRIANGLES: usize = 1_000_000;
/// The most triangles one render draws, counted once for each frame: about 2 seconds of
/// native work (docs/VALIDATION.md).
pub const MAX_DRAWS: usize = 20_000_000;
/// The most cells one render tests against triangles, summed over every triangle's bounding
/// box in every frame: about 2 seconds of native work (docs/VALIDATION.md).
pub const MAX_TESTS: usize = 400_000_000;

type Point = [f64; 3];

/// A model's triangles, each 3 corners.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    triangles: Vec<[[f32; 3]; 3]>,
}
impl Mesh {
    pub fn triangles(&self) -> usize {
        self.triangles.len()
    }
}

/// Read a model by its file name, in any case: `.stl` (binary or text) or `.obj`.
///
/// Refuses a file with no triangles, more than 1,000,000, a coordinate that is not a finite
/// number, a facet without 3 corners, or a face that names a corner the file does not have.
pub fn read(name: &str, bytes: &[u8]) -> Result<Mesh> {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    let triangles = match extension.as_deref() {
        Some("stl") => stl(bytes)?,
        Some("obj") => obj(bytes)?,
        _ => return Err("A model must be an .stl or .obj file".into()),
    };
    if triangles.is_empty() {
        return Err("The model has no triangles".into());
    }
    Ok(Mesh { triangles })
}

fn too_many() -> String {
    format!("The model has more than {MAX_TRIANGLES} triangles")
}

fn stl(bytes: &[u8]) -> Result<Vec<[[f32; 3]; 3]>> {
    // A binary file is an 80-byte header, a count and 50 bytes for each triangle. Its header
    // may begin with "solid" too, so the exact length decides.
    if let Some((head, body)) = bytes.split_at_checked(84) {
        let count = u32::from_le_bytes([head[80], head[81], head[82], head[83]]) as usize;
        if count.checked_mul(50) == Some(body.len()) {
            if count > MAX_TRIANGLES {
                return Err(too_many());
            }
            return body
                .as_chunks::<50>()
                .0
                .iter()
                .map(|record| {
                    let mut triangle = [[0.0; 3]; 3];
                    // The stored normal, the first 12 bytes, is skipped; normals are recomputed.
                    let numbers = record[12..48].as_chunks::<4>().0;
                    for (slot, bytes) in triangle.as_flattened_mut().iter_mut().zip(numbers) {
                        *slot = finite(f32::from_le_bytes(*bytes))?;
                    }
                    Ok(triangle)
                })
                .collect();
        }
    }
    let text = std::str::from_utf8(bytes)
        .ok()
        .filter(|text| text.trim_start().starts_with("solid"))
        .ok_or("Not an STL file: neither a binary STL of the right length nor text starting with solid")?;
    let mut triangles = Vec::new();
    let mut facet: Option<Vec<[f32; 3]>> = None;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let mut words = line.split_whitespace();
        match words.next() {
            Some("facet") => facet = Some(Vec::with_capacity(3)),
            Some("vertex") => facet
                .as_mut()
                .ok_or_else(|| format!("Line {number} has a vertex outside a facet"))?
                .push(coordinates(&mut words, number)?),
            Some("endfacet") => {
                let corners = facet
                    .take()
                    .ok_or_else(|| format!("Line {number} ends a facet that did not begin"))?;
                let triangle: [[f32; 3]; 3] = corners.try_into().map_err(|corners: Vec<_>| {
                    format!(
                        "The facet ending on line {number} has {} corners; an STL facet has 3",
                        corners.len()
                    )
                })?;
                if triangles.len() == MAX_TRIANGLES {
                    return Err(too_many());
                }
                triangles.push(triangle);
            }
            _ => {}
        }
    }
    if facet.is_some() {
        return Err("The STL file ends inside a facet".into());
    }
    Ok(triangles)
}

fn obj(bytes: &[u8]) -> Result<Vec<[[f32; 3]; 3]>> {
    let text = std::str::from_utf8(bytes).map_err(|_| "An OBJ file must be text")?;
    let mut corners: Vec<[f32; 3]> = Vec::new();
    // Each face as its line and corner numbers, counted from 1, resolved once every corner is read.
    let mut faces: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut count = 0usize;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let mut words = line.split_whitespace();
        match words.next() {
            Some("v") => corners.push(coordinates(&mut words, number)?),
            Some("f") => {
                let face = words
                    .map(|word| {
                        let position = word.split('/').next().unwrap_or_default();
                        match position.parse::<i64>() {
                            // A negative number counts back from the latest corner.
                            Ok(back @ ..0) => usize::try_from(back.unsigned_abs())
                                .ok()
                                .and_then(|back| (corners.len() + 1).checked_sub(back))
                                .filter(|&at| at > 0),
                            Ok(at) => usize::try_from(at).ok().filter(|&at| at > 0),
                            Err(_) => None,
                        }
                        .ok_or_else(|| {
                            format!("Line {number} has a face corner {word:?} that names no corner")
                        })
                    })
                    .collect::<Result<Vec<usize>>>()?;
                if face.len() < 3 {
                    return Err(format!(
                        "Line {number} has a face with fewer than 3 corners"
                    ));
                }
                count += face.len() - 2;
                if count > MAX_TRIANGLES {
                    return Err(too_many());
                }
                faces.push((number, face));
            }
            _ => {}
        }
    }
    let mut triangles = Vec::with_capacity(count);
    for (number, face) in faces {
        let corner = |at: usize| {
            corners.get(at - 1).copied().ok_or_else(|| {
                format!(
                    "Line {number} names corner {at}, but the file has {}",
                    corners.len()
                )
            })
        };
        // A polygon becomes a fan of triangles from its first corner.
        let first = corner(face[0])?;
        for pair in face[1..].windows(2) {
            triangles.push([first, corner(pair[0])?, corner(pair[1])?]);
        }
    }
    Ok(triangles)
}

/// 3 finite numbers from a vertex line.
fn coordinates<'a>(words: &mut impl Iterator<Item = &'a str>, number: usize) -> Result<[f32; 3]> {
    let mut point = [0.0; 3];
    for axis in &mut point {
        let word = words
            .next()
            .ok_or_else(|| format!("Line {number} has fewer than 3 coordinates"))?;
        let value = word
            .parse::<f32>()
            .map_err(|_| format!("Line {number} has {word:?} where a number belongs"))?;
        *axis = finite(value).map_err(|error| format!("Line {number}: {error}"))?;
    }
    Ok(point)
}

fn finite(value: f32) -> Result<f32> {
    match value.is_finite() {
        true => Ok(value),
        false => Err("a coordinate is not a finite number".into()),
    }
}

/// The axis the model turns about.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spin {
    /// Upright, like a turntable.
    #[default]
    Y,
    /// Head over heels.
    X,
    /// Both at once.
    XY,
}

/// Where the light comes from, as seen from the front.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Light {
    #[default]
    TopLeft,
    Top,
    Front,
    Left,
    Right,
}
impl Light {
    fn direction(self) -> Point {
        let [x, y, z]: Point = match self {
            Self::TopLeft => [-1.0, 1.0, 1.0],
            Self::Top => [0.0, 1.0, 0.4],
            Self::Front => [0.0, 0.0, 1.0],
            Self::Left => [-1.0, 0.0, 0.4],
            Self::Right => [1.0, 0.0, 0.4],
        };
        let length = (x * x + y * y + z * z).sqrt();
        [x / length, y / length, z / length]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelOptions {
    pub width: usize,
    pub height: usize,
    /// Frames for one full turn, 1 to 600.
    pub frames: usize,
    /// Frames per second, 1 to 60.
    pub fps: u32,
    pub spin: Spin,
    pub light: Light,
    /// 2 to 256 printable ASCII characters from light to dense; the first is used only where
    /// there is no surface, so every surface shows.
    pub ramp: String,
}
impl Default for ModelOptions {
    fn default() -> Self {
        Self {
            width: 72,
            height: 24,
            frames: 36,
            fps: 12,
            spin: Spin::Y,
            light: Light::TopLeft,
            ramp: " .:-=+*#%@".into(),
        }
    }
}

/// Draw a model turning one full circle over the frames, looping.
///
/// Refuses options outside their ranges, a model with no size, and a render past
/// [`MAX_DRAWS`] triangles or [`MAX_TESTS`] cell tests, so one render's work is bounded.
pub fn render(mesh: &Mesh, options: &ModelOptions) -> Result<Animation> {
    let (width, height) = (options.width, options.height);
    let cells = dimensions(width, height)?;
    if options.frames == 0
        || options.frames > MAX_FRAMES
        || cells
            .checked_mul(options.frames)
            .is_none_or(|n| n > MAX_PROJECT_CELLS)
    {
        return Err("Animation exceeds frame or cell limits".into());
    }
    if !(1..=60).contains(&options.fps) {
        return Err("FPS must be 1..=60".into());
    }
    let ramp = options.ramp.as_bytes();
    printable(ramp)?;
    if !(2..=256).contains(&ramp.len()) {
        return Err("Ramp needs 2..=256 printable ASCII characters".into());
    }
    if mesh.triangles().saturating_mul(options.frames) > MAX_DRAWS {
        return Err(format!(
            "{} triangles over {} frames is more than the {MAX_DRAWS} triangle draws one render allows; use fewer frames or a simpler model",
            mesh.triangles(),
            options.frames
        ));
    }
    let corners = || mesh.triangles.iter().flatten();
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for corner in corners() {
        for axis in 0..3 {
            low[axis] = low[axis].min(f64::from(corner[axis]));
            high[axis] = high[axis].max(f64::from(corner[axis]));
        }
    }
    let centre: Point = std::array::from_fn(|axis| (low[axis] + high[axis]) / 2.0);
    let radius = corners()
        .map(|corner| {
            (0..3)
                .map(|axis| (f64::from(corner[axis]) - centre[axis]).powi(2))
                .sum::<f64>()
        })
        .fold(0.0, f64::max)
        .sqrt();
    if radius == 0.0 || !radius.is_finite() {
        return Err("The model has no size".into());
    }
    // Columns per model unit: the turning model's circle fits both ways, a row being 2 columns
    // tall.
    let scale = (width as f64 / 2.0).min(height as f64) / radius;
    let light = options.light.direction();
    let turns: Vec<(f64, f64)> = (0..options.frames)
        .map(|index| (index as f64 / options.frames as f64 * std::f64::consts::TAU).sin_cos())
        .collect();
    let turn =
        |corner: &[f32; 3], (sin, cos): (f64, f64)| turned(corner, centre, options.spin, sin, cos);
    // Column, row and depth, a row being 2 columns tall.
    let project = |[x, y, z]: Point| -> Point {
        [
            width as f64 / 2.0 + x * scale,
            height as f64 / 2.0 - y * scale / 2.0,
            z,
        ]
    };
    // Each cell in a triangle's bounding box is tested once. The whole render's tests are
    // counted before any drawing, so a render that would take too long is refused at once.
    let mut tests = 0usize;
    for &pose in &turns {
        for triangle in &mesh.triangles {
            let [a, b, c] = triangle
                .each_ref()
                .map(|corner| project(turn(corner, pose)));
            let columns = span([a[0], b[0], c[0]], width).len();
            tests = tests.saturating_add(columns * span([a[1], b[1], c[1]], height).len());
        }
    }
    if tests > MAX_TESTS {
        return Err(format!(
            "The model covers too much of the frame to draw within {MAX_TESTS} cell tests; use fewer frames or a smaller size"
        ));
    }
    let mut frames = Vec::with_capacity(options.frames);
    for &pose in &turns {
        let mut depth = vec![f64::NEG_INFINITY; cells];
        let mut glyphs = vec![b' '; cells];
        for triangle in &mesh.triangles {
            let turned = triangle.each_ref().map(|corner| turn(corner, pose));
            let [a, b, c] = turned.map(project);
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            let [p, q, r] = turned;
            let (u, v): (Point, Point) = (
                std::array::from_fn(|axis| q[axis] - p[axis]),
                std::array::from_fn(|axis| r[axis] - p[axis]),
            );
            let normal = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let length = normal.iter().map(|n| n * n).sum::<f64>().sqrt();
            // Seen edge on, a triangle covers no cell.
            if area == 0.0 || length == 0.0 {
                continue;
            }
            // Both sides of a surface take light, so a model's winding does not matter.
            let facing = normal[2].signum() / length;
            let lit: f64 = (0..3).map(|axis| normal[axis] * facing * light[axis]).sum();
            let brightness = 0.15 + 0.85 * lit.max(0.0);
            let glyph =
                ramp[1 + ((brightness * (ramp.len() - 1) as f64) as usize).min(ramp.len() - 2)];
            let columns = span([a[0], b[0], c[0]], width);
            for row in span([a[1], b[1], c[1]], height) {
                let py = row as f64 + 0.5;
                for column in columns.clone() {
                    let px = column as f64 + 0.5;
                    let edge = |p: Point, q: Point| {
                        (q[0] - p[0]) * (py - p[1]) - (q[1] - p[1]) * (px - p[0])
                    };
                    let weights = [edge(b, c) / area, edge(c, a) / area, edge(a, b) / area];
                    if weights.iter().any(|&w| w < 0.0) {
                        continue;
                    }
                    let z = weights[0] * a[2] + weights[1] * b[2] + weights[2] * c[2];
                    let cell = row * width + column;
                    if z > depth[cell] {
                        depth[cell] = z;
                        glyphs[cell] = glyph;
                    }
                }
            }
        }
        frames.push(TimedFrame {
            frame: Frame::new(width, height, glyphs)?,
            duration_ms: 1000 / options.fps,
        });
    }
    Animation::new(frames, true)
}

/// A corner turned about the model's centre, in model units, with z towards the viewer.
fn turned(corner: &[f32; 3], centre: Point, spin: Spin, sin: f64, cos: f64) -> Point {
    let [x, y, z]: Point = std::array::from_fn(|axis| f64::from(corner[axis]) - centre[axis]);
    let (x, z) = match spin {
        Spin::X => (x, z),
        Spin::Y | Spin::XY => (x * cos + z * sin, z * cos - x * sin),
    };
    match spin {
        Spin::Y => [x, y, z],
        Spin::X | Spin::XY => [x, y * cos - z * sin, y * sin + z * cos],
    }
}

/// The cells whose centres lie between the lowest and highest of `values`, within `limit`.
fn span(values: [f64; 3], limit: usize) -> std::ops::Range<usize> {
    let low = values.iter().copied().fold(f64::INFINITY, f64::min);
    let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let first = (low - 0.5).ceil().clamp(0.0, limit as f64) as usize;
    let last = ((high - 0.5).floor() + 1.0).clamp(0.0, limit as f64) as usize;
    first..last.max(first)
}
