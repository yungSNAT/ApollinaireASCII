// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! The character picker and its palettes. The picker shows the chosen palette's characters as
//! buttons, 16 to a row. Choosing one puts it in the Character field, so every tool that draws
//! with it uses it. One button is in the tab order; the arrow keys, Home and End move between
//! them. Palettes added or opened stay for the session, up to 16; a palette file is
//! Ascii-Motion's character palette JSON (see [`crate::palette`]).
use super::{document, download, input, listen, node, report, status};
use crate::{Result, image_renderer::RAMPS, palette::Palette};
use std::cell::RefCell;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Element, Event, HtmlElement, HtmlSelectElement, KeyboardEvent};

const COLUMNS: usize = 16;
const MAX_ADDED: usize = 16;
const MAX_FILE_BYTES: f64 = 64.0 * 1024.0;

thread_local! {
    /// Palettes added or opened in this session, after the built-in ones.
    static ADDED: RefCell<Vec<Palette>> = const { RefCell::new(Vec::new()) };
}

/// Every printable character, then the conversion ramps.
fn built_in() -> Vec<Palette> {
    let all: Vec<u8> = (32..=126).collect();
    std::iter::once(Palette::new("All 95 printable characters", &all))
        .chain(
            RAMPS
                .iter()
                .map(|(name, ramp)| Palette::new(&format!("Ramp: {name}"), ramp.as_bytes())),
        )
        .collect::<Result<_>>()
        .expect("built-in palettes are valid")
}
fn palettes() -> Vec<Palette> {
    let mut list = built_in();
    ADDED.with(|added| list.extend(added.borrow().iter().cloned()));
    list
}
/// The chosen palette's place in [`palettes`] and the palette.
fn chosen() -> Result<(usize, Palette)> {
    let index: usize = super::select("palette")?
        .parse()
        .map_err(|_| "Choose a palette")?;
    let palette = palettes().get(index).cloned().ok_or("Choose a palette")?;
    Ok((index, palette))
}
fn text(palette: &Palette) -> String {
    palette
        .characters()
        .iter()
        .map(|&c| char::from(c))
        .collect()
}

pub(super) fn start() -> std::result::Result<(), wasm_bindgen::JsValue> {
    report(show(0));
    listen("palette", "change", |_| {
        report(chosen().and_then(|(index, _)| show(index)))
    })?;
    listen("char-picker", "click", |event| report(click(&event)))?;
    listen("char-picker", "keydown", |event| report(key(&event)))?;
    listen("brush", "input", |_| report(sync()))?;
    for id in [
        "palette-add",
        "palette-remove",
        "palette-gradient",
        "palette-convert",
        "palette-save",
    ] {
        listen(id, "click", move |_| report(button(id)))?;
    }
    listen("palette-file", "change", |_| report(open()))?;
    Ok(())
}

/// List the palettes, choose the one at `index` and show its characters.
fn show(index: usize) -> Result<()> {
    let list = palettes();
    let fail = |_| "Cannot list the palettes".to_string();
    let select = node::<HtmlSelectElement>("palette")?;
    select.set_text_content(None);
    for (value, palette) in list.iter().enumerate() {
        let option = document().create_element("option").map_err(fail)?;
        option
            .set_attribute("value", &value.to_string())
            .map_err(fail)?;
        option.set_text_content(Some(palette.name()));
        select.append_child(&option).map_err(fail)?;
    }
    select.set_value(&index.to_string());
    let fail = |_| "Cannot show the palette".to_string();
    let grid = node::<Element>("char-picker")?;
    grid.set_text_content(None);
    for &code in list[index].characters() {
        let button = document().create_element("button").map_err(fail)?;
        button.set_attribute("type", "button").map_err(fail)?;
        button
            .set_attribute("data-code", &code.to_string())
            .map_err(fail)?;
        button.set_text_content(Some(&char::from(code).to_string()));
        if code == b' ' {
            button.set_attribute("aria-label", "Space").map_err(fail)?;
        }
        grid.append_child(&button).map_err(fail)?;
    }
    let remove = node::<Element>("palette-remove")?;
    if index < built_in().len() {
        remove.set_attribute("disabled", "")
    } else {
        remove.remove_attribute("disabled")
    }
    .map_err(fail)?;
    sync()
}

/// Keep `palette` for the session, replacing an added one of the same name, and choose it.
fn add(palette: Palette) -> Result<()> {
    let index = ADDED.with(|added| {
        let mut added = added.borrow_mut();
        let at = match added.iter().position(|p| p.name() == palette.name()) {
            Some(at) => {
                added[at] = palette;
                at
            }
            None if added.len() >= MAX_ADDED => {
                return Err(format!(
                    "Up to {MAX_ADDED} palettes can be added. Remove one first."
                ));
            }
            None => {
                added.push(palette);
                added.len() - 1
            }
        };
        Ok(built_in().len() + at)
    })?;
    show(index)
}

