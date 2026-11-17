// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::canvas::{GradientStyle, bounds};
use apollinaire_ascii::diagram::{TableRules, flowchart, table, tree};
use apollinaire_ascii::{Canvas, Frame};

fn canvas(text: &str) -> Canvas {
    Canvas::from_frame(&Frame::from_text(text).unwrap())
}
fn blank(width: usize, height: usize) -> Canvas {
    Canvas::new(width, height).unwrap()
}
fn text(canvas: Canvas) -> String {
    canvas.finish().text()
}

#[test]
fn bounds_normalise_corners_in_any_order() {
    assert_eq!(bounds((4, 1), (1, 3)), (1, 1, 4, 3));
    assert_eq!(bounds((2, 2), (2, 2)), (2, 2, 1, 1));
    assert_eq!(
        bounds((i32::MIN, 0), (i32::MAX, 0)).2,
        u32::MAX as usize + 1
    );
}

#[test]
fn get_reads_cells_and_reports_outside_as_none() {
    let grid = canvas("ab\ncd");
    assert_eq!((grid.width(), grid.height()), (2, 2));
    assert_eq!(grid.get(1, 1), Some(b'd'));
    assert_eq!(
        (grid.get(-1, 0), grid.get(2, 0), grid.get(0, 2)),
        (None, None, None)
    );
}

#[test]
fn rectangle_draws_a_clipped_outline_and_rejects_coordinate_overflow() {
    let mut grid = blank(4, 3);
    grid.rectangle(-1, 0, 4, 3, b'#').unwrap();
    assert!(grid.rectangle(i32::MAX, 0, 2, 1, b'!').is_err());
    assert!(grid.rectangle(0, i32::MAX, 1, 2, b'!').is_err());
    assert_eq!(text(grid), "### \n  # \n### ");
}

#[test]
fn text_and_overlays_clip_to_the_canvas_without_wrapping() {
    let mut grid = canvas("abcd\nefgh");
    grid.text(-1, 0, "Croft").unwrap();
    grid.text(i32::MAX, 0, "XY").unwrap();
    assert!(grid.text(0, 0, "é").is_err());

    let stamp = Frame::from_text(" X").unwrap();
    grid.overlay(&stamp, 0, 1, true).unwrap();
    grid.overlay(&stamp, 2, 1, false).unwrap();
    assert_eq!(text(grid), "roft\neX X");
}

#[test]
fn oversized_text_is_refused_and_overflowing_overlays_are_clipped() {
    let mut grid = canvas("ab\ncd");
    let stamp = Frame::from_text("WX\nYZ").unwrap();
    grid.overlay(&stamp, i32::MAX, i32::MAX, false).unwrap();
    let oversized = "A".repeat(apollinaire_ascii::MAX_CELLS + 1);
    assert!(
        grid.text(0, 0, &oversized)
            .unwrap_err()
            .contains("too long")
    );
    assert_eq!(text(grid), "ab\ncd");
}

#[test]
fn ellipse_outlines_are_symmetrical_at_odd_and_even_sizes() {
    let mut grid = blank(9, 5);
    grid.ellipse(0, 0, 9, 5, b'#').unwrap();
    assert_eq!(
        text(grid),
        "  #####  \n##     ##\n#       #\n##     ##\n  #####  "
    );
    let mut grid = blank(6, 4);
    grid.ellipse(0, 0, 6, 4, b'o').unwrap();
    assert_eq!(text(grid), " oooo \no    o\no    o\n oooo ");
}

#[test]
fn ellipse_collapses_to_a_line_or_cell_clips_and_validates() {
    let mut grid = blank(5, 3);
    grid.ellipse(1, 1, 3, 1, b'-').unwrap();
    grid.ellipse(0, 0, 1, 3, b'|').unwrap();
    grid.ellipse(4, 2, 1, 1, b'.').unwrap();
    assert_eq!(text(grid), "|    \n|--- \n|   .");
    let mut grid = blank(4, 3);
    grid.ellipse(-2, -1, 5, 3, b'#').unwrap();
    assert_eq!(text(grid), "  # \n##  \n    ");
    let mut grid = blank(2, 2);
    grid.ellipse(i32::MAX, i32::MIN, 3, 3, b'#').unwrap();
    assert_eq!(text(grid), "  \n  ");
    assert!(blank(2, 2).ellipse(0, 0, 0, 1, b'#').is_err());
    assert!(blank(2, 2).ellipse(0, 0, 1, 1, b'\n').is_err());
}

#[test]
fn curve_bends_towards_its_control_point_and_stays_joined() {
    let mut grid = blank(9, 3);
    // The control point may lie outside the grid.
    grid.curve((0, 2), (4, -2), (8, 2), b'#').unwrap();
    assert_eq!(text(grid), "   ###   \n ##   ## \n#       #");
    let mut grid = blank(7, 4);
    grid.curve((0, 0), (6, 0), (6, 3), b'#').unwrap();
    assert_eq!(text(grid), "####   \n    ## \n      #\n      #");
    // A control point on the line, or at an end, draws the straight line.
    for control in [(2, 0), (0, 0), (4, 0)] {
        let mut grid = blank(5, 1);
        grid.curve((0, 0), control, (4, 0), b'*').unwrap();
        assert_eq!(text(grid), "*****");
    }
    let mut grid = blank(2, 1);
    grid.curve((1, 0), (1, 0), (1, 0), b'.').unwrap();
    assert_eq!(text(grid), " .");
    assert!(blank(2, 2).curve((0, 0), (4097, 0), (1, 1), b'#').is_err());
    assert!(blank(2, 2).curve((0, -4097), (0, 0), (1, 1), b'#').is_err());
    assert!(blank(2, 2).curve((0, 0), (0, 0), (1, 1), b'\n').is_err());
}

