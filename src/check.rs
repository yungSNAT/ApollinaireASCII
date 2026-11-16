// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! `apollinaire check` on text that is not valid art: every problem, by line and column,
//! rather than the first one only.

use apollinaire_ascii::{MAX_HEIGHT, MAX_WIDTH};

/// At most this many problems are listed; the rest are counted.
pub const LISTED: usize = 200;

/// The problems found in text art: the first [`LISTED`] as `line L, column C: what`, in reading
/// order, and how many there are in all. None for text that loads.
pub fn problems(bytes: &[u8]) -> (Vec<String>, usize) {
    let mut found = Found::default();
    let bytes = match bytes.strip_prefix(b"\xef\xbb\xbf") {
        Some(rest) => {
            found.add(1, 1, "a byte order mark; save the file without it".into());
            rest
        }
        None => bytes,
    };
    let text = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    if text.is_empty() {
        found.add(1, 1, "the file has no rows".into());
        return found.into_parts();
    }
    // Rows past the limit are checked for characters, but only the first rows set the width.
    let mut widths = Vec::new();
    let mut lines = 0;
    for line in text.split(|&b| b == b'\n') {
        lines += 1;
        let width = row(line.strip_suffix(b"\r").unwrap_or(line), lines, &mut found);
        if lines <= MAX_HEIGHT {
            widths.push(width);
        }
    }
    if lines > MAX_HEIGHT {
        found.add(
            MAX_HEIGHT + 1,
            1,
            format!("art can be at most {MAX_HEIGHT} rows"),
        );
    }
    let (widest_line, widest) = widths
        .iter()
        .enumerate()
        .max_by_key(|&(index, &width)| (width, std::cmp::Reverse(index)))
        .map(|(index, &width)| (index + 1, width))
        .expect("text has a row");
    if widest == 0 {
        found.add(1, 1, "every row is empty".into());
    }
    if widest > MAX_WIDTH {
        found.add(
            widest_line,
            MAX_WIDTH + 1,
            format!("art can be at most {MAX_WIDTH} columns"),
        );
    }
    for (index, &width) in widths.iter().enumerate() {
        if width < widest {
            found.add(
                index + 1,
                width + 1,
                format!(
                    "the row is shorter than line {widest_line}, which has {widest} columns; pad it with spaces, or import with --pad"
                ),
            );
        }
    }
    found.into_parts()
}

/// Check one row's characters, returning its width in columns. Each byte that is not UTF-8
/// counts as one column.
fn row(line: &[u8], number: usize, found: &mut Found) -> usize {
    let mut column = 0;
    for chunk in line.utf8_chunks() {
        for character in chunk.valid().chars() {
            column += 1;
            if let Some(problem) = character_problem(character) {
                found.add(number, column, problem);
            }
        }
        for byte in chunk.invalid() {
            column += 1;
            found.add(
                number,
                column,
                format!("byte 0x{byte:02x} is not UTF-8 text"),
            );
        }
    }
    column
}

/// What is wrong with one character, if anything, and the ASCII to use instead.
fn character_problem(character: char) -> Option<String> {
    let code = u32::from(character);
    match character {
        ' '..='~' => None,
        '\t' => Some("a tab; use spaces".into()),
        '\r' => Some("a carriage return inside a row".into()),
        '\0'..='\x1f' | '\x7f' => Some(format!("control character 0x{code:02x}")),
        _ => Some(match lookalike(character) {
            Some(ascii) => format!("U+{code:04X} is not ASCII; use {ascii}"),
            None => format!("U+{code:04X} is not ASCII"),
        }),
    }
}

/// The ASCII that common lookalike characters stand in for.
fn lookalike(character: char) -> Option<&'static str> {
    Some(match character {
        '\u{a0}' | '\u{2002}'..='\u{200a}' | '\u{3000}' => "a space",
        '\u{2018}' | '\u{2019}' | '\u{2032}' => "'",
        '\u{201c}' | '\u{201d}' | '\u{2033}' => "\"",
        '\u{2010}'..='\u{2015}' | '\u{2212}' | '\u{2500}' | '\u{2501}' | '\u{2550}' => "-",
        '\u{2502}' | '\u{2503}' | '\u{2551}' => "|",
        '\u{250c}' | '\u{2510}' | '\u{2514}' | '\u{2518}' | '\u{253c}' => "+",
        '\u{2026}' => "...",
        '\u{d7}' => "x",
        '\u{2022}' | '\u{b7}' => "* or .",
        _ => return None,
    })
}

