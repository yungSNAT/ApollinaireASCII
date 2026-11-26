// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Named elements in project files: version 4, its limits and the documented example.

mod element_support;

use element_support::*;

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
fn the_documented_version_4_example_loads_and_saves_unchanged() {
    // docs/FORMAT.md's example.
    let example = r#"{"format":"asciiforge","version":4,"width":6,"height":3,"loop":true,"next_element":3,"layers":[{"name":"Layer 1","visible":true,"elements":[{"id":"e2","frame":0,"kind":"box","x":0,"y":0,"width":6,"height":3,"style":"rounded","text":"db"}]}],"frames":[{"duration_ms":100,"layers":[".----.\n|db  |\n'----'"]}]}"#;
    let project = Layered::from_json(example).unwrap();
    assert_eq!(project.next_element(), 3);
    assert_eq!(project.layers()[0].elements[0].name(), "e2");
    let saved: Value = serde_json::from_str(&project.to_json()).unwrap();
    assert_eq!(saved, serde_json::from_str::<Value>(example).unwrap());
}

#[test]
fn malformed_element_lists_and_older_layer_lists_are_refused_by_name() {
    let start = project(&[("Base", false, &["ab\ncd"])]);
    let (_, p) = apply(
        &start,
        json!({"id": 1, "op": "element_add", "kind": "text", "x": 0, "y": 0, "text": "Z"}),
    );
    let json: Value = serde_json::from_str(&p.to_json()).unwrap();
    let refused = |edit: &dyn Fn(&mut Value), message: &str| {
        let mut file = json.clone();
        edit(&mut file);
        let error = Layered::from_json(&file.to_string()).unwrap_err();
        assert!(error.contains(message), "{message}: {error}");
    };
    refused(
        &|file| file["layers"][0]["elements"] = json!(5),
        "at most 4096 elements",
    );
    refused(
        &|file| file["layers"][0]["elements"] = json!([5]),
        "an element with id, frame, kind, x, y, width and height",
    );
    refused(
        &|file| file["layers"][0]["elements"][0]["colour"] = json!("red"),
        "unknown field `colour`",
    );
    for version in [2, 3] {
        refused(
            &|file| {
                file["version"] = json!(version);
                file.as_object_mut().unwrap().remove("next_element");
            },
            "unknown field `elements`",
        );
    }
}
