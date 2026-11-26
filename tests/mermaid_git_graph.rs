// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::diagram::{git_graph, mermaid, mermaid_with_max_width};
use std::{fs, path::Path};

/// Each `tests/fixtures/git-graph/*.mmd` drawn matches the `.txt` beside it. A
/// `%% max-width: N` first line draws it within N columns. `BLESS=1` rewrites the expected files.
#[test]
fn git_graph_fixtures_match_their_golden_drawings() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/git-graph");
    let mut checked = 0;
    for entry in fs::read_dir(&folder).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "mmd") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
        let width = source
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("%% max-width: "))
            .map(|n| n.trim().parse::<usize>().unwrap());
        let frame = match width {
            Some(width) => mermaid_with_max_width(&source, width),
            None => mermaid(&source),
        }
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let drawn = frame.text() + "\n";
        assert!(drawn.bytes().all(|b| b == b'\n' || (32..=126).contains(&b)));
        if let Some(width) = width {
            assert!(frame.width() <= width, "{}:\n{drawn}", path.display());
        }
        let golden = path.with_extension("txt");
        if std::env::var_os("BLESS").is_some() {
            fs::write(&golden, &drawn).unwrap();
        }
        let expected = fs::read_to_string(&golden)
            .unwrap_or_else(|error| panic!("{}: {error}", golden.display()))
            .replace("\r\n", "\n");
        assert_eq!(drawn, expected, "{}", path.display());
        checked += 1;
    }
    assert_eq!(checked, 8);
}