#[derive(Default)]
struct Found {
    listed: Vec<(usize, usize, String)>,
    total: usize,
}

impl Found {
    fn add(&mut self, line: usize, column: usize, what: String) {
        self.total += 1;
        self.listed.push((line, column, what));
        // Keep the earliest problems, so a large file full of them stays small in memory.
        if self.listed.len() > 2 * LISTED {
            self.keep_first();
        }
    }
    fn keep_first(&mut self) {
        // Stable, so problems at one place keep the order they were found in.
        self.listed.sort_by_key(|&(line, column, _)| (line, column));
        self.listed.truncate(LISTED);
    }
    fn into_parts(mut self) -> (Vec<String>, usize) {
        self.keep_first();
        let listed = self
            .listed
            .into_iter()
            .map(|(line, column, what)| format!("line {line}, column {column}: {what}"))
            .collect();
        (listed, self.total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_text_has_no_problems() {
        for text in ["ab\ncd", "ab\r\ncd\n", "ab\ncd\r\n", "x"] {
            assert_eq!(problems(text.as_bytes()), (vec![], 0), "{text:?}");
        }
    }

    #[test]
    fn every_problem_is_listed_by_line_and_column() {
        let text = [
            "\u{feff}a\u{2019}b\tc\r\nab\x01cd\n".as_bytes(),
            b"\xff",
            "xyz\u{2603}\r\nz".as_bytes(),
        ]
        .concat();
        let (listed, total) = problems(&text);
        assert_eq!(
            listed,
            [
                "line 1, column 1: a byte order mark; save the file without it",
                "line 1, column 2: U+2019 is not ASCII; use '",
                "line 1, column 4: a tab; use spaces",
                "line 2, column 3: control character 0x01",
                "line 3, column 1: byte 0xff is not UTF-8 text",
                "line 3, column 5: U+2603 is not ASCII",
                "line 4, column 2: the row is shorter than line 1, which has 5 columns; pad it with spaces, or import with --pad",
            ]
        );
        assert_eq!(total, 7);
    }

    #[test]
    fn empty_oversized_and_many_problems() {
        assert_eq!(problems(b"").0, ["line 1, column 1: the file has no rows"]);
        assert_eq!(problems(b"\n").1, 1);
        assert_eq!(
            problems(b"\n\n").0,
            ["line 1, column 1: every row is empty"]
        );
        let wide = "x".repeat(MAX_WIDTH + 1);
        assert_eq!(
            problems(wide.as_bytes()).0,
            ["line 1, column 513: art can be at most 512 columns"]
        );
        let tall = "x\n".repeat(MAX_HEIGHT + 1);
        assert_eq!(
            problems(tall.as_bytes()).0,
            ["line 257, column 1: art can be at most 256 rows"]
        );
        // A lone carriage return is a problem; one before a line feed ends the line.
        assert_eq!(
            problems(b"a\rb").0,
            ["line 1, column 2: a carriage return inside a row"]
        );
        // The first 200 are listed and the rest counted.
        let tabs = "\t".repeat(1000);
        let (listed, total) = problems(tabs.as_bytes());
        assert_eq!((listed.len(), total), (LISTED, 1000));
        assert_eq!(listed[199], "line 1, column 200: a tab; use spaces");
        for character in [
            '\u{a0}', '\u{201c}', '\u{2014}', '\u{2502}', '\u{250c}', '\u{2026}', '\u{d7}',
            '\u{2022}',
        ] {
            let (listed, _) = problems(character.to_string().as_bytes());
            assert!(listed[0].contains("; use "), "{character}");
        }
    }
}
