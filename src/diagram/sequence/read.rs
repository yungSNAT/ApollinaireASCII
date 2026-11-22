// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Reading Mermaid sequence diagram text into participants and drawn events.

use super::{Diagram, Event, MAX_EVENTS, MAX_PARTICIPANTS, Place};
use crate::diagram::{decode, entity_length};
use crate::Result;
use std::{borrow::Cow, collections::HashMap};

/// Message arrows, longest first so a shorter one never matches the start of a longer one.
const ARROWS: &[(&str, Option<u8>, bool)] = &[
    ("<<-->>", Some(b'>'), true),
    ("<<->>", Some(b'>'), true),
    ("-->>", Some(b'>'), false),
    ("->>", Some(b'>'), false),
    ("--x", Some(b'x'), false),
    ("--)", Some(b')'), false),
    ("-->", None, false),
    ("-x", Some(b'x'), false),
    ("-)", Some(b')'), false),
    ("->", None, false),
];

#[derive(Default)]
pub(super) struct Reader {
    started: bool,
    ids: HashMap<String, usize>,
    labels: Vec<String>,
    events: Vec<Event>,
    /// Open blocks: their keyword, whether they are drawn, and the line that opened them.
    blocks: Vec<(String, bool, usize)>,
    /// The next message's number once `autonumber` is on.
    number: Option<usize>,
}

/// The statements in a line, split at each `;` outside an entity.
fn statements(line: &str) -> Vec<&str> {
    let (mut parts, mut start, mut at) = (Vec::new(), 0, 0);
    while let Some(&byte) = line.as_bytes().get(at) {
        let entity = (byte == b'#').then(|| entity_length(&line[at..])).flatten();
        if byte == b';' {
            parts.push(&line[start..at]);
            start = at + 1;
        }
        at += entity.unwrap_or(1);
    }
    parts.push(&line[start..]);
    parts
}

/// `text` without its first word when that word is `prefix` in any letter case.
fn keyword<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let (word, rest) = text.split_once(' ').unwrap_or((text, ""));
    word.eq_ignore_ascii_case(prefix).then(|| rest.trim())
}

impl Reader {
    pub(super) fn line(&mut self, number: usize, line: &str) -> Result<()> {
        if line.trim_start().starts_with("%%") {
            return Ok(());
        }
        let line = if line.contains('\t') {
            Cow::Owned(line.replace('\t', " "))
        } else {
            Cow::Borrowed(line)
        };
        for statement in statements(&line) {
            let statement = statement.trim();
            if statement.is_empty() {
                continue;
            }
            if !self.started {
                if statement != "sequenceDiagram" {
                    return Err(format!(
                        "Line {number}: a sequence diagram starts with sequenceDiagram"
                    ));
                }
                self.started = true;
                continue;
            }
            self.statement(number, statement)?;
        }
        Ok(())
    }

    fn statement(&mut self, number: usize, statement: &str) -> Result<()> {
        let (word, rest) = statement.split_once(' ').unwrap_or((statement, ""));
        let rest = rest.trim();
        let word = word.to_ascii_lowercase();
        match word.as_str() {
            "participant" | "actor" => {
                // `as` in any letter case; ASCII lowering keeps every byte offset.
                let (name, label) = match rest.to_ascii_lowercase().find(" as ") {
                    Some(at) => (&rest[..at], Some(rest[at + 4..].trim())),
                    None => (rest, None),
                };
                let id = self.participant(number, name)?;
                if let Some(label) = label {
                    self.labels[id] = decode(number, label)?;
                }
                Ok(())
            }
            "note" => self.note(number, rest),
            "loop" | "opt" | "alt" | "par" | "critical" | "break" => {
                self.blocks.push((word.clone(), true, number));
                self.push(
                    number,
                    Event::Open(decode(number, format!("{word} {rest}").trim())?),
                )
            }
            "rect" => {
                self.blocks.push((word, false, number));
                Ok(())
            }
            "else" | "and" | "option" => {
                let parent = match word.as_str() {
                    "else" => "alt",
                    "and" => "par",
                    _ => "critical",
                };
                if self.blocks.last().map(|block| block.0.as_str()) != Some(parent) {
                    return Err(format!(
                        "Line {number}: {word} is only read inside {parent}"
                    ));
                }
                self.push(
                    number,
                    Event::Divide(decode(number, format!("{word} {rest}").trim())?),
                )
            }
            "end" if rest.is_empty() => match self.blocks.pop() {
                Some((_, true, _)) => self.push(number, Event::End),
                Some(_) => Ok(()),
                None => Err(format!("Line {number}: end closes no block")),
            },
            "activate" | "deactivate" => self.participant(number, rest).map(drop),
            "autonumber" if rest.is_empty() => {
                self.number = Some(1);
                Ok(())
            }
            "autonumber" | "box" | "create" | "destroy" | "title" | "link" | "links"
            | "properties" | "details" => Err(format!("Line {number}: {word} is not supported")),
            _ => self.message(number, statement),
        }
    }