#[test]
fn fill_area_is_solid_clipped_and_validated() {
    let mut grid = blank(4, 3);
    grid.fill_area(-1, 1, 3, 9, b'#').unwrap();
    assert_eq!(text(grid), "    \n##  \n##  ");
    let mut grid = blank(2, 2);
    grid.fill_area(i32::MAX, i32::MIN, usize::MAX, usize::MAX, b'x')
        .unwrap();
    assert_eq!(text(grid), "  \n  ");
    let mut grid = blank(2, 2);
    grid.fill_area(0, 0, usize::MAX, usize::MAX, b'x').unwrap();
    assert_eq!(text(grid), "xx\nxx");
    assert!(blank(2, 2).fill_area(0, 0, 1, 1, b'\n').is_err());
}

#[test]
fn flood_fill_replaces_only_the_four_connected_run() {
    let mut grid = canvas("#####\n#   #\n#####\n     ");
    grid.flood_fill(2, 1, b'.').unwrap();
    assert_eq!(text(grid), "#####\n#...#\n#####\n     ");
    // Corners touch the start cell only diagonally, so they stay blank.
    let mut grid = canvas(" # \n# #\n # ");
    grid.flood_fill(0, 0, b'.').unwrap();
    assert_eq!(text(grid), ".# \n# #\n # ");
    let mut grid = canvas("ab\nab");
    grid.flood_fill(0, 0, b'a').unwrap();
    grid.flood_fill(5, 5, b'z').unwrap();
    assert_eq!(text(grid), "ab\nab");
    assert!(blank(1, 1).flood_fill(0, 0, 0x7f).is_err());
}

const LINEAR: GradientStyle = GradientStyle {
    radial: false,
    dither: false,
};
const RADIAL: GradientStyle = GradientStyle {
    radial: true,
    dither: false,
};

#[test]
fn gradient_fill_spreads_the_ramp_along_the_drag_within_the_run() {
    let mut grid = blank(5, 1);
    grid.gradient_fill((0, 0), (4, 0), b"abcde", LINEAR)
        .unwrap();
    assert_eq!(text(grid), "abcde");
    // Before the start and past the end, the first and last characters hold.
    let mut grid = blank(7, 1);
    grid.gradient_fill((1, 0), (3, 0), b"abc", LINEAR).unwrap();
    assert_eq!(text(grid), "aabcccc");
    // Only the run of the start cell's character changes.
    let mut grid = canvas("#####\n#   #\n#####");
    grid.gradient_fill((1, 1), (3, 1), b"123", LINEAR).unwrap();
    assert_eq!(text(grid), "#####\n#123#\n#####");
    // The ramp may hold the character it replaces.
    let mut grid = blank(3, 1);
    grid.gradient_fill((0, 0), (2, 0), b" x", LINEAR).unwrap();
    assert_eq!(text(grid), " xx");
}

#[test]
fn radial_gradient_counts_rows_double() {
    let mut grid = blank(5, 3);
    grid.gradient_fill((2, 1), (4, 1), b"abc", RADIAL).unwrap();
    // One row up is as far as 2 columns across.
    assert_eq!(text(grid), "ccccc\ncbabc\nccccc");
}

#[test]
fn dithered_gradient_mixes_the_2_nearest_characters_in_a_4_by_4_pattern() {
    let dither = GradientStyle {
        radial: false,
        dither: true,
    };
    // Columns 0 to 8 run from a to c: column 1 is a quarter of the way to b, column 2 half and
    // column 3 three quarters, so the 4 x 4 block of columns 0 to 3 holds 0 + 0 + 2 + 4 b.
    let mut grid = blank(9, 4);
    grid.gradient_fill((0, 0), (8, 0), b"abc", dither).unwrap();
    assert_eq!(text(grid), "aabbbbccc\naaabbbbcc\naabbbbccc\naaabbbbcc");
    // At a character exactly, every cell takes it.
    let mut grid = blank(4, 4);
    grid.gradient_fill((0, 0), (0, 0), b"ab", dither).unwrap();
    assert_eq!(text(grid), "aaaa\naaaa\naaaa\naaaa");
}

#[test]
fn gradient_fill_without_length_or_start_uses_the_first_character_or_nothing() {
    for style in [LINEAR, RADIAL] {
        let mut grid = blank(2, 1);
        grid.gradient_fill((0, 0), (0, 0), b"ab", style).unwrap();
        assert_eq!(text(grid), "aa");
        let mut grid = blank(2, 1);
        grid.gradient_fill((5, 0), (0, 0), b"ab", style).unwrap();
        assert_eq!(text(grid), "  ");
    }
    assert!(
        blank(1, 1)
            .gradient_fill((0, 0), (0, 0), b"", LINEAR)
            .is_err()
    );
    assert!(
        blank(1, 1)
            .gradient_fill((0, 0), (0, 0), b"a\n", LINEAR)
            .is_err()
    );
}

