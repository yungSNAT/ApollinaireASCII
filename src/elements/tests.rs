// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

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
    assert!(elements[0].fits(5, 2, 2) && !elements[0].fits(5, 2, 1) && !elements[0].fits(0, 2, 2));
}
