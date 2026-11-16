// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::{
    Animation, Frame, Layer, Layered, TimedFrame,
    motion::{Easing, Key, Motion, Property},
};
use serde_json::{Value, json};

fn key(frame: usize, value: impl Into<f64>, easing: Easing) -> Key {
    Key {
        frame,
        value: value.into(),
        easing,
    }
}
fn keyed(tracks: &[(Property, &[Key])]) -> Motion {
    let mut motion = Motion::default();
    for (property, keys) in tracks {
        for key in *keys {
            motion.set_key(*property, *key).unwrap();
        }
    }
    motion
}
fn frame(text: &str) -> Frame {
    Frame::from_text(text).unwrap()
}
/// `count` frames of the same art, 100 ms each.
fn still(text: &str, count: usize) -> Animation {
    let frames = (0..count)
        .map(|_| TimedFrame {
            frame: frame(text),
            duration_ms: 100,
        })
        .collect();
    Animation::new(frames, true).unwrap()
}
fn layer(name: &str, art: Animation, motion: Motion) -> Layer {
    Layer {
        name: name.into(),
        visible: true,
        locked: false,
        art,
        motion,
    }
}
fn texts(project: &Layered) -> Vec<String> {
    (0..project.layers()[0].art.frames().len())
        .map(|index| project.frame(index).text())
        .collect()
}

#[test]
fn easing_presets_follow_their_curves() {
    for easing in Easing::PRESETS {
        let name = easing.name().unwrap();
        assert_eq!(Easing::from_name(name), Some(easing));
        if easing != Easing::Hold {
            assert!(easing.ease(0.0).abs() < 1e-6, "{name}");
            assert!((easing.ease(1.0) - 1.0).abs() < 1e-6, "{name}");
        }
    }
    assert_eq!(Easing::from_name("ease"), None);
    assert!((Easing::Linear.ease(0.25) - 0.25).abs() < 1e-6);
    assert!((Easing::EaseInOut.ease(0.5) - 0.5).abs() < 1e-6);
    assert!(Easing::EaseIn.ease(0.5) < 0.4);
    assert!(Easing::EaseOut.ease(0.5) > 0.6);
    assert_eq!(Easing::Hold.ease(0.9), 0.0);
    assert_eq!(Easing::Hold.points(), None);
    // The back curves leave 0 to 1 on their way.
    assert!((0..=100).any(|i| Easing::EaseOutBack.ease(f64::from(i) / 100.0) > 1.05));
    assert!((0..=100).any(|i| Easing::EaseInBack.ease(f64::from(i) / 100.0) < -0.05));
    assert!((0..=100).any(|i| Easing::Bounce.ease(f64::from(i) / 100.0) > 1.02));
    // Progress outside 0 to 1 is held at the ends.
    assert!((Easing::EaseIn.ease(2.0) - 1.0).abs() < 1e-6);
    // A point so close to the start that the curve is flat there is still found.
    let straight = Easing::curve([0.0, 0.0, 1.0, 1.0]).unwrap();
    assert!((straight.ease(1e-7) - 1e-7).abs() < 1e-9);
}

#[test]
fn custom_curves_are_checked_and_compared_exactly() {
    let curve = Easing::curve([0.25, -1.5, 0.75, 2.5]).unwrap();
    assert_eq!(curve.name(), None);
    assert_eq!(curve.points(), Some([0.25, -1.5, 0.75, 2.5]));
    assert_eq!(curve, Easing::curve([0.25, -1.5, 0.75, 2.5]).unwrap());
    assert_ne!(curve, Easing::curve([0.25, -1.5, 0.75, 2.4]).unwrap());
    assert_ne!(curve, Easing::Linear);
    assert!((Easing::curve([0.0, 0.0, 1.0, 1.0]).unwrap().ease(0.3) - 0.3).abs() < 1e-6);
    for bad in [
        [-0.1, 0.0, 1.0, 1.0],
        [0.0, 0.0, 1.1, 1.0],
        [0.0, 5.5, 1.0, 1.0],
        [0.0, 0.0, 1.0, -6.0],
        [f64::NAN, 0.0, 1.0, 1.0],
        [0.0, f64::INFINITY, 1.0, 1.0],
    ] {
        assert!(
            Easing::curve(bad).unwrap_err().contains("easing curve"),
            "{bad:?}"
        );
        // A curve built directly is checked when it is keyed, so a project never holds a key
        // its saved JSON could not reopen.
        let key = Key {
            frame: 0,
            value: 0.0,
            easing: Easing::Curve(bad),
        };
        let error = Motion::default().set_key(Property::X, key).unwrap_err();
        assert!(error.contains("easing curve"), "{bad:?}");
    }
}

