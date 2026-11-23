// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid class diagrams (`classDiagram`) drawn as compartmented class boxes and UML
//! relationships; see [`class_diagram`].

use super::graph::{
    BoxNode, Edge, EndMark, Flow, Graph, MAX_LINKS, MAX_NODES, layout_fitted, parse_direction,
};
use super::{decode, split_statements};
use crate::{Frame, MAX_INPUT_BYTES, Result};
use std::collections::HashMap;

/// Relationship operators, longest first so a shorter one never matches a prefix of a longer one.
struct RelationOp {
    symbol: &'static str,
    tail: Option<EndMark>,
    head: Option<EndMark>,
    dotted: bool,
    tag: Option<&'static str>,
}

const RELATIONS: &[RelationOp] = &[
    RelationOp {
        symbol: "<|--",
        tail: Some(EndMark::Arrow),
        head: None,
        dotted: false,
        tag: Some("<|--"),
    },
    RelationOp {
        symbol: "--|>",
        tail: None,
        head: Some(EndMark::Arrow),
        dotted: false,
        tag: Some("--|>"),
    },
    RelationOp {
        symbol: "<|..",
        tail: Some(EndMark::Arrow),
        head: None,
        dotted: true,
        tag: Some("<|.."),
    },
    RelationOp {
        symbol: "..|>",
        tail: None,
        head: Some(EndMark::Arrow),
        dotted: true,
        tag: Some("..|>"),
    },
    RelationOp {
        symbol: "<-->",
        tail: Some(EndMark::Arrow),
        head: Some(EndMark::Arrow),
        dotted: false,
        tag: None,
    },
    RelationOp {
        symbol: "<..>",
        tail: Some(EndMark::Arrow),
        head: Some(EndMark::Arrow),
        dotted: true,
        tag: None,
    },
    RelationOp {
        symbol: "*--",
        tail: Some(EndMark::Composition),
        head: None,
        dotted: false,
        tag: None,
    },
    RelationOp {
        symbol: "--*",
        tail: None,
        head: Some(EndMark::Composition),
        dotted: false,
        tag: None,
    },
    RelationOp {
        symbol: "o--",
        tail: Some(EndMark::Aggregation),
        head: None,
        dotted: false,
        tag: None,
    },
    RelationOp {
        symbol: "--o",
        tail: None,
        head: Some(EndMark::Aggregation),
        dotted: false,
        tag: None,
    },
    RelationOp {
        symbol: "-->",
        tail: None,
        head: Some(EndMark::Arrow),
        dotted: false,
        tag: None,
    },
    RelationOp {
        symbol: "<--",
        tail: Some(EndMark::Arrow),
        head: None,
        dotted: false,
        tag: None,
    },
    RelationOp {
        symbol: "..>",
        tail: None,
        head: Some(EndMark::Arrow),
        dotted: true,
        tag: None,
    },
    RelationOp {
        symbol: "<..",
        tail: Some(EndMark::Arrow),
        head: None,
        dotted: true,
        tag: None,
    },
    RelationOp {
        symbol: "--",
        tail: None,
        head: None,
        dotted: false,
        tag: None,
    },
    RelationOp {
        symbol: "..",
        tail: None,
        head: None,
        dotted: true,
        tag: None,
    },
];

/// Draw a Mermaid class diagram (`classDiagram`) as compartmented class boxes and relationship
/// connectors in printable ASCII.
///
/// The source starts with `classDiagram` and may sit inside one Markdown code block fenced with
/// ```` ``` ```` or `~~~`. Statements end at a line end or `;`; lines starting `%%` are comments.
///
/// - Classes may be declared with `class Name { ... }`, `class Name["Label"]`, `class Name`,
///   `Name : member`, `<<stereotype>> Name` or implicitly in relationships.
/// - Members containing `(` and `)` sit in the methods compartment; other members sit in fields.
/// - Relationships support `<|--`, `--|>`, `<|..`, `..|>`, `*--`, `--*`, `o--`, `--o`, `<-->`,
///   `-->`, `<--`, `<..>`, `..>`, `<..`, `--` and `..`, with optional quoted multiplicities and
///   `: label`. Dotted connectors draw with `.` and `:`.
/// - `direction TD|TB|BT|LR|RL` chooses the flow direction (default `TD`).
///
/// ```
/// use apollinaire_ascii::diagram::class_diagram;
/// let chart = class_diagram("classDiagram\n  class Animal {\n    +name: String\n    +speak()\n  }\n  Animal <|-- Dog")?;
/// assert!(chart.text().contains("Animal"));
/// assert!(chart.text().contains("+name: String"));
/// assert!(chart.text().contains("+speak()"));
/// # Ok::<(), String>(())
/// ```
pub fn class_diagram(source: &str) -> Result<Frame> {
    class_diagram_fitted(source, None)
}

