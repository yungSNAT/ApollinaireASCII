// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::{Frame, boxes};

fn draw(design: &boxes::Design, text: &str) -> String {
    design
        .draw(&Frame::from_text(text).unwrap())
        .unwrap()
        .text()
}

/// One design `a` holding `body`, in a file of its own.
fn file(body: &str) -> String {
    format!("BOX a\n{body}\nEND a\n")
}

fn error(text: &str) -> String {
    boxes::parse(text).unwrap_err()
}

const VALID: &str = "shapes { n (\"-\") }\nelastic (n)\nsample\n-\nends";

#[test]
fn bundled_designs_draw_their_samples() {
    let designs = boxes::bundled();
    let names: Vec<_> = designs.iter().map(boxes::Design::name).collect();
    assert_eq!(
        names,
        [
            "curly", "zigzag", "rail", "film", "ticket", "shadow", "bracket", "crest"
        ]
    );
    for design in &designs {
        let drawn = draw(design, "Hi");
        let trimmed: Vec<_> = drawn.lines().map(str::trim_end).collect();
        assert_eq!(trimmed.join("\n"), design.sample(), "{}", design.name());
    }
    assert_eq!(
        boxes::design("CURLY").unwrap(),
        designs[0],
        "names ignore letter case"
    );
    assert_eq!(
        boxes::design("nope").unwrap_err(),
        "Unknown box design nope; run `apollinaire list` for the designs"
    );
}

#[test]
fn elastic_shapes_repeat_evenly_and_the_box_grows_to_fit_them() {
    let design = |name| boxes::design(name).unwrap();
    // 2 elastic shapes either side of a fixed one share the repeats.
    assert_eq!(
        draw(&design("crest"), "Hello"),
        "+==<o>==+\n| Hello |\n+=======+"
    );
    // A 2-column shape cannot make 5 columns, so the box takes 6 and the text sits left of
    // the middle.
    assert_eq!(
        draw(&design("zigzag"), "Hey"),
        "+/\\/\\/\\+\n< Hey  >\n+\\/\\/\\/+"
    );
    assert_eq!(
        draw(&design("film"), "ab\ncd"),
        "+------+\n|[] [] |\n|  ab  |\n|  cd  |\n|[] [] |\n+------+"
    );
    // Open sides between corners are blank.
    assert_eq!(
        draw(&design("bracket"), "a\nb"),
        "+- -+\n| a |\n| b |\n+- -+"
    );
    assert_eq!(
        draw(&design("shadow"), "Hi"),
        "+----+ \n| Hi |#\n+----+#\n ######"
    );
}

#[test]
fn boxes_too_big_for_the_canvas_are_refused() {
    let wide = Frame::from_text(&"x".repeat(511)).unwrap();
    assert_eq!(
        boxes::design("curly").unwrap().draw(&wide).unwrap_err(),
        "Box design curly would need more than 512 columns"
    );
    let tall = Frame::from_text(&vec!["x"; 253].join("\n")).unwrap();
    assert_eq!(
        boxes::design("film").unwrap().draw(&tall).unwrap_err(),
        "Box design film would need more than 256 rows"
    );
    // 252 rows fit between the film's 2-row top and bottom.
    let fits = Frame::from_text(&vec!["x"; 252].join("\n")).unwrap();
    assert_eq!(
        boxes::design("film").unwrap().draw(&fits).unwrap().height(),
        256
    );
}