#[test]
fn values_hold_before_and_after_keys_and_ease_between_them() {
    let mut motion = keyed(&[(
        Property::X,
        &[
            key(2, 10, Easing::Linear),
            key(6, 30, Easing::Hold),
            key(8, 0, Easing::EaseIn),
        ],
    )]);
    assert_eq!(motion.value_at(Property::Y, 0), None);
    assert_eq!(motion.value_at(Property::X, 0), Some(10.0));
    assert_eq!(motion.value_at(Property::X, 2), Some(10.0));
    assert_eq!(motion.value_at(Property::X, 4), Some(20.0));
    assert_eq!(motion.value_at(Property::X, 6), Some(30.0));
    // Hold keeps the value until the next key.
    assert_eq!(motion.value_at(Property::X, 7), Some(30.0));
    assert_eq!(motion.value_at(Property::X, 8), Some(0.0));
    assert_eq!(motion.value_at(Property::X, 99), Some(0.0));
    // A key on a frame that has one replaces it.
    motion
        .set_key(Property::X, key(6, 20, Easing::Linear))
        .unwrap();
    assert_eq!(motion.keys(Property::X).len(), 3);
    assert_eq!(motion.value_at(Property::X, 7), Some(10.0));
    assert!(motion.remove_key(Property::X, 6));
    assert!(!motion.remove_key(Property::X, 6));
    assert_eq!(motion.len(), 2);
    assert!(!motion.is_empty());
}

#[test]
fn key_values_stay_in_each_property_range() {
    let mut motion = Motion::default();
    for (property, low, high) in [
        (Property::X, -1024.0, 1024.0),
        (Property::AnchorY, -1024.0, 1024.0),
        (Property::ScaleX, 0.0, 1000.0),
        (Property::Rotation, -3600.0, 3600.0),
    ] {
        assert_eq!(Property::from_name(property.name()), Some(property));
        assert_eq!(property.range(), low..=high);
        motion
            .set_key(property, key(0, low, Easing::Linear))
            .unwrap();
        motion
            .set_key(property, key(1, high, Easing::Linear))
            .unwrap();
        let error = motion
            .set_key(property, key(2, high + 0.5, Easing::Linear))
            .unwrap_err();
        assert!(error.contains(property.name()) && error.contains(&high.to_string()));
        for bad in [low - 0.5, f64::NAN, f64::INFINITY] {
            assert!(
                motion
                    .set_key(property, key(2, bad, Easing::Linear))
                    .is_err()
            );
        }
    }
    // Fractions are kept, and -0.0 is stored as 0.0 so equal keys compare equal.
    motion
        .set_key(Property::X, key(3, 2.25, Easing::Linear))
        .unwrap();
    assert_eq!(motion.value_at(Property::X, 3), Some(2.25));
    motion
        .set_key(Property::Y, key(0, -0.0, Easing::Linear))
        .unwrap();
    assert_eq!(motion.keys(Property::Y)[0], key(0, 0.0, Easing::Linear));
    assert_ne!(motion.keys(Property::Y)[0], key(0, 0.5, Easing::Linear));
    assert_ne!(motion.keys(Property::Y)[0], key(1, 0.0, Easing::Linear));
    assert_eq!(Property::from_name("opacity"), None);
    assert_eq!(Property::ALL.len(), 7);
}