#[test]
fn flood_fill_covers_the_largest_grid() {
    let mut grid = blank(apollinaire_ascii::MAX_WIDTH, apollinaire_ascii::MAX_HEIGHT);
    grid.flood_fill(100, 100, b'~').unwrap();
    assert!(grid.finish().cells().iter().all(|cell| *cell == b'~'));
}

#[test]
fn diagram_boxes_have_corners_labels_and_join_crossings() {
    let mut grid = blank(13, 5);
    grid.diagram_box((10, 4), (0, 0), "VARROCK").unwrap();
    assert_eq!(
        text(grid),
        "+---------+  \n|         |  \n| VARROCK |  \n|         |  \n+---------+  "
    );
    let mut grid = blank(7, 5);
    grid.diagram_box((0, 0), (4, 2), "").unwrap();
    grid.diagram_box((2, 1), (6, 4), "").unwrap();
    assert_eq!(text(grid), "+---+  \n| +-+-+\n+-+-+ |\n  |   |\n  +---+");
    let mut grid = blank(5, 3);
    grid.diagram_box((0, 0), (4, 2), "LUMBRIDGE").unwrap();
    assert_eq!(text(grid), "+---+\n|LUM|\n+---+");
}

#[test]
fn connectors_bend_once_join_boxes_and_point_their_arrows() {
    let mut grid = blank(8, 4);
    grid.connector((0, 0), (6, 3), true).unwrap();
    assert_eq!(text(grid), "------+ \n      | \n      | \n      v ");
    let mut grid = blank(3, 5);
    grid.connector((0, 4), (2, 0), true).unwrap();
    assert_eq!(text(grid), "+->\n|  \n|  \n|  \n|  ");
    for (end, head) in [
        ((0, 2), b'<'),
        ((2, 0), b'^'),
        ((4, 2), b'>'),
        ((2, 4), b'v'),
    ] {
        let mut grid = blank(5, 5);
        grid.connector((2, 2), end, true).unwrap();
        assert_eq!(grid.get(end.0, end.1), Some(head));
    }
    let mut grid = blank(3, 3);
    grid.connector((1, 1), (1, 1), true).unwrap();
    assert_eq!(text(grid), "   \n + \n   ");
}

#[test]
fn connectors_cross_lines_and_attach_to_box_edges() {
    let mut grid = blank(7, 5);
    grid.connector((3, 0), (3, 4), false).unwrap();
    grid.connector((0, 2), (6, 2), true).unwrap();
    assert_eq!(text(grid), "   |   \n   |   \n---+-->\n   |   \n   |   ");
    let mut grid = blank(8, 3);
    grid.diagram_box((0, 0), (3, 2), "").unwrap();
    grid.connector((3, 1), (7, 1), true).unwrap();
    assert_eq!(text(grid), "+--+    \n|  +--->\n+--+    ");
}

#[test]
fn arrowheads_keep_the_edges_they_reach_and_join_lines_that_cross_them() {
    let mut grid = blank(8, 3);
    grid.diagram_box((4, 0), (7, 2), "").unwrap();
    grid.connector((0, 1), (4, 1), true).unwrap();
    assert_eq!(text(grid), "    +--+\n--->|  |\n    +--+");
    let mut grid = blank(8, 3);
    grid.connector((3, 0), (3, 2), false).unwrap();
    grid.diagram_box((4, 0), (7, 2), "").unwrap();
    grid.connector((0, 1), (4, 1), true).unwrap();
    assert_eq!(text(grid), "   |+--+\n-->+|  |\n   |+--+");
    // Every cell before the edge is a junction, so no arrowhead fits.
    let mut grid = blank(5, 3);
    for x in 1..4 {
        grid.connector((x, 0), (x, 2), false).unwrap();
    }
    grid.diagram_box((4, 0), (4, 2), "").unwrap();
    grid.connector((1, 1), (4, 1), true).unwrap();
    assert_eq!(text(grid), " |||+\n +++|\n |||+");
    let mut grid = blank(5, 3);
    grid.connector((2, 0), (2, 2), true).unwrap();
    grid.connector((0, 2), (4, 2), false).unwrap();
    assert_eq!(text(grid), "  |  \n  |  \n--+--");
}

#[test]
fn diagram_output_is_plain_ascii_and_input_is_validated() {
    let mut grid = blank(4, 4);
    assert!(grid.diagram_box((0, 0), (3, 3), "caf\u{e9}").is_err());
    assert!(grid.diagram_box((0, 0), (5000, 3), "").is_err());
    assert!(grid.connector((0, 0), (0, -5000), false).is_err());
    grid.diagram_box((-3, -3), (9, 9), "").unwrap();
    grid.connector((-2, 1), (9, 1), true).unwrap();
    let frame = grid.finish();
    assert!(frame.cells().iter().all(|b| b" +-|<>^v".contains(b)));
}

