// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Named elements through the editor's side of `mcp --live`, and in project files.

use apollinaire_ascii::{
    Animation, Frame, Layer, Layered, TimedFrame,
    live_edit::{State, answer},
    motion::Motion,
};
use serde_json::{Value, json};

fn project(layers: &[(&str, bool, &[&str])]) -> Layered {
    Layered::new(
        layers
            .iter()
            .map(|&(name, locked, texts)| Layer {
                name: name.into(),
                visible: true,
                locked,
                art: Animation::new(
                    texts
                        .iter()
                        .map(|text| TimedFrame {
                            frame: Frame::from_text(text).unwrap(),
                            duration_ms: 100,
                        })
                        .collect(),
                    true,
                )
                .unwrap(),
                motion: Motion::default(),
                group: None,
                elements: Vec::new(),
            })
            .collect(),
    )
    .unwrap()
}

/// Send one request; give the result or error and the changed project.
fn ask(project: &Layered, request: Value) -> (Result<Value, String>, Option<Layered>) {
    let state = State {
        project,
        active: 0,
        shown: 0,
        generation: 5,
    };
    let (reply, updated) = answer(&state, &request);
    match reply.get("error") {
        Some(error) => (Err(error.as_str().unwrap().to_owned()), updated),
        None => (Ok(reply["result"].clone()), updated),
    }
}

fn apply(project: &Layered, request: Value) -> (Value, Layered) {
    let (result, updated) = ask(project, request);
    (result.unwrap(), updated.expect("a committing edit"))
}

fn text(project: &Layered, layer: usize, frame: usize) -> String {
    project.layers()[layer].art.frames()[frame].frame.text()
}

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
fn version_4_files_keep_elements_and_their_ids_and_older_versions_refuse_them() {
    let start = project(&[("Base", false, &["ab\ncd", "ef\ngh"])]);
    let (_, p) = apply(
        &start,
        json!({"id": 1, "op": "element_add", "kind": "text", "x": 0, "y": 0, "frame": 1, "text": "Z"}),
    );
    let json: Value = serde_json::from_str(&p.to_json()).unwrap();
    assert_eq!(json["version"], 4);
    assert_eq!(json["next_element"], 2);
    assert_eq!(
        json["layers"][0]["elements"],
        json!([{"id": "e1", "frame": 1, "kind": "text", "x": 0, "y": 0, "width": 1, "height": 1,
            "text": "Z", "under": "e"}])
    );
    let reloaded = Layered::from_json(&p.to_json()).unwrap();
    assert_eq!(reloaded, p);

    let mut v3 = json.clone();
    v3["version"] = json!(3);
    v3.as_object_mut().unwrap().remove("next_element");
    assert!(
        Layered::from_json(&v3.to_string())
            .unwrap_err()
            .contains("elements")
    );
    let mut missing = json.clone();
    missing.as_object_mut().unwrap().remove("next_element");
    assert!(
        Layered::from_json(&missing.to_string())
            .unwrap_err()
            .contains("next_element")
    );
    let mut low = json.clone();
    low["next_element"] = json!(1);
    assert!(
        Layered::from_json(&low.to_string())
            .unwrap_err()
            .contains("above every element id")
    );
    let mut twice = json.clone();
    let first = twice["layers"][0]["elements"][0].clone();
    twice["layers"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(first);
    assert!(
        Layered::from_json(&twice.to_string())
            .unwrap_err()
            .contains("used twice")
    );
    let mut outside = json.clone();
    outside["layers"][0]["elements"][0]["x"] = json!(2);
    assert!(
        Layered::from_json(&outside.to_string())
            .unwrap_err()
            .contains("outside the project")
    );
    let mut wrong_under = json.clone();
    wrong_under["layers"][0]["elements"][0]["under"] = json!("ee");
    assert!(Layered::from_json(&wrong_under.to_string()).is_err());

    // A project that held an element and lost it is still version 4, so the id is not reused.
    let (_, emptied) = apply(
        &p,
        json!({"id": 2, "op": "element_delete", "element": "e1"}),
    );
    let saved: Value = serde_json::from_str(&emptied.to_json()).unwrap();
    assert_eq!(
        (saved["version"].clone(), saved["next_element"].clone()),
        (json!(4), json!(2))
    );
    assert_eq!(text(&emptied, 0, 1), "ef\ngh");
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

#[test]
fn the_documented_version_4_example_loads_and_saves_unchanged() {
    // docs/FORMAT.md's example.
    let example = r#"{"format":"asciiforge","version":4,"width":6,"height":3,"loop":true,"next_element":3,"layers":[{"name":"Layer 1","visible":true,"elements":[{"id":"e2","frame":0,"kind":"box","x":0,"y":0,"width":6,"height":3,"style":"rounded","text":"db"}]}],"frames":[{"duration_ms":100,"layers":[".----.\n|db  |\n'----'"]}]}"#;
    let project = Layered::from_json(example).unwrap();
    assert_eq!(project.next_element(), 3);
    assert_eq!(project.layers()[0].elements[0].name(), "e2");
    let saved: Value = serde_json::from_str(&project.to_json()).unwrap();
    assert_eq!(saved, serde_json::from_str::<Value>(example).unwrap());
}