#[test]
fn drawing_moves_scales_and_turns_the_art() {
    let bar = frame("     \n --- \n     ");
    let none = Motion::default();
    assert_eq!(none.draw(&bar, 0), bar);
    let moved = keyed(&[
        (Property::X, &[key(0, 2, Easing::Linear)]),
        (Property::Y, &[key(0, -1, Easing::Linear)]),
    ]);
    assert_eq!(moved.draw(&bar, 0).text(), "   --\n     \n     ");
    // A row is as tall as 2 columns are wide, so a quarter turn makes a bar 3 columns long
    // about one row and a half tall (one row here) and 2 columns thick.
    let quarter = keyed(&[(Property::Rotation, &[key(0, 90, Easing::Linear)])]);
    assert_eq!(quarter.draw(&bar, 0).text(), "     \n  || \n     ");
    let back = keyed(&[(Property::Rotation, &[key(0, -90, Easing::Linear)])]);
    assert_eq!(back.draw(&bar, 0).text(), "     \n ||  \n     ");
    let half = keyed(&[(Property::Rotation, &[key(0, 180, Easing::Linear)])]);
    assert_eq!(half.draw(&frame("ab/-"), 0).text(), "-/ba");
    // About the centre, or about a point one column in from the left edge.
    assert_eq!(half.draw(&frame("ab   "), 0).text(), "   ba");
    let mut pivot = half.clone();
    pivot
        .set_key(Property::AnchorX, key(0, 1, Easing::Linear))
        .unwrap();
    assert_eq!(pivot.draw(&frame("ab   "), 0).text(), "ba   ");
    // 45 degrees turns each line glyph one step; other glyphs keep their shape.
    let eighth = keyed(&[(Property::Rotation, &[key(0, 45, Easing::Linear)])]);
    assert_eq!(
        eighth.draw(&frame("     \n  -  \n     "), 0).text(),
        "     \n  \\  \n     "
    );
    assert_eq!(
        eighth.draw(&frame("     \n  /  \n     "), 0).text(),
        "     \n  -  \n     "
    );
    assert_eq!(
        eighth.draw(&frame("     \n  x  \n     "), 0).text(),
        "     \n  x  \n     "
    );
    // Halfway between 2 steps, a glyph turns away from zero.
    let dash = frame("     \n  -  \n     ");
    let tie = keyed(&[(Property::Rotation, &[key(0, 22.5, Easing::Linear)])]);
    assert_eq!(tie.draw(&dash, 0).text(), "     \n  \\  \n     ");
    let tie = keyed(&[(Property::Rotation, &[key(0, -22.5, Easing::Linear)])]);
    assert_eq!(tie.draw(&dash, 0).text(), "     \n  /  \n     ");
    let full = keyed(&[(Property::Rotation, &[key(0, 360, Easing::Linear)])]);
    assert_eq!(full.draw(&bar, 0), bar);
    // Doubling the width around the centre stretches the bar to the edges.
    let wide = keyed(&[(Property::ScaleX, &[key(0, 200, Easing::Linear)])]);
    assert_eq!(wide.draw(&bar, 0).text(), "     \n-----\n     ");
    let gone = keyed(&[(Property::ScaleY, &[key(0, 0, Easing::Linear)])]);
    assert_eq!(gone.draw(&bar, 0), Frame::blank(5, 3).unwrap());
}

#[test]
fn keyed_layers_move_in_every_frame_flatten_and_json() {
    let slide = keyed(&[(
        Property::X,
        &[key(0, 0, Easing::Linear), key(3, 3, Easing::Linear)],
    )]);
    let project = Layered::new(vec![
        layer("Sky", still(". . .", 4), Motion::default()),
        layer("Bird", still("v    ", 4), slide.clone()),
    ])
    .unwrap();
    assert_eq!(texts(&project), ["v . .", ".v. .", ". v .", ". .v."]);
    assert_eq!(project.flatten().frames()[3].frame.text(), ". .v.");
    let json = project.to_json();
    let value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["version"], 3);
    assert_eq!(value["layers"][0], json!({"name":"Sky","visible":true}));
    assert_eq!(
        value["layers"][1]["motion"],
        json!({"x":[{"frame":0,"value":0.0,"easing":"linear"},{"frame":3,"value":3.0,"easing":"linear"}]})
    );
    assert_eq!(Layered::from_json(&json).unwrap(), project);
    // Every command that reads one animation sees the motion.
    assert_eq!(Animation::from_json(&json).unwrap(), project.flatten());

    // One keyed layer is never written as a plain version 1 project.
    let single = Layered::new(vec![layer("Layer 1", still("v    ", 4), slide)]).unwrap();
    assert_eq!(single.plain(), None);
    let json = single.to_json();
    assert_eq!(serde_json::from_str::<Value>(&json).unwrap()["version"], 3);
    assert_eq!(Layered::from_json(&json).unwrap(), single);
    assert_eq!(single.flatten().frames()[2].frame.text(), "  v  ");
    assert_eq!(Animation::from_json(&json).unwrap(), single.flatten());
    // A fraction of a cell is kept; each cell shows the character under its centre.
    let part = keyed(&[(Property::X, &[key(0, 1.6, Easing::Linear)])]);
    let nudged = Layered::new(vec![layer("Layer 1", still("v    ", 1), part)]).unwrap();
    assert_eq!(nudged.frame(0).text(), "  v  ");
    assert_eq!(Layered::from_json(&nudged.to_json()).unwrap(), nudged);
    // A hidden keyed layer is kept but not shown.
    let mut layers = project.layers().to_vec();
    layers[1].visible = false;
    let hidden = Layered::new(layers).unwrap();
    assert_eq!(texts(&hidden), [". . ."; 4]);
}