pub(super) fn class_diagram_fitted(source: &str, max_width: Option<usize>) -> Result<Frame> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("The class diagram exceeds the 16 MiB input limit".into());
    }
    let mut reader = Reader::default();
    super::mermaid_lines(source, |number, line| reader.line(number, line))?;
    layout_fitted(&reader.finish()?, max_width)
}

struct ClassEntry {
    label: String,
    stereotypes: Vec<String>,
    fields: Vec<String>,
    methods: Vec<String>,
    self_loop: Option<String>,
}

impl ClassEntry {
    fn new(name: &str) -> Self {
        Self {
            label: name.to_owned(),
            stereotypes: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            self_loop: None,
        }
    }

    fn into_box(self) -> BoxNode {
        let mut header = self.stereotypes;
        header.push(self.label);
        let mut sections = vec![header];
        if !self.fields.is_empty() {
            sections.push(self.fields);
        }
        if !self.methods.is_empty() {
            sections.push(self.methods);
        }
        BoxNode {
            sections,
            self_loop: self.self_loop,
        }
    }
}

#[derive(Default)]
struct Reader {
    started: bool,
    flow: Option<Flow>,
    ids: HashMap<String, usize>,
    classes: Vec<ClassEntry>,
    links: Vec<Edge>,
    /// Currently open `class Name {` block: (class id, opening line number).
    open_block: Option<(usize, usize)>,
}

fn valid_class_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'~'))
}

fn is_method(member: &str) -> bool {
    member.contains('(') && member.contains(')')
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
            if let Some((class_id, _)) = self.open_block {
                if let Some(before) = stmt.strip_suffix('}') {
                    let inner = before.trim();
                    if !inner.is_empty() {
                        self.add_member(number, class_id, inner)?;
                    }
                    self.open_block = None;
                } else {
                    self.add_member(number, class_id, stmt)?;
                }
                continue;
            }
            if !self.started {
                if stmt != "classDiagram" {
                    return Err(format!(
                        "Line {number}: a class diagram starts with classDiagram"
                    ));
                }
                self.started = true;
                continue;
            }
            self.statement(number, stmt)?;
        }
        Ok(())
    }

    fn class_id(&mut self, number: usize, name: &str) -> Result<usize> {
        let name = name.trim();
        if !valid_class_name(name) {
            return Err(format!(
                "Line {number}: a class is named with ASCII letters, digits, _, - or ~, not '{name}'"
            ));
        }
        if let Some(&id) = self.ids.get(name) {
            return Ok(id);
        }
        if self.classes.len() == MAX_NODES {
            return Err(format!(
                "Line {number}: a class diagram has at most {MAX_NODES} classes"
            ));
        }
        let id = self.classes.len();
        self.ids.insert(name.to_owned(), id);
        self.classes.push(ClassEntry::new(name));
        Ok(id)
    }

    fn add_member(&mut self, number: usize, class_id: usize, raw: &str) -> Result<()> {
        let member = decode(number, raw.trim())?;
        if member.starts_with("<<") && member.ends_with(">>") {
            self.classes[class_id].stereotypes.push(member);
        } else if is_method(&member) {
            self.classes[class_id].methods.push(member);
        } else {
            self.classes[class_id].fields.push(member);
        }
        Ok(())
    }

    fn statement(&mut self, number: usize, stmt: &str) -> Result<()> {
        if stmt == "}" {
            return Err(format!("Line {number}: }} closes no class block"));
        }
        let first = stmt.split_whitespace().next().unwrap_or_default();
        if matches!(first, "classDef" | "style" | "cssClass") {
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
        if first == "class" {
            return self.class_decl(number, stmt[5..].trim());
        }
        if stmt.starts_with("<<")
            && let Some(end) = stmt.find(">>")
        {
            let stereo = decode(number, &stmt[..end + 2])?;
            let class_name = stmt[end + 2..].trim();
            let id = self.class_id(number, class_name)?;
            self.classes[id].stereotypes.push(stereo);
            return Ok(());
        }
        if self.try_relation(number, stmt)? {
            return Ok(());
        }
        if let Some((name, member)) = stmt.split_once(':') {
            let id = self.class_id(number, name)?;
            let member = member.trim();
            if !member.is_empty() {
                self.add_member(number, id, member)?;
            }
            return Ok(());
        }
        Err(format!(
            "Line {number}: '{stmt}' is not a class diagram statement this reads"
        ))
    }

    fn class_decl(&mut self, number: usize, rest: &str) -> Result<()> {
        let (header, body) = match rest.split_once('{') {
            Some((head, tail)) => (head.trim(), Some(tail.trim())),
            None => (rest, None),
        };
        let (name, display) = parse_class_header(number, header)?;
        let id = self.class_id(number, name)?;
        if let Some(label) = display {
            self.classes[id].label = label;
        }
        if let Some(tail) = body {
            if let Some(inner) = tail.strip_suffix('}') {
                if !inner.trim().is_empty() {
                    self.add_member(number, id, inner)?;
                }
            } else {
                if !tail.is_empty() {
                    self.add_member(number, id, tail)?;
                }
                self.open_block = Some((id, number));
            }
        }
        Ok(())
    }

    fn try_relation(&mut self, number: usize, stmt: &str) -> Result<bool> {
        let (rel_part, label_part) = match stmt.split_once(':') {
            Some((r, l)) => (r.trim(), Some(l.trim())),
            None => (stmt, None),
        };
        let Some((pos, op)) = find_relation(rel_part) else {
            return Ok(false);
        };
        let left_raw = rel_part[..pos].trim();
        let right_raw = rel_part[pos + op.symbol.len()..].trim();
        let (left_name, card_left) = split_left_card(left_raw);
        let (card_right, right_name) = split_right_card(right_raw);
        let from = self.class_id(number, left_name)?;
        let to = self.class_id(number, right_name)?;
        let label = match label_part {
            Some(text) => decode(number, text)?,
            None => String::new(),
        };
        let edge_text = compose_edge_label(op.tag, card_left, card_right, &label);
        if from == to {
            let mut node = BoxNode {
                sections: Vec::new(),
                self_loop: self.classes[from].self_loop.take(),
            };
            node.add_self_loop(&edge_text);
            self.classes[from].self_loop = node.self_loop;
            return Ok(true);
        }
        if self.links.len() == MAX_LINKS {
            return Err(format!(
                "Line {number}: a class diagram has at most {MAX_LINKS} relationships"
            ));
        }
        self.links.push(Edge {
            from,
            to,
            text: if edge_text.is_empty() {
                Vec::new()
            } else {
                vec![edge_text]
            },
            head: op.head,
            tail: op.tail,
            dotted: op.dotted,
            length: 1,
        });
        Ok(true)
    }

    fn finish(self) -> Result<Graph> {
        if !self.started {
            return Err("A class diagram starts with classDiagram".into());
        }
        if let Some((_, line)) = self.open_block {
            return Err(format!(
                "The class block opened on line {line} is not closed"
            ));
        }
        if self.classes.is_empty() {
            return Err("A class diagram needs at least one class".into());
        }
        Ok(Graph {
            kind: "class diagram",
            flow: self.flow.unwrap_or(Flow::Down),
            nodes: self.classes.into_iter().map(ClassEntry::into_box).collect(),
            links: self.links,
        })
    }
}

