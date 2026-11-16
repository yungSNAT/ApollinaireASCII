// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Validate while deserialising, before an untrusted JSON tree can grow.
use crate::layers::{Layer, Layered, MAX_LAYERS};
use crate::motion::{Easing, Key, Motion, Property};
use crate::{Animation, Frame, MAX_FRAMES, MAX_PROJECT_CELLS, Result, TimedFrame, dimensions};
use serde::de::{self, Deserialize, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use std::fmt;

pub(crate) fn parse(input: &str) -> Result<Layered> {
    serde_json::from_str::<Project>(input)
        .map(|project| project.0)
        .map_err(|error| format!("Invalid project: {error}"))
}

const FIELDS: &[&str] = &[
    "format", "version", "width", "height", "loop", "layers", "frames",
];

struct Project(Layered);
impl<'de> Deserialize<'de> for Project {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct ProjectVisitor;
        impl<'de> Visitor<'de> for ProjectVisitor {
            type Value = Project;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an asciiforge version 1, 2 or 3 project")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Project, M::Error> {
                let (mut format, mut version, mut width, mut height, mut looping) =
                    (None, None, None, None, None);
                let (mut layers, mut frames): (Option<Few<Meta>>, _) = (None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "format" => field(&mut format, &mut map, "format")?,
                        "version" => field(&mut version, &mut map, "version")?,
                        "width" => field(&mut width, &mut map, "width")?,
                        "height" => field(&mut height, &mut map, "height")?,
                        "loop" => field(&mut looping, &mut map, "loop")?,
                        "layers" => field(&mut layers, &mut map, "layers")?,
                        "frames" => field(&mut frames, &mut map, "frames")?,
                        _ => return Err(de::Error::unknown_field(&key, FIELDS)),
                    }
                }
                let version = required::<u32, M::Error>(version, "version")?;
                if required::<String, M::Error>(format, "format")? != "asciiforge"
                    || !(1..=3).contains(&version)
                {
                    return Err(de::Error::custom(
                        "Expected asciiforge project version 1, 2 or 3",
                    ));
                }
                let width = required::<usize, M::Error>(width, "width")?;
                let height = required::<usize, M::Error>(height, "height")?;
                dimensions(width, height).map_err(de::Error::custom)?;
                let looping = required::<bool, M::Error>(looping, "loop")?;
                let frames = required::<Frames, M::Error>(frames, "frames")?.0;
                if frames
                    .iter()
                    .flat_map(|entry| &entry.texts)
                    .any(|frame| frame.width() != width || frame.height() != height)
                {
                    return Err(de::Error::custom(
                        "Frame dimensions do not match the project",
                    ));
                }
                // Version 1 frames hold "text"; version 2 frames hold one text per layer.
                let names = match (version, layers) {
                    (1, None) => None,
                    (1, Some(_)) => return Err(de::Error::unknown_field("layers", FIELDS)),
                    (2, Some(Few(names))) if names.iter().any(|meta| meta.motion.is_some()) => {
                        return Err(de::Error::unknown_field("motion", &["name", "visible"]));
                    }
                    (_, Some(Few(names))) => Some(names),
                    (_, None) => return Err(de::Error::missing_field("layers")),
                };
                let count = names.as_ref().map_or(1, Vec::len);
                if frames
                    .iter()
                    .any(|entry| entry.layered != names.is_some() || entry.texts.len() != count)
                {
                    return Err(de::Error::custom(if names.is_some() {
                        "Each version 2 frame needs \"layers\" with one text per layer"
                    } else {
                        "Each version 1 frame needs \"text\""
                    }));
                }
                let mut columns: Vec<Vec<TimedFrame>> = vec![Vec::new(); count];
                for entry in frames {
                    for (column, frame) in columns.iter_mut().zip(entry.texts) {
                        column.push(TimedFrame {
                            frame,
                            duration_ms: entry.duration_ms,
                        });
                    }
                }
                let mut animations = columns
                    .into_iter()
                    .map(|frames| Animation::new(frames, looping))
                    .collect::<Result<Vec<_>>>()
                    .map_err(de::Error::custom)?;
                match names {
                    None => Ok(Layered::single(animations.remove(0))),
                    Some(names) => Layered::parsed(
                        names
                            .into_iter()
                            .zip(animations)
                            .map(|(meta, art)| Layer {
                                name: meta.name,
                                visible: meta.visible,
                                art,
                                motion: meta.motion.unwrap_or_default(),
                            })
                            .collect(),
                    ),
                }
                .map(Project)
                .map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_map(ProjectVisitor)
    }
}

fn field<'de, T: Deserialize<'de>, M: MapAccess<'de>>(
    slot: &mut Option<T>,
    map: &mut M,
    name: &'static str,
) -> std::result::Result<(), M::Error> {
    if slot.is_some() {
        return Err(de::Error::duplicate_field(name));
    }
    *slot = Some(map.next_value()?);
    Ok(())
}
fn required<T, E: de::Error>(value: Option<T>, name: &'static str) -> std::result::Result<T, E> {
    value.ok_or_else(|| E::missing_field(name))
}

