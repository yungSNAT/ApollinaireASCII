// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

#![no_main]
//! STL and OBJ bytes from a user's file, through the model reader and a small render. The first
//! byte picks the format; the rest is the file.

use apollinaire_ascii::model::{ModelOptions, read, render};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&kind, bytes)) = data.split_first() else {
        return;
    };
    let name = if kind % 2 == 0 { "fuzz.stl" } else { "fuzz.obj" };
    if let Ok(mesh) = read(name, bytes) {
        let options = ModelOptions {
            width: 24,
            height: 12,
            frames: 4,
            ..ModelOptions::default()
        };
        let _ = render(&mesh, &options);
    }
});
