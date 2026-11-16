// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! The Keyframes panel: key the active layer's position, scale, rotation and anchor at the
//! selected frame, with an easing to the next key, and list the layer's keys.
use super::{
    App, decimal, element, input, listen, media::set_disabled, node, report, select, state, status,
};
use crate::{
    Result,
    motion::{Easing, Key, Property},
};
use web_sys::{Element, HtmlSelectElement};

const CURVE: [&str; 4] = ["key-x1", "key-y1", "key-x2", "key-y2"];

pub(super) fn start() -> std::result::Result<(), wasm_bindgen::JsValue> {
    for id in ["key-set", "key-remove"] {
        listen(id, "click", move |_| report(command(id)))?;
    }
    listen("key-list", "change", |_| report(command("key-list")))?;
    listen("key-property", "change", |_| {
        report(command("key-property"))
    })?;
    for id in ["key-easing", "key-x1", "key-y1", "key-x2", "key-y2"] {
        listen(id, "input", |_| report(plot()))?;
    }
    plot().map_err(|e| wasm_bindgen::JsValue::from_str(&e))
}

fn property() -> Result<Property> {
    Property::from_name(&select("key-property")?).ok_or_else(|| "Choose a property".into())
}
fn easing() -> Result<Easing> {
    match select("key-easing")?.as_str() {
        "custom" => {
            let [x1, y1, x2, y2] = CURVE;
            Easing::curve([decimal(x1)?, decimal(y1)?, decimal(x2)?, decimal(y2)?])
        }
        name => Easing::from_name(name).ok_or_else(|| "Choose an easing".into()),
    }
}
/// Words for a property, as the Property list names it.
fn label(property: Property) -> &'static str {
    match property {
        Property::X => "Position across",
        Property::Y => "Position down",
        Property::ScaleX => "Width",
        Property::ScaleY => "Height",
        Property::Rotation => "Rotation",
        Property::AnchorX => "Anchor across",
        Property::AnchorY => "Anchor down",
    }
}

fn command(id: &str) -> Result<()> {
    state(|app| {
        app.clean()?;
        app.drawing_cancel();
        let frame = app.selected;
        let mut layers = app.layers.layers().to_vec();
        let motion = &mut layers[app.active].motion;
        match id {
            "key-list" => {
                // Each option's value is `property:frame`.
                let chosen = select("key-list")?;
                let key = chosen
                    .split_once(':')
                    .and_then(|(name, at)| Some((Property::from_name(name)?, at.parse().ok()?)))
                    .and_then(|(property, at): (Property, usize)| {
                        let key = motion.keys(property).iter().find(|key| key.frame == at)?;
                        Some((property, *key))
                    });
                let (property, key) = key.ok_or("Choose a key")?;
                show(property, key)?;
                app.seek(key.frame);
                return app.display(true);
            }
            "key-property" => {
                let property = property()?;
                let art = app.project();
                let value = motion
                    .value_at(property, frame)
                    .unwrap_or_else(|| property.default_value(art.width(), art.height()));
                input("key-value")?.set_value(&value.to_string());
                return Ok(());
            }
            "key-set" => {
                let property = property()?;
                let value = decimal("key-value")?;
                motion.set_key(
                    property,
                    Key {
                        frame,
                        value,
                        easing: easing()?,
                    },
                )?;
                status(
                    &format!("{} {value} keyed at frame {}.", label(property), frame + 1),
                    false,
                );
            }
            _ => {
                let property = property()?;
                if !motion.remove_key(property, frame) {
                    return Err(format!(
                        "{} has no key at frame {}",
                        label(property),
                        frame + 1
                    ));
                }
                status(
                    &format!("{} key removed from frame {}.", label(property), frame + 1),
                    false,
                );
            }
        }
        app.replace_layers(crate::Layered::new(layers)?, app.active);
        app.display(true)
    })
}