    fn push(&mut self, number: usize, event: Event) -> Result<()> {
        if self.events.len() == MAX_EVENTS {
            return Err(format!(
                "Line {number}: a sequence diagram has at most {MAX_EVENTS} messages, notes and block lines"
            ));
        }
        self.events.push(event);
        Ok(())
    }

    /// The participant called `name`, added after the others when it is new.
    fn participant(&mut self, number: usize, name: &str) -> Result<usize> {
        let name = name.trim();
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(format!(
                "Line {number}: a participant is named with ASCII letters, digits and _, not '{name}'"
            ));
        }
        if let Some(&id) = self.ids.get(name) {
            return Ok(id);
        }
        if self.labels.len() == MAX_PARTICIPANTS {
            return Err(format!(
                "Line {number}: a sequence diagram has at most {MAX_PARTICIPANTS} participants"
            ));
        }
        self.ids.insert(name.to_owned(), self.labels.len());
        self.labels.push(name.to_owned());
        Ok(self.labels.len() - 1)
    }

    fn note(&mut self, number: usize, rest: &str) -> Result<()> {
        let (place, text) = rest
            .split_once(':')
            .ok_or_else(|| format!("Line {number}: a note is written Note over A: text"))?;
        let (place, names) = if let Some(names) = keyword(place, "over") {
            (Place::Over, names)
        } else if let Some(names) = keyword(place, "left").and_then(|of| keyword(of, "of")) {
            (Place::Left, names)
        } else if let Some(names) = keyword(place, "right").and_then(|of| keyword(of, "of")) {
            (Place::Right, names)
        } else {
            return Err(format!(
                "Line {number}: a note is placed right of, left of or over a participant"
            ));
        };
        let (first, last) = match (place, names.split_once(',')) {
            (Place::Over, Some((a, b))) => {
                let (a, b) = (self.participant(number, a)?, self.participant(number, b)?);
                (a.min(b), a.max(b))
            }
            _ => {
                let id = self.participant(number, names)?;
                (id, id)
            }
        };
        let text = decode(number, text.trim())?;
        self.push(
            number,
            Event::Note {
                first,
                last,
                place,
                text,
            },
        )
    }

    fn message(&mut self, number: usize, statement: &str) -> Result<()> {
        let at = statement
            .find(['-', '<'])
            .ok_or_else(|| format!("Line {number}: '{statement}' is not a statement this reads"))?;
        let (sender, rest) = statement.split_at(at);
        let &(arrow, head, both) = ARROWS
            .iter()
            .find(|(arrow, ..)| rest.starts_with(arrow))
            .ok_or_else(|| {
                format!("Line {number}: the arrow in '{statement}' is not one this reads")
            })?;
        let rest = rest[arrow.len()..].trim_start();
        // An activation mark draws nothing here.
        let rest = rest.strip_prefix(['+', '-']).unwrap_or(rest);
        let (receiver, text) = rest.split_once(':').unwrap_or((rest, ""));
        let from = self.participant(number, sender)?;
        let to = self.participant(number, receiver)?;
        let mut text = decode(number, text.trim())?;
        if let Some(next) = self.number.as_mut() {
            text = format!("{next}. {text}").trim_end().to_owned();
            *next += 1;
        }
        let stroke = if arrow.contains("--") { b'.' } else { b'-' };
        self.push(
            number,
            Event::Message {
                from,
                to,
                stroke,
                head,
                both,
                text,
            },
        )
    }

    pub(super) fn finish(self) -> Result<Diagram> {
        if !self.started {
            return Err("A sequence diagram starts with sequenceDiagram".into());
        }
        if let Some((word, _, line)) = self.blocks.last() {
            return Err(format!(
                "The {word} block opened on line {line} is not closed"
            ));
        }
        if self.labels.is_empty() {
            return Err("A sequence diagram needs at least one participant".into());
        }
        Ok(Diagram {
            labels: self.labels,
            events: self.events,
        })
    }
}
