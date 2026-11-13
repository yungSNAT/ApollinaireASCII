// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

#![no_main]
//! A user's FLF or TLF font file: parsed, and when valid, used to render text.
//!
//! The input is the font, optionally followed by a NUL byte and a control block, so the
//! bundled fonts still seed it as they are. The control block's first 3 bytes choose the
//! alignment and layouts, the wrap width and the canvas width; the rest is the text.

use apollinaire_ascii::lettering::{Alignment, Layout, Options, render_with, validate_font};
use libfuzzer_sys::fuzz_target;

const LAYOUTS: [Layout; 4] = [Layout::Default, Layout::Full, Layout::Fitted, Layout::Smush];

fuzz_target!(|data: &[u8]| {
    let (font, control) = match data.iter().position(|&byte| byte == 0) {
        Some(nul) => (&data[..nul], &data[nul + 1..]),
        None => (data, &[][..]),
    };
    let Ok(contents) = std::str::from_utf8(font) else {
        return;
    };
    if validate_font(contents).is_err() {
        return;
    }
    let byte = |index: usize| control.get(index).copied().unwrap_or(0);
    let layout = byte(0);
    let options = Options {
        alignment: [Alignment::Left, Alignment::Center, Alignment::Right][usize::from(layout % 3)],
        horizontal: LAYOUTS[usize::from(layout / 3 % 4)],
        vertical: LAYOUTS[usize::from(layout / 12 % 4)],
        wrap: (byte(1) != 0).then(|| usize::from(byte(1))),
        width: (byte(2) != 0).then(|| usize::from(byte(2))),
    };
    // Map the remaining bytes onto newline and printable ASCII, so most inputs reach the layout.
    let text: String = control
        .iter()
        .skip(3)
        .map(|&byte| {
            if byte == b'\n' {
                '\n'
            } else {
                char::from(32 + byte % 95)
            }
        })
        .collect();
    let text = if text.is_empty() { "Fuzz" } else { &text };
    let Ok(lettering) = render_with(text, "standard", Some(contents), &options) else {
        return;
    };
    // Frame itself enforces printable ASCII; check the layout contract of Options.
    if let Some(width) = options.width {
        assert_eq!(
            lettering.width(),
            width,
            "lettering ignored the canvas width"
        );
    } else if let Some(wrap) = options.wrap {
        assert!(
            lettering.width() <= wrap,
            "lettering exceeded the wrap width"
        );
    }
});
