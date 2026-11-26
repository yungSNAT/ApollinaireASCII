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
fn a_frame_cannot_shrink_under_an_element_and_files_cannot_hold_too_many_element_cells() {
    let start = project(&[("Base", false, &["....\n....\n...."])]);
    let (_, mut p) = apply(
        &start,
        json!({"id": 1, "op": "element_add", "kind": "box", "x": 2, "y": 1, "width": 2, "height": 2}),
    );
    let before = p.clone();
    let error = p
        .replace_frame(0, 0, Frame::from_text("...\n...").unwrap())
        .unwrap_err();
    assert!(
        error.contains("Element e1 would leave the 3 x 2 frame"),
        "{error}"
    );
    assert_eq!(p, before);
    p.replace_frame(0, 0, Frame::from_text("#####\n#####\n#####").unwrap())
        .unwrap();
    let (listed, _) = ask(&p, json!({"id": 2, "op": "elements"}));
    assert_eq!(listed.unwrap()["elements"][0]["drifted"], 4);

    // Each element keeps the cells under it, so a file may not make many large ones from a
    // few bytes: 16 elements the size of a 512 x 256 canvas cover more than 2,000,000 cells.
    // Spread over the 16 layers a file may have, they are counted together.
    let row = ".".repeat(512);
    let canvas = vec![row.as_str(); 256].join("\n");
    let big = project(&[("Base", false, &[canvas.as_str()])]);
    let mut file: Value = serde_json::from_str(&big.to_json()).unwrap();
    file["version"] = json!(4);
    file["next_element"] = json!(17);
    file["layers"][0]["elements"] = (1..=16)
        .map(|id| {
            json!({"id": format!("e{id}"), "frame": 0, "kind": "box", "x": 0, "y": 0,
                "width": 512, "height": 256})
        })
        .collect();
    let error = Layered::from_json(&file.to_string()).unwrap_err();
    assert!(error.contains("cover at most 2000000 cells"), "{error}");
    file["layers"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .truncate(15);
    file["next_element"] = json!(16);
    assert!(Layered::from_json(&file.to_string()).is_ok());

    // A file of a few kilobytes: a 1 x 1 canvas whose 2 layers each claim 8 canvas-sized
    // elements. It is refused for their cells together, before any element's cells are made
    // and before the canvas is checked.
    let claims: Vec<Value> = (0..2)
        .map(|layer| {
            let elements: Vec<Value> = (1..=8)
                .map(|id| {
                    json!({"id": format!("e{}", layer * 8 + id), "frame": 0, "kind": "box",
                        "x": 0, "y": 0, "width": 512, "height": 256})
                })
                .collect();
            json!({"name": format!("L{layer}"), "visible": true, "elements": elements})
        })
        .collect();
    let small = json!({"format": "asciiforge", "version": 4, "width": 1, "height": 1,
        "loop": false, "next_element": 17, "layers": claims,
        "frames": [{"duration_ms": 100, "layers": [".", "."]}]})
    .to_string();
    assert!(small.len() < 4096);
    let error = Layered::from_json(&small).unwrap_err();
    assert!(error.contains("cover at most 2000000 cells"), "{error}");
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
fn element_counts_ids_and_versions_are_bounded() {
    let start = project(&[("Base", false, &["ab\ncd"])]);
    let (_, p) = apply(
        &start,
        json!({"id": 1, "op": "element_add", "kind": "text", "x": 0, "y": 0, "text": "Z"}),
    );
    let json: Value = serde_json::from_str(&p.to_json()).unwrap();
    let element = json["layers"][0]["elements"][0].clone();
    let numbered = |id: usize| {
        let mut e = element.clone();
        e["id"] = json!(format!("e{id}"));
        e
    };
    // At most 4,096 in one layer, refused as the list is read.
    let mut many = json.clone();
    many["layers"][0]["elements"] = (1..=4097).map(numbered).collect();
    many["next_element"] = json!(4098);
    assert!(
        Layered::from_json(&many.to_string())
            .unwrap_err()
            .contains("Too many elements")
    );
    // And at most 4,096 in the project, across its layers.
    let mut two = json.clone();
    let mut second = two["layers"][0].clone();
    second["name"] = json!("Top");
    second["elements"] = (4001..=4097).map(numbered).collect();
    two["layers"][0]["elements"] = (1..=4000).map(numbered).collect();
    two["layers"].as_array_mut().unwrap().push(second);
    for frame in two["frames"].as_array_mut().unwrap() {
        let text = frame["layers"][0].clone();
        frame["layers"].as_array_mut().unwrap().push(text);
    }
    two["next_element"] = json!(4098);
    assert!(
        Layered::from_json(&two.to_string())
            .unwrap_err()
            .contains("at most 4096 elements")
    );
    // The last id there is leaves nothing for the next element.
    let mut last = json.clone();
    last["layers"][0]["elements"][0]["id"] = json!(format!("e{}", u32::MAX));
    last["next_element"] = json!(u32::MAX);
    assert!(
        Layered::from_json(&last.to_string())
            .unwrap_err()
            .contains("Element ids are used up")
    );
    // Ids are written one way only.
    let mut padded = json.clone();
    padded["layers"][0]["elements"][0]["id"] = json!("e01");
    assert!(Layered::from_json(&padded.to_string()).is_err());
    // Versions 1 to 3 have no next element.
    let mut v3 = json.clone();
    v3["version"] = json!(3);
    v3["layers"][0].as_object_mut().unwrap().remove("elements");
    assert!(
        Layered::from_json(&v3.to_string())
            .unwrap_err()
            .contains("next_element")
    );
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
