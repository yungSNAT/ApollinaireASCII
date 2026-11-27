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
        // Mermaid turns `\r\n` and a lone `\r` into `\n` before it reads anything.
        for ending in ["\r", "\r\n"] {
            let ended = format!("{FRONT}{source}").replace('\n', ending);
            assert_eq!(mermaid(&ended).unwrap().text(), plain, "{ended:?}");
        }
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
        // A top-level title in a flow mapping, or less indented than the opening, is still one.
        (
            "---\n{title: Pets}\n---\npie\n  \"a\" : 1",
            "Line 2: a title in front matter is not drawn",
        ),
        (
            "  ---\ntitle: Pets\n  ---\npie\n  \"a\" : 1",
            "Line 2: a title in front matter is not drawn",
        ),
        // A top-level explicit key may be a title, and an escaped key is not decoded.
        (
            "---\n? title\n: Pets\n---\npie\n  \"a\" : 1",
            "Line 2: an explicit key (?) at the top of front matter is not read",
        ),
        (
            "---\n\"ti\\u0074le\": Pets\n---\npie\n  \"a\" : 1",
            "Line 2: an escaped key in front matter is not read",
        ),
        (
            "---\n{\"ti\\x74le\" : Pets}\n---\npie\n  \"a\" : 1",
            "Line 2: an escaped key in front matter is not read",
        ),
        // The closing line follows at least one line of body, and a line follows it.
        (
            "---\n---\ngraph TD\n  A-->B\n",
            "The front matter opened on line 1 is not closed",
        ),
        (
            "---\nconfig: {}\n---",
            "The front matter opened on line 1 is not closed",
        ),
        (
            "---\n\n---\n",
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
    // A block scalar's lines are text, not keys.
    let described = "---\ndescription: >-\n  title: x and mainBranchName: y\n---\npie\n  \"a\" : 1";
    assert_eq!(
        mermaid(described).unwrap().text(),
        mermaid("pie\n  \"a\" : 1").unwrap().text()
    );
    // Escapes in values, a nested explicit key and an unclosed quote are read past.
    let escaped = "---\ndescription: \"a \\\"b\\\"\" and \"c\"\nconfig:\n  ? theme\n  : dark\nnote: \"open\n---\npie\n  \"a\" : 1";
    assert_eq!(
        mermaid(escaped).unwrap().text(),
        mermaid("pie\n  \"a\" : 1").unwrap().text()
    );
}

#[test]
fn the_opening_runs_on_over_blank_lines_whilst_a_closing_line_follows() {
    // Mermaid 12.3.0's matchFrontMatter: the opening takes the blank lines after it, so the
    // `---` after them is body and the next one closes it.
    assert_eq!(
        mermaid("---\n\n---\nconfig: x\n---\ngraph TD\n  A-->B\n")
            .unwrap()
            .text(),
        mermaid("graph TD\n  A-->B").unwrap().text()
    );
}

#[test]
fn front_matter_names_the_main_branch_of_a_git_graph_as_a_directive_does() {
    let branches = "gitGraph\n  commit\n  branch dev\n  commit\n  checkout trunk\n  commit";
    let directive = mermaid(&format!(
        "%%{{init: {{'gitGraph': {{'mainBranchName': 'trunk'}}}}}}%%\n{branches}"
    ))
    .unwrap()
    .text();
    assert!(directive.contains("trunk"), "{directive}");
    for value in [
        "trunk",
        "'trunk'",
        "\"trunk\"",
        "trunk  # the default branch",
    ] {
        let front = format!(
            "---\nconfig:\n  theme: dark\n  gitGraph:\n    showBranches: true\n    mainBranchName: {value}\n---\n{branches}"
        );
        assert_eq!(mermaid(&front).unwrap().text(), directive, "{value}");
    }
    let quoted = format!(
        "---\n\"config\": # settings\n  sequence:\n    mirrorActors: false\n  'gitGraph':\n    mainBranchName: 'trunk' # a comment\n---\n{branches}"
    );
    assert_eq!(mermaid(&quoted).unwrap().text(), directive);
    // In a flow mapping, on one line or several, or under other keys, it is refused with its
    // line, not dropped.
    for (front, line) in [
        ("{config: {gitGraph: {mainBranchName: trunk}}}", 2),
        ("config:\n  gitGraph: {mainBranchName: trunk}", 3),
        ("config:\n  gitGraph: { 'mainBranchName' : trunk }", 3),
        ("config:\n  gitGraph: {\n    mainBranchName: trunk\n  }", 4),
        ("{config: {gitGraph: {\n  mainBranchName: trunk}}}", 3),
        ("config:\n  mainBranchName: trunk", 3),
        ("mainBranchName: trunk", 2),
        // An explicit key, and the name as a value or alias, are refused too.
        ("config:\n  gitGraph:\n    ? mainBranchName\n    : trunk", 4),
        ("config:\n  gitGraph:\n    - mainBranchName: trunk", 4),
        (
            "name: &b trunk\nconfig:\n  gitGraph: {\"mainBranchName\": *b}",
            4,
        ),
    ] {
        assert_eq!(
            mermaid(&format!("---\n{front}\n---\n{branches}")).unwrap_err(),
            format!(
                "Line {line}: mainBranchName is read only on its own line under config: and gitGraph:"
            ),
            "{front}"
        );
    }
    // A key that only ends in the name is another setting.
    let lookalike =
        "---\nconfig:\n  xmainBranchName: trunk\n  mainBranchName_old: x\n---\ngitGraph\n  commit";
    assert_eq!(
        mermaid(lookalike).unwrap().text(),
        mermaid("gitGraph\n  commit").unwrap().text()
    );
    // A directive wins over front matter, as Mermaid merges them.
    let both = format!(
        "---\nconfig:\n  gitGraph:\n    mainBranchName: other\n---\n%%{{init: {{'gitGraph': {{'mainBranchName': 'trunk'}}}}}}%%\n{branches}"
    );
    assert_eq!(mermaid(&both).unwrap().text(), directive);
    assert_eq!(
        mermaid("---\nconfig:\n  gitGraph:\n    mainBranchName: ''\n---\ngitGraph\n  commit")
            .unwrap_err(),
        "Line 4: mainBranchName names a branch"
    );
    // An escaped key or value is not decoded, so it is refused, not read as another name.
    for front in [
        "config:\n  gitGraph:\n    \"mainBranch\\u004eame\": trunk",
        "config:\n  gitGraph:\n    ? \"mainBranch\\u004eame\"\n    : trunk",
    ] {
        assert_eq!(
            mermaid(&format!("---\n{front}\n---\n{branches}")).unwrap_err(),
            "Line 4: an escaped key in front matter is not read",
            "{front}"
        );
    }
    assert_eq!(
        mermaid(&format!(
            "---\nconfig:\n  gitGraph:\n    mainBranchName: \"tr\\u0075nk\"\n---\n{branches}"
        ))
        .unwrap_err(),
        "Line 4: an escaped mainBranchName is not read"
    );
}
