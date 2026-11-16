// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! The Layers panel: choose the layer to edit, add, delete, reorder, rename, hide and merge.
//! The list shows the top layer first, as it is drawn over the others.
use super::{
    App, checked, document, element, input, listen, media::set_disabled, node, report, state,
    status,
};
use crate::{Animation, Frame, Layer, Layered, Result, TimedFrame, layers::MAX_LAYERS};
use web_sys::HtmlSelectElement;

pub(super) const MERGE_FIRST: &str =
    "Every layer shares one size and frame count. Merge layers first to change them.";

pub(super) fn start() -> std::result::Result<(), wasm_bindgen::JsValue> {
    for id in [
        "layer-add",
        "layer-delete",
        "layer-up",
        "layer-down",
        "layer-merge",
    ] {
        listen(id, "click", move |_| report(command(id)))?;
    }
    for id in ["layer", "layer-visible", "layer-name"] {
        listen(id, "change", move |_| report(command(id)))?;
    }
    listen("layer-name", "focus", |_| {
        report(state(|app| {
            app.pause();
            app.display(false)
        }))
    })
}

fn command(id: &str) -> Result<()> {
    state(|app| {
        let result = change(app, id);
        if result.is_err() {
            // Put the panel back to the project as it stands.
            input("layer-name")?.set_value(&app.layers.layers()[app.active].name);
            app.display(false)?;
        }
        result
    })
}

fn change(app: &mut App, id: &str) -> Result<()> {
    app.clean()?;
    app.drawing_cancel();
    let count = app.layers.layers().len();
    let mut active = app.active;
    match id {
        "layer" => {
            let row = node::<HtmlSelectElement>("layer")?.selected_index();
            let row = usize::try_from(row).map_err(|_| "Choose a layer")?;
            app.active = count.checked_sub(row + 1).ok_or("Choose a layer")?;
            app.reset_drawing();
            return app.display(true);
        }
        "layer-merge" => {
            app.replace_layers(Layered::single(app.layers.flatten()), 0);
            status("Layers merged into one.", false);
            return app.display(true);
        }
        _ => {}
    }
    let mut layers = app.layers.layers().to_vec();
    match id {
        "layer-add" => {
            if count == MAX_LAYERS {
                return Err(format!("A project holds at most {MAX_LAYERS} layers"));
            }
            let art = app.project();
            let blank = Frame::blank(art.width(), art.height())?;
            let frames = art
                .frames()
                .iter()
                .map(|f| TimedFrame {
                    frame: blank.clone(),
                    duration_ms: f.duration_ms,
                })
                .collect();
            active += 1;
            layers.insert(
                active,
                Layer {
                    name: format!("Layer {}", count + 1),
                    visible: true,
                    art: Animation::new(frames, art.looping())?,
                    motion: Default::default(),
                },
            );
        }
        "layer-delete" => {
            layers.remove(active);
            active = active.min(layers.len() - 1);
        }
        "layer-up" => {
            if active + 1 == count {
                return Err("Already the top layer".into());
            }
            layers.swap(active, active + 1);
            active += 1;
        }
        "layer-down" => {
            active = active.checked_sub(1).ok_or("Already the bottom layer")?;
            layers.swap(active, active + 1);
        }
        "layer-visible" => layers[active].visible = checked("layer-visible")?,
        _ => layers[active].name = input("layer-name")?.value(),
    }
    app.replace_layers(Layered::new(layers)?, active);
    app.display(true)
}

/// Fill the panel from the project and enable what applies.
pub(super) fn controls(app: &App, busy: bool, editor: bool) -> Result<()> {
    let layers = app.layers.layers();
    let list = node::<HtmlSelectElement>("layer")?;
    let labels: Vec<String> = layers
        .iter()
        .rev()
        .map(|layer| {
            if layer.visible {
                layer.name.clone()
            } else {
                format!("{} (hidden)", layer.name)
            }
        })
        .collect();
    // Names are printable ASCII, so a line break cannot occur inside one.
    let key = labels.join("\n");
    if list.get_attribute("data-labels").as_deref() != Some(key.as_str()) {
        list.set_text_content(None);
        for label in &labels {
            let option = element("option", label)?;
            list.append_child(&option)
                .map_err(|_| "Cannot list the layers")?;
        }
        list.set_attribute("data-labels", &key)
            .map_err(|_| "Cannot list the layers")?;
    }
    list.set_selected_index((layers.len() - 1 - app.active) as i32);
    let layer = &layers[app.active];
    if editor {
        input("layer-name")?.set_value(&layer.name);
    }
    input("layer-visible")?.set_checked(layer.visible);
    let single = layers.len() == 1;
    let focused = document()
        .active_element()
        .map(|e| e.id())
        .unwrap_or_default();
    let mut dropped = false;
    for (id, disabled) in [
        ("layer", busy),
        ("layer-name", busy),
        ("layer-visible", busy),
        ("layer-add", busy || layers.len() == MAX_LAYERS),
        ("layer-delete", busy || single),
        ("layer-up", busy || app.active + 1 == layers.len()),
        ("layer-down", busy || app.active == 0),
        // One layer with keys can be merged to draw its motion into its frames.
        ("layer-merge", busy || single && layers[0].motion.is_empty()),
    ] {
        set_disabled(id, disabled)?;
        dropped |= disabled && id == focused;
    }
    // A button that disables itself, as Move up does on reaching the top, would drop keyboard
    // focus to the page; keep it in the panel on the list.
    if dropped && !busy {
        list.focus().map_err(|_| "Cannot focus the layer list")?;
    }
    Ok(())
}
