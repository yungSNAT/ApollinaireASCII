// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid state diagrams (`stateDiagram-v2` and `stateDiagram`) drawn as state boxes and
//! transitions; see [`state_diagram`].

use super::graph::{
    BoxNode, Edge, EndMark, Flow, Graph, MAX_LINKS, MAX_NODES, count_entry, layout_fitted,
    parse_direction,
};
use super::{decode, split_statements};
use crate::{Frame, MAX_INPUT_BYTES, Result};
use std::collections::HashMap;

/// Draw a Mermaid state diagram (`stateDiagram-v2` or `stateDiagram`) as boxes and arrows in
/// printable ASCII.
///
/// The source starts with `stateDiagram-v2` (or `stateDiagram`) and may sit inside one Markdown
/// code block fenced with ```` ``` ```` or `~~~`. Statements end at a line end or `;`; lines
/// starting `%%` are comments.
///
/// - `[*]` on the left of `-->` is the start state and on the right is the end state, each drawn
///   as `(*)`.
/// - Transitions `A --> B` and `A --> B : label` join states with arrows; a self-transition
///   `A --> A : label` loops back to the state box.
/// - `state "Display text" as Id`, `state Id` and `Id : description` name and describe states.
/// - `direction TD|TB|BT|LR|RL` chooses the flow direction (default `TD`).
/// - `classDef`, `class` and `style` statements are skipped because the output has no colour.
///
/// ```
/// use apollinaire_ascii::diagram::state_diagram;
/// let chart = state_diagram("stateDiagram-v2\n  [*] --> Idle\n  Idle --> Done : finish")?;
/// assert!(chart.text().contains("| (*) |"));
/// assert!(chart.text().contains("| Idle |"));
/// assert!(chart.text().contains("finish"));
/// # Ok::<(), String>(())
/// ```
pub fn state_diagram(source: &str) -> Result<Frame> {
    state_diagram_fitted(source, None)
}

pub(super) fn state_diagram_fitted(source: &str, max_width: Option<usize>) -> Result<Frame> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("The state diagram exceeds the 16 MiB input limit".into());
    }
    let mut reader = Reader::default();
    super::mermaid_lines(source, |number, line| reader.line(number, line))?;
    layout_fitted(&reader.finish()?, max_width)
}

#[derive(Default)]
struct Reader {
    started: bool,
    flow: Option<Flow>,
    ids: HashMap<String, usize>,
    nodes: Vec<BoxNode>,
    links: Vec<Edge>,
    entries: usize,
}