/// The drawing's rows without the right padding.
fn rows(source: &str) -> Vec<String> {
    let text = git_graph(source).unwrap().text();
    text.lines()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

#[test]
fn git_graph_joins_each_commit_to_its_parents() {
    // The first commit on a branch comes from the last commit on main above it.
    assert_eq!(
        rows("gitGraph\ncommit id: \"A\"\nbranch dev\ncommit id: \"B\"\ncheckout main\nmerge dev"),
        [
            "main",
            "|  dev",
            "o  |  A",
            "+->o  B",
            "@<-+",
            "o commit  @ merge"
        ]
    );
    // When main moves on first, the line leaves from the branch point and runs down the lane.
    assert_eq!(
        rows(
            "gitGraph\ncommit id: \"A\"\nbranch dev\ncheckout main\ncommit id: \"B\"\ncheckout dev\ncommit id: \"C\""
        ),
        [
            "main", "|  dev", "o->+  A", "o  |  B", "|  o  C", "o commit"
        ]
    );
    // Merging main into a branch brings main's head across to the branch's merge commit.
    assert_eq!(
        rows(
            "gitGraph\ncommit\nbranch dev\ncommit\ncheckout main\ncommit\ncheckout dev\nmerge main"
        )[5],
        "+->@"
    );
}

#[test]
fn git_graph_crosses_lanes_between_parent_and_child() {
    let drawn = rows(
        "gitGraph\ncommit\nbranch a\nbranch b\ncommit\ncheckout main\nmerge b\ncheckout a\ncommit",
    );
    assert_eq!(
        drawn,
        [
            "main",
            "|  a",
            "|  |  b",
            "o->+  |",
            "+--|->o",
            "@<-|--+",
            "|  o  |",
            "o commit  @ merge"
        ]
    );
}

#[test]
fn git_graph_orders_lanes_as_mermaid_does() {
    // Branches without an order follow main in the order made, then those with one by its value.
    let drawn = rows("gitGraph\ncommit\nbranch x order: 1\nbranch y\nbranch z order: 0");
    assert_eq!(&drawn[..4], ["main", "|  z", "|  |  y", "|  |  |  x"]);
    // Mermaid reads the place as a decimal, so the 10th branch made sits beside the 1st.
    let source: String = (1..=10).fold("gitGraph\ncommit\n".into(), |text, n| {
        text + &format!("branch b{n}\n")
    });
    let names: Vec<String> = rows(&source)[..11]
        .iter()
        .map(|row| row.trim_start_matches(['|', ' ']).to_owned())
        .collect();
    assert_eq!(
        names,
        [
            "main", "b1", "b10", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9"
        ]
    );
}

#[test]
fn git_graph_draws_upward_and_names_the_main_branch() {
    let down = rows("gitGraph TB:\ncommit id: \"A\"\ncommit id: \"B\"");
    assert_eq!(
        down,
        rows("gitGraph LR:\ncommit id: \"A\"\ncommit id: \"B\"")
    );
    assert_eq!(down, rows("gitGraph\ncommit id: \"A\"\ncommit id: \"B\""));
    assert_eq!(
        rows("gitGraph BT:\ncommit id: \"A\"\ncommit id: \"B\""),
        ["o  B", "o  A", "main", "o commit"]
    );
    assert_eq!(
        rows(
            "%%{init: {'gitGraph': {'mainBranchName': 'trunk'}}}%%\ngitGraph\ncommit\ncheckout trunk"
        ),
        ["trunk", "o", "o commit"]
    );
    // An empty graph is the main branch alone, as Mermaid draws it.
    assert_eq!(rows("gitGraph"), ["main"]);
}

#[test]
fn git_graph_labels_ids_tags_and_cherry_picks() {
    let drawn = rows(
        "gitGraph\ncommit id: \"A\" tag: \"v1\" tag: \"stable\"\nbranch b\ncommit id: \"M1\"\ncheckout main\nmerge b\nmerge b id: \"never\"",
    );
    assert_eq!(drawn[2], "o  |  A [v1] [stable]");
    assert_eq!(drawn[4], "@<-+");
    let picked = rows(
        "gitGraph\ncommit id: \"A\"\nbranch b\ncommit id: \"B\"\ncheckout main\ncommit\ncherry-pick id: \"B\"\ncheckout b\ncherry-pick id: \"A\" tag: \"\"",
    );
    assert_eq!(picked[2], "o->+  A");
    assert_eq!(picked[5], "%<-+  [cherry-pick:B]");
    assert_eq!(picked[6], "|  %");
    let merged = rows(
        "gitGraph\ncommit id: \"A\"\nbranch b\ncommit id: \"B\"\ncheckout main\ncommit id: \"C\"\nmerge b id: \"M\"\ncheckout b\ncherry-pick id: \"M\" parent: \"C\"",
    );
    assert_eq!(merged.last().unwrap(), "o commit  @ merge  % cherry-pick");
    assert!(
        merged
            .iter()
            .any(|row| row.ends_with("[cherry-pick:M|parent:C]")),
        "{merged:?}"
    );
}

#[test]
fn git_graph_refuses_what_mermaid_refuses_with_the_line() {
    for (source, error) in [
        (
            "gitGraph\ncheckout dev",
            "Line 2: there is no branch dev to check out; make it with branch dev",
        ),
        (
            "gitGraph\nbranch main",
            "Line 2: branch main already exists; use checkout main to work on it",
        ),
        (
            "gitGraph\nmerge main",
            "Line 2: cannot merge branch main into itself",
        ),
        (
            "gitGraph\nbranch dev\ncommit\ncheckout main\nmerge dev",
            "Line 5: branch main has no commits to merge into",
        ),
        (
            "gitGraph\ncommit\nmerge dev",
            "Line 3: there is no branch dev to merge",
        ),
        (
            "gitGraph\ncommit\nbranch dev\ncheckout main\nmerge dev",
            "Line 5: branch dev has no commits that main lacks",
        ),
        (
            "gitGraph\ncommit\nbranch dev\nmerge main",
            "Line 4: cannot merge branch main into itself",
        ),
        (
            "gitGraph\ncommit id: \"A\"\ncommit id: \"A\"",
            "Line 3: a commit with id \"A\" already exists; use another id",
        ),
        (
            "gitGraph\ncommit\ncherry-pick id: \"Z\"",
            "Line 3: cherry-pick needs the id: of a commit already drawn",
        ),
        (
            "gitGraph\ncommit id: \"A\"\ncherry-pick id: \"A\"",
            "Line 3: the commit picked is already on this branch",
        ),
        (
            "gitGraph\nbranch b\ncommit id: \"A\"\ncheckout main\ncherry-pick id: \"A\"",
            "Line 5: cherry-pick needs a commit on this branch first",
        ),
        (
            "gitGraph\ncommit id: \"A\"\nbranch b\ncommit id: \"B\"\ncheckout main\nmerge b id: \"M\"\ncheckout b\ncherry-pick id: \"M\"",
            "Line 8: cherry-picking a merge needs the parent: it is picked against",
        ),
        (
            "gitGraph\ncommit id: \"A\"\nbranch b\ncommit id: \"B\"\ncheckout main\nmerge b id: \"M\"\ncheckout b\ncherry-pick id: \"M\" parent: \"Q\"",
            "Line 8: the parent: given is not an immediate parent of the commit picked",
        ),
        (
            "gitGraph\ncommit type: BOLD",
            "Line 2: type: is NORMAL, REVERSE or HIGHLIGHT",
        ),
        ("gitGraph\ncommit id: A", "Line 2: id: takes quoted text"),
        (
            "gitGraph\ncommit order: 1",
            "Line 2: 'order' is not read here; this statement takes id:, msg:, tag:, type:",
        ),
        (
            "gitGraph\nbranch dev order: first",
            "Line 2: a branch is written branch name, with an optional order: N",
        ),
        ("gitGraph\nbranch -dev", "Line 2: a branch name is expected"),
        (
            "gitGraph\ncommit id: \"A",
            "Line 2: the text's \" is not closed",
        ),
        (
            "gitGraph\nreset main",
            "Line 2: 'reset main' is not a Git graph statement; use commit, branch, checkout, merge or cherry-pick",
        ),
        (
            "gitGraph RL:",
            "Line 1: a Git graph starts with gitGraph, gitGraph LR:, gitGraph TB: or gitGraph BT:",
        ),
        (
            "gitGraph\n%%{init: {'gitGraph': {'mainBranchName': 'trunk'}}}%%",
            "Line 2: mainBranchName is read from a directive before gitGraph, as a quoted name",
        ),
    ] {
        assert_eq!(git_graph(source).unwrap_err(), error, "{source}");
    }
    assert_eq!(
        mermaid_with_max_width("gitGraph\ncommit\nbranch a\nbranch b", 9).unwrap_err(),
        "This Git graph needs a --max-width of at least 10 columns"
    );
}

#[test]
fn git_graph_draws_only_printable_ascii_and_refuses_other_bytes() {
    for source in [
        "gitGraph\ncommit id: \"caf\u{e9}\"",
        "gitGraph\nbranch \"f\u{fc}r\"",
    ] {
        let error = mermaid(source).unwrap_err();
        assert!(error.contains("not printable ASCII"), "{error}");
    }
    assert_eq!(
        git_graph("gitGraph\ncommit tag: \"#8364;\"").unwrap_err(),
        "Line 2: #8364; is not an entity for a printable ASCII character"
    );
    let long = format!(
        "gitGraph\ntitle {}\ncommit id: \"{}\"\nbranch \"{}\"\ncommit",
        "word ".repeat(40),
        "label ".repeat(30),
        "name-".repeat(20)
    );
    for width in [7, 12, 40] {
        let frame = mermaid_with_max_width(&long, width).unwrap();
        assert!(frame.width() <= width, "{width}:\n{}", frame.text());
        assert!(
            frame
                .text()
                .bytes()
                .all(|b| b == b'\n' || (32..=126).contains(&b))
        );
    }
}