#[test]
fn design_files_take_aliases_delimiters_padding_and_ignored_entries() {
    let text = "# A rule above and below, with no sides.\n\
        BOX rule, line # aliases follow commas\n\
        author \"someone\"\n\
        tags (\"simple\", \"rule\")\n\
        tags \"old style\"\n\
        reverse once \"x\" to \"y\"\n\
        reverse \"x\" to \"y\"\n\
        delim ?'\n\
        sample\n\
        ---\n\
        \x20Hi\n\
        ---\n\
        ends\n\
        shapes { n ('-') s ('?'') }\n\
        elastic (n, s)\n\
        padding { all 0 vertical 1 left 1 right 0 }\n\
        indent 'box'\n\
        END rule\n\
        BOX sides\n\
        SHAPES { N (\"-\") W (\"|\") E (\"|\") S (\"-\") }\n\
        Elastic ( n , e , s , w )\n\
        Padding { horiz 2 vert 0 top 1 right 1 bottom 0 left 0 horizontal 0 }\n\
        Sample\nx\nEnds\n\
        End sides\n";
    let designs = boxes::parse(text).unwrap();
    let rule = boxes::find(&designs, "LINE").unwrap();
    assert_eq!(rule.name(), "rule");
    assert_eq!(rule.sample(), "---\n Hi\n---");
    assert_eq!(draw(rule, "Hi"), "---\n   \n Hi\n   \n'''");
    // Missing corners are blank; top padding 1, the rest 0.
    let sides = boxes::find(&designs, "sides").unwrap();
    assert_eq!(draw(sides, "Hi"), " -- \n|  |\n|Hi|\n -- ");
    assert!(boxes::find(&designs, "none").is_none());
    // CRLF line ends read the same.
    assert_eq!(boxes::parse(&text.replace('\n', "\r\n")).unwrap(), designs);
}

