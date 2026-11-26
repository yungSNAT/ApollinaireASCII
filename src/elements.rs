// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Named elements: boxes, text blocks and lines drawn into a layer's frame and kept beside its
//! cells, so `apollinaire mcp --live` can move, resize, restyle, retext or delete one by id.
//!
//! The cells stay the picture. Each element remembers the cells it covered when it was drawn
//! (`under`). An edit lifts the elements at and above the one changed, topmost first, putting
//! back what each covered, then draws them again; cells no element covers are never touched.
//! An element whose cells no longer match its drawing (changed by hand) is drifted, and an
//! edit that would lift it is refused unless forced.

use crate::{Frame, Result};

/// Elements a project holds at most.
pub const MAX_ELEMENTS: usize = 4096;
/// Cells a project's elements cover at most, counted once per element, as every element keeps
/// a copy of the cells under it.
pub const MAX_ELEMENT_CELLS: usize = 2_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Box,
    Text,
    Line,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Plain,
    Rounded,
    Dotted,
    Heavy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrow {
    None,
    Start,
    End,
    Both,
}

macro_rules! names {
    ($type:ty, $what:literal, $list:literal, $($value:ident = $name:literal),+) => {
        impl $type {
            pub fn name(self) -> &'static str {
                match self { $(Self::$value => $name),+ }
            }
            pub fn from_name(name: &str) -> Result<Self> {
                match name {
                    $($name => Ok(Self::$value),)+
                    _ => Err(format!("Unknown {} {name:?}; use {}", $what, $list)),
                }
            }
        }
    };
}
names!(
    Kind,
    "kind",
    "box, text or line",
    Box = "box",
    Text = "text",
    Line = "line"
);
names!(
    Style,
    "style",
    "plain, rounded, dotted or heavy",
    Plain = "plain",
    Rounded = "rounded",
    Dotted = "dotted",
    Heavy = "heavy"
);
names!(
    Arrow,
    "arrow",
    "none, start, end or both",
    None = "none",
    Start = "start",
    End = "end",
    Both = "both"
);

/// What an element looks like, without its place in the project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    pub kind: Kind,
    pub width: usize,
    pub height: usize,
    pub style: Style,
    pub arrow: Arrow,
    /// A box's text, from its top-left inner cell, or a text element's rows; empty for a line.
    pub text: String,
}

impl Shape {
    /// Refuses a shape that cannot be drawn as asked.
    pub fn check(&self) -> Result<()> {
        let (w, h) = (self.width, self.height);
        crate::dimensions(w, h)?;
        if self.kind != Kind::Line && self.arrow != Arrow::None {
            return Err("Only a line has arrows".into());
        }
        match self.kind {
            Kind::Box => {
                if w < 2 || h < 2 {
                    return Err("A box needs width and height of at least 2".into());
                }
                if !self.text.is_empty() {
                    let (tw, th) = Frame::padded_dimensions(&self.text)?;
                    if tw > w - 2 || th > h - 2 {
                        return Err(format!(
                            "The text needs {tw} x {th} inside the box; it has {} x {}",
                            w - 2,
                            h - 2
                        ));
                    }
                }
            }
            Kind::Text => {
                if self.style != Style::Plain {
                    return Err("A text element has no style".into());
                }
                if Frame::padded_dimensions(&self.text)? != (w, h) {
                    return Err("A text element's size is its text's".into());
                }
            }
            Kind::Line => {
                if w != 1 && h != 1 {
                    return Err("A line is 1 row high or 1 column wide".into());
                }
                if self.style == Style::Rounded {
                    return Err("A line is plain, dotted or heavy".into());
                }
                if !self.text.is_empty() {
                    return Err("A line has no text".into());
                }
                if self.arrow == Arrow::Both && w.max(h) < 2 {
                    return Err("A line with 2 arrows needs a length of at least 2".into());
                }
            }
        }
        Ok(())
    }

