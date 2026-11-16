// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! ApollinaireASCII lettering and art as a [ratatui](https://ratatui.rs) widget.
//!
//! [`Ascii`] draws any [`Frame`]: lettering in the 24 bundled FIGlet and TOIlet fonts, a
//! catalogue piece, or a frame you made. Every cell it writes is printable ASCII.
//!
//! ```
//! use apollinaire_ratatui::Ascii;
//! use ratatui_core::{buffer::Buffer, layout::{Alignment, Rect}, widgets::Widget};
//!
//! let mut buf = Buffer::empty(Rect::new(0, 0, 12, 4));
//! Ascii::lettering("Hi", "small")?
//!     .alignment(Alignment::Center)
//!     .render(buf.area, &mut buf);
//! assert_eq!(
//!     buf,
//!     Buffer::with_lines(["   _  _ _   ", "  | || (_)  ", "  | __ | |  ", "  |_||_|_|  "])
//! );
//! # Ok::<(), String>(())
//! ```

use apollinaire_ascii::{Frame, art, lettering};
use ratatui_core::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::Style,
    widgets::Widget,
};

/// A frame of ASCII art drawn from the top of its area, aligned left, centred or right.
///
/// It writes the frame's cells, spaces included, with its style, and leaves the rest of the
/// area as it was. A frame wider or taller than the area is clipped on the right and bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ascii {
    frame: Frame,
    style: Style,
    alignment: Alignment,
}

impl Ascii {
    pub fn new(frame: Frame) -> Self {
        Self {
            frame,
            style: Style::default(),
            alignment: Alignment::Left,
        }
    }
    /// Printable ASCII `text` lettered in a bundled font, such as `standard`, `small` or
    /// `slant`; `apollinaire_ascii::lettering::FONTS` names them all.
    pub fn lettering(text: &str, font: &str) -> Result<Self, String> {
        let options = lettering::Options::default();
        Ok(Self::new(lettering::render_with(text, font, None, &options)?))
    }
    /// A piece from the catalogue, such as `cat`; `apollinaire list` names them.
    pub fn art(name: &str) -> Result<Self, String> {
        Ok(Self::new(art::template(name)?))
    }
    #[must_use]
    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }
    #[must_use]
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
    /// The columns and rows the frame needs to be drawn whole.
    pub fn size(&self) -> (usize, usize) {
        (self.frame.width(), self.frame.height())
    }
}

impl Widget for &Ascii {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        let width = self.frame.width();
        let spare = usize::from(area.width).saturating_sub(width);
        let indent = match self.alignment {
            Alignment::Left => 0,
            Alignment::Center => spare / 2,
            Alignment::Right => spare,
        };
        let x = area.x + indent as u16;
        let columns = usize::from(area.width) - indent;
        for (y, row) in (area.y..area.bottom()).zip(0..self.frame.height()) {
            let cells = self.frame.row(row).unwrap_or_default();
            // Frames hold printable ASCII only, so each byte is one character and one column.
            let text = std::str::from_utf8(cells).unwrap_or_default();
            buf.set_stringn(x, y, text, columns, self.style);
        }
    }
}

impl Widget for Ascii {
    fn render(self, area: Rect, buf: &mut Buffer) {
        (&self).render(area, buf);
    }
}