struct Frames(Vec<Entry>);
impl<'de> Deserialize<'de> for Frames {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct FramesVisitor;
        impl<'de> Visitor<'de> for FramesVisitor {
            type Value = Frames;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("1 to 600 bounded frames")
            }
            fn visit_seq<S: SeqAccess<'de>>(
                self,
                mut sequence: S,
            ) -> std::result::Result<Frames, S::Error> {
                let mut frames = Vec::new();
                let mut cells = 0usize;
                while frames.len() < MAX_FRAMES {
                    let Some(entry) = sequence.next_element::<Entry>()? else {
                        return Ok(Frames(frames));
                    };
                    cells += entry.texts.iter().map(|t| t.cells().len()).sum::<usize>();
                    if cells > MAX_PROJECT_CELLS {
                        return Err(de::Error::custom("Project exceeds 8,000,000 cells"));
                    }
                    frames.push(entry);
                }
                if sequence.next_element::<IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("Too many frames"));
                }
                Ok(Frames(frames))
            }
        }
        deserializer.deserialize_seq(FramesVisitor)
    }
}

/// One frame: `text` in version 1, or `layers` (one text per layer) in version 2.
struct Entry {
    duration_ms: u32,
    texts: Vec<Frame>,
    layered: bool,
}
impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct EntryVisitor;
        impl<'de> Visitor<'de> for EntryVisitor {
            type Value = Entry;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a frame with duration_ms and text or layers")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Entry, M::Error> {
                let (mut text, mut layers, mut duration) = (None, None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "text" => field(&mut text, &mut map, "text")?,
                        "layers" => field(&mut layers, &mut map, "layers")?,
                        "duration_ms" => field(&mut duration, &mut map, "duration_ms")?,
                        _ => {
                            return Err(de::Error::unknown_field(
                                &key,
                                &["text", "layers", "duration_ms"],
                            ));
                        }
                    }
                }
                let (texts, layered) = match (text, layers) {
                    (Some(Text(frame)), None) => (vec![frame], false),
                    (None, Some(Few(texts))) => {
                        (texts.into_iter().map(|Text(frame)| frame).collect(), true)
                    }
                    (Some(_), Some(_)) => {
                        return Err(de::Error::custom("A frame has text or layers, not both"));
                    }
                    (None, None) => return Err(de::Error::missing_field("text")),
                };
                let duration_ms = required::<u32, M::Error>(duration, "duration_ms")?;
                if !(10..=60_000).contains(&duration_ms) {
                    return Err(de::Error::custom(
                        "Frame duration must be 10..=60000 milliseconds",
                    ));
                }
                Ok(Entry {
                    duration_ms,
                    texts,
                    layered,
                })
            }
        }
        deserializer.deserialize_map(EntryVisitor)
    }
}

/// 1 to [`MAX_LAYERS`] items, refused as soon as the list grows past that.
struct Few<T>(Vec<T>);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Few<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct FewVisitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for FewVisitor<T> {
            type Value = Few<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "1 to {MAX_LAYERS} layers")
            }
            fn visit_seq<S: SeqAccess<'de>>(
                self,
                mut sequence: S,
            ) -> std::result::Result<Few<T>, S::Error> {
                let mut items = Vec::new();
                while let Some(item) = sequence.next_element()? {
                    if items.len() == MAX_LAYERS {
                        return Err(de::Error::custom("Too many layers"));
                    }
                    items.push(item);
                }
                if items.is_empty() {
                    return Err(de::Error::invalid_length(0, &self));
                }
                Ok(Few(items))
            }
        }
        deserializer.deserialize_seq(FewVisitor(std::marker::PhantomData))
    }
}

/// A layer's `name`, `visible` and, in version 3, `motion`; [`Layered::new`] checks the name
/// and that every key is on a frame of the project.
struct Meta {
    name: String,
    visible: bool,
    motion: Option<Motion>,
}
impl<'de> Deserialize<'de> for Meta {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct MetaVisitor;
        impl<'de> Visitor<'de> for MetaVisitor {
            type Value = Meta;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a layer with name and visible")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Meta, M::Error> {
                let (mut name, mut visible, mut motion) = (None, None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => field(&mut name, &mut map, "name")?,
                        "visible" => field(&mut visible, &mut map, "visible")?,
                        "motion" => field(&mut motion, &mut map, "motion")?,
                        _ => {
                            return Err(de::Error::unknown_field(
                                &key,
                                &["name", "visible", "motion"],
                            ));
                        }
                    }
                }
                Ok(Meta {
                    name: required(name, "name")?,
                    visible: required(visible, "visible")?,
                    motion: motion.map(|Tracks(motion)| motion),
                })
            }
        }
        deserializer.deserialize_map(MetaVisitor)
    }
}