#[test]
fn tables_size_columns_to_their_widest_cell_and_rule_the_chosen_rows() {
    let ledger = "Item\tQty\nRune scimitar\t1\n Lobster \t 28 \n";
    assert_eq!(
        table(ledger, b'\t', TableRules::Header).unwrap().text(),
        "+---------------+-----+\n\
         | Item          | Qty |\n\
         +---------------+-----+\n\
         | Rune scimitar | 1   |\n\
         | Lobster       | 28  |\n\
         +---------------+-----+"
    );
    assert_eq!(
        table("a,b\nc,d", b',', TableRules::Every).unwrap().text(),
        "+---+---+\n| a | b |\n+---+---+\n| c | d |\n+---+---+"
    );
    assert_eq!(
        table("a|b\r\n\r\nc", b'|', TableRules::None)
            .unwrap()
            .text(),
        "+---+---+\n| a | b |\n| c |   |\n+---+---+"
    );
    // One row has no header to rule off; an empty cell keeps its padding.
    assert_eq!(
        table("|x", b'|', TableRules::Header).unwrap().text(),
        "+--+---+\n|  | x |\n+--+---+"
    );
}

#[test]
fn tables_refuse_blank_text_unprintable_bytes_and_oversized_results() {
    for blank in ["", "\n \n"] {
        assert_eq!(
            table(blank, b'\t', TableRules::Header).unwrap_err(),
            "A table needs at least one row of text"
        );
    }
    assert!(
        table("a\tb", b',', TableRules::Header)
            .unwrap_err()
            .contains("printable ASCII")
    );
    assert!(table("caf\u{e9}", b',', TableRules::Header).is_err());
    assert_eq!(
        table("a", b'\n', TableRules::Header).unwrap_err(),
        "The separator must be a tab or printable ASCII"
    );
    // The line and byte are named, and a CR is dropped only before LF.
    assert_eq!(
        table("a,b\nc,d\x07", b',', TableRules::Header).unwrap_err(),
        "Line 2: byte 4 is 0x07, not printable ASCII"
    );
    assert_eq!(
        table("a,b\r", b',', TableRules::Header).unwrap_err(),
        "Line 1: byte 4 is 0x0d, not printable ASCII"
    );
    assert!(table("a,b\r\nc,d\r\n", b',', TableRules::Header).is_ok());
    assert_eq!(
        table(&"x".repeat(600), b',', TableRules::Header).unwrap_err(),
        "The table is wider than 512 columns"
    );
    // Many empty columns are refused by width before any rule is drawn.
    assert_eq!(
        table(&",".repeat(200_000), b',', TableRules::Header).unwrap_err(),
        "The table is wider than 512 columns"
    );
    assert!(table(&"x\n".repeat(300), b',', TableRules::Header).is_err());
}

/// Rows padded with spaces to the widest, as a frame holds them.
fn padded(rows: &[&str]) -> String {
    let width = rows.iter().map(|r| r.len()).max().unwrap();
    let rows: Vec<String> = rows.iter().map(|r| format!("{r:width$}")).collect();
    rows.join("\n")
}

#[test]
fn trees_draw_indented_outlines_with_ascii_branches() {
    let outline = "src\n  lib.rs\n  web\n      draw.rs\n      tests.rs\n  main.rs\n";
    assert_eq!(
        tree(outline).unwrap().text(),
        padded(&[
            "src",
            "|-- lib.rs",
            "|-- web",
            "|   |-- draw.rs",
            "|   `-- tests.rs",
            "`-- main.rs",
        ])
    );
    // Several roots, a dedent of more than one level, blank lines and CRLF.
    let outline =
        "Misthalin\r\n Varrock\r\n  Palace\r\n\r\n Lumbridge\r\nKandarin\r\n Ardougne\r\n   Castle";
    assert_eq!(
        tree(outline).unwrap().text(),
        padded(&[
            "Misthalin",
            "|-- Varrock",
            "|   `-- Palace",
            "`-- Lumbridge",
            "Kandarin",
            "`-- Ardougne",
            "    `-- Castle",
        ])
    );
}

#[test]
fn trees_refuse_unmatched_dedents_tabs_and_blank_outlines() {
    assert_eq!(
        tree("a\n    b\n  c").unwrap_err(),
        "Line 3: its indentation matches no outer line"
    );
    assert_eq!(
        tree("a\n\tb").unwrap_err(),
        "Line 2: indent with spaces, not tabs"
    );
    assert_eq!(
        tree(" \n").unwrap_err(),
        "A tree needs at least one line of text"
    );
    // A first line that is indented is a root like any other at that depth.
    assert_eq!(tree("  a\n  b").unwrap().text(), "a\nb");
    // A dedent past every earlier line matches none of them either.
    assert_eq!(
        tree("  a\nb").unwrap_err(),
        "Line 2: its indentation matches no outer line"
    );
    assert!(tree("a\n caf\u{e9}").is_err());
    assert!(tree("root\r").is_err());
    assert!(tree(&"x\n".repeat(300)).is_err());
}

fn chart(source: &str) -> String {
    flowchart(source).unwrap().text()
}

