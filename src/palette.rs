// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Character palettes: named sets of characters to draw, shade or convert with. A palette file
//! is one JSON object in Ascii-Motion's character palette export format,
//! `{"name": "...", "characters": [" ", ".", ...], "category": "ascii"}`, so palettes move
//! between the 2 editors. Only printable ASCII characters are accepted.

use crate::{Result, printable};
use serde_json::{Value, json};

/// The longest palette name, in characters.
pub const MAX_NAME: usize = 64;

/// A named list of distinct printable ASCII characters, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    name: String,
    characters: Vec<u8>,
}

impl Palette {
    /// A palette of 1 or more distinct printable ASCII characters. The name is trimmed and must
    /// then hold 1 to [`MAX_NAME`] characters, none a control character.
    pub fn new(name: &str, characters: &[u8]) -> Result<Self> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME || name.chars().any(char::is_control)
        {
            return Err(format!(
                "A palette name needs 1 to {MAX_NAME} characters and no control characters"
            ));
        }
        printable(characters)?;
        if characters.is_empty() {
            return Err("A palette needs at least one character".into());
        }
        let mut seen = [false; 128];
        let mut repeated = String::new();
        for &code in characters {
            if seen[usize::from(code)] && !repeated.contains(char::from(code)) {
                repeated.push(char::from(code));
            }
            seen[usize::from(code)] = true;
        }
        if !repeated.is_empty() {
            return Err(format!(
                "A palette lists each character once; repeated: {repeated:?}"
            ));
        }
        Ok(Self {
            name: name.into(),
            characters: characters.to_vec(),
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn characters(&self) -> &[u8] {
        &self.characters
    }
    /// Read a palette file. Its `category` and any other fields are ignored.
    pub fn from_json(text: &str) -> Result<Self> {
        let value: Value =
            serde_json::from_str(text).map_err(|error| format!("Invalid palette: {error}"))?;
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .ok_or("A palette file needs a \"name\" text")?;
        let list = value
            .get("characters")
            .and_then(Value::as_array)
            .ok_or("A palette file needs a \"characters\" list")?;
        let characters = list
            .iter()
            .enumerate()
            .map(|(index, item)| match item.as_str().map(str::as_bytes) {
                Some(&[code]) => Ok(code),
                _ => Err(format!(
                    "Palette character {} is not one ASCII character",
                    index + 1
                )),
            })
            .collect::<Result<Vec<u8>>>()?;
        Self::new(name, &characters)
    }
    /// The palette file, with the category `ascii`.
    pub fn to_json(&self) -> String {
        let characters: Vec<String> = self
            .characters
            .iter()
            .map(|code| char::from(*code).to_string())
            .collect();
        json!({"name": self.name, "characters": characters, "category": "ascii"}).to_string()
    }
}
