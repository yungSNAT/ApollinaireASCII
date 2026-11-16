// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! The character picker: the 95 printable ASCII characters as buttons, 16 to a row. Choosing
//! one puts it in the Character field, so every tool that draws with it uses it. One button is
//! in the tab order; the arrow keys, Home and End move between them.
use super::{document, input, listen, node, report};
use crate::Result;
use wasm_bindgen::JsCast;
use web_sys::{Element, Event, HtmlElement, KeyboardEvent};

const COLUMNS: u8 = 16;

pub(super) fn start() -> std::result::Result<(), wasm_bindgen::JsValue> {
    let grid = node::<Element>("char-picker").map_err(|e| wasm_bindgen::JsValue::from_str(&e))?;
    for code in 32..=126u8 {
        let button = document().create_element("button")?;
        button.set_attribute("type", "button")?;
        button.set_attribute("data-code", &code.to_string())?;
        button.set_text_content(Some(&char::from(code).to_string()));
        if code == b' ' {
            button.set_attribute("aria-label", "Space")?;
        }
        grid.append_child(&button)?;
    }
    listen("char-picker", "click", |event| report(click(&event)))?;
    listen("char-picker", "keydown", |event| report(key(&event)))?;
    listen("brush", "input", |_| report(sync()))?;
    report(sync());
    Ok(())
}

/// The picker's button for `code`.
fn button(code: u8) -> Result<HtmlElement> {
    document()
        .query_selector(&format!("#char-picker [data-code=\"{code}\"]"))
        .ok()
        .flatten()
        .and_then(|e| e.dyn_into().ok())
        .ok_or_else(|| "The character picker is missing a character".into())
}

/// The character code of the picker button an event came from.
fn code(event: &Event) -> Option<u8> {
    event
        .target()?
        .dyn_into::<Element>()
        .ok()?
        .get_attribute("data-code")?
        .parse()
        .ok()
}

fn click(event: &Event) -> Result<()> {
    let Some(code) = code(event) else {
        return Ok(());
    };
    let brush = input("brush")?;
    brush.set_value(&char::from(code).to_string());
    // Tools read the field on its input event, as they do when a user types in it.
    brush
        .dispatch_event(&Event::new("input").map_err(|_| "Cannot choose the character")?)
        .map_err(|_| "Cannot choose the character")?;
    Ok(())
}

fn key(event: &Event) -> Result<()> {
    let Some(code) = code(event) else {
        return Ok(());
    };
    let event: &KeyboardEvent = event.dyn_ref().ok_or("Expected a keyboard event")?;
    let index = code - 32;
    let last = 126 - 32;
    let to = match event.key().as_str() {
        "ArrowLeft" => index.checked_sub(1),
        "ArrowRight" => (index < last).then(|| index + 1),
        "ArrowUp" => index.checked_sub(COLUMNS),
        "ArrowDown" => (index + COLUMNS <= last).then(|| index + COLUMNS),
        "Home" => Some(0),
        "End" => Some(last),
        _ => return Ok(()),
    };
    event.prevent_default();
    if let Some(to) = to {
        roam(to + 32)?;
        button(to + 32)?
            .focus()
            .map_err(|_| "Cannot move in the character picker")?;
    }
    Ok(())
}

/// Make `code`'s button the picker's one tab stop.
fn roam(code: u8) -> Result<()> {
    for other in 32..=126u8 {
        button(other)?
            .set_attribute("tabindex", if other == code { "0" } else { "-1" })
            .map_err(|_| "Cannot update the character picker")?;
    }
    Ok(())
}

/// Mark the Character field's character as pressed and make it the tab stop.
fn sync() -> Result<()> {
    let value = input("brush")?.value();
    let chosen = match value.as_bytes() {
        [code @ 32..=126] => Some(*code),
        _ => None,
    };
    for code in 32..=126u8 {
        button(code)?
            .set_attribute(
                "aria-pressed",
                if Some(code) == chosen {
                    "true"
                } else {
                    "false"
                },
            )
            .map_err(|_| "Cannot update the character picker")?;
    }
    roam(chosen.unwrap_or(b' '))
}
