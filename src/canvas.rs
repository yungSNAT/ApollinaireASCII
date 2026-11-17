// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use crate::{Frame, Result, dimensions, printable};

/// Left column, top row, width and height of the rectangle whose opposite corners are `a` and `b`.
pub fn bounds(a: (i32, i32), b: (i32, i32)) -> (i32, i32, usize, usize) {
    (
        a.0.min(b.0),
        a.1.min(b.1),
        (a.0.abs_diff(b.0) as usize).saturating_add(1),
        (a.1.abs_diff(b.1) as usize).saturating_add(1),
    )
}

/// Mutable ASCII drawing surface. Drawing outside the grid is clipped.
#[derive(Clone, Debug)]
pub struct Canvas {
    width: usize,
    height: usize,
    cells: Vec<u8>,
}
impl Canvas {
    pub fn new(width: usize, height: usize) -> Result<Self> {
        Ok(Self {
            width,
            height,
            cells: vec![b' '; dimensions(width, height)?],
        })
    }
    pub fn from_frame(f: &Frame) -> Self {
        Self {
            width: f.width(),
            height: f.height(),
            cells: f.cells().to_vec(),
        }
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    /// The character at a cell, or `None` outside the grid.
    pub fn get(&self, x: i32, y: i32) -> Option<u8> {
        (x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height)
            .then(|| self.cells[y as usize * self.width + x as usize])
    }
    pub fn set(&mut self, x: i32, y: i32, value: u8) -> Result<()> {
        printable(&[value])?;
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            self.cells[y as usize * self.width + x as usize] = value;
        }
        Ok(())
    }
    pub fn text(&mut self, x: i32, y: i32, text: &str) -> Result<()> {
        printable(text.as_bytes())?;
        if text.len() > crate::MAX_CELLS {
            return Err("Text stamp is too long".into());
        }
        for (offset, value) in text.bytes().enumerate() {
            if let Some(px) = x.checked_add(offset as i32) {
                self.set(px, y, value)?;
            }
        }
        Ok(())
    }
    pub fn overlay(
        &mut self,
        other: &Frame,
        x: i32,
        y: i32,
        transparent_spaces: bool,
    ) -> Result<()> {
        for row in 0..other.height() {
            for col in 0..other.width() {
                let value = other.cells()[row * other.width() + col];
                if transparent_spaces && value == b' ' {
                    continue;
                }
                if let (Some(px), Some(py)) = (x.checked_add(col as i32), y.checked_add(row as i32))
                {
                    self.set(px, py, value)?;
                }
            }
        }
        Ok(())
    }
    pub fn line(&mut self, start: (i32, i32), end: (i32, i32), value: u8) -> Result<()> {
        printable(&[value])?;
        if [start.0, start.1, end.0, end.1]
            .iter()
            .any(|v| !(-4096..=4096).contains(v))
        {
            return Err("Line coordinates must be within -4096..=4096".into());
        }
        let (mut x, mut y) = start;
        let dx = (end.0 - x).abs();
        let dy = -(end.1 - y).abs();
        let sx = if x < end.0 { 1 } else { -1 };
        let sy = if y < end.1 { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            self.set(x, y, value)?;
            if (x, y) == end {
                break;
            }
            let twice = error * 2;
            if twice >= dy {
                error += dy;
                x += sx;
            }
            if twice <= dx {
                error += dx;
                y += sy;
            }
        }
        Ok(())
    }
    pub fn rectangle(
        &mut self,
        x: i32,
        y: i32,
        width: usize,
        height: usize,
        value: u8,
    ) -> Result<()> {
        dimensions(width, height)?;
        let right = x
            .checked_add(width as i32 - 1)
            .ok_or("Rectangle coordinate overflow")?;
        let bottom = y
            .checked_add(height as i32 - 1)
            .ok_or("Rectangle coordinate overflow")?;
        for (a, b) in [
            ((x, y), (right, y)),
            ((right, y), (right, bottom)),
            ((right, bottom), (x, bottom)),
            ((x, bottom), (x, y)),
        ] {
            self.line(a, b, value)?;
        }
        Ok(())
    }
    /// Outline the ellipse inscribed in a box, clipped to the grid: the cells whose centres lie
    /// inside it and which touch a cell that does not.
    pub fn ellipse(
        &mut self,
        x: i32,
        y: i32,
        width: usize,
        height: usize,
        value: u8,
    ) -> Result<()> {
        printable(&[value])?;
        dimensions(width, height)?;
        let (w, h) = (width as i64, height as i64);
        // Doubled offsets from the centre keep even sizes whole, and so the outline symmetrical.
        let inside = |column: i64, row: i64| {
            let (dx, dy) = (2 * column - (w - 1), 2 * row - (h - 1));
            dx * dx * h * h + dy * dy * w * w <= w * w * h * h
        };
        for row in 0..h {
            for column in 0..w {
                let interior = inside(column - 1, row)
                    && inside(column + 1, row)
                    && inside(column, row - 1)
                    && inside(column, row + 1);
                if inside(column, row)
                    && !interior
                    && let (Some(px), Some(py)) =
                        (x.checked_add(column as i32), y.checked_add(row as i32))
                {
                    self.set(px, py, value)?;
                }
            }
        }
        Ok(())
    }
    /// Set every cell of a solid rectangle, clipped to the grid.
    pub fn fill_area(
        &mut self,
        x: i32,
        y: i32,
        width: usize,
        height: usize,
        value: u8,
    ) -> Result<()> {
        printable(&[value])?;
        let span = |start: i32, length: usize, limit: usize| {
            let end = i64::from(start).saturating_add(i64::try_from(length).unwrap_or(i64::MAX));
            (i64::from(start).clamp(0, limit as i64) as usize)
                ..(end.clamp(0, limit as i64) as usize)
        };
        let columns = span(x, width, self.width);
        for row in span(y, height, self.height) {
            let start = row * self.width;
            self.cells[start + columns.start..start + columns.end].fill(value);
        }
        Ok(())
    }
    /// Replace the 4-connected run of identical characters containing a cell.
    pub fn flood_fill(&mut self, x: i32, y: i32, value: u8) -> Result<()> {
        printable(&[value])?;
        for index in self.run(x, y) {
            self.cells[index] = value;
        }
        Ok(())
    }
    /// Fill the 4-connected run of identical characters containing `start` with `ramp`: its
    /// first character at `start`, its last at `end` and beyond, and between them by the
    /// distance along the line from `start` to `end`, or, when `radial`, from `start` alone.
    /// Rows count double, as a cell is about twice as tall as it is wide.
    pub fn gradient_fill(
        &mut self,
        start: (i32, i32),
        end: (i32, i32),
        ramp: &[u8],
        radial: bool,
    ) -> Result<()> {
        printable(ramp)?;
        let last = ramp
            .len()
            .checked_sub(1)
            .ok_or("A gradient needs at least one character")?;
        let offset = |x: i32, y: i32| {
            (
                f64::from(x) - f64::from(start.0),
                2.0 * (f64::from(y) - f64::from(start.1)),
            )
        };
        let (dx, dy) = offset(end.0, end.1);
        let reach = dx * dx + dy * dy;
        for index in self.run(start.0, start.1) {
            let (x, y) = offset((index % self.width) as i32, (index / self.width) as i32);
            let share = match (reach == 0.0, radial) {
                (true, _) => 0.0,
                (false, true) => ((x * x + y * y) / reach).sqrt(),
                (false, false) => (x * dx + y * dy) / reach,
            };
            self.cells[index] = ramp[(share.clamp(0.0, 1.0) * last as f64).round() as usize];
        }
        Ok(())
    }
    /// The indices of the 4-connected run of identical characters containing a cell; empty
    /// outside the grid.
    fn run(&self, x: i32, y: i32) -> Vec<usize> {
        let Some(target) = self.get(x, y) else {
            return Vec::new();
        };
        let mut seen = vec![false; self.cells.len()];
        let mut run = Vec::new();
        let mut pending = vec![y as usize * self.width + x as usize];
        while let Some(index) = pending.pop() {
            if seen[index] || self.cells[index] != target {
                continue;
            }
            seen[index] = true;
            run.push(index);
            let (column, row) = (index % self.width, index / self.width);
            if column > 0 {
                pending.push(index - 1);
            }
            if column + 1 < self.width {
                pending.push(index + 1);
            }
            if row > 0 {
                pending.push(index - self.width);
            }
            if row + 1 < self.height {
                pending.push(index + self.width);
            }
        }
        run
    }
    pub fn finish(self) -> Frame {
        Frame::new(self.width, self.height, self.cells).expect("Canvas preserves ASCII invariant")
    }
}