// The expected drawings below follow the layout rules: boxes are their text plus 4 wide, and
// wider where several links leave or arrive, 2 apart and centred; a rank is placed over the
// ports above it, then under the ports below it; a link that does not line up turns once in a
// lane of the channel between ranks, and an arrowhead stops one cell short of its box.
#[test]
fn flowcharts_rank_boxes_and_route_links_between_them() {
    let two = padded(&[
        "+---+", "| A |", "+-+-+", "  v  ", "+---+", "| B |", "+---+",
    ]);
    assert_eq!(chart("flowchart TD\n  A --> B"), two);
    assert_eq!(
        chart("```mermaid\nflowchart TD; A-->B; B-.->C; C==>D; D---E\n```"),
        padded(&[
            "+---+", "| A |", "+-+-+", "  v  ", "+---+", "| B |", "+-+-+", "  v  ", "+---+",
            "| C |", "+-+-+", "  v  ", "+---+", "| D |", "+-+-+", "  |  ", "+-+-+", "| E |",
            "+---+",
        ])
    );
    // Fanning out and back in: each jog takes the first lane where it clears the others.
    assert_eq!(
        chart("flowchart TD\n  A --> B & C\n  B --> D\n  C --> D"),
        padded(&[
            "  +-----+   ",
            "  |  A  |   ",
            "  +-+-+-+   ",
            "    | |     ",
            "  +-+ +--+  ",
            "  v      v  ",
            "+---+  +---+",
            "| B |  | C |",
            "+-+-+  +-+-+",
            "  |      |  ",
            "  | +----+  ",
            "  v v       ",
            "+-----+     ",
            "|  D  |     ",
            "+-----+     ",
        ])
    );
    // A rank is reordered to uncross its links: D moves under A and C under B.
    assert_eq!(
        chart("graph TD\nC\nD\nA --> D\nB --> C"),
        padded(&[
            "+---+  +---+",
            "| A |  | B |",
            "+-+-+  +-+-+",
            "  v      v  ",
            "+---+  +---+",
            "| D |  | C |",
            "+---+  +---+",
        ])
    );
    // A longer arrow passes through a rank of its own.
    assert_eq!(
        chart("graph TD\n  A ---> B"),
        padded(&[
            "+---+", "| A |", "+-+-+", "  |  ", "  |  ", "  v  ", "+---+", "| B |", "+---+"
        ])
    );
    // Link text takes the first rank of its link, and the line runs under it.
    assert_eq!(
        chart("graph TD\n  A -->|a long label here| B\n  A --> C"),
        padded(&[
            "     +-----+            ",
            "     |  A  |            ",
            "     +-+-+-+            ",
            "       | |              ",
            "  +----+ +-----+        ",
            "  v            |        ",
            "+---+          |        ",
            "| C |  a long label here",
            "+---+          |        ",
            "               v        ",
            "             +---+      ",
            "             | B |      ",
            "             +---+      ",
        ])
    );
    // A cycle: the link back up keeps its arrowhead at the box it points to.
    assert_eq!(
        chart("graph TD\n  A --> B --> C --> A"),
        padded(&[
            " +-----+", " |  A  |", " +-+---+", "   | ^  ", "  ++ +-+", "  v    |", "+---+  |",
            "| B |  |", "+-+-+  |", "  |    |", "  | +--+", "  v |   ", "+---+-+ ", "|  C  | ",
            "+-----+ ",
        ])
    );
    // Across and mirrored: LR, BT with text, and RL with arrowheads at both ends.
    assert_eq!(
        chart("graph LR\n  A --> B"),
        padded(&["+---+ +---+", "| A +>| B |", "+---+ +---+"])
    );
    assert_eq!(
        chart("flowchart BT\n  A -- go --> B"),
        padded(&[
            "+---+", "| B |", "+---+", "  ^  ", "  go ", "  |  ", "+-+-+", "| A |", "+---+"
        ])
    );
    assert_eq!(
        chart("flowchart RL\n  A <--> B"),
        padded(&["+---+  +---+", "| B |<>| A |", "+---+  +---+"])
    );
}

#[test]
fn flowcharts_keep_crossing_links_apart() {
    // N2 -> N1 and N5 -> N3 each leave from the column where the other arrives, so neither
    // can turn first: N2 -> N1, the later link, takes a second jog through column 20.
    assert_eq!(
        chart("flowchart TD\n  N2 --> N4\n  N5 --> N3\n  N2 --> N3\n  N5 --> N1\n  N2 --> N1"),
        padded(&[
            "      +-------+  +-----+",
            "      |  N2   |  | N5  |",
            "      +-+-+-+-+  +-+-+-+",
            "        | | |      | |  ",
            "  +-----+ | +------|+|  ",
            "  |       | +------+||  ",
            "  |       | |      ++|  ",
            "  v       v v      v v  ",
            "+----+  +-----+  +-----+",
            "| N4 |  | N3  |  | N1  |",
            "+----+  +-----+  +-----+",
        ])
    );
    // Where links cross, the vertical line is kept, so `+` marks only bends and box edges.
    let k33 = "flowchart TD\n  A & B & C --> D & E & F";
    let drawn = chart(k33);
    assert_eq!(drawn.matches('v').count(), 9);
    assert!(drawn.contains("-|-"), "{drawn}");
    assert_eq!(chart(k33), drawn);
}

