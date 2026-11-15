// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::{
    Animation, Frame, Layer, Layered, MAX_INPUT_BYTES, TimedFrame, layers::DEFAULT_NAME,
};
use serde_json::{Value, json};

fn art(texts: &[&str], looping: bool) -> Animation {
    let frames = texts
        .iter()
        .enumerate()
        .map(|(i, text)| TimedFrame {
            frame: Frame::from_text(text).unwrap(),
            duration_ms: 10 * (i as u32 + 1),
        })
        .collect();
    Animation::new(frames, looping).unwrap()
}
fn layer(name: &str, visible: bool, texts: &[&str]) -> Layer {
    Layer {
        name: name.into(),
        visible,
        art: art(texts, true),
    }
}
/// Sky stays still while the bird moves across it; a hidden layer covers everything.
fn scene() -> Layered {
    Layered::new(vec![
        layer("Sky", true, &["* * *", "* * *"]),
        layer("Bird", true, &[" v   ", "   v "]),
        layer("Fog", false, &["#####", "#####"]),
    ])
    .unwrap()
}

#[test]
fn layers_with_their_own_contents_survive_json_and_flatten_visible_layers_only() {
    let project = scene();
    let json = project.to_json();
    let value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["version"], 2);
    assert_eq!(
        value["layers"],
        json!([{"name":"Sky","visible":true},{"name":"Bird","visible":true},{"name":"Fog","visible":false}])
    );
    assert_eq!(
        value["frames"][1],
        json!({"duration_ms":20,"layers":["* * *","   v ","#####"]})
    );
    assert_eq!(Layered::from_json(&json).unwrap(), project);

    let flat = project.flatten();
    assert_eq!(flat, art(&["*v* *", "* *v*"], true));
    assert_eq!(project.frame(1).text(), "* *v*");
    // Every command that reads one animation sees the flattened project.
    assert_eq!(Animation::from_json(&json).unwrap(), flat);
}

#[test]
fn a_plain_single_layer_is_written_as_version_1_and_anything_else_as_version_2() {
    let plain = Layered::single(art(&["ab"], false));
    assert_eq!(plain.plain(), Some(&art(&["ab"], false)));
    assert_eq!(plain.to_json(), art(&["ab"], false).to_json());
    assert_eq!(Layered::from_json(&plain.to_json()).unwrap(), plain);
    for (name, visible) in [("Sky", true), (DEFAULT_NAME, false)] {
        let one = Layered::new(vec![layer(name, visible, &["ab"])]).unwrap();
        assert_eq!(one.plain(), None);
        let value: Value = serde_json::from_str(&one.to_json()).unwrap();
        assert_eq!(value["version"], 2, "{name} {visible}");
        assert_eq!(Layered::from_json(&one.to_json()).unwrap(), one);
    }
    let hidden = Layered::new(vec![layer(DEFAULT_NAME, false, &["ab"])]).unwrap();
    assert_eq!(hidden.flatten().frames()[0].frame.text(), "  ");
}

