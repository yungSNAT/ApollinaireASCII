// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

#![no_main]
//! A saved project file, which the CLI and browser editor both load.

use apollinaire_ascii::{Animation, Layered};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(project) = Layered::from_json(input) {
        // A project that loads must survive its own round trip, layered and flattened.
        assert_eq!(Layered::from_json(&project.to_json()), Ok(project.clone()));
        let flat = project.flatten();
        assert_eq!(Animation::from_json(&flat.to_json()), Ok(flat));
    }
});