#[test]
fn curves_and_presets_survive_json() {
    let motion = keyed(&[
        (
            Property::Rotation,
            &[
                key(0, -45, Easing::curve([0.1, -0.25, 0.9, 1.75]).unwrap()),
                key(1, 3600, Easing::Bounce),
            ],
        ),
        (Property::ScaleY, &[key(1, 1000, Easing::EaseOutBack)]),
        (Property::AnchorX, &[key(0, -1024, Easing::Hold)]),
    ]);
    let project = Layered::new(vec![layer("Spin", still("ab", 2), motion)]).unwrap();
    let value: Value = serde_json::from_str(&project.to_json()).unwrap();
    assert_eq!(
        value["layers"][0]["motion"]["rotation"][0]["easing"],
        json!([0.1, -0.25, 0.9, 1.75])
    );
    assert_eq!(Layered::from_json(&project.to_json()).unwrap(), project);
}

/// A version 3 project with one layer whose `motion` is `motion`, 2 frames of 2 by 1.
fn with_motion(version: u32, motion: Value) -> String {
    json!({"format":"asciiforge","version":version,"width":2,"height":1,"loop":true,
        "layers":[{"name":"A","visible":true,"motion":motion}],
        "frames":[{"duration_ms":100,"layers":["ab"]},{"duration_ms":100,"layers":["cd"]}]})
    .to_string()
}

#[test]
fn malformed_motion_is_refused() {
    let ok = json!({"x":[{"frame":0,"value":1,"easing":"linear"}]});
    assert!(Layered::from_json(&with_motion(3, ok.clone())).is_ok());
    for (version, motion, message) in [
        (2, ok.clone(), "unknown field `motion`"),
        (4, ok, "version 1, 2 or 3"),
        (3, json!({"opacity":[]}), "unknown field `opacity`"),
        (3, json!({"x":[]}), "invalid length 0"),
        (
            3,
            json!({"x":[{"frame":1,"value":1,"easing":"linear"},{"frame":1,"value":2,"easing":"linear"}]}),
            "rising frames",
        ),
        (
            3,
            json!({"x":[{"frame":1,"value":1,"easing":"linear"},{"frame":0,"value":2,"easing":"linear"}]}),
            "rising frames",
        ),
        (
            3,
            json!({"x":[{"frame":2,"value":1,"easing":"linear"}]}),
            "frame the project does not have",
        ),
        (
            3,
            json!({"rotation":[{"frame":0,"value":3601,"easing":"linear"}]}),
            "from -3600 to 3600",
        ),
        (
            3,
            json!({"x":[{"frame":0,"value":1,"easing":"ease"}]}),
            "Unknown easing",
        ),
        (
            3,
            json!({"x":[{"frame":0,"value":1,"easing":[0,0,2,1]}]}),
            "easing curve",
        ),
        (
            3,
            json!({"x":[{"frame":0,"value":1,"easing":[0,0,1]}]}),
            "invalid length 3",
        ),
        (
            3,
            json!({"x":[{"frame":0,"value":1,"easing":7}]}),
            "an easing name",
        ),
        (
            3,
            json!({"x":[{"frame":0,"value":1}]}),
            "missing field `easing`",
        ),
        (
            3,
            json!({"x":[{"frame":0,"easing":"hold"}]}),
            "missing field `value`",
        ),
        (
            3,
            json!({"x":[{"value":0,"easing":"hold"}]}),
            "missing field `frame`",
        ),
        (
            3,
            json!({"x":[{"frame":0,"value":1,"easing":"hold","speed":2}]}),
            "unknown field `speed`",
        ),
        (
            3,
            json!({"x":[{"frame":0,"value":"1","easing":"hold"}]}),
            "invalid type",
        ),
        (3, json!([]), "keys for x"),
        (3, json!({"x":{}}), "1 to 600 keys"),
        (3, json!({"x":[[]]}), "a key with frame"),
    ] {
        let error = Layered::from_json(&with_motion(version, motion.clone())).unwrap_err();
        assert!(error.contains(message), "{motion}: {error}");
    }
    let duplicate = with_motion(3, json!({"x":[{"frame":0,"value":1,"easing":"linear"}]})).replace(
        "\"motion\":{",
        "\"motion\":{\"x\":[{\"frame\":1,\"value\":0,\"easing\":\"hold\"}],",
    );
    assert!(
        Layered::from_json(&duplicate)
            .unwrap_err()
            .contains("duplicate field `x`")
    );
    let twice = with_motion(3, json!({})).replace("\"motion\":{}", "\"motion\":{},\"motion\":{}");
    assert!(
        Layered::from_json(&twice)
            .unwrap_err()
            .contains("duplicate field `motion`")
    );
    let many: Vec<Value> = (0..601)
        .map(|frame| json!({"frame":frame,"value":0,"easing":"hold"}))
        .collect();
    assert!(
        Layered::from_json(&with_motion(3, json!({ "x": many })))
            .unwrap_err()
            .contains("Too many keys")
    );
    let error = Layered::new(vec![layer(
        "A",
        still("ab", 2),
        keyed(&[(Property::Y, &[key(2, 0, Easing::Linear)])]),
    )])
    .unwrap_err();
    assert!(error.contains("frame the project does not have"));
}

