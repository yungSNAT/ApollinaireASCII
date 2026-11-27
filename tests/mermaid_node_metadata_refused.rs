// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid node metadata forms refused with their line, as Mermaid 12.3.0 reads them.

use apollinaire_ascii::diagram::flowchart;

/// Anything Mermaid refuses, draws without text or with a setting not drawn here, and any
/// form not proven to read as Mermaid reads it, is refused with its line.
#[test]
fn node_metadata_outside_its_grammar_is_refused_with_its_line() {
    let refused = [
        (
            "A@{ shape: sm-circ }",
            "Line 2: the shape sm-circ is drawn with no text",
        ),
        (
            "A@{ shape: start }",
            "Line 2: the shape start is drawn with no text",
        ),
        (
            "A@{ shape: small-circle }",
            "Line 2: the shape small-circle is drawn with no text",
        ),
        (
            "A@{ shape: fr-circ }",
            "Line 2: the shape fr-circ is drawn with no text",
        ),
        (
            "A@{ shape: stop }",
            "Line 2: the shape stop is drawn with no text",
        ),
        (
            "A@{ shape: framed-circle }",
            "Line 2: the shape framed-circle is drawn with no text",
        ),
        (
            "A@{ shape: hourglass }",
            "Line 2: the shape hourglass is drawn with no text",
        ),
        (
            "A@{ shape: collate }",
            "Line 2: the shape collate is drawn with no text",
        ),
        (
            "A@{ label: m } & A[n] --> B",
            "Line 2: a node named twice in one group is not read",
        ),
        (
            "X --> A@{ label: m } & A[n]",
            "Line 2: a node named twice in one group is not read",
        ),
        (
            "A@{ label: m } & A[n]",
            "Line 2: a node named twice in one group is not read",
        ),
        (
            "A & B & A --> C",
            "Line 2: a node named twice in one group is not read",
        ),
        ("A@{ shape: \" rect \" }", "Line 2: no such shape:  rect "),
        ("A@{ shape: ' rect ' }", "Line 2: no such shape:  rect "),
        (
            "A@{ label: 'a^b' }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A --> B\nL_A_B_0@{ label: x }",
            "Line 3: the node name L_A_B_0 can be a link's name",
        ),
        (
            "A --> B\nL_A_B_0",
            "Line 3: the node name L_A_B_0 can be a link's name",
        ),
        (
            "L_x_y_12 --> B",
            "Line 2: the node name L_x_y_12 can be a link's name",
        ),
        (
            "A@{ label: fa:fa-user }",
            "Line 2: node metadata at column 2 has a label Mermaid draws as other than text",
        ),
        (
            "A@{ label: \"a $$xy$$\" }",
            "Line 2: node metadata at column 2 has a label Mermaid draws as other than text",
        ),
        ("A@{ shape: Rect }", "Line 2: no such shape: Rect"),
        (
            "A@{ shape: lean_right }",
            "Line 2: no such shape: lean_right",
        ),
        ("A@{ shape: squircle }", "Line 2: no such shape: squircle"),
        (
            "A@{ shape: constructor }",
            "Line 2: no such shape: constructor",
        ),
        ("A@{ shape: state }", "Line 2: no such shape: state"),
        ("A@{ shape: icon }", "Line 2: no such shape: icon"),
        (
            "A@{ shape: f-circ }",
            "Line 2: the shape f-circ is drawn with no text",
        ),
        (
            "A@{ shape: junction }",
            "Line 2: the shape junction is drawn with no text",
        ),
        (
            "A@{ shape: fork }",
            "Line 2: the shape fork is drawn with no text",
        ),
        (
            "A@{ shape: join }",
            "Line 2: the shape join is drawn with no text",
        ),
        (
            "A@{ shape: bolt }",
            "Line 2: the shape bolt is drawn with no text",
        ),
        (
            "A@{ shape: cross-circ }",
            "Line 2: the shape cross-circ is drawn with no text",
        ),
        (
            "A@{ shape: summary }",
            "Line 2: the shape summary is drawn with no text",
        ),
        (
            "A@{ icon: fa:user }",
            "Line 2: node metadata icon is not drawn",
        ),
        (
            "A@{ shape: rect, img: x.png }",
            "Line 2: node metadata img is not drawn",
        ),
        (
            "A@{ animate: true }",
            "Line 2: node metadata animate is not drawn",
        ),
        (
            "A@{ shape: rect, shape: hex }",
            "Line 2: node metadata names shape twice",
        ),
        (
            "A@{ shape: rect",
            "Line 2: the node metadata at column 2 is not closed",
        ),
        (
            "A@{ shape: rect,",
            "Line 2: the node metadata at column 2 is not closed",
        ),
        (
            "A@{ shape: rect, }",
            "Line 2: node metadata at column 2 holds an empty entry",
        ),
        (
            "A@{ shape:rect }",
            "Line 2: node metadata at column 2 is not key: value",
        ),
        (
            "A@{ rect }",
            "Line 2: node metadata at column 2 is not key: value",
        ),
        (
            "A@{ label: }",
            "Line 2: node metadata at column 2 has an empty label",
        ),
        (
            "A@{ label: \"\" }",
            "Line 2: node metadata at column 2 has an empty label",
        ),
        (
            "A@{ label: 12 }",
            "Line 2: node metadata at column 2 has a value YAML reads as other than text: 12",
        ),
        (
            "A@{ label: null }",
            "Line 2: node metadata at column 2 has a value YAML reads as other than text: null",
        ),
        (
            "A@{ label: True }",
            "Line 2: node metadata at column 2 has a value YAML reads as other than text: True",
        ),
        (
            "A@{ label: ~ }",
            "Line 2: node metadata at column 2 has a value YAML reads as other than text: ~",
        ),
        (
            "A@{ label: .5 }",
            "Line 2: node metadata at column 2 has a value YAML reads as other than text: .5",
        ),
        (
            "A@{ label: a: b }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: a #b }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: a [b] }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: a^b }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: it's }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: *x }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: \"a\\nb\" }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: 'it''s' }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: 'a}b' }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: \"a\" b }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: `x` }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: \"`x`\" }",
            "Line 2: node metadata at column 2 has a label Mermaid draws as other than text",
        ),
        (
            "A@{ label: a*b* }",
            "Line 2: node metadata at column 2 has a label Mermaid draws as other than text",
        ),
        (
            "A@{ label: x_y }",
            "Line 2: node metadata at column 2 has a label Mermaid draws as other than text",
        ),
        (
            "A@{ label: a~b }",
            "Line 2: node metadata at column 2 has a label Mermaid draws as other than text",
        ),
        (
            "A@{ label: a&amp;b }",
            "Line 2: node metadata at column 2 has a label Mermaid draws as other than text",
        ),
        (
            "A@{ label: \"a<br>b\" }",
            "Line 2: node metadata at column 2 has a label Mermaid draws as other than text",
        ),
        (
            "A@{ label: \"a [b]\" }",
            "Line 2: node metadata at column 2 has a label Mermaid draws as other than text",
        ),
        (
            "A@{ label: http://x }",
            "Line 2: node metadata at column 2 has a label Mermaid draws as other than text",
        ),
        (
            "A@{ label: Www.x.com }",
            "Line 2: node metadata at column 2 has a label Mermaid draws as other than text",
        ),
        (
            "A@{ label: a@b }",
            "Line 2: node metadata at column 2 has a label Mermaid draws as other than text",
        ),
        (
            "A@{ label: \"a\\b\" }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        ("A@{ label: \"a\tb\" }", "Line 2: text holds a tab"),
        (
            "A:::c@{ shape: rect }",
            "Line 2: node metadata after a class is not read",
        ),
        (
            "A @{ shape: rect }",
            "Line 2: expected a link such as --> at column 3",
        ),
        (
            "A@{ label: x }:::c",
            "Line 2: expected a link such as --> at column 15",
        ),
        (
            "A@{ label: x }[y]",
            "Line 2: expected a link such as --> at column 15",
        ),
        (
            "A@{ shape: rect }@{ label: x }",
            "Line 2: expected a link such as --> at column 18",
        ),
        ("A@{ Label: x }", "Line 2: node metadata Label is not drawn"),
        (
            "A@{ label: x, label: y }",
            "Line 2: node metadata names label twice",
        ),
        (
            "A@{ label: a, b }",
            "Line 2: node metadata at column 2 is not key: value",
        ),
        (
            "A@{ \"label\": x }",
            "Line 2: node metadata at column 2 is not key: value",
        ),
        (
            "A@{ shape: }",
            "Line 2: node metadata at column 2 has an empty shape",
        ),
        (
            "A@{ shape: filled-circle }",
            "Line 2: the shape filled-circle is drawn with no text",
        ),
        (
            "A@{ shape: crossed-circle }",
            "Line 2: the shape crossed-circle is drawn with no text",
        ),
        (
            "A@{ shape: com-link }",
            "Line 2: the shape com-link is drawn with no text",
        ),
        (
            "A@{ shape: lightning-bolt }",
            "Line 2: the shape lightning-bolt is drawn with no text",
        ),
        (
            "A@{ label: \"  \" }",
            "Line 2: node metadata at column 2 has an empty label",
        ),
        (
            "A@{ label: -.5 }",
            "Line 2: node metadata at column 2 has a value YAML reads as other than text: -.5",
        ),
        (
            "A@{ label: FALSE }",
            "Line 2: node metadata at column 2 has a value YAML reads as other than text: FALSE",
        ),
        (
            "A@{ label: x #59 y }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: x # y }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: x: }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        (
            "A@{ label: \"x }",
            "Line 2: the node metadata at column 2 is not closed",
        ),
        (
            "A@{ label: #59;x }",
            "Line 2: node metadata at column 2 has a value outside plain or quoted text",
        ),
        ("A@{	label: x }", "Line 2: text holds a tab"),
    ];
    let mut wrong = Vec::new();
    for (body, error) in refused {
        let got = flowchart(&format!("flowchart LR\n{body}")).map(|f| f.text());
        if got.as_ref().err().map(String::as_str) != Some(error) {
            wrong.push(format!("{body:?}: {got:?}, want {error:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "not refused as expected:\n{}",
        wrong.join("\n")
    );
}
