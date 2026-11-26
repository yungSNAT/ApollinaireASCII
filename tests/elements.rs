// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Named elements through the editor's side of `mcp --live`.

mod element_support;

use element_support::*;

#[test]
fn an_agent_adds_moves_restyles_retexts_and_deletes_by_id() {
    let start = project(&[("Base", false, &["..........\n..........\n.........."])]);
    let (added, p) = apply(
        &start,
        json!({"id": 1, "op": "element_add", "kind": "box", "x": 0, "y": 0, "width": 5, "height": 3, "text": "db"}),
    );
    assert_eq!(added["element"], "e1");
    assert_eq!(added["changed"], 15);
    assert_eq!(text(&p, 0, 0), "+---+.....\n|db |.....\n+---+.....");
    let (line, p) = apply(
        &p,
        json!({"id": 2, "op": "element_add", "kind": "line", "x": 5, "y": 1, "width": 4, "arrow": "end"}),
    );
    assert_eq!(line["element"], "e2");
    assert_eq!(text(&p, 0, 0), "+---+.....\n|db |--->.\n+---+.....");

    // Move the box right by one: the line above it in drawing order is redrawn on top.
    let (_, p) = apply(
        &p,
        json!({"id": 3, "op": "element_edit", "element": "e1", "x": 1}),
    );
    assert_eq!(text(&p, 0, 0), ".+---+....\n.|db --->.\n.+---+....");
    // Restyle and retext.
    let (_, p) = apply(
        &p,
        json!({"id": 4, "op": "element_edit", "element": "e1", "style": "rounded", "text": "api"}),
    );
    assert_eq!(text(&p, 0, 0), "..---.....\n.|api--->.\n.'---'....");
    // Delete both: the frame is as it started.
    let (_, p) = apply(
        &p,
        json!({"id": 5, "op": "element_delete", "element": "e2"}),
    );
    let (_, p) = apply(
        &p,
        json!({"id": 6, "op": "element_delete", "element": "e1"}),
    );
    assert_eq!(text(&p, 0, 0), text(&start, 0, 0));
    // Ids are never given again, through a save and reload too.
    let p = Layered::from_json(&p.to_json()).unwrap();
    let (added, _) = apply(
        &p,
        json!({"id": 7, "op": "element_add", "kind": "text", "x": 0, "y": 0, "text": "hi"}),
    );
    assert_eq!(added["element"], "e3");
    let (result, _) = ask(
        &p,
        json!({"id": 8, "op": "element_delete", "element": "e1"}),
    );
    assert!(result.unwrap_err().starts_with("No element e1;"));
}

#[test]
fn listing_reports_revisions_and_hand_changes_and_edits_check_them() {
    let start = project(&[("Base", false, &["      \n      "])]);
    let (added, p) = apply(
        &start,
        json!({"id": 1, "op": "element_add", "kind": "text", "x": 1, "y": 0, "text": "ab\ncd"}),
    );
    let (listed, _) = ask(&p, json!({"id": 2, "op": "elements"}));
    let listed = listed.unwrap();
    assert_eq!(
        listed,
        json!({"elements": [{"element": "e1", "layer": 0, "frame": 0, "kind": "text", "x": 1,
            "y": 0, "width": 2, "height": 2, "text": "ab\ncd", "revision": added["revision"],
            "drifted": 0}], "next_element": "e2"})
    );
    // A hand change to one of its cells.
    let mut layers = p.layers().to_vec();
    layers[0]
        .art
        .replace_frame(
            0,
            TimedFrame {
                frame: Frame::from_text(" aX   \n cd   ").unwrap(),
                duration_ms: 100,
            },
        )
        .unwrap();
    let mut changed = Layered::new(layers).unwrap();
    changed.raise_next_element(p.next_element());
    let (listed, _) = ask(&changed, json!({"id": 3, "op": "elements"}));
    let listed = listed.unwrap();
    assert_eq!(listed["elements"][0]["drifted"], 1);
    assert_ne!(listed["elements"][0]["revision"], added["revision"]);
    // The old revision is stale; without force the hand change is kept.
    let (stale, _) = ask(
        &changed,
        json!({"id": 4, "op": "element_edit", "element": "e1", "x": 3, "revision": added["revision"]}),
    );
    assert!(stale.unwrap_err().contains("changed since it was listed"));
    let (refused, none) = ask(
        &changed,
        json!({"id": 5, "op": "element_edit", "element": "e1", "x": 3}),
    );
    assert_eq!(
        refused.unwrap_err(),
        "Element e1 was changed by hand in 1 cells; pass force: true to redraw over them"
    );
    assert!(none.is_none());
    let (_, forced) = apply(
        &changed,
        json!({"id": 6, "op": "element_edit", "element": "e1", "x": 3, "force": true,
            "revision": listed["elements"][0]["revision"]}),
    );
    assert_eq!(text(&forced, 0, 0), "   ab \n   cd ");
}

