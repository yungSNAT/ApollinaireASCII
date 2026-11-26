// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! YAML front matter, as Mermaid 12.3.0 reads it (`diagram-api/regexes.ts`): `---` on the
//! first line, optionally indented, up to the first line of the same indent and `---`.
use apollinaire_ascii::diagram::mermaid;

/// One small diagram of each family.
const FAMILIES: &[&str] = &[
    "graph TD\n  A-->B",
    "sequenceDiagram\n  A->>B: hi",
    "stateDiagram-v2\n  [*] --> A",
    "classDiagram\n  A <|-- B",
    "erDiagram\n  A ||--o{ B : has",
    "gantt\n  dateFormat YYYY-MM-DD\n  A :a, 2026-01-01, 2d",
    "pie\n  \"a\" : 1",
    "gitGraph\n  commit",
    "timeline\n  2026 : A",
    "journey\n  section Day\n  Wake: 3: Me",
];

/// Front matter as Mermaid's own test fixtures write it: a description (here with a byte outside
/// printable ASCII, which is never drawn) and settings this drawing has no use for.
const FRONT: &str = "---\ndescription: A diagram \u{2014} with front matter\nconfig:\n  look: handDrawn\n  theme: dark\n  gitGraph:\n    showBranches: true\n---\n";

#[test]
fn every_family_draws_the_same_after_front_matter() {
    for source in FAMILIES {
        let plain = mermaid(source).unwrap().text();
        assert_eq!(
            mermaid(&format!("{FRONT}{source}")).unwrap().text(),
            plain,
            "{source}"
        );
        let fenced = format!("```mermaid\n{FRONT}{source}\n```\n");
        assert_eq!(mermaid(&fenced).unwrap().text(), plain, "{source}");
        let indented = format!("  ---\n  config:\n    theme: dark\n  ---\n{source}");
        assert_eq!(mermaid(&indented).unwrap().text(), plain, "{source}");
    }
}

#[test]
fn front_matter_is_refused_when_unclosed_titled_or_not_first() {
    for (source, error) in [
        (
            "---\nconfig:\n  theme: dark\npie\n  \"a\" : 1",
            "The front matter opened on line 1 is not closed",
        ),
        // A closing line at another indent, or with a fourth dash, does not close it.
        (
            "  ---\nconfig: {}\n---\npie\n  \"a\" : 1",
            "The front matter opened on line 1 is not closed",
        ),
        (
            "---\nconfig: {}\n----\npie\n  \"a\" : 1",
            "The front matter opened on line 1 is not closed",
        ),
        (
            "---\ntitle: Pets\n---\npie\n  \"a\" : 1",
            "Line 2: a title in front matter is not drawn",
        ),
        (
            "---\n\"title\" : Pets\n---\npie\n  \"a\" : 1",
            "Line 2: a title in front matter is not drawn",
        ),
        (
            "---\n---\n",
            "The text is empty: a Mermaid diagram starts with flowchart, graph, sequenceDiagram, stateDiagram-v2, classDiagram, erDiagram, gantt, pie, gitGraph, timeline or journey",
        ),
    ] {
        assert_eq!(mermaid(source).unwrap_err(), error, "{source:?}");
    }
    // A nested title is a setting, and a family's own title statement still draws.
    let nested = "---\nconfig:\n  title: x\n---\npie title Pets\n  \"a\" : 1";
    assert_eq!(
        mermaid(nested).unwrap().text(),
        mermaid("pie title Pets\n  \"a\" : 1").unwrap().text()
    );
    // Front matter after the first statement is not front matter.
    assert!(mermaid("pie\n---\n\"a\" : 1").is_err());
}