/// A layer's `motion`: one nonempty track of keys per property named, frames rising.
struct Tracks(Motion);
impl<'de> Deserialize<'de> for Tracks {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct TracksVisitor;
        impl<'de> Visitor<'de> for TracksVisitor {
            type Value = Tracks;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("keys for x, y, scale_x, scale_y, rotation, anchor_x or anchor_y")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Tracks, M::Error> {
                const NAMES: &[&str] = &[
                    "x", "y", "scale_x", "scale_y", "rotation", "anchor_x", "anchor_y",
                ];
                let mut motion = Motion::default();
                while let Some(name) = map.next_key::<String>()? {
                    let property = Property::from_name(&name)
                        .ok_or_else(|| de::Error::unknown_field(&name, NAMES))?;
                    if !motion.keys(property).is_empty() {
                        return Err(de::Error::custom(format!("duplicate field `{name}`")));
                    }
                    let Track(keys) = map.next_value()?;
                    if keys.windows(2).any(|pair| pair[0].frame >= pair[1].frame) {
                        return Err(de::Error::custom(format!(
                            "The {name} keys must be on rising frames"
                        )));
                    }
                    for key in keys {
                        motion.set_key(property, key).map_err(de::Error::custom)?;
                    }
                }
                Ok(Tracks(motion))
            }
        }
        deserializer.deserialize_map(TracksVisitor)
    }
}

/// 1 to [`MAX_FRAMES`] keys, refused as soon as the list grows past that.
struct Track(Vec<Key>);
impl<'de> Deserialize<'de> for Track {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct TrackVisitor;
        impl<'de> Visitor<'de> for TrackVisitor {
            type Value = Track;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "1 to {MAX_FRAMES} keys")
            }
            fn visit_seq<S: SeqAccess<'de>>(
                self,
                mut sequence: S,
            ) -> std::result::Result<Track, S::Error> {
                let mut keys = Vec::new();
                while let Some(KeyEntry(key)) = sequence.next_element()? {
                    if keys.len() == MAX_FRAMES {
                        return Err(de::Error::custom("Too many keys"));
                    }
                    keys.push(key);
                }
                if keys.is_empty() {
                    return Err(de::Error::invalid_length(0, &self));
                }
                Ok(Track(keys))
            }
        }
        deserializer.deserialize_seq(TrackVisitor)
    }
}

/// A key: `frame`, `value` and `easing`, each required.
struct KeyEntry(Key);
impl<'de> Deserialize<'de> for KeyEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct KeyVisitor;
        impl<'de> Visitor<'de> for KeyVisitor {
            type Value = KeyEntry;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a key with frame, value and easing")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<KeyEntry, M::Error> {
                let (mut frame, mut value, mut easing) = (None, None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "frame" => field(&mut frame, &mut map, "frame")?,
                        "value" => field(&mut value, &mut map, "value")?,
                        "easing" => field(&mut easing, &mut map, "easing")?,
                        _ => {
                            return Err(de::Error::unknown_field(
                                &key,
                                &["frame", "value", "easing"],
                            ));
                        }
                    }
                }
                let EasingField(easing) = required(easing, "easing")?;
                Ok(KeyEntry(Key {
                    frame: required(frame, "frame")?,
                    value: required(value, "value")?,
                    easing,
                }))
            }
        }
        deserializer.deserialize_map(KeyVisitor)
    }
}

/// A preset easing's name, or the 4 control values of a custom curve.
struct EasingField(Easing);
impl<'de> Deserialize<'de> for EasingField {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct EasingVisitor;
        impl<'de> Visitor<'de> for EasingVisitor {
            type Value = EasingField;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an easing name or [x1, y1, x2, y2]")
            }
            fn visit_str<E: de::Error>(self, name: &str) -> std::result::Result<EasingField, E> {
                Easing::from_name(name)
                    .map(EasingField)
                    .ok_or_else(|| E::custom(format!("Unknown easing {name:?}")))
            }
            fn visit_seq<S: SeqAccess<'de>>(
                self,
                sequence: S,
            ) -> std::result::Result<EasingField, S::Error> {
                let points =
                    <[f64; 4]>::deserialize(de::value::SeqAccessDeserializer::new(sequence))?;
                Easing::curve(points)
                    .map(EasingField)
                    .map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_any(EasingVisitor)
    }
}

struct Text(Frame);
impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct TextVisitor;
        impl Visitor<'_> for TextVisitor {
            type Value = Text;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("rectangular printable ASCII text")
            }
            fn visit_str<E: de::Error>(self, text: &str) -> std::result::Result<Text, E> {
                Frame::from_text(text).map(Text).map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_str(TextVisitor)
    }
}