    /// The cells the shape draws, row by row, all printable ASCII.
    pub fn render(&self) -> Frame {
        let (w, h) = (self.width, self.height);
        let mut cells = vec![b' '; w * h];
        match self.kind {
            Kind::Box => {
                let [top, bottom, across, side] = match self.style {
                    Style::Plain => *b"++-|",
                    Style::Rounded => *b".'-|",
                    Style::Dotted => *b".:.:",
                    Style::Heavy => *b"##=#",
                };
                for x in 0..w {
                    cells[x] = across;
                    cells[(h - 1) * w + x] = across;
                }
                for y in 0..h {
                    cells[y * w] = side;
                    cells[y * w + w - 1] = side;
                }
                for x in [0, w - 1] {
                    cells[x] = top;
                    cells[(h - 1) * w + x] = bottom;
                }
                if !self.text.is_empty() {
                    let text = Frame::from_text_padded(&self.text).expect("checked shape");
                    paste(&mut cells, w, 1, 1, &text);
                }
            }
            Kind::Text => {
                cells = Frame::from_text_padded(&self.text)
                    .expect("checked shape")
                    .cells()
                    .to_vec();
            }
            Kind::Line => {
                let vertical = h > 1;
                let (body, start, end) = match (vertical, self.style) {
                    (false, Style::Dotted) => (b'.', b'<', b'>'),
                    (false, Style::Heavy) => (b'=', b'<', b'>'),
                    (false, _) => (b'-', b'<', b'>'),
                    (true, Style::Dotted) => (b':', b'^', b'v'),
                    (true, Style::Heavy) => (b'#', b'^', b'v'),
                    (true, _) => (b'|', b'^', b'v'),
                };
                cells.fill(body);
                let last = cells.len() - 1;
                if matches!(self.arrow, Arrow::Start | Arrow::Both) {
                    cells[0] = start;
                }
                if matches!(self.arrow, Arrow::End | Arrow::Both) {
                    cells[last] = end;
                }
            }
        }
        Frame::new(w, h, cells).expect("printable cells")
    }
}

/// An element in a layer: its id, timeline frame, top-left cell, shape and the cells it
/// covered when last drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    id: u32,
    frame: usize,
    x: usize,
    y: usize,
    shape: Shape,
    under: Frame,
}

impl Element {
    /// An element as a project file holds it. `under` is `None` for all spaces.
    pub(crate) fn parsed(
        id: u32,
        frame: usize,
        (x, y): (usize, usize),
        shape: Shape,
        under: Option<Frame>,
    ) -> Result<Self> {
        shape.check()?;
        let under = match under {
            Some(under) if (under.width(), under.height()) == (shape.width, shape.height) => under,
            Some(_) => return Err(format!("Element e{id}'s under is not its size")),
            None => Frame::blank(shape.width, shape.height)?,
        };
        if id == 0 {
            return Err("Element ids start at e1".into());
        }
        Ok(Self {
            id,
            frame,
            x,
            y,
            shape,
            under,
        })
    }
    pub fn id(&self) -> u32 {
        self.id
    }
    /// `e` and the id, as tools and files name it.
    pub fn name(&self) -> String {
        format!("e{}", self.id)
    }
    pub fn frame(&self) -> usize {
        self.frame
    }
    pub fn x(&self) -> usize {
        self.x
    }
    pub fn y(&self) -> usize {
        self.y
    }
    pub fn shape(&self) -> &Shape {
        &self.shape
    }
    /// The cells it covered when last drawn.
    pub fn under(&self) -> &Frame {
        &self.under
    }
    /// Whether it lies inside a project of this size and frame count.
    pub fn fits(&self, width: usize, height: usize, frames: usize) -> bool {
        self.frame < frames
            && self.x.saturating_add(self.shape.width) <= width
            && self.y.saturating_add(self.shape.height) <= height
    }
    pub(crate) fn set_frame(&mut self, frame: usize) {
        self.frame = frame;
    }
}

/// Parse `e<id>`.
pub fn parse_id(name: &str) -> Result<u32> {
    name.strip_prefix('e')
        // One spelling for each id: no sign and no leading zero.
        .filter(|digits| {
            !digits.starts_with('0')
                && !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit())
        })
        .and_then(|digits| digits.parse().ok())
        .filter(|&id| id > 0)
        .ok_or_else(|| format!("{name:?} is not an element id like e1"))
}

/// One change to the elements of a frame.
pub enum Edit {
    /// Draw a new element, given its id, on top.
    Add {
        id: u32,
        at: (usize, usize),
        shape: Shape,
    },
    /// Move, resize, restyle or retext an element where it is in the drawing order.
    Change {
        id: u32,
        at: (usize, usize),
        shape: Shape,
    },
    Delete {
        id: u32,
    },
}