#[test]
fn flowcharts_read_every_shape_and_link_form_alike() {
    let boxed = padded(&[
        "+---+", "| x |", "+-+-+", "  v  ", "+---+", "| B |", "+---+",
    ]);
    for node in [
        "A[x]",
        "A(x)",
        "A{x}",
        "A((x))",
        "A(((x)))",
        "A([x])",
        "A[[x]]",
        "A[(x)]",
        "A{{x}}",
        "A>x]",
        "A[/x/]",
        "A[/x\\]",
        "A[\\x\\]",
        "A[\\x/]",
        "A[\"x\"]",
        "A[ x ]",
        "A[\"x\" ]",
        "A[x]:::warm",
    ] {
        assert_eq!(chart(&format!("graph TD\n{node} --> B")), boxed, "{node}");
    }
    assert_eq!(chart("graph TD\nA --> B\nA[x]"), boxed);
    // Styling is skipped to the end of its statement, not its line.
    assert_eq!(
        chart("graph TD; A --> B; style A fill:#f00; B --> C"),
        chart("graph TD; A --> B; B --> C")
    );
    assert_eq!(
        chart("graph TD; A --> B; classDef warm fill:#f96;class A warm; B --> C;"),
        chart("graph TD; A --> B; B --> C")
    );
    let arrow = chart("graph TD\nA --> B");
    for source in [
        "graph\nA -.-> B",
        "flowchart TB\nA ==> B",
        "flowchart TD;A-->B;",
        "graph TD\n%% a comment\n\nA:::warm\t-->\tB\r\nclassDef warm fill:#f96\nclass A warm\nstyle A fill:#f00\nlinkStyle 0 stroke:#f00",
        "~~~\ngraph TD\nA --> B\n~~~\n",
        "```\ngraph TD\nA --> B\n```\n\n",
    ] {
        assert_eq!(chart(source), arrow, "{source}");
    }
    let open = chart("graph TD\nA --- B");
    assert_eq!(
        open,
        padded(&[
            "+---+", "| A |", "+-+-+", "  |  ", "+-+-+", "| B |", "+---+"
        ])
    );
    // A name starting with o or x is a node, not a circle or cross end.
    assert_eq!(
        chart("graph TD\nA --- xray"),
        padded(&[
            " +---+", " | A |", " +-+-+", "   |", "+--+---+", "| xray |", "+------+"
        ])
    );
    assert_eq!(chart("graph TD\nA -.- B"), open);
    assert_eq!(chart("graph TD\nA === B"), open);
    let text = chart("graph TD\nA -->|t| B");
    for source in [
        "graph TD\nA -- t --> B",
        "graph TD\nA -. t .-> B",
        "graph TD\nA == t ==> B",
    ] {
        assert_eq!(chart(source), text, "{source}");
    }
    assert_eq!(
        chart("graph TD\nA -- t --- B"),
        chart("graph TD\nA ---|t| B")
    );
    assert_eq!(chart("graph TD\nA -..-> B"), chart("graph TD\nA ---> B"));
    assert_eq!(chart("graph TD\nA ===> B"), chart("graph TD\nA ---> B"));
    assert_eq!(
        chart("graph TD\nA <-- t --> B"),
        chart("graph TD\nA <-->|t| B")
    );
}

#[test]
fn flowcharts_refuse_what_they_cannot_draw_with_its_line() {
    for (source, error) in [
        ("", "A flowchart starts with flowchart or graph"),
        ("graph TD\n", "A flowchart needs at least one node"),
        (
            "graph XY",
            "Line 1: the direction XY is not one of TD, TB, BT, LR or RL",
        ),
        (
            "sequenceDiagram",
            "Line 1: a flowchart starts with flowchart or graph and a direction",
        ),
        (
            "graph TD\nA --> caf\u{e9}",
            "Line 2: byte 10 is 0xc3, not printable ASCII",
        ),
        ("graph TD\nA[a\tb] --> B", "Line 2: text holds a tab"),
        (
            "graph TD\nsubgraph one",
            "Line 2: subgraph is not supported",
        ),
        ("graph TD\nend", "Line 2: end is not supported"),
        ("graph TD\nclick A go", "Line 2: click is not supported"),
        (
            "graph TD\ndirection LR",
            "Line 2: direction is not supported",
        ),
        (
            "graph TD\nA --> A",
            "Line 2: a link from a node to itself is not drawn",
        ),
        (
            "graph TD\nA[x",
            "Line 2: the shape at column 2 is not closed",
        ),
        (
            "graph TD\nA[\"x] --> B",
            "Line 2: the shape at column 2 is not closed",
        ),
        (
            "graph TD\nA[\"x\" y] --> B",
            "Line 2: the shape at column 2 is not closed",
        ),
        (
            "graph TD\nA[`x`]",
            "Line 2: Markdown strings are not supported",
        ),
        (
            "graph TD\nA --o B",
            "Line 2: circle and cross link ends are not drawn",
        ),
        (
            "graph TD\nA --x B",
            "Line 2: circle and cross link ends are not drawn",
        ),
        (
            "graph TD\nA <--- B",
            "Line 2: the link at column 3 has an arrowhead at its start only",
        ),
        // The shortest such link is not mistaken for link text that is never closed.
        (
            "graph TD\nA <-- B",
            "Line 2: the link at column 3 has an arrowhead at its start only",
        ),
        (
            "graph TD\nA@{ shape: rect }",
            "Line 2: expected a link such as --> at column 2",
        ),
        (
            "graph TD\nA -> B",
            "Line 2: expected a link such as --> at column 3",
        ),
        (
            "graph TD\nA B",
            "Line 2: expected a link such as --> at column 3",
        ),
        (
            "graph TD\nA -.-. B",
            "Line 2: expected a link such as --> at column 3",
        ),
        (
            "graph TD\nA -- B",
            "Line 2: the link text at column 3 is not closed",
        ),
        (
            "graph TD\nA -- t -- B",
            "Line 2: the link text at column 3 is not closed",
        ),
        (
            "graph TD\nA -->|t B",
            "Line 2: the link text at column 6 is not closed",
        ),
        ("graph TD\nA -- s -->|t| B", "Line 2: a link has text twice"),
        (
            "graph TD\nA -->|| B",
            "Line 2: link text must hold printable characters and no tab",
        ),
        (
            "graph TD\nA -->|a\tb| B",
            "Line 2: link text must hold printable characters and no tab",
        ),
        (
            "graph TD\nA --> ",
            "Line 2: expected a node name at column 7",
        ),
        (
            "graph TD\n--> B",
            "Line 2: expected a node name at column 1",
        ),
        (
            "```mermaid\ngraph TD\nA-->B",
            "The code block opened on line 1 is not closed",
        ),
        ("```js\n```", "Line 1: only a mermaid code block is read"),
        (
            "```\ngraph TD\nA\n```\nmore",
            "Line 5: text after the code block",
        ),
    ] {
        assert_eq!(flowchart(source).unwrap_err(), error, "{source}");
    }
    let nodes: String = (0..1025).map(|i| format!("N{i}\n")).collect();
    assert_eq!(
        flowchart(&format!("graph TD\n{nodes}")).unwrap_err(),
        "A flowchart has at most 1024 nodes"
    );
    let group = |name: &str, count: usize| {
        (0..count)
            .map(|i| format!("{name}{i}"))
            .collect::<Vec<_>>()
            .join(" & ")
    };
    assert_eq!(
        flowchart(&format!(
            "graph TD\n{} --> {}",
            group("A", 65),
            group("B", 64)
        ))
        .unwrap_err(),
        "A flowchart has at most 4096 links"
    );
}

