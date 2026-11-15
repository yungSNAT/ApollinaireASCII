// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Validate while deserialising, before an untrusted JSON tree can grow.
use crate::layers::{Layer, Layered, MAX_LAYERS};
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
                f.write_str("an asciiforge version 1 or 2 project")
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
                    || !(1..=2).contains(&version)
                {
                    return Err(de::Error::custom(
                        "Expected asciiforge project version 1 or 2",
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

/// A layer's `name` and `visible`; [`Layered::new`] checks the name.
struct Meta {
    name: String,
    visible: bool,
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
                let (mut name, mut visible) = (None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => field(&mut name, &mut map, "name")?,
                        "visible" => field(&mut visible, &mut map, "visible")?,
                        _ => return Err(de::Error::unknown_field(&key, &["name", "visible"])),
                    }
                }
                Ok(Meta {
                    name: required(name, "name")?,
                    visible: required(visible, "visible")?,
                })
            }
        }
        deserializer.deserialize_map(MetaVisitor)
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