/// Put a key's property, value and easing in the fields.
fn show(property: Property, key: Key) -> Result<()> {
    node::<HtmlSelectElement>("key-property")?.set_value(property.name());
    input("key-value")?.set_value(&key.value.to_string());
    let easing = node::<HtmlSelectElement>("key-easing")?;
    match key.easing.name() {
        Some(name) => easing.set_value(name),
        None => {
            easing.set_value("custom");
            for (id, point) in CURVE.iter().zip(key.easing.points().expect("A curve")) {
                input(id)?.set_value(&point.to_string());
            }
        }
    }
    plot()
}

/// Draw the chosen easing in the panel as ASCII: time across, value up, with the 0 and 1
/// levels marked.
fn plot() -> Result<()> {
    node::<Element>("key-curve-fields")?
        .toggle_attribute_with_force("hidden", select("key-easing")? != "custom")
        .map_err(|_| "Cannot update controls")?;
    let graph = node::<Element>("key-curve");
    let easing = match easing() {
        Ok(easing) => easing,
        Err(error) => {
            graph?.set_text_content(Some(&error));
            return Ok(());
        }
    };
    const WIDTH: usize = 32;
    const HEIGHT: usize = 9;
    // Rows from 1.5 at the top to -0.5 at the bottom, so overshoot shows.
    let row = |value: f64| ((1.5 - value) / 2.0 * (HEIGHT - 1) as f64).round();
    let mut cells = vec![vec![b' '; WIDTH]; HEIGHT];
    for (level, mark) in [(1.0, b'-'), (0.0, b'-')] {
        let y = row(level) as usize;
        cells[y].fill(mark);
    }
    for (x, column) in (0..WIDTH).map(|x| (x, x as f64 / (WIDTH - 1) as f64)) {
        let y = row(easing.ease(column));
        if (0.0..HEIGHT as f64).contains(&y) {
            cells[y as usize][x] = b'*';
        }
    }
    let text: Vec<String> = cells
        .into_iter()
        .map(|line| String::from_utf8(line).expect("ASCII"))
        .collect();
    graph?.set_text_content(Some(&text.join("\n")));
    Ok(())
}

/// Fill the key list from the active layer and enable what applies.
pub(super) fn controls(app: &App, busy: bool) -> Result<()> {
    let motion = &app.layers.layers()[app.active].motion;
    let mut keys: Vec<(Property, Key)> = Property::ALL
        .into_iter()
        .flat_map(|property| {
            motion
                .keys(property)
                .iter()
                .map(move |key| (property, *key))
        })
        .collect();
    keys.sort_by_key(|(property, key)| (key.frame, *property as usize));
    let list = node::<HtmlSelectElement>("key-list")?;
    let labels: Vec<(String, String)> = keys
        .iter()
        .map(|(property, key)| {
            let easing = key.easing.name().unwrap_or("custom curve");
            (
                format!("{}:{}", property.name(), key.frame),
                format!(
                    "Frame {}: {} {}, {easing}",
                    key.frame + 1,
                    label(*property),
                    key.value
                ),
            )
        })
        .collect();
    let joined: String = labels
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if list.get_attribute("data-labels").as_deref() != Some(joined.as_str()) {
        list.set_text_content(None);
        for (value, text) in &labels {
            let option = element("option", text)?;
            option
                .set_attribute("value", value)
                .map_err(|_| "Cannot list the keys")?;
            list.append_child(&option)
                .map_err(|_| "Cannot list the keys")?;
        }
        list.set_attribute("data-labels", &joined)
            .map_err(|_| "Cannot list the keys")?;
    }
    let here = Property::ALL.into_iter().any(|property| {
        motion
            .keys(property)
            .iter()
            .any(|key| key.frame == app.selected)
    });
    for (id, disabled) in [
        ("key-property", busy),
        ("key-value", busy),
        ("key-easing", busy),
        ("key-set", busy),
        ("key-remove", busy || !here),
        ("key-list", busy || keys.is_empty()),
    ] {
        set_disabled(id, disabled)?;
    }
    Ok(())
}
