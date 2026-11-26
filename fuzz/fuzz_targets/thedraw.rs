// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

#![no_main]
//! A user's TheDraw `.tdf` font file: parsed, and each font it holds turned into a FIGlet font
//! that must load and letter text in printable ASCII.

use apollinaire_ascii::lettering::{Alignment, render, validate_font};
use apollinaire_ascii::thedraw;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(fonts) = thedraw::parse(data) else {
        return;
    };
    for font in fonts {
        let flf = font.to_flf();
        validate_font(&flf).expect("a TheDraw font becomes a valid FIGlet font");
        render("Fuzz me!", "standard", Some(&flf), Alignment::Left, None)
            .expect("a TheDraw font letters printable text");
    }
});