#[test]
fn flowcharts_refuse_drawings_over_512_columns_or_256_rows() {
    let chain = |count: usize| -> String {
        (0..count)
            .map(|i| format!("  N{i} --> N{}\n", i + 1))
            .collect()
    };
    // 70 ranks of 3 rows with a row between each: refused before any layout.
    assert_eq!(
        flowchart(&format!("flowchart TD\n{}", chain(69))).unwrap_err(),
        "The flowchart needs at least 279 rows; the limit is 256"
    );
    let row: String = (0..200).map(|i| format!("  N{i}\n")).collect();
    assert_eq!(
        flowchart(&format!("flowchart TD\n{row}")).unwrap_err(),
        "The flowchart needs at least 1888 columns; the limit is 512"
    );
    let column: String = (0..90).map(|i| format!("  N{i}\n")).collect();
    assert_eq!(
        flowchart(&format!("flowchart LR\n{column}")).unwrap_err(),
        "The flowchart needs at least 359 rows; the limit is 256"
    );
    // 64 ranks fit in 255 rows until a jog adds a lane and 2 rows.
    assert_eq!(
        flowchart(&format!("flowchart TD\n{}  N0 --> M\n", chain(63))).unwrap_err(),
        "The flowchart needs 14 columns by 257 rows; the limit is 512 by 256"
    );
    // A link spanning 1,000 ranks needs a row for each and one between each pair, at least:
    // refused before any rank is laid out.
    assert_eq!(
        flowchart(&format!("graph TD\nA {}> B", "-".repeat(1001))).unwrap_err(),
        "The flowchart needs at least 2001 rows; the limit is 256"
    );
    assert_eq!(
        flowchart(&format!("graph LR\nA {}> B", "-".repeat(301))).unwrap_err(),
        "The flowchart needs at least 601 columns; the limit is 512"
    );
    // Links that each fit can still chain into more ranks than fit: 511 ranks need 1,021
    // columns at least, refused before any rank is measured.
    let long = "-".repeat(256);
    assert_eq!(
        flowchart(&format!("graph LR\nA {long}> B {long}> C")).unwrap_err(),
        "The flowchart needs at least 1021 columns; the limit is 512"
    );
    // A box widens to hold its links' ends 2 apart: 4,096 links leaving A need 8,195 columns,
    // refused with the other early checks, before the ranks are ordered.
    assert_eq!(
        flowchart(&format!("graph TD\n{}", "A --> B\n".repeat(4096))).unwrap_err(),
        "The flowchart needs at least 8195 columns; the limit is 512"
    );
    // Arriving and leaving links each count on their own: X's 127 ports need 257 rows whilst
    // every other rank fits.
    assert_eq!(
        flowchart(&format!(
            "graph LR\n{}B ---> X\n",
            "A ----> X\n".repeat(126)
        ))
        .unwrap_err(),
        "The flowchart needs at least 257 rows; the limit is 256"
    );
    assert_eq!(
        flowchart(&format!(
            "graph LR\n{}X ---> B\n",
            "X ----> A\n".repeat(126)
        ))
        .unwrap_err(),
        "The flowchart needs at least 257 rows; the limit is 256"
    );
    // Input over 16 MiB is refused before it is read.
    assert_eq!(
        flowchart(&format!(
            "graph TD\nA{}",
            " ".repeat(apollinaire_ascii::MAX_INPUT_BYTES)
        ))
        .unwrap_err(),
        "The flowchart exceeds the 16 MiB input limit"
    );
}