#[test]
fn layers_must_share_one_timeline_a_limit_and_valid_names() {
    let base = || layer("A", true, &["ab", "cd"]);
    for (other, expected) in [
        (layer("B", true, &["ab"]), "same dimensions, frames"),
        (layer("B", true, &["abc", "def"]), "same dimensions, frames"),
        (
            Layer {
                art: art(&["ab", "cd"], false),
                ..base()
            },
            "same dimensions, frames",
        ),
        (
            Layer {
                art: Animation::new(
                    vec![
                        TimedFrame {
                            frame: Frame::from_text("ab").unwrap(),
                            duration_ms: 10,
                        };
                        2
                    ],
                    true,
                )
                .unwrap(),
                ..base()
            },
            "same dimensions, frames",
        ),
        (layer("", true, &["ab", "cd"]), "1 to 32 printable ASCII"),
        (layer(&"n".repeat(33), true, &["ab", "cd"]), "1 to 32"),
        (layer("caf\u{e9}", true, &["ab", "cd"]), "1 to 32"),
        (layer("tab\there", true, &["ab", "cd"]), "1 to 32"),
    ] {
        let error = Layered::new(vec![base(), other]).unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
    assert!(Layered::new(vec![]).unwrap_err().contains("1..=16 layers"));
    assert!(Layered::new(vec![base(); 16]).is_ok());
    assert!(
        Layered::new(vec![base(); 17])
            .unwrap_err()
            .contains("1..=16 layers")
    );
    let big = || Layer {
        name: "Big".into(),
        visible: true,
        art: Animation::new(
            vec![
                TimedFrame {
                    frame: Frame::blank(512, 256).unwrap(),
                    duration_ms: 10,
                };
                4
            ],
            false,
        )
        .unwrap(),
    };
    // 16 layers of 4 frames of 131,072 cells is 8,388,608 cells.
    assert!(
        Layered::new(vec![big(); 16])
            .unwrap_err()
            .contains("8,000,000 cells")
    );
    assert!(Layered::new(vec![big(); 15]).is_ok());
}

#[test]
fn replacing_a_frame_changes_one_layer_and_keeps_the_timing() {
    let mut project = scene();
    project
        .replace_frame(1, 0, Frame::from_text("  >  ").unwrap())
        .unwrap();
    assert_eq!(project.layers()[1].art.frames()[0].frame.text(), "  >  ");
    assert_eq!(project.layers()[1].art.frames()[0].duration_ms, 10);
    assert_eq!(project.layers()[0], scene().layers()[0]);
    let before = project.clone();
    for (layer, index, text) in [(3, 0, "     "), (0, 2, "     "), (0, 0, "    ")] {
        assert!(
            project
                .replace_frame(layer, index, Frame::from_text(text).unwrap())
                .is_err()
        );
    }
    assert_eq!(project, before);
}

fn two_layer_json(edit: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(&scene().to_json()).unwrap();
    edit(&mut value);
    value.to_string()
}

#[test]
fn version_2_json_is_strict() {
    for (input, expected) in [
        (
            two_layer_json(|v| {
                v.as_object_mut().unwrap().remove("layers");
            }),
            "missing field `layers`",
        ),
        (
            two_layer_json(|v| v["frames"][0]["layers"].as_array_mut().unwrap().truncate(2)),
            "one text per layer",
        ),
        (
            two_layer_json(|v| v["frames"][0] = json!({"duration_ms":10,"text":"* * *"})),
            "one text per layer",
        ),
        (
            two_layer_json(|v| v["frames"][0]["text"] = json!("* * *")),
            "text or layers, not both",
        ),
        (
            two_layer_json(|v| v["layers"][0]["extra"] = json!(1)),
            "unknown field `extra`",
        ),
        (
            two_layer_json(|v| {
                v["layers"][0].as_object_mut().unwrap().remove("visible");
            }),
            "missing field `visible`",
        ),
        (
            two_layer_json(|v| v["layers"][0]["name"] = json!("")),
            "1 to 32 printable ASCII",
        ),
        (
            two_layer_json(|v| v["layers"] = json!([])),
            "1 to 16 layers",
        ),
        (
            two_layer_json(|v| {
                v["layers"] = json!(vec![json!({"name":"x","visible":true}); 17]);
            }),
            "Too many layers",
        ),
        (
            two_layer_json(|v| v["frames"][0]["layers"] = json!(vec!["* * *"; 17])),
            "Too many layers",
        ),
        (
            two_layer_json(|v| v["frames"][1]["layers"][2] = json!("####")),
            "Frame dimensions do not match",
        ),
        (
            two_layer_json(|v| v["version"] = json!(1)),
            "unknown field `layers`",
        ),
        (
            art(&["ab"], false)
                .to_json()
                .replace("\"version\":1", "\"version\":2"),
            "missing field `layers`",
        ),
    ] {
        let error = Layered::from_json(&input).unwrap_err();
        assert!(error.contains(expected), "{input}: {error}");
        assert!(Animation::from_json(&input).is_err());
    }
}

#[test]
fn version_2_json_is_refused_once_its_cells_pass_the_limit() {
    let big = Frame::blank(512, 256).unwrap().text();
    let project = |frames: usize| {
        json!({
            "format": "asciiforge", "version": 2, "width": 512, "height": 256, "loop": false,
            "layers": vec![json!({"name": "x", "visible": true}); 16],
            "frames": vec![json!({"duration_ms": 10, "layers": vec![big.as_str(); 16]}); frames],
        })
        .to_string()
    };
    assert_eq!(Layered::from_json(&project(3)).unwrap().layers().len(), 16);
    assert!(
        Layered::from_json(&project(4))
            .unwrap_err()
            .contains("8,000,000 cells")
    );
}

/// 16 layers of `frames` frames, 3 by 256: every frame backslashes, which JSON doubles, except
/// layer 0's frames from `spaces` on.
fn narrow(frames: usize, spaces: usize) -> Result<Layered, String> {
    let slash = Frame::from_text(&vec!["\\".repeat(3); 256].join("\n")).unwrap();
    let space = Frame::blank(3, 256).unwrap();
    Layered::new(
        (0..16)
            .map(|i| Layer {
                name: format!("L{i}"),
                visible: true,
                art: Animation::new(
                    (0..frames)
                        .map(|f| TimedFrame {
                            frame: if i == 0 && f >= spaces {
                                space.clone()
                            } else {
                                slash.clone()
                            },
                            duration_ms: 10,
                        })
                        .collect(),
                    false,
                )
                .unwrap(),
            })
            .collect(),
    )
}

#[test]
fn every_project_that_can_be_built_or_edited_loads_again() {
    // 7,372,800 cells is within the cell limit, but its JSON would be over 19 MiB.
    assert!(narrow(600, 600).unwrap_err().contains("16 MiB input limit"));
    // Find the most backslashes layer 0 can hold in 512 frames.
    assert!(narrow(512, 512).is_err());
    let (mut fits, mut over) = (0, 512);
    while over - fits > 1 {
        let middle = (fits + over) / 2;
        if narrow(512, middle).is_ok() {
            fits = middle;
        } else {
            over = middle;
        }
    }
    let mut project = narrow(512, fits).unwrap();
    let json = project.to_json();
    assert!(json.len() <= MAX_INPUT_BYTES, "{}", json.len());
    assert_eq!(Layered::from_json(&json).unwrap(), project);
    // One more frame of backslashes, as typing would make, is refused and changes nothing.
    let before = project.clone();
    let slash = before.layers()[1].art.frames()[0].frame.clone();
    assert!(
        project
            .replace_frame(0, fits, slash)
            .unwrap_err()
            .contains("16 MiB input limit")
    );
    assert_eq!(project, before);
}