/// 6 frames numbered in their text, and a layer keyed on frames 1, 3 and 5.
fn numbered() -> Layered {
    let frames = (0..6)
        .map(|i| TimedFrame {
            frame: frame(&i.to_string()),
            duration_ms: 100,
        })
        .collect();
    let art = Animation::new(frames, false).unwrap();
    let motion = keyed(&[(
        Property::Y,
        &[
            key(1, 1, Easing::Linear),
            key(3, 3, Easing::Linear),
            key(5, 5, Easing::Linear),
        ],
    )]);
    Layered::new(vec![layer("N", art, motion)]).unwrap()
}
/// Each key as (frame index, text of the frame it is on).
fn keys_on(project: &Layered) -> Vec<(usize, String)> {
    let layer = &project.layers()[0];
    layer
        .motion
        .keys(Property::Y)
        .iter()
        .map(|key| (key.frame, layer.art.frames()[key.frame].frame.text()))
        .collect()
}

#[test]
fn keys_move_with_their_frames() {
    let pairs = |list: &[(usize, &str)]| -> Vec<(usize, String)> {
        list.iter().map(|(i, t)| (*i, (*t).to_string())).collect()
    };
    let mut project = numbered();
    project.insert_blank_frame(2, 50).unwrap();
    assert_eq!(keys_on(&project), pairs(&[(1, "1"), (4, "3"), (6, "5")]));
    assert_eq!(project.layers()[0].art.frames()[2].duration_ms, 50);

    let mut project = numbered();
    project.duplicate_frames(1..4).unwrap();
    assert_eq!(keys_on(&project), pairs(&[(1, "1"), (3, "3"), (8, "5")]));

    let mut project = numbered();
    project.remove_frames(2..4).unwrap();
    assert_eq!(keys_on(&project), pairs(&[(1, "1"), (3, "5")]));

    // Earlier: frames 3 and 4 to the front.
    let mut project = numbered();
    project.move_frames(3..5, 0).unwrap();
    assert_eq!(keys_on(&project), pairs(&[(0, "3"), (3, "1"), (5, "5")]));
    // Later: frames 0 and 1 to the end.
    let mut project = numbered();
    project.move_frames(0..2, 4).unwrap();
    assert_eq!(keys_on(&project), pairs(&[(1, "3"), (3, "5"), (5, "1")]));
    // Later, past one keyed frame only.
    let mut project = numbered();
    project.move_frames(0..2, 1).unwrap();
    assert_eq!(keys_on(&project), pairs(&[(2, "1"), (3, "3"), (5, "5")]));

    // A refused operation changes nothing.
    let mut project = numbered();
    assert!(project.remove_frames(0..6).is_err());
    assert!(project.move_frames(0..2, 5).is_err());
    assert!(project.insert_blank_frame(9, 100).is_err());
    assert_eq!(project, numbered());
}

#[test]
fn projects_without_keys_are_written_as_before() {
    let two = Layered::new(vec![
        layer("A", still("a", 1), Motion::default()),
        layer("B", still("b", 1), Motion::default()),
    ])
    .unwrap();
    assert_eq!(
        two.to_json(),
        r#"{"format":"asciiforge","frames":[{"duration_ms":100,"layers":["a","b"]}],"height":1,"layers":[{"name":"A","visible":true},{"name":"B","visible":true}],"loop":true,"version":2,"width":1}"#
    );
    let one = Layered::single(still("a", 1));
    assert_eq!(one.to_json(), still("a", 1).to_json());
}