fn valid_id(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

impl Reader {
    fn line(&mut self, number: usize, line: &str) -> Result<()> {
        if line.trim_start().starts_with("%%") {
            return Ok(());
        }
        for raw in split_statements(line) {
            let stmt = raw.trim();
            if stmt.is_empty() {
                continue;
            }
            if !self.started {
                if !matches!(stmt, "stateDiagram-v2" | "stateDiagram") {
                    return Err(format!(
                        "Line {number}: a state diagram starts with stateDiagram-v2 or stateDiagram"
                    ));
                }
                self.started = true;
                continue;
            }
            self.statement(number, stmt)?;
        }
        Ok(())
    }

    fn node(&mut self, number: usize, key: &str, label: &str) -> Result<usize> {
        if let Some(&id) = self.ids.get(key) {
            return Ok(id);
        }
        if self.nodes.len() == MAX_NODES {
            return Err(format!(
                "Line {number}: a state diagram has at most {MAX_NODES} states"
            ));
        }
        let id = self.nodes.len();
        self.ids.insert(key.to_owned(), id);
        self.nodes.push(BoxNode::single(label.to_owned()));
        Ok(id)
    }

    fn endpoint(&mut self, number: usize, raw: &str, start_side: bool) -> Result<usize> {
        let name = raw.trim();
        if name == "[*]" {
            let key = if start_side { "__start__" } else { "__end__" };
            return self.node(number, key, "(*)");
        }
        if !valid_id(name) {
            return Err(format!(
                "Line {number}: a state is named with ASCII letters, digits and _, or [*], not '{name}'"
            ));
        }
        self.node(number, name, name)
    }

    fn statement(&mut self, number: usize, stmt: &str) -> Result<()> {
        let first = stmt.split_whitespace().next().unwrap_or_default();
        if matches!(first, "classDef" | "class" | "style") {
            return Ok(());
        }
        if first == "direction" {
            let mut parts = stmt.split_whitespace();
            let _ = parts.next();
            let dir = parts.next().ok_or_else(|| {
                format!("Line {number}: direction needs one of TD, TB, BT, LR or RL")
            })?;
            if parts.next().is_some() {
                return Err(format!(
                    "Line {number}: unexpected text after direction {dir}"
                ));
            }
            self.flow = Some(parse_direction(number, dir)?);
            return Ok(());
        }
        if let Some((left, right)) = stmt.split_once("-->") {
            return self.transition(number, left, right);
        }
        if first == "state" {
            return self.state_decl(number, stmt[5..].trim());
        }
        if let Some((name, desc)) = stmt.split_once(':')
            && !name.trim().contains(char::is_whitespace)
        {
            let name = name.trim();
            if !valid_id(name) {
                return Err(format!(
                    "Line {number}: a state is named with ASCII letters, digits and _, not '{name}'"
                ));
            }
            let desc = decode(number, desc.trim())?;
            let id = self.node(number, name, name)?;
            if !desc.is_empty() {
                count_entry(&mut self.entries, number, "a state diagram", "descriptions")?;
                if self.nodes[id].sections.len() == 1 {
                    self.nodes[id].sections.push(vec![desc]);
                } else {
                    self.nodes[id].sections[1].push(desc);
                }
            }
            return Ok(());
        }
        if valid_id(stmt) {
            self.node(number, stmt, stmt)?;
            return Ok(());
        }
        Err(format!(
            "Line {number}: '{stmt}' is not a state diagram statement this reads"
        ))
    }

    fn state_decl(&mut self, number: usize, rest: &str) -> Result<()> {
        if rest.contains('{') {
            return Err(format!("Line {number}: composite states are not supported"));
        }
        if let Some(after_quote) = rest.strip_prefix('"') {
            let (label, tail) = after_quote
                .split_once('"')
                .ok_or_else(|| format!("Line {number}: state description quote is not closed"))?;
            let tail = tail.trim();
            let name = tail
                .strip_prefix("as")
                .filter(|s| s.starts_with([' ', '\t']))
                .map(str::trim)
                .ok_or_else(|| {
                    format!("Line {number}: expected 'as <state>' after state description")
                })?;
            if !valid_id(name) {
                return Err(format!(
                    "Line {number}: a state is named with ASCII letters, digits and _, not '{name}'"
                ));
            }
            let decoded = decode(number, label)?;
            let id = self.node(number, name, &decoded)?;
            self.nodes[id].sections[0][0] = decoded;
            return Ok(());
        }
        if !valid_id(rest) {
            return Err(format!(
                "Line {number}: a state is named with ASCII letters, digits and _, not '{rest}'"
            ));
        }
        self.node(number, rest, rest)?;
        Ok(())
    }

    fn transition(&mut self, number: usize, left: &str, right: &str) -> Result<()> {
        let (target, label) = match right.split_once(':') {
            Some((target, label)) => (target, decode(number, label.trim())?),
            None => (right, String::new()),
        };
        let from = self.endpoint(number, left, true)?;
        let to = self.endpoint(number, target, false)?;
        if from == to {
            count_entry(&mut self.entries, number, "a state diagram", "descriptions")?;
            self.nodes[from].add_self_loop(&label);
            return Ok(());
        }
        if self.links.len() == MAX_LINKS {
            return Err(format!(
                "Line {number}: a state diagram has at most {MAX_LINKS} transitions"
            ));
        }
        self.links.push(Edge {
            from,
            to,
            text: if label.is_empty() {
                Vec::new()
            } else {
                vec![label]
            },
            head: Some(EndMark::Arrow),
            tail: None,
            dotted: false,
            length: 1,
        });
        Ok(())
    }

    fn finish(self) -> Result<Graph> {
        if !self.started {
            return Err("A state diagram starts with stateDiagram-v2 or stateDiagram".into());
        }
        if self.nodes.is_empty() {
            return Err("A state diagram needs at least one state".into());
        }
        Ok(Graph {
            kind: "state diagram",
            flow: self.flow.unwrap_or(Flow::Down),
            nodes: self.nodes,
            links: self.links,
        })
    }
}
