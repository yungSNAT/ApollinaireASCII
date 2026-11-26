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
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
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
            shape.check()?;
            if at.0.saturating_add(shape.width) > width
                || at.1.saturating_add(shape.height) > height
            {
                return Err(format!(
                    "Element e{id} would leave the {width} x {height} frame"
                ));
            }
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
mod tests {
    use super::*;

    fn shape(kind: Kind, width: usize, height: usize, text: &str) -> Shape {
        Shape {
            kind,
            width,
            height,
            style: Style::Plain,
            arrow: Arrow::None,
            text: text.into(),
        }
    }

    fn add(
        art: &Frame,
        elements: &[Element],
        id: u32,
        at: (usize, usize),
        s: Shape,
    ) -> (Frame, Vec<Element>) {
        apply(art, elements, 0, Edit::Add { id, at, shape: s }, false).unwrap()
    }

    #[test]
    fn shapes_render_in_printable_ascii() {
        let mut boxed = shape(Kind::Box, 6, 3, "web");
        assert_eq!(boxed.render().text(), "+----+\n|web |\n+----+");
        boxed.style = Style::Rounded;
        assert_eq!(boxed.render().text(), ".----.\n|web |\n'----'");
        boxed.style = Style::Heavy;
        assert_eq!(boxed.render().text(), "#====#\n#web #\n#====#");
        boxed.style = Style::Dotted;
        assert_eq!(boxed.render().text(), "......\n:web :\n:....:");
        assert_eq!(shape(Kind::Box, 2, 2, "").render().text(), "++\n++");
        let mut line = shape(Kind::Line, 4, 1, "");
        line.arrow = Arrow::End;
        assert_eq!(line.render().text(), "--->");
        line = shape(Kind::Line, 1, 3, "");
        line.arrow = Arrow::Both;
        line.style = Style::Dotted;
        assert_eq!(line.render().text(), "^\n:\nv");
        assert_eq!(
            shape(Kind::Text, 3, 2, "ab\nabc").render().text(),
            "ab \nabc"
        );
    }

    #[test]
    fn shapes_that_cannot_be_drawn_are_refused() {
        let cases = [
            (shape(Kind::Box, 1, 3, ""), "at least 2"),
            (
                shape(Kind::Box, 4, 3, "hello"),
                "needs 5 x 1 inside the box; it has 2 x 1",
            ),
            (shape(Kind::Text, 2, 1, "abc"), "size is its text's"),
            (shape(Kind::Line, 2, 2, ""), "1 row high or 1 column wide"),
            (shape(Kind::Line, 3, 1, "x"), "no text"),
            (shape(Kind::Text, 1, 1, "\u{e9}"), "printable"),
        ];
        for (s, message) in cases {
            let error = s.check().unwrap_err();
            assert!(error.contains(message), "{error}");
        }
        let mut arrowed = shape(Kind::Box, 3, 3, "");
        arrowed.arrow = Arrow::End;
        assert!(arrowed.check().unwrap_err().contains("Only a line"));
        let mut both = shape(Kind::Line, 1, 1, "");
        both.arrow = Arrow::Both;
        assert!(both.check().unwrap_err().contains("at least 2"));
        let mut rounded = shape(Kind::Line, 3, 1, "");
        rounded.style = Style::Rounded;
        assert!(rounded.check().is_err());
    }

    #[test]
    fn ids_parse_only_as_e_and_digits() {
        assert_eq!(parse_id("e12"), Ok(12));
        for bad in ["e0", "e", "12", "E1", "e1x", "e-1", "e99999999999"] {
            assert!(parse_id(bad).is_err(), "{bad}");
        }
        assert_eq!(Kind::from_name("line"), Ok(Kind::Line));
        assert!(Style::from_name("fancy").unwrap_err().contains("plain"));
    }

