// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! The Layers panel: choose the layer to edit, add, delete, reorder, rename, hide, group and
//! merge. The list shows the top layer first, as it is drawn over the others. A group is a
//! label on adjacent layers ([`crate::Layer::group`]); its commands change every member in one
//! undo step, and a lone layer moves past a whole group rather than into it.
use super::{
    App, checked, document, element, input, listen, media::set_disabled, node, report, state,
    status,
};
use crate::{Animation, Frame, Layer, Layered, Result, TimedFrame, layers::MAX_LAYERS};
use std::ops::Range;
use web_sys::{HtmlSelectElement, HtmlTextAreaElement};

/// What the stage shows. A view setting: not saved, not undone, and copies, saves and exports
/// still show every visible layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Solo {
    #[default]
    Off,
    /// Only the active layer.
    Layer,
    /// Only the active layer's group.
    Group,
}

impl Solo {
    /// The layers shown on the stage.
    pub(super) fn shown(self, layers: &Layered, active: usize) -> Range<usize> {
        match self {
            Solo::Off => 0..layers.layers().len(),
            Solo::Layer => active..active + 1,
            Solo::Group => layers.block(active),
        }
    }
}

pub(super) const MERGE_FIRST: &str =
    "Every layer shares one size and frame count. Merge layers first to change them.";

pub(super) fn start() -> std::result::Result<(), wasm_bindgen::JsValue> {
    for id in [
        "layer-add",
        "layer-delete",
        "layer-up",
        "layer-down",
        "layer-merge",
        "group-up",
        "group-down",
        "group-delete",
    ] {
        listen(id, "click", move |_| report(command(id)))?;
    }
    for id in [
        "layer",
        "layer-visible",
        "layer-locked",
        "layer-solo",
        "layer-name",
        "layer-group",
        "group-name",
        "group-visible",
        "group-locked",
        "group-solo",
    ] {
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
            let layer = &app.layers.layers()[app.active];
            input("layer-name")?.set_value(&layer.name);
            input("layer-group")?.set_value(layer.group.as_deref().unwrap_or_default());
            input("group-name")?.set_value(layer.group.as_deref().unwrap_or_default());
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
        "layer-solo" | "group-solo" => {
            app.solo = match (checked(id)?, id) {
                (false, _) => Solo::Off,
                (true, "layer-solo") => Solo::Layer,
                (true, _) => Solo::Group,
            };
            if app.solo != Solo::Off {
                let what = if app.solo == Solo::Layer {
                    "layer"
                } else {
                    "group"
                };
                status(
                    &format!(
                        "Showing only the chosen {what}. Copies, saves and exports still show every visible layer."
                    ),
                    false,
                );
            }
            return app.display(true);
        }
        "layer-merge" => {
            if let Some(layer) = app.layers.layers().iter().find(|layer| layer.locked) {
                return Err(format!(
                    "{} is locked. Untick Lock this layer to merge.",
                    layer.name
                ));
            }
            app.replace_layers(Layered::single(app.layers.flatten()), 0);
            status("Layers merged into one.", false);
            return app.display(true);
        }
        _ => {}
    }
    let mut layers = app.layers.layers().to_vec();
    let block = app.layers.block(active);
    let group = layers[active].group.clone();
    let in_group = || group.clone().ok_or("The chosen layer is not in a group");
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
                    locked: false,
                    motion: Default::default(),
                    // Joining the chosen layer's group keeps the group together.
                    group: group.clone(),
                },
            );
        }
        "layer-delete" => {
            app.unlocked()?;
            layers.remove(active);
            active = active.min(layers.len() - 1);
        }
        "layer-up" | "layer-down" => {
            let up = id == "layer-up";
            let edge = if up { block.end - 1 } else { block.start };
            match &group {
                // Inside a group a layer swaps with its neighbour; it leaves only by name.
                Some(_) if active != edge => {
                    let other = if up { active + 1 } else { active - 1 };
                    layers.swap(active, other);
                    active = other;
                }
                Some(name) => {
                    return Err(format!(
                        "{} is at the edge of group {name}. Clear its group to move it out, or move the whole group.",
                        layers[active].name
                    ));
                }
                None => active = move_block(&app.layers, &mut layers, block, up)?,
            }
        }
        "group-up" | "group-down" => {
            in_group()?;
            let offset = active - block.start;
            active = move_block(&app.layers, &mut layers, block, id == "group-up")? + offset;
        }
        "group-delete" => {
            let name = in_group()?;
            if let Some(layer) = layers[block.clone()].iter().find(|layer| layer.locked) {
                return Err(format!(
                    "{} is locked. Untick Lock this group to delete it.",
                    layer.name
                ));
            }
            if block.len() == count {
                return Err(format!(
                    "Group {name} holds every layer, and a project needs one."
                ));
            }
            layers.drain(block.clone());
            active = block.start.min(layers.len() - 1);
        }
        "layer-visible" => layers[active].visible = checked("layer-visible")?,
        "layer-locked" => layers[active].locked = checked("layer-locked")?,
        "group-visible" | "group-locked" => {
            in_group()?;
            let on = checked(id)?;
            for layer in &mut layers[block] {
                *if id == "group-visible" {
                    &mut layer.visible
                } else {
                    &mut layer.locked
                } = on;
            }
        }
        "layer-group" => {
            let name = input(id)?.value();
            layers[active].group = (!name.is_empty()).then_some(name);
        }
        "group-name" => {
            let old = in_group()?;
            let name = input(id)?.value();
            if name != old
                && layers
                    .iter()
                    .any(|layer| layer.group.as_ref() == Some(&name))
            {
                return Err(format!("There is already a group named {name}"));
            }
            for layer in &mut layers[block] {
                layer.group = Some(name.clone());
            }
        }
        _ => {
            app.unlocked()?;
            layers[active].name = input("layer-name")?.value();
        }
    }
    app.replace_layers(Layered::new(layers)?, active);
    app.display(true)
}

