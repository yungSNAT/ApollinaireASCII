// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Projects and requests shared by the element tests.
// Each test file uses only some of these.
#![allow(dead_code, unused_imports)]

pub use apollinaire_ascii::{
    Animation, Frame, Layer, Layered, TimedFrame,
    live_edit::{State, answer},
    motion::Motion,
};
pub use serde_json::{Value, json};

pub fn project(layers: &[(&str, bool, &[&str])]) -> Layered {
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
pub fn ask(project: &Layered, request: Value) -> (Result<Value, String>, Option<Layered>) {
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

pub fn apply(project: &Layered, request: Value) -> (Value, Layered) {
    let (result, updated) = ask(project, request);
    (result.unwrap(), updated.expect("a committing edit"))
}

pub fn text(project: &Layered, layer: usize, frame: usize) -> String {
    project.layers()[layer].art.frames()[frame].frame.text()
}