fn button(id: &str) -> Result<()> {
    let (index, palette) = chosen()?;
    match id {
        "palette-add" => {
            add(Palette::new(
                &input("palette-name")?.value(),
                input("palette-characters")?.value().as_bytes(),
            )?)?;
            status("Palette added for this session.", false);
        }
        "palette-remove" => {
            let at = index
                .checked_sub(built_in().len())
                .ok_or("Built-in palettes cannot be removed")?;
            ADDED.with(|added| added.borrow_mut().remove(at));
            show(0)?;
            status("Palette removed.", false);
        }
        "palette-gradient" => {
            input("gradient-ramp")?.set_value(&text(&palette));
            status(
                "Gradient characters set from the palette. Choose Gradient to draw with them.",
                false,
            );
        }
        "palette-convert" => {
            input("ramp")?.set_value(&text(&palette));
            node::<HtmlSelectElement>("preset")?.set_value("custom");
            status(
                "Conversion characters set from the palette, light to dense.",
                false,
            );
        }
        "palette-save" => {
            let stem: String = palette
                .name()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            download(
                &palette.to_json(),
                &format!("{stem}.json"),
                "application/json",
            )?;
            status("Palette saved.", false);
        }
        _ => return Err("Unknown palette action".into()),
    }
    Ok(())
}

/// Read the chosen palette file and add it.
fn open() -> Result<()> {
    let field = input("palette-file")?;
    let Some(file) = field.files().and_then(|files| files.get(0)) else {
        return Ok(());
    };
    // Clearing the control lets the same file be chosen again.
    field.set_value("");
    if file.size() > MAX_FILE_BYTES {
        return Err("A palette file is limited to 64 KiB".into());
    }
    spawn_local(async move {
        let result = async {
            let contents = JsFuture::from(file.text())
                .await
                .map_err(|_| "The palette file could not be read".to_string())?;
            let contents = contents
                .as_string()
                .ok_or("The palette file could not be read")?;
            add(Palette::from_json(&contents)?)?;
            status("Palette opened for this session.", false);
            Ok::<(), String>(())
        };
        report(result.await);
    });
    Ok(())
}

/// The picker's buttons, in order.
fn buttons() -> Result<Vec<HtmlElement>> {
    let fail = || "The character picker is missing".to_string();
    let list = document()
        .query_selector_all("#char-picker button")
        .map_err(|_| fail())?;
    (0..list.length())
        .map(|i| {
            list.item(i)
                .and_then(|b| b.dyn_into().ok())
                .ok_or_else(fail)
        })
        .collect()
}

/// The picker button an event came from.
fn target(event: &Event) -> Option<Element> {
    event
        .target()?
        .dyn_into::<Element>()
        .ok()
        .filter(|e| e.has_attribute("data-code"))
}

fn click(event: &Event) -> Result<()> {
    let Some(code) = target(event).and_then(|e| e.text_content()) else {
        return Ok(());
    };
    let brush = input("brush")?;
    brush.set_value(&code);
    // Tools read the field on its input event, as they do when a user types in it.
    brush
        .dispatch_event(&Event::new("input").map_err(|_| "Cannot choose the character")?)
        .map_err(|_| "Cannot choose the character")?;
    Ok(())
}

fn key(event: &Event) -> Result<()> {
    let Some(from) = target(event) else {
        return Ok(());
    };
    let event: &KeyboardEvent = event.dyn_ref().ok_or("Expected a keyboard event")?;
    let buttons = buttons()?;
    let Some(index) = buttons.iter().position(|b| **b == from) else {
        return Ok(());
    };
    let last = buttons.len() - 1;
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
        roam(&buttons, to)?;
        buttons[to]
            .focus()
            .map_err(|_| "Cannot move in the character picker")?;
    }
    Ok(())
}

/// Make the button at `index` the picker's one tab stop.
fn roam(buttons: &[HtmlElement], index: usize) -> Result<()> {
    for (at, button) in buttons.iter().enumerate() {
        button
            .set_attribute("tabindex", if at == index { "0" } else { "-1" })
            .map_err(|_| "Cannot update the character picker")?;
    }
    Ok(())
}

/// Mark the Character field's character as pressed and make it the tab stop, or the first
/// button when the palette does not hold it.
fn sync() -> Result<()> {
    let value = input("brush")?.value();
    let buttons = buttons()?;
    let mut pressed = None;
    for (index, button) in buttons.iter().enumerate() {
        let on = button.text_content().as_deref() == Some(value.as_str());
        if on {
            pressed = Some(index);
        }
        button
            .set_attribute("aria-pressed", if on { "true" } else { "false" })
            .map_err(|_| "Cannot update the character picker")?;
    }
    roam(&buttons, pressed.unwrap_or(0))
}