fn parse_class_header(number: usize, header: &str) -> Result<(&str, Option<String>)> {
    if let Some((name, rest)) = header.split_once("[\"") {
        let label = rest
            .strip_suffix("\"]")
            .ok_or_else(|| format!("Line {number}: class label bracket is not closed"))?;
        return Ok((name.trim(), Some(decode(number, label)?)));
    }
    Ok((header, None))
}

fn find_relation(text: &str) -> Option<(usize, &'static RelationOp)> {
    let mut in_quotes = false;
    let bytes = text.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'"' {
            in_quotes = !in_quotes;
            continue;
        }
        if in_quotes {
            continue;
        }
        if let Some(op) = RELATIONS.iter().find(|op| text[i..].starts_with(op.symbol)) {
            // Avoid matching `o--`, `--` or `..` inside an identifier such as `Foo--Bar`.
            let needs_boundary = matches!(op.symbol, "o--" | "--" | "..");
            let at_boundary = i > 0 && matches!(bytes[i - 1], b' ' | b'\t' | b'"');
            if needs_boundary && !at_boundary {
                continue;
            }
            return Some((i, op));
        }
    }
    None
}

fn split_left_card(text: &str) -> (&str, Option<&str>) {
    if let Some(rest) = text.strip_suffix('"')
        && let Some((name, card)) = rest.rsplit_once('"')
    {
        return (name.trim(), Some(card));
    }
    (text, None)
}

fn split_right_card(text: &str) -> (Option<&str>, &str) {
    if let Some(rest) = text.strip_prefix('"')
        && let Some((card, name)) = rest.split_once('"')
    {
        return (Some(card), name.trim());
    }
    (None, text)
}

fn compose_edge_label(
    tag: Option<&str>,
    card_left: Option<&str>,
    card_right: Option<&str>,
    label: &str,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(tag) = tag {
        parts.push(tag.to_owned());
    }
    match (card_left, card_right) {
        (Some(l), Some(r)) => parts.push(format!("{l}..{r}")),
        (Some(c), None) | (None, Some(c)) => parts.push(c.to_owned()),
        (None, None) => {}
    }
    if !label.is_empty() {
        parts.push(label.to_owned());
    }
    parts.join(" ")
}