/// Move `block` past the block above it (`up`) or below it, in one step, and return where the
/// block now starts.
fn move_block(
    project: &Layered,
    layers: &mut [Layer],
    block: Range<usize>,
    up: bool,
) -> Result<usize> {
    if up {
        if block.end == layers.len() {
            return Err("Already at the top".into());
        }
        let above = project.block(block.end);
        layers[block.start..above.end].rotate_left(block.len());
        Ok(block.start + above.len())
    } else {
        if block.start == 0 {
            return Err("Already at the bottom".into());
        }
        let below = project.block(block.start - 1);
        layers[below.start..block.end].rotate_right(block.len());
        Ok(below.start)
    }
}

/// Fill the panel from the project and enable what applies.
pub(super) fn controls(app: &App, busy: bool, editor: bool) -> Result<()> {
    let layers = app.layers.layers();
    let list = node::<HtmlSelectElement>("layer")?;
    let labels: Vec<String> = layers
        .iter()
        .rev()
        .map(|layer| {
            let name = match &layer.group {
                Some(group) => format!("{group} > {}", layer.name),
                None => layer.name.clone(),
            };
            if layer.visible {
                name
            } else {
                format!("{name} (hidden)")
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
    let group = layer.group.as_deref().unwrap_or_default();
    if editor {
        input("layer-name")?.set_value(&layer.name);
        input("layer-group")?.set_value(group);
        input("group-name")?.set_value(group);
    }
    input("layer-visible")?.set_checked(layer.visible);
    input("layer-locked")?.set_checked(layer.locked);
    input("layer-solo")?.set_checked(app.solo == Solo::Layer);
    input("group-solo")?.set_checked(app.solo == Solo::Group);
    let block = app.layers.block(app.active);
    let members = &layers[block.clone()];
    // All, none or some: a mixed group shows a dash, and ticking sets every member.
    for (id, flag) in [
        (
            "group-visible",
            members.iter().filter(|l| l.visible).count(),
        ),
        ("group-locked", members.iter().filter(|l| l.locked).count()),
    ] {
        let check = input(id)?;
        check.set_checked(flag == members.len());
        check.set_indeterminate(flag != 0 && flag != members.len());
    }
    let grouped = layer.group.is_some();
    // A locked layer's frame cannot be typed in; its duration still can.
    node::<HtmlTextAreaElement>("editor")?.set_read_only(layer.locked);
    let any_locked = layers.iter().any(|layer| layer.locked);
    let single = layers.len() == 1;
    let focused = document()
        .active_element()
        .map(|e| e.id())
        .unwrap_or_default();
    let mut dropped = false;
    for (id, disabled) in [
        ("layer", busy),
        ("layer-name", busy || layer.locked),
        ("layer-visible", busy),
        ("layer-locked", busy),
        ("layer-solo", busy),
        ("layer-add", busy || layers.len() == MAX_LAYERS),
        ("layer-delete", busy || single || layer.locked),
        // A layer at its group's edge moves out only by clearing its group.
        (
            "layer-up",
            busy || app.active + 1 == layers.len() || grouped && app.active + 1 == block.end,
        ),
        (
            "layer-down",
            busy || app.active == 0 || grouped && app.active == block.start,
        ),
        ("layer-group", busy),
        ("group-name", busy || !grouped),
        ("group-visible", busy || !grouped),
        ("group-locked", busy || !grouped),
        ("group-solo", busy || !grouped && app.solo != Solo::Group),
        ("group-up", busy || !grouped || block.end == layers.len()),
        ("group-down", busy || !grouped || block.start == 0),
        (
            "group-delete",
            busy || !grouped || block.len() == layers.len() || members.iter().any(|l| l.locked),
        ),
        // One layer with keys can be merged to draw its motion into its frames.
        (
            "layer-merge",
            busy || any_locked || single && layers[0].motion.is_empty(),
        ),
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
