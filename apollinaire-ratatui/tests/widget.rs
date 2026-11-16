// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::Frame;
use apollinaire_ratatui::Ascii;
use ratatui_core::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::Widget,
};

fn filled(width: u16, height: u16) -> Buffer {
    Buffer::with_lines(vec!["~".repeat(width.into()); height.into()])
}
fn frame(text: &str) -> Ascii {
    Ascii::new(Frame::from_text(text).unwrap())
}

#[test]
fn draws_from_the_top_left_and_leaves_the_rest_of_the_area() {
    let mut buf = filled(6, 3);
    frame("ab\n c").render(buf.area, &mut buf);
    assert_eq!(buf, Buffer::with_lines(["ab~~~~", " c~~~~", "~~~~~~"]));
}

#[test]
fn aligns_within_the_area() {
    for (alignment, rows) in [
        (Alignment::Center, ["~ab~~~", "~ c~~~"]),
        (Alignment::Right, ["~~~~ab", "~~~~ c"]),
    ] {
        let mut buf = filled(6, 2);
        frame("ab\n c").alignment(alignment).render(buf.area, &mut buf);
        assert_eq!(buf, Buffer::with_lines(rows), "{alignment:?}");
    }
}

#[test]
fn clips_to_the_area_and_the_buffer() {
    let mut buf = filled(5, 4);
    // The area starts inside the buffer and runs past its right and bottom edges.
    frame("abc\ndef\nghi").render(Rect::new(3, 2, 9, 9), &mut buf);
    assert_eq!(buf, Buffer::with_lines(["~~~~~", "~~~~~", "~~~ab", "~~~de"]));
    // A frame wider than the area is clipped on the right, whatever the alignment.
    let mut narrow = filled(2, 1);
    frame("abc").alignment(Alignment::Right).render(narrow.area, &mut narrow);
    assert_eq!(narrow, Buffer::with_lines(["ab"]));
    let mut nothing = filled(2, 1);
    frame("abc").render(Rect::new(0, 0, 0, 1), &mut nothing);
    assert_eq!(nothing, Buffer::with_lines(["~~"]));
}

#[test]
fn styles_only_the_cells_it_writes() {
    let mut buf = filled(3, 1);
    let red = Style::default().fg(Color::Red);
    frame("a ").style(red).render(buf.area, &mut buf);
    let mut expected = Buffer::with_lines(["a ~"]);
    expected.set_style(Rect::new(0, 0, 2, 1), red);
    assert_eq!(buf, expected);
}

#[test]
fn letters_text_in_a_bundled_font_and_draws_catalogue_art() {
    let hi = Ascii::lettering("Hi", "small").unwrap();
    let expected = apollinaire_ascii::lettering::render_with(
        "Hi",
        "small",
        None,
        &apollinaire_ascii::lettering::Options::default(),
    )
    .unwrap();
    assert_eq!(hi.size(), (expected.width(), expected.height()));
    let (width, height) = hi.size();
    let mut buf = Buffer::empty(Rect::new(0, 0, width as u16, height as u16));
    hi.render(buf.area, &mut buf);
    let lines: Vec<String> = expected.text().split('\n').map(String::from).collect();
    assert_eq!(buf, Buffer::with_lines(lines));

    let cat = apollinaire_ascii::art::template("cat").unwrap();
    assert_eq!(Ascii::art("cat").unwrap(), Ascii::new(cat));
    assert!(Ascii::lettering("Hi", "no-such-font").is_err());
    assert!(Ascii::lettering("caf\u{e9}", "small").is_err());
    assert!(Ascii::art("no-such-piece").is_err());
}