#[test]
fn design_files_refuse_what_they_cannot_draw_with_its_line() {
    for (text, expected) in [
        (
            "PARENT \"x\"",
            "Line 1: PARENT is not read: put every design in one file",
        ),
        ("BIX a", "Line 1: expected BOX, found BIX"),
        ("\n(", "Line 2: expected BOX"),
        ("# nothing\n", "The design file holds no BOX design"),
        (
            "BOX 1a",
            "Line 1: expected a design name starting with a letter",
        ),
        (
            "BOX a\n(",
            "Line 2: expected an entry such as SHAPES, or END",
        ),
        (
            "BOX a\nshapes { n (\"-\") }",
            "Line 1: Box design a has no END line",
        ),
        (
            &format!("BOX a\n{VALID}\nEND b"),
            "Line 7: END b does not close BOX a",
        ),
        (
            &file("parent \"x\""),
            "Line 2: PARENT is not read: put every design in one file",
        ),
        (
            &file("replace \"a\" with \"b\""),
            "Line 2: REPLACE rewrites the text in the box and is not supported",
        ),
        (&file("shapes n"), "Line 2: expected {"),
        (
            &file("shapes { n (x) }"),
            "Line 2: expected a string between \" marks",
        ),
        (
            &file("shapes { n (\"-) }"),
            "Line 2: a string is not closed on its line",
        ),
        (
            &file("shapes { n (\"-\\\n\") }"),
            "Line 2: a string is not closed on its line",
        ),
        (
            &file("shapes { north (\"-\") }"),
            "Line 2: expected a shape name such as nw, found \"north\"",
        ),
        (
            &file("shapes { n (\"-\") n (\"=\") }"),
            "Line 2: the shape n is given twice",
        ),
        (
            &file("shapes { n (\"\u{1b}[31m-\") }"),
            "Line 2: the shape n holds byte 0x1b; shapes are printable ASCII, without colour codes",
        ),
        (
            &file("shapes { n (\"\u{e9}\") }"),
            "Line 2: the shape n holds byte 0xc3; shapes are printable ASCII, without colour codes",
        ),
        (
            &file("shapes { n (\"--\", \"-\") }"),
            "Line 2: the lines of the shape n must all have the same width, at least 1",
        ),
        (
            &file("shapes { n (\"\") }"),
            "Line 2: the lines of the shape n must all have the same width, at least 1",
        ),
        (&file("elastic (n"), "Line 3: expected )"),
        (
            &file("padding { middle 1 }"),
            "Line 2: expected a padding side such as all, found \"middle\"",
        ),
        (
            &file("padding { all x }"),
            "Line 2: expected a padding of 0 to 512",
        ),
        (
            &file("padding { all 513 }"),
            "Line 2: expected a padding of 0 to 512",
        ),
        (&file("sample\nx"), "Line 2: SAMPLE has no ENDS line"),
        // The line count carries on past a sample.
        (
            &file("sample\nx\nends\nbogus x"),
            "Line 5: expected a string between \" marks",
        ),
        (
            "BOX a\ndelim ?",
            "Line 2: DELIM takes an escape character and a quote",
        ),
        (
            &file("delim ab"),
            "Line 2: DELIM takes an escape character from @~?!\\ and a different quote from \"~'!|",
        ),
        (
            &file("delim !!"),
            "Line 2: DELIM takes an escape character from @~?!\\ and a different quote from \"~'!|",
        ),
        (
            &file("shapes { n (\"-\") }\nelastic (n)"),
            "Line 1: Box design a has no SAMPLE entry",
        ),
        (
            &file("shapes { n (\"-\") }\nsample\n-\nends"),
            "Line 1: Box design a has no ELASTIC entry",
        ),
        (
            &file("elastic (n)\nsample\n-\nends"),
            "Line 1: Box design a has no SHAPES entry",
        ),
        (
            &file("shapes { }\nelastic (n)\nsample\nends"),
            "Line 1: Box design a gives no shapes",
        ),
        (
            &file("shapes { n (\"-\") }\nelastic (n, s)\nsample\nends"),
            "Line 1: Box design a: the elastic shape s is not given",
        ),
        (
            &file("shapes { nw (\"+\") n (\"-\") }\nelastic (nw, n)\nsample\nends"),
            "Line 1: Box design a: the corner nw cannot be elastic",
        ),
        (
            &file("shapes { nw (\"+\", \"+\") n (\"-\") }\nelastic (n)\nsample\nends"),
            "Line 1: Box design a: the shapes nw, n must all have the same height",
        ),
        (
            &file("shapes { nw (\"++\") n (\"-\") w (\"|\") }\nelastic (n, w)\nsample\nends"),
            "Line 1: Box design a: the shapes nw, w must all have the same width",
        ),
        (
            &file("shapes { nnw (\"-\") n (\"=\") }\nelastic (nnw, n)\nsample\nends"),
            "Line 1: Box design a: nnw and n are neighbours, so only one may be elastic",
        ),
        (
            &file("shapes { n (\"-\") w (\"|\") }\nelastic (w)\nsample\nends"),
            "Line 1: Box design a: one of nnw, n, nne must be elastic",
        ),
        (
            &format!("BOX a\n{VALID}\nEND a\nBOX b, A\n{VALID}\nEND b"),
            "Line 8: Box design A is defined twice",
        ),
        (
            &file("reverse \"x\" with \"y\""),
            "Line 2: REVERSE takes once or global, if either, then \"pattern\" TO \"text\"",
        ),
        (
            &file("reverse always \"x\" to \"y\""),
            "Line 2: REVERSE takes once or global, if either, then \"pattern\" TO \"text\"",
        ),
        (
            &file("indent \"left\""),
            "Line 2: INDENT takes \"box\", \"text\" or \"none\", not \"left\"",
        ),
    ] {
        assert_eq!(error(text), expected, "{text}");
    }
    assert!(boxes::parse(&file(VALID)).is_ok());
    // Names ignore letter case at END too.
    assert!(boxes::parse(&format!("BOX a\n{VALID}\nEND A")).is_ok());
    assert!(
        boxes::parse(&file("REVERSE GLOBAL \"x\" TO \"y\"")).is_err_and(|e| e.contains("SHAPES"))
    );
}