#[test]
fn dry_runs_locks_bounds_and_bad_arguments_are_refused_or_change_nothing() {
    let p = project(&[
        ("Base", false, &["....\n...."]),
        ("Lock", true, &["    \n    "]),
    ]);
    let (preview, updated) = ask(
        &p,
        json!({"id": 1, "op": "element_add", "kind": "line", "x": 0, "y": 0, "height": 2, "style": "heavy", "dry_run": true}),
    );
    let preview = preview.unwrap();
    assert!(updated.is_none());
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["text"], "#...\n#...");
    assert_eq!(p.next_element(), 1);
    let refusals = [
        (
            json!({"op": "element_add", "kind": "box", "x": 0, "y": 0, "width": 2, "height": 2, "layer": 1}),
            "Layer \"Lock\" is locked",
        ),
        (
            json!({"op": "element_add", "kind": "box", "x": 3, "y": 0, "width": 2, "height": 2}),
            "would leave the 4 x 2 frame",
        ),
        (
            json!({"op": "element_add", "kind": "box", "x": 0, "y": 0, "width": 2}),
            "A box needs width and height",
        ),
        (
            json!({"op": "element_add", "kind": "text", "x": 0, "y": 0, "text": "a", "width": 1}),
            "omit width and height",
        ),
        (
            json!({"op": "element_add", "kind": "star", "x": 0, "y": 0}),
            "Unknown kind \"star\"",
        ),
        (
            json!({"op": "element_add", "kind": "text", "text": "a"}),
            "Give x and y",
        ),
        (
            json!({"op": "element_add", "kind": "text", "x": 0, "y": 0, "text": "\u{7}"}),
            "printable ASCII",
        ),
        (
            json!({"op": "element_add", "kind": "line", "x": 0, "y": 0, "colour": "red"}),
            "Unknown argument colour",
        ),
        (
            json!({"op": "element_edit", "element": "e1"}),
            "Give at least one of",
        ),
        (
            json!({"op": "element_edit", "element": "box", "x": 1}),
            "is not an element id",
        ),
        (
            json!({"op": "element_delete", "element": "e9"}),
            "No element e9;",
        ),
        (
            json!({"op": "elements", "layer": 2}),
            "Layer 2 is not in the project",
        ),
    ];
    for (request, message) in refusals {
        let (result, updated) = ask(&p, request.clone());
        let error = result.expect_err(&request.to_string());
        assert!(error.contains(message), "{request}: {error}");
        assert!(updated.is_none());
    }
}

#[test]
fn an_element_on_a_later_frame_leaves_same_size_writes_alone() {
    let start = project(&[("Base", false, &["....\n....", "....\n...."])]);
    let (_, mut p) = apply(
        &start,
        json!({"id": 1, "op": "element_add", "kind": "text", "x": 0, "y": 0, "frame": 1, "text": "A"}),
    );
    let read = ask(&p, json!({"id": 2, "op": "read", "frame": 0}))
        .0
        .unwrap();
    for dry_run in [true, false] {
        let (written, _) = ask(
            &p,
            json!({"id": 3, "op": "write", "frame": 0, "revision": read["revision"], "x": 1,
                "y": 1, "text": "Z", "dry_run": dry_run}),
        );
        written.unwrap();
    }
    let (batched, _) = ask(
        &p,
        json!({"id": 4, "op": "batch", "writes": [{"frame": 0, "revision": read["revision"],
            "x": 1, "y": 1, "text": "Z"}]}),
    );
    batched.unwrap();
    // The editor's typing replaces the whole frame the same way.
    p.replace_frame(0, 0, Frame::from_text("....\n.Z..").unwrap())
        .unwrap();
    assert_eq!(text(&p, 0, 0), "....\n.Z..");
    assert_eq!(text(&p, 0, 1), "A...\n....");
}

