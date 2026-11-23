// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid entity-relationship diagrams (`erDiagram`) drawn as entity boxes and cardinality
//! connectors; see [`er_diagram`].

use super::graph::{
    BoxNode, Edge, Flow, Graph, MAX_LINKS, MAX_NODES, count_entry, layout_fitted, parse_direction,
};
use super::{decode, split_statements};
use crate::{Frame, MAX_INPUT_BYTES, Result};
use std::collections::HashMap;

const LEFT_CARDS: &[&str] = &["||", "|o", "o|", "}|", "|{", "}o", "o{"];
const RIGHT_CARDS: &[&str] = &["||", "o|", "|o", "|{", "}|", "o{", "}o"];

/// Draw a Mermaid entity-relationship diagram (`erDiagram`) as entity boxes and cardinality
/// connectors in printable ASCII.
///
/// The source starts with `erDiagram` and may sit inside one Markdown code block fenced with
/// ```` ``` ```` or `~~~`. Statements end at a line end or `;`; lines starting `%%` are comments.
///
/// - Entities may be declared with attribute blocks `ENTITY { type name PK "comment" }`, alone on
///   a line, or implicitly in relationships.
/// - Relationships join 2 entities with Crow's Foot cardinality connectors (`||`, `|o`, `o|`,
///   `}|`, `|{`, `}o`, `o{` on each side of `--` identifying or `..` non-identifying lines) and
///   an optional `: label`. Identifying links draw solid lines; non-identifying links draw dotted
///   lines.
/// - `direction TD|TB|BT|LR|RL` chooses the flow direction (default `TD`).
///
/// ```
/// use apollinaire_ascii::diagram::er_diagram;
/// let chart = er_diagram("erDiagram\n  CUSTOMER ||--o{ ORDER : places")?;
/// assert!(chart.text().contains("CUSTOMER"));
/// assert!(chart.text().contains("||--o{ places"));
/// assert!(chart.text().contains("ORDER"));
/// # Ok::<(), String>(())
/// ```
pub fn er_diagram(source: &str) -> Result<Frame> {
    er_diagram_fitted(source, None)
}

pub(super) fn er_diagram_fitted(source: &str, max_width: Option<usize>) -> Result<Frame> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("The ER diagram exceeds the 16 MiB input limit".into());
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
    /// Currently open `ENTITY {` block: (entity id, opening line number).
    open_block: Option<(usize, usize)>,
}

fn valid_entity_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn parse_connector(connector: &str) -> Option<bool> {
    let bytes = connector.as_bytes();
    if bytes.len() != 6 {
        return None;
    }
    let (left, mid, right) = (&connector[0..2], &connector[2..4], &connector[4..6]);
    if !LEFT_CARDS.contains(&left) || !RIGHT_CARDS.contains(&right) {
        return None;
    }
    match mid {
        "--" => Some(false),
        ".." => Some(true),
        _ => None,
    }
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
            if let Some((entity_id, _)) = self.open_block {
                if let Some(before) = stmt.strip_suffix('}') {
                    let inner = before.trim();
                    if !inner.is_empty() {
                        self.add_attr(number, entity_id, inner)?;
                    }
                    self.open_block = None;
                } else {
                    self.add_attr(number, entity_id, stmt)?;
                }
                continue;
            }
            if !self.started {
                if stmt != "erDiagram" {
                    return Err(format!(
                        "Line {number}: an ER diagram starts with erDiagram"
                    ));
                }
                self.started = true;
                continue;
            }
            self.statement(number, stmt)?;
        }
        Ok(())
    }

    fn entity_id(&mut self, number: usize, name: &str) -> Result<usize> {
        let name = name.trim();
        if !valid_entity_name(name) {
            return Err(format!(
                "Line {number}: an entity is named with ASCII letters, digits, _ or -, not '{name}'"
            ));
        }
        if let Some(&id) = self.ids.get(name) {
            return Ok(id);
        }
        if self.nodes.len() == MAX_NODES {
            return Err(format!(
                "Line {number}: an ER diagram has at most {MAX_NODES} entities"
            ));
        }
        let id = self.nodes.len();
        self.ids.insert(name.to_owned(), id);
        self.nodes.push(BoxNode::single(name.to_owned()));
        Ok(id)
    }

    fn add_attr(&mut self, number: usize, entity_id: usize, raw: &str) -> Result<()> {
        count_entry(&mut self.entries, number, "an ER diagram", "attributes")?;
        let attr = decode(number, raw.trim())?;
        if self.nodes[entity_id].sections.len() == 1 {
            self.nodes[entity_id].sections.push(vec![attr]);
        } else {
            self.nodes[entity_id].sections[1].push(attr);
        }
        Ok(())
    }

    fn statement(&mut self, number: usize, stmt: &str) -> Result<()> {
        if stmt == "}" {
            return Err(format!("Line {number}: }} closes no entity block"));
        }
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
        if let Some((head, tail)) = stmt.split_once('{')
            && !head.trim().contains(char::is_whitespace)
        {
            let id = self.entity_id(number, head)?;
            let tail = tail.trim();
            if let Some(inner) = tail.strip_suffix('}') {
                if !inner.trim().is_empty() {
                    self.add_attr(number, id, inner)?;
                }
            } else {
                if !tail.is_empty() {
                    self.add_attr(number, id, tail)?;
                }
                self.open_block = Some((id, number));
            }
            return Ok(());
        }
        if self.try_relation(number, stmt)? {
            return Ok(());
        }
        if valid_entity_name(stmt) {
            self.entity_id(number, stmt)?;
            return Ok(());
        }
        Err(format!(
            "Line {number}: '{stmt}' is not an ER diagram statement this reads"
        ))
    }

    fn try_relation(&mut self, number: usize, stmt: &str) -> Result<bool> {
        let (rel_part, label_part) = match stmt.split_once(':') {
            Some((r, l)) => (r.trim(), Some(l.trim())),
            None => (stmt, None),
        };
        let parts: Vec<&str> = rel_part.split_whitespace().collect();
        if parts.len() != 3 {
            return Ok(false);
        }
        let Some(dotted) = parse_connector(parts[1]) else {
            return Ok(false);
        };
        let from = self.entity_id(number, parts[0])?;
        let to = self.entity_id(number, parts[2])?;
        let raw_label = label_part.unwrap_or_default();
        let unquoted = raw_label
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(raw_label);
        let label = decode(number, unquoted)?;
        let edge_text = if label.is_empty() {
            parts[1].to_owned()
        } else {
            format!("{} {label}", parts[1])
        };
        if from == to {
            count_entry(&mut self.entries, number, "an ER diagram", "attributes")?;
            self.nodes[from].add_self_loop(&edge_text);
            return Ok(true);
        }
        if self.links.len() == MAX_LINKS {
            return Err(format!(
                "Line {number}: an ER diagram has at most {MAX_LINKS} relationships"
            ));
        }
        self.links.push(Edge {
            from,
            to,
            text: vec![edge_text],
            head: None,
            tail: None,
            dotted,
            length: 1,
        });
        Ok(true)
    }

    fn finish(self) -> Result<Graph> {
        if !self.started {
            return Err("An ER diagram starts with erDiagram".into());
        }
        if let Some((_, line)) = self.open_block {
            return Err(format!(
                "The entity block opened on line {line} is not closed"
            ));
        }
        if self.nodes.is_empty() {
            return Err("An ER diagram needs at least one entity".into());
        }
        Ok(Graph {
            kind: "ER diagram",
            flow: self.flow.unwrap_or(Flow::Down),
            nodes: self.nodes,
            links: self.links,
        })
    }
}