#[test]
fn design_files_past_the_input_limit_are_refused_before_reading() {
    assert_eq!(
        error(&" ".repeat(apollinaire_ascii::MAX_INPUT_BYTES + 1)),
        "The design file exceeds the 16 MiB input limit"
    );
    // A long sample is read in one pass.
    let long = file(&format!(
        "shapes {{ n (\"-\") }}\nelastic (n)\nsample\n{}ends",
        "\n".repeat(1 << 20)
    ));
    assert_eq!(
        boxes::parse(&long).unwrap()[0].sample(),
        "\n".repeat((1 << 20) - 1)
    );
}

/// A plain box of `+-|` around the frame, with `entries` added.
fn plain(entries: &str) -> boxes::Design {
    let body = format!(
        "shapes {{ nw (\"+\") n (\"-\") ne (\"+\") e (\"|\") se (\"+\") s (\"-\") sw (\"+\") \
         w (\"|\") }}\nelastic (n, e, s, w)\n{entries}\nsample\nx\nends"
    );
    boxes::parse(&file(&body)).unwrap().remove(0)
}

#[test]
fn indent_modes_place_indented_art_as_the_boxes_documentation_describes() {
    let art = "  Hi\n   !";
    // box, the default: the box moves in by the art's indentation, which the art loses.
    let moved = "  +---+\n  |Hi |\n  | ! |\n  +---+";
    assert_eq!(draw(&plain(""), art), moved);
    assert_eq!(draw(&plain("INDENT \"Box\""), art), moved);
    // text: the box stays at the left edge and the art keeps its indentation.
    assert_eq!(
        draw(&plain("indent \"text\""), art),
        "+-----+\n|  Hi |\n|   ! |\n+-----+"
    );
    // none: the indentation goes.
    assert_eq!(
        draw(&plain("indent \"none\""), art),
        "+---+\n|Hi |\n| ! |\n+---+"
    );
    // Blank rows do not count towards the indentation.
    assert_eq!(draw(&plain(""), "   \n  a"), "  +-+\n  | |\n  |a|\n  +-+");
    // The indentation counts towards the canvas's 512 columns.
    let fits = Frame::from_text(&format!("  {}", "x".repeat(508))).unwrap();
    assert_eq!(plain("").draw(&fits).unwrap().width(), 512);
    let wide = Frame::from_text(&format!("  {}", "x".repeat(509))).unwrap();
    assert_eq!(
        plain("").draw(&wide).unwrap_err(),
        "Box design a would need more than 512 columns"
    );
}

#[test]
fn a_side_of_spaces_alone_is_left_out() {
    let open_left = boxes::parse(&file(
        "shapes { nw (\" \") w (\" \") sw (\" \") n (\"-\") ne (\"+\") e (\"|\") se (\"+\") \
         s (\"-\") }\nelastic (n, e, s, w)\nsample\nx\nends",
    ))
    .unwrap()
    .remove(0);
    assert_eq!(draw(&open_left, "Hi"), "--+\nHi|\n--+");
    let open_top = boxes::parse(&file(
        "shapes { nw (\" \") n (\"  \") ne (\" \") w (\"|\") e (\"|\") sw (\"+\") s (\"-\") \
         se (\"+\") }\nelastic (n, e, s, w)\nsample\nx\nends",
    ))
    .unwrap()
    .remove(0);
    assert_eq!(draw(&open_top, "Hi"), "|Hi|\n+--+");
}

#[test]
fn elastic_shapes_of_different_widths_balance_their_repeat_counts() {
    // 5 spare columns: 1 of the 1-wide shape and 2 of the 2-wide one are closer in count than
    // 3 and 1.
    let design = boxes::parse(&file(
        "shapes { nw (\"+\") nnw (\"-\") n (\"o\") nne (\"==\") ne (\"+\") e (\"|\") se (\"+\") \
         s (\"-\") sw (\"+\") w (\"|\") }\nelastic (nnw, nne, e, s, w)\nsample\nx\nends",
    ))
    .unwrap()
    .remove(0);
    assert_eq!(draw(&design, "abcdef"), "+-o====+\n|abcdef|\n+------+");
}