#[test]
fn a_whole_frame_live_write_cannot_strand_an_element() {
    let start = project(&[(
        "Base",
        false,
        &["..........\n..........\n..........\n..........\n.........."],
    )]);
    let (_, p) = apply(
        &start,
        json!({"id": 1, "op": "element_add", "kind": "box", "x": 5, "y": 2, "width": 4, "height": 3}),
    );
    let revision = ask(&p, json!({"id": 9, "op": "read"})).0.unwrap()["revision"].clone();
    let (refused, unchanged) = ask(
        &p,
        json!({"id": 2, "op": "write", "revision": revision, "text": "ab"}),
    );
    let error = refused.unwrap_err();
    assert!(
        error.contains("Element e1 would leave the 2 x 1 frame"),
        "{error}"
    );
    assert!(unchanged.is_none());
    // A dry run goes through the same check and agrees.
    let (dry, none) = ask(
        &p,
        json!({"id": 6, "op": "write", "revision": revision, "text": "ab", "dry_run": true}),
    );
    assert_eq!(dry.unwrap_err(), error);
    assert!(none.is_none());
    // A batch takes the same path.
    let (refused, _) = ask(
        &p,
        json!({"id": 3, "op": "batch", "writes": [{"revision": revision, "text": "ab"}]}),
    );
    assert!(refused.unwrap_err().contains("would leave"));
    // A write the element still fits in keeps it, lists it and saves a file that loads.
    let (_, grown) = apply(
        &p,
        json!({"id": 4, "op": "write", "revision": revision, "text": "############\n############\n############\n############\n############\n############"}),
    );
    let (listed, _) = ask(&grown, json!({"id": 5, "op": "elements"}));
    assert_ne!(listed.unwrap()["elements"][0]["drifted"], 0);
    assert_eq!(Layered::from_json(&grown.to_json()).unwrap(), grown);
}

#[test]
fn elements_follow_their_frames_and_ids_survive_rebuilds() {
    let start = project(&[("Base", false, &["aa", "bb", "cc"])]);
    let (_, p) = apply(
        &start,
        json!({"id": 1, "op": "element_add", "kind": "text", "x": 0, "y": 0, "frame": 1, "text": "X"}),
    );
    let (_, p) = apply(
        &p,
        json!({"id": 2, "op": "element_add", "kind": "text", "x": 1, "y": 0, "frame": 2, "text": "Y"}),
    );
    let frames = |p: &Layered| -> Vec<(u32, usize)> {
        p.layers()[0]
            .elements
            .iter()
            .map(|e| (e.id(), e.frame()))
            .collect()
    };
    let mut moved = p.clone();
    moved.move_frames(2..3, 0).unwrap();
    assert_eq!(frames(&moved), vec![(1, 2), (2, 0)]);
    let mut copied = p.clone();
    copied.duplicate_frames(1..2).unwrap();
    assert_eq!(frames(&copied), vec![(1, 1), (2, 3)]);
    let mut removed = p.clone();
    removed.remove_frames(1..2).unwrap();
    assert_eq!(frames(&removed), vec![(2, 1)]);
    // Removing the element with the highest id keeps the counter above it.
    assert_eq!(removed.next_element(), 3);
    // A rebuild from the layers alone starts from its own ids; the editor raises it again.
    let mut rebuilt = Layered::new(removed.layers().to_vec()).unwrap();
    assert_eq!(rebuilt.next_element(), 3);
    let mut none = Layered::new(start.layers().to_vec()).unwrap();
    none.raise_next_element(removed.next_element());
    assert_eq!(none.next_element(), 3);
    rebuilt.raise_next_element(1);
    assert_eq!(rebuilt.next_element(), 3);
}