    #[test]
    fn moving_and_deleting_put_back_the_cells_underneath() {
        let art = Frame::from_text("abcdefgh\nijklmnop\nqrstuvwx").unwrap();
        let (art, elements) = add(&art, &[], 1, (1, 0), shape(Kind::Box, 4, 3, "hi"));
        assert_eq!(art.text(), "a+--+fgh\ni|hi|nop\nq+--+vwx");
        // A second element over the first.
        let (art, elements) = add(&art, &elements, 2, (3, 1), shape(Kind::Text, 3, 1, "XYZ"));
        assert_eq!(art.text(), "a+--+fgh\ni|hXYZop\nq+--+vwx");
        // Moving the lower one lifts the upper, puts back the letters and draws both again.
        let moved = Edit::Change {
            id: 1,
            at: (4, 0),
            shape: shape(Kind::Box, 4, 3, "hi"),
        };
        let (art, elements) = apply(&art, &elements, 0, moved, false).unwrap();
        assert_eq!(art.text(), "abcd+--+\nijkXYZi|\nqrst+--+");
        assert_eq!(drift(&art, &elements, 0), vec![(1, 0), (2, 0)]);
        // Deleting both brings back the original exactly.
        let (art, elements) = apply(&art, &elements, 0, Edit::Delete { id: 2 }, false).unwrap();
        let (art, elements) = apply(&art, &elements, 0, Edit::Delete { id: 1 }, false).unwrap();
        assert_eq!(art.text(), "abcdefgh\nijklmnop\nqrstuvwx");
        assert!(elements.is_empty());
    }

    #[test]
    fn hand_changes_are_refused_unless_forced_and_cells_outside_stay() {
        let art = Frame::from_text("........\n........").unwrap();
        let (art, elements) = add(&art, &[], 1, (0, 0), shape(Kind::Text, 3, 1, "abc"));
        let (art, elements) = add(&art, &elements, 2, (4, 0), shape(Kind::Text, 2, 1, "zz"));
        // Hand edits: one cell of e1, one cell no element covers.
        let mut cells = art.cells().to_vec();
        cells[1] = b'#';
        cells[9] = b'@';
        let art = Frame::new(8, 2, cells).unwrap();
        assert_eq!(drift(&art, &elements, 0), vec![(1, 1), (2, 0)]);
        let delete = || Edit::Delete { id: 1 };
        let error = apply(&art, &elements, 0, delete(), false).unwrap_err();
        assert!(
            error.contains("e1 was changed by hand in 1 cells"),
            "{error}"
        );
        // e2 is above e1 in drawing order, so a hand change to it blocks e1's edit too.
        let mut cells = art.cells().to_vec();
        cells[1] = b'b';
        cells[4] = b'!';
        let art = Frame::new(8, 2, cells).unwrap();
        let error = apply(&art, &elements, 0, delete(), false).unwrap_err();
        assert!(error.contains("e2 above e1"), "{error}");
        let (art, elements) = apply(&art, &elements, 0, delete(), true).unwrap();
        assert_eq!(art.text(), "....zz..\n.@......");
        assert_eq!(elements.len(), 1);
    }

    #[test]
    fn edits_check_ids_frames_and_bounds() {
        let art = Frame::blank(5, 2).unwrap();
        let error = apply(&art, &[], 0, Edit::Delete { id: 3 }, false).unwrap_err();
        assert!(error.contains("No element e3"), "{error}");
        let error = apply(
            &art,
            &[],
            0,
            Edit::Add {
                id: 1,
                at: (3, 0),
                shape: shape(Kind::Box, 3, 2, ""),
            },
            false,
        )
        .unwrap_err();
        assert_eq!(error, "Element e1 would leave the 5 x 2 frame");
        // An element on another timeline frame is not found on this one.
        let (_, elements) = add(&art, &[], 1, (0, 0), shape(Kind::Text, 1, 1, "x"));
        let mut elements = elements;
        elements[0].set_frame(1);
        assert!(apply(&art, &elements, 0, Edit::Delete { id: 1 }, false).is_err());
        assert!(
            elements[0].fits(5, 2, 2) && !elements[0].fits(5, 2, 1) && !elements[0].fits(0, 2, 2)
        );
    }
}