/// The frame and the layer's elements after `edit` on timeline frame `frame`, or why not. An
/// element that would be lifted and was changed by hand is refused unless `force`.
pub fn apply(
    art: &Frame,
    elements: &[Element],
    frame: usize,
    edit: Edit,
    force: bool,
) -> Result<(Frame, Vec<Element>)> {
    let (width, height) = (art.width(), art.height());
    if let Edit::Add { id, at, shape } | Edit::Change { id, at, shape } = &edit {
        shape.check()?;
        if at.0.saturating_add(shape.width) > width || at.1.saturating_add(shape.height) > height {
            return Err(format!(
                "Element e{id} would leave the {width} x {height} frame"
            ));
        }
    }
    let mut cells = art.cells().to_vec();
    let mut elements = elements.to_vec();
    let target = match &edit {
        Edit::Add { .. } => None,
        Edit::Change { id, .. } | Edit::Delete { id } => Some(*id),
    };
    // Where lifting starts: the target, or past the end for a new element.
    let from = match target {
        Some(id) => elements
            .iter()
            .position(|e| e.id == id && e.frame == frame)
            .ok_or_else(|| {
                format!(
                    "No element e{id}; live_elements lists the current ones (ids are never reused)"
                )
            })?,
        None => elements.len(),
    };
    for element in elements[from..].iter().rev().filter(|e| e.frame == frame) {
        let drift = drifted(&cells, width, element);
        if drift > 0 && !force {
            return Err(match target {
                Some(id) if id != element.id => format!(
                    "Element {} above e{id} was changed by hand in {drift} cells; pass force: true to redraw over them",
                    element.name()
                ),
                _ => format!(
                    "Element {} was changed by hand in {drift} cells; pass force: true to redraw over them",
                    element.name()
                ),
            });
        }
        paste(&mut cells, width, element.x, element.y, &element.under);
    }
    match edit {
        Edit::Add { id, at, shape } | Edit::Change { id, at, shape } => {
            let under = Frame::blank(shape.width, shape.height)?;
            let element = Element {
                id,
                frame,
                x: at.0,
                y: at.1,
                shape,
                under,
            };
            match target {
                Some(_) => elements[from] = element,
                None => elements.push(element),
            }
        }
        Edit::Delete { .. } => {
            elements.remove(from);
        }
    }
    for element in elements[from..].iter_mut().filter(|e| e.frame == frame) {
        let (w, h) = (element.shape.width, element.shape.height);
        element.under = crop(&cells, width, element.x, element.y, w, h);
        paste(
            &mut cells,
            width,
            element.x,
            element.y,
            &element.shape.render(),
        );
    }
    Ok((Frame::new(width, height, cells)?, elements))
}

/// Cells of each element of timeline frame `frame` that differ from its drawing, by element
/// id, lifting the elements above it first.
pub fn drift(art: &Frame, elements: &[Element], frame: usize) -> Vec<(u32, usize)> {
    let width = art.width();
    let mut cells = art.cells().to_vec();
    let mut found = Vec::new();
    for element in elements.iter().rev().filter(|e| e.frame == frame) {
        found.push((element.id, drifted(&cells, width, element)));
        paste(&mut cells, width, element.x, element.y, &element.under);
    }
    found.reverse();
    found
}

fn drifted(cells: &[u8], width: usize, element: &Element) -> usize {
    let drawn = element.shape.render();
    let w = drawn.width();
    (0..drawn.height())
        .map(|row| {
            let at = (element.y + row) * width + element.x;
            cells[at..at + w]
                .iter()
                .zip(&drawn.cells()[row * w..row * w + w])
                .filter(|(a, b)| a != b)
                .count()
        })
        .sum()
}

fn paste(cells: &mut [u8], width: usize, x: usize, y: usize, block: &Frame) {
    let w = block.width();
    for row in 0..block.height() {
        let at = (y + row) * width + x;
        cells[at..at + w].copy_from_slice(&block.cells()[row * w..row * w + w]);
    }
}

fn crop(cells: &[u8], width: usize, x: usize, y: usize, w: usize, h: usize) -> Frame {
    let rows = (0..h).flat_map(|row| {
        let at = (y + row) * width + x;
        cells[at..at + w].iter().copied()
    });
    Frame::new(w, h, rows.collect()).expect("cells from a frame")
}

#[cfg(test)]
mod tests;
