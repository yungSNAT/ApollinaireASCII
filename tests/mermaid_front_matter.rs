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
    "mindmap\n  root",
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
            "Line 2: front matter is read only as block key: value lines",
        ),
        // A top-level flow mapping is not read; a title less indented than the opening is one.
        (
            "---\n{title: Pets}\n---\npie\n  \"a\" : 1",
            "Line 2: front matter is read only as block key: value lines",
        ),
        (
            "  ---\ntitle: Pets\n  ---\npie\n  \"a\" : 1",
            "Line 2: a title in front matter is not drawn",
        ),
        // Any other form that may name a title is outside the grammar.
        (
            "---\n? title\n: Pets\n---\npie\n  \"a\" : 1",
            "Line 2: front matter is read only as block key: value lines",
        ),
        (
            "---\n\"ti\\u0074le\": Pets\n---\npie\n  \"a\" : 1",
            "Line 2: front matter is read only as block key: value lines",
        ),
        (
            "---\nt: &t title\n*t : Pets\n---\npie\n  \"a\" : 1",
            "Line 2: front matter is read only as block key: value lines",
        ),
        (
            "---\n!!str title: Pets\n---\npie\n  \"a\" : 1",
            "Line 2: front matter is read only as block key: value lines",
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
            "The text is empty: a Mermaid diagram starts with flowchart, graph, sequenceDiagram, stateDiagram-v2, classDiagram, erDiagram, gantt, pie, gitGraph, timeline, journey or mindmap",
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
    let commented = format!(
        "---\nconfig: # settings\n  sequence:\n    mirrorActors: false\n\n  # the graph\n  gitGraph:\n    mainBranchName: 'trunk' # a comment\n---\n{branches}"
    );
    assert_eq!(mermaid(&commented).unwrap().text(), directive);
    // Mermaid dedents indented front matter by its opening's indent.
    let indented = "  ---\n  config:\n    gitGraph:\n      mainBranchName: trunk\n  ---\n  gitGraph\n    commit\n";
    assert_eq!(
        mermaid(indented).unwrap().text(),
        mermaid("%%{init: {'gitGraph': {'mainBranchName': 'trunk'}}}%%\ngitGraph\n  commit")
            .unwrap()
            .text()
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
}

const NOT_READ: &str = "front matter is read only as block key: value lines";
const OWN_LINE: &str = "mainBranchName is read only on its own line under config: and gitGraph:";
const PLAIN: &str = "mainBranchName is read only as a plain or quoted name on its line";
const TITLE: &str = "a title in front matter is not drawn";
const REPEAT: &str = "a key repeated in front matter is not YAML";

/// The front matter grammar `front_matter` in `src/diagram.rs` states, in 2 parts: forms it
/// accepts, with the main branch each draws (`main` when nothing names it), as Mermaid 12.3.0
/// reads them (js-yaml, `JSON_SCHEMA`); and forms it refuses, with the line and error, among
/// them every input an independent review found read wrongly in rounds 8 and 9, verbatim.
#[test]
fn front_matter_is_read_only_in_its_grammar() {
    let git = "gitGraph\n  commit\n  branch dev\n  commit";
    let main = |name: &str| {
        mermaid(&format!(
            "%%{{init: {{'gitGraph': {{'mainBranchName': '{name}'}}}}}}%%\n{git}"
        ))
        .unwrap()
        .text()
    };
    let under = |rest: &str| format!("config:\n  gitGraph:\n{rest}");
    let block = |value: &str| under(&format!("    mainBranchName: {value}"));
    let accepted: Vec<(String, &str)> = vec![
        (block("trunk"), "trunk"),
        (
            "config:\n  theme: base\n  themeVariables:\n    primaryColor: '#ff0000'\n  gitGraph: # settings\n    showBranches: true\n    mainBranchName: develop  # default\n    rotateCommitLabel: false".into(),
            "develop",
        ),
        (
            "description: A diagram \u{2014} with {braces}, [brackets] and a:colon\n\n# a comment\nconfig:\n  look: handDrawn\n  fontSize: 16\n  gitGraph:\n    mainBranchName: trunk".into(),
            "trunk",
        ),
        // The name inside another key's value, or in a longer key, is text.
        (
            "description: mainBranchName\nnote: \"mainBranchName here\"\nconfig:\n  look: mainBranchName\n  xmainBranchName: trunk\n  mainBranchName_old: x".into(),
            "main",
        ),
        (block("'trunk'"), "trunk"),
        (block("\"trunk\" # a comment"), "trunk"),
        (block("\"release/2.0\""), "release/2.0"),
        (block("\"1.0\""), "1.0"),
        (block("trunk  # the default branch"), "trunk"),
        (block("release/v1"), "release/v1"),
        (block("Main Line"), "Main Line"),
        (block("~trunk"), "~trunk"),
        (block("yes"), "yes"),
        // `\r\n` and a lone `\r` end lines, as Mermaid's cleanupText makes `\n`.
        (block("trunk").replace('\n', "\r\n"), "trunk"),
        (block("trunk").replace('\n', "\r"), "trunk"),
        (block("trunk\r"), "trunk"),
    ];
    let refused: Vec<(String, usize, &str)> = vec![
        // Round 8 and round 9 inputs, verbatim (the reviewer's c01 to c19, c22, c24 and c30).
        ("---\nconfig:\n  gitGraph: {\n    x: 1, mainBranchName: trunk\n  }\n---\ngitGraph\n  commit\n".into(), 3, NOT_READ),
        ("---\nconfig:\n  gitGraph: !!map {mainBranchName: trunk}\n---\ngitGraph\n  commit\n".into(), 3, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    x: \"a\n      mainBranchName: trunk\n      b\"\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    x: 'a\n    mainBranchName: trunk\n    b'\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    x: \"a\\\"\n      mainBranchName: trunk\n      \"\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    x: !!str |\n      mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    x: &a |\n      mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    - |\n      mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    ? |\n      mainBranchName: trunk\n    : x\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    - x: 1\n      mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    -\n      mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    'it''s':\n      mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    {a: 1}:\n      mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    ? \"mainBranch\\\n      Name\"\n    : trunk\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  ? gitGraph\n  : mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 3, NOT_READ),
        ("---\nconfig:\n  gitGraph: !<tag:yaml.org,2002:map> {mainBranchName: trunk}\n---\ngitGraph\n  commit\n".into(), 3, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    mainBranchName: a\n    mainBranchName: b\n---\ngitGraph\n  commit\n".into(), 5, REPEAT),
        ("---\nconfig:\n  gitGraph:\n    mainBranchName: trunk\n  gitGraph:\n    showBranches: true\n---\ngitGraph\n  commit\n".into(), 5, REPEAT),
        ("---\nconfig:\n\tgitGraph:\n\t\tmainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 3, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    \"mainBranchName\":trunk\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    x: a\n      mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 5, NOT_READ),
        ("---\nx: 1\n...\nconfig:\n  gitGraph:\n    mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 3, NOT_READ),
        // Round 10 inputs, verbatim (the reviewer's p01 to p07).
        ("---\nnull: a\nNull: b\nconfig:\n  gitGraph:\n    mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 2, NOT_READ),
        ("---\ntrue: a\nTRUE: b\nconfig:\n  gitGraph:\n    mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 2, NOT_READ),
        ("---\n  ---\nconfig:\n  gitGraph:\n    mainBranchName: trunk\n---\ngitGraph\n  commit\n".into(), 2, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    mainBranchName: <a x=\"y\">\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    mainBranchName: '<b class=\"k\">'\n---\ngitGraph\n  commit\n".into(), 4, NOT_READ),
        ("---\nconfig:\n  gitGraph:\n    mainBranchName: a#59;b\n---\ngitGraph\n  commit\n".into(), 4, PLAIN),
        ("---\nconfig:\n  gitGraph:\n    mainBranchName: trunk\u{a0}\n---\ngitGraph\n  commit\n".into(), 4, PLAIN),
        // The rewrite runs from a `<word` on one line to a `>` on a later one.
        ("---\n# <a\nconfig:\n  gitGraph:\n    mainBranchName: \"x\" # >\n---\ngitGraph\n  commit\n".into(), 2, NOT_READ),
    ];
    let wrapped: Vec<(String, usize, &str)> = vec![
        // Round 8's null, boolean and number values, and other values not read as text.
        (block("~"), 4, PLAIN),
        (block("null"), 4, PLAIN),
        (block("NULL"), 4, PLAIN),
        (block("1.0"), 4, PLAIN),
        (block("0x10"), 4, PLAIN),
        (block("true"), 4, PLAIN),
        (block("False"), 4, PLAIN),
        (block("+1"), 4, PLAIN),
        (block(".inf"), 4, PLAIN),
        (block("2026-release"), 4, PLAIN),
        (block(""), 4, PLAIN),
        (block("*b"), 4, PLAIN),
        (block("&b trunk"), 4, PLAIN),
        (block("!!str trunk"), 4, PLAIN),
        (block("|"), 4, PLAIN),
        (block("[trunk]"), 4, PLAIN),
        (block("- trunk"), 4, PLAIN),
        (block("a: b"), 4, PLAIN),
        (block("trunk:"), 4, PLAIN),
        (block("\"tr\\u0075nk\""), 4, PLAIN),
        (block("'it''s'"), 4, PLAIN),
        (block("'trunk' x"), 4, PLAIN),
        (block("'trunk'#c"), 4, PLAIN),
        (block("\"trunk"), 4, PLAIN),
        // The key anywhere else, or the name as a key in another form.
        ("config:\n  mainBranchName: trunk".into(), 3, OWN_LINE),
        ("mainBranchName: trunk".into(), 2, OWN_LINE),
        (
            "config:\n  gitGraph: {mainBranchName: trunk}".into(),
            3,
            NOT_READ,
        ),
        ("d: &a mainBranchName".into(), 2, NOT_READ),
        (under("    \"mainBranch\\u004eame\": trunk"), 4, NOT_READ),
        // A top-level title, and other lines outside the grammar.
        ("title: Pets".into(), 2, TITLE),
        ("config:\n  look: a: b".into(), 3, NOT_READ),
        ("description: >-\n  text".into(), 2, NOT_READ),
        ("description: \"a \\\"b\\\"\"".into(), 2, NOT_READ),
        ("config:\n  <<: {theme: dark}".into(), 3, NOT_READ),
        ("config :\n  look: x".into(), 2, NOT_READ),
        ("1st: x".into(), 2, NOT_READ),
        ("a: 1\n  b: 2".into(), 3, NOT_READ),
        ("  a: 1\nb: 2".into(), 3, NOT_READ),
        ("config:\n  a:\n    b:\n      c: 1".into(), 5, NOT_READ),
        (block("tr\n      unk"), 5, NOT_READ),
        ("a: 1\t".into(), 2, NOT_READ),
        ("a: x\u{2028}y".into(), 2, NOT_READ),
    ];
    let mut wrong: Vec<String> = accepted
        .into_iter()
        .filter_map(|(front, name)| {
            let drawn = mermaid(&format!("---\n{front}\n---\n{git}")).map(|chart| chart.text());
            (drawn != Ok(main(name))).then(|| format!("{front:?}: {drawn:?}"))
        })
        .collect();
    let wrapped = wrapped
        .into_iter()
        .map(|(front, line, error)| (format!("---\n{front}\n---\n{git}"), line, error));
    wrong.extend(
        refused
            .into_iter()
            .chain(wrapped)
            .filter_map(|(source, line, error)| {
                let drawn = mermaid(&source).map(|chart| chart.text());
                (drawn != Err(format!("Line {line}: {error}")))
                    .then(|| format!("{source:?}: {drawn:?}"))
            }),
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