#[test]
fn cow_files_give_their_art_under_a_tailless_bubble() {
    use apollinaire_ascii::{BorderStyle, bubble::cowsay};
    // An owl drawn for this test, using each substitution and escape a cow file may hold.
    let owl = r#"## An owl drawn for these tests
$the_cow = <<EOC
  $thoughts
   $thoughts  {${eyes}}
      /)  (\\  \$5
     ("\@)$tongue@)  $
EOC

# A comment after the art
"#;
    let expected = [
        "+-----+         ",
        "| Hoo |         ",
        "+-----+         ",
        "  \\             ",
        "   \\  {oo}      ",
        "      /)  (\\  $5",
        "     (\"@)  @)  $",
    ];
    assert_eq!(
        cowsay("Hoo", 40, BorderStyle::Plain, owl).unwrap().text(),
        expected.join("\n")
    );
    assert_eq!(
        cowsay("Hoo", 40, BorderStyle::Plain, &owl.replace('\n', "\r\n"))
            .unwrap()
            .text(),
        expected.join("\n")
    );
    // A single-quoted heredoc is taken as it stands, as Perl would.
    let raw = "$the_cow = <<'EOC';\n$thoughts \\\\\nEOC\n";
    assert_eq!(
        cowsay("Hoo", 40, BorderStyle::Plain, raw).unwrap().text(),
        "+-----+     \n| Hoo |     \n+-----+     \n$thoughts \\\\"
    );
    // Perl ends the variable before these, as the drawing does.
    let ends = "$the_cow = <<EOC;\n$eyes-x\n$eyes'\n$eyes-> |\nEOC\n";
    assert_eq!(
        cowsay("Hoo", 40, BorderStyle::Plain, ends).unwrap().text(),
        "+-----+\n| Hoo |\n+-----+\noo-x   \noo'    \noo-> | "
    );
    for (cow, error) in [
        (
            "$eyes = \"..\" unless ($eyes);\n$the_cow = <<EOC;\nx\nEOC\n",
            "Line 1: only the art is read, and this Perl code would be needed to draw it",
        ),
        (
            "$the_cow = <<EOC;\nx\nEOC\nprint 1;\n",
            "Line 4: only the art is read, and this Perl code would be needed to draw it",
        ),
        (
            "$the_cow = <<EOC;\n$foo\nEOC\n",
            "Line 2: the variable $foo is not drawn",
        ),
        (
            "$the_cow = <<EOC;\n\\e[31m\nEOC\n",
            "Line 2: the escape \\e is not drawn; a backslash keeps only punctuation",
        ),
        (
            "$the_cow = <<EOC;\n@ARGV\nEOC\n",
            "Line 2: an unescaped @ would be read by Perl as an array",
        ),
        (
            "$the_cow = <<EOC;\n$)\nEOC\n",
            "Line 2: an unescaped $ would be read by Perl as a variable",
        ),
        // Perl would read these as array elements, hash entries or package variables, and
        // @- and @+ as its match arrays.
        (
            "$the_cow = <<EOC;\n$eyes[0]\nEOC\n",
            "Line 2: $eyes followed by [ would be read by Perl as part of a larger variable expression",
        ),
        (
            "$the_cow = <<EOC;\n${eyes}[0]\nEOC\n",
            "Line 2: $eyes followed by [ would be read by Perl as part of a larger variable expression",
        ),
        (
            "$the_cow = <<EOC;\n$thoughts{key}\nEOC\n",
            "Line 2: $thoughts followed by { would be read by Perl as part of a larger variable expression",
        ),
        (
            "$the_cow = <<EOC;\n$tongue::x\nEOC\n",
            "Line 2: $tongue followed by :: would be read by Perl as part of a larger variable expression",
        ),
        (
            "$the_cow = <<EOC;\n$eyes'x\nEOC\n",
            "Line 2: $eyes followed by ' would be read by Perl as part of a larger variable expression",
        ),
        (
            "$the_cow = <<EOC;\n$eyes->[0]\nEOC\n",
            "Line 2: $eyes followed by ->[ would be read by Perl as part of a larger variable expression",
        ),
        (
            "$the_cow = <<EOC;\n(@-@)\nEOC\n",
            "Line 2: an unescaped @ would be read by Perl as an array",
        ),
        (
            "$the_cow = <<EOC;\n@+\nEOC\n",
            "Line 2: an unescaped @ would be read by Perl as an array",
        ),
        (
            "$the_cow = <<EOC;\ncaf\u{e9}\nEOC\n",
            "Line 2: Only printable ASCII is allowed: byte 3 is 0xc3",
        ),
        ("$the_cow = <<EOC;\nx\n", "The art has no closing line EOC"),
        (
            "$the_cow = <<EOC;\nx \\\nEOC\n",
            "Line 2: a backslash ends the line",
        ),
        (
            "## no art\n",
            "The cow file has no art: no line such as $the_cow = <<\"EOC\";",
        ),
    ] {
        assert_eq!(
            cowsay("Hoo", 40, BorderStyle::Plain, cow).unwrap_err(),
            error,
            "{cow}"
        );
    }
    // Art that fits on its own can still be too tall under the bubble.
    let tall = format!("$the_cow = <<EOC;\n{}EOC\n", "x\n".repeat(254));
    assert_eq!(
        cowsay("Hoo", 40, BorderStyle::Plain, &tall).unwrap_err(),
        "Dimensions must be 1..=512 columns and 1..=256 rows"
    );
    let huge = "#".repeat(apollinaire_ascii::MAX_INPUT_BYTES + 1);
    assert_eq!(
        cowsay("Hoo", 40, BorderStyle::Plain, &huge).unwrap_err(),
        "The cow file exceeds the 16 MiB input limit"
    );
}
