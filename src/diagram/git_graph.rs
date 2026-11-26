// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid Git graphs (`gitGraph`) drawn as branch lanes with one row per commit; see
//! [`git_graph`].

use super::decode;
use super::graph::wrap_lines;
use crate::{Frame, MAX_HEIGHT, MAX_INPUT_BYTES, MAX_WIDTH, Result};
use std::collections::HashMap;

/// Columns from one lane to the next.
const LANE: usize = 3;
/// Each lane takes 3 columns, so no more fit the widest drawing.
const MAX_BRANCHES: usize = MAX_WIDTH / LANE;

/// Draw a Mermaid Git graph (`gitGraph`) in printable ASCII.
///
/// Each branch is a lane, named at its head and ordered as Mermaid orders them: the main
/// branch, then branches in the order made, then those with `order: N` by that number. Each
/// commit takes a row, in the order written, so time runs down the page (`gitGraph BT:` runs up
/// it; `LR:` and `TB:` run down). A commit is drawn `o`, `x` with `type: REVERSE`, `#` with
/// `type: HIGHLIGHT`, `@` for a merge and `%` for a cherry-pick, with a legend for the kinds
/// used. A line from another lane joins a commit to a parent there: `+->o` or `o<-+` where the
/// parent is the last commit above on that lane, else `o->+` from the parent's row down the
/// lane to the child, so every `+` on a row joins the commit on that row. A parent neither
/// line can reach without passing another commit is joined at the child's row by a line ending
/// in `*` on the parent's lane (none when it is on the child's own lane), and the child's label
/// names it, `(from <id>)`, unless a `cherry-pick:<id>` tag already does.
///
/// The source starts with `gitGraph` and may sit inside one Markdown code block fenced with
/// ```` ``` ```` or `~~~`. Lines starting `%%` are comments, apart from a `%%{init: ...}%%`
/// directive's `mainBranchName`.
///
/// - `commit` with `id: "..."`, `tag: "..."` (any number), `type: NORMAL`, `REVERSE` or
///   `HIGHLIGHT` and a message (`msg: "..."`, not drawn).
/// - `branch name` (with `order: N`), `checkout name` or `switch name`, `merge name` (with `id`,
///   `tag` and `type`) and `cherry-pick id: "..."` (with `parent` and `tag`), refused where
///   Mermaid refuses them.
/// - `title` names the graph; `accTitle` and `accDescr` are skipped.
///
/// Mermaid labels a commit without an `id` with a random one; those are not drawn. A merge
/// shows its `id` only when one is given, and a cherry-pick shows `cherry-pick:<id>` unless it
/// has tags. A commit `id` given twice is refused.
///
/// ```
/// use apollinaire_ascii::diagram::git_graph;
/// let graph = git_graph(
///     "gitGraph\n  commit id: \"A\"\n  branch dev\n  commit id: \"B\"\n  checkout main\n  merge dev",
/// )?;
/// assert_eq!(
///     graph.text(),
///     "main             \n|  dev           \no  |  A          \n+->o  B          \n@<-+             \no commit  @ merge"
/// );
/// # Ok::<(), String>(())
/// ```
pub fn git_graph(source: &str) -> Result<Frame> {
    git_graph_fitted(source, None)
}

pub(super) fn git_graph_fitted(source: &str, max_width: Option<usize>) -> Result<Frame> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("The Git graph exceeds the 16 MiB input limit".into());
    }
    let mut reader = Reader {
        branches: vec![Branch {
            name: "main".into(),
            order: Some(0.0),
            head: None,
            last: None,
        }],
        ..Reader::default()
    };
    super::mermaid_lines(source, |number, line| reader.line(number, line))?;
    if !reader.started {
        return Err("A Git graph starts with gitGraph".into());
    }
    if reader.in_block {
        return Err(format!(
            "The accDescr block opened on line {} is not closed",
            reader.block_line
        ));
    }
    draw(&reader, max_width)
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Normal,
    Reverse,
    Highlight,
    Merge,
    CherryPick,
}

struct Commit {
    branch: usize,
    parents: Vec<usize>,
    /// How it is drawn; `type:` can draw a merge as another kind.
    kind: Kind,
    /// Whether it is a merge, whatever its `type:`, as Mermaid checks a cherry-pick by this.
    merge: bool,
    label: String,
    /// Parents no lane line can reach; each is joined by a marked line at this row.
    marked: Vec<usize>,
}

struct Branch {
    name: String,
    /// `None` takes Mermaid's default, made from the branch's place in the list.
    order: Option<f64>,
    head: Option<usize>,
    /// The last commit drawn on this branch's lane.
    last: Option<usize>,
}

#[derive(Default)]
struct Reader {
    started: bool,
    upward: bool,
    in_block: bool,
    block_line: usize,
    title: Option<String>,
    branches: Vec<Branch>,
    current: usize,
    commits: Vec<Commit>,
    ids: HashMap<String, usize>,
}

/// A statement's words after its keyword: `key:` names, quoted strings and bare words.
#[derive(PartialEq)]
enum Token {
    Key(String),
    Text(String),
    Word(String),
}

fn tokens(number: usize, text: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        if rest.starts_with("%%") {
            break;
        }
        if rest.starts_with(['"', '\'']) {
            let quote = &rest[..1];
            let Some(end) = rest[1..].find(quote) else {
                return Err(format!("Line {number}: the text's {quote} is not closed"));
            };
            tokens.push(Token::Text(decode(
                number,
                &rest[1..=end].replace('\t', " "),
            )?));
            rest = &rest[end + 2..];
        } else {
            let end = rest
                .find([' ', '\t', '"', '\'', ':'])
                .map_or(rest.len(), |at| {
                    at + usize::from(rest.as_bytes()[at] == b':')
                });
            let word = &rest[..end];
            tokens.push(match word.strip_suffix(':') {
                Some(key) => Token::Key(key.to_owned()),
                None => Token::Word(word.to_owned()),
            });
            rest = &rest[end..];
        }
        rest = rest.trim_start();
    }
    Ok(tokens)
}

/// Whether `text` starts an `accTitle:` or `accDescr:` line (`Some(false)`) or an
/// `accDescr {` block (`Some(true)`); Mermaid's Git graph grammar reads both names in this case
/// only.
fn accessibility(text: &str) -> Option<bool> {
    let (rest, describes) = match text.strip_prefix("accTitle") {
        Some(rest) => (rest, false),
        None => (text.strip_prefix("accDescr")?, true),
    };
    match rest.trim_start().bytes().next() {
        Some(b':') => Some(false),
        Some(b'{') if describes => Some(true),
        _ => None,
    }
}

/// Mermaid's branch names: a letter, digit or `_`, then those, `-`, `.` and `/`, not ending in
/// `.` or `/`; or any quoted text.
fn branch_name(number: usize, token: Option<&Token>) -> Result<String> {
    match token {
        Some(Token::Text(name)) if !name.is_empty() => Ok(name.clone()),
        Some(Token::Word(name))
            if name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-./".contains(&b))
                && !name.starts_with(['-', '.', '/'])
                && !name.ends_with(['.', '/']) =>
        {
            Ok(name.clone())
        }
        _ => Err(format!("Line {number}: a branch name is expected")),
    }
}

impl Reader {
    fn line(&mut self, number: usize, line: &str) -> Result<()> {
        let mut stmt = line.trim();
        loop {
            // An accDescr block's text runs to its '}', comments included; the line goes on
            // after it.
            if self.in_block {
                let Some(at) = stmt.find('}') else {
                    return Ok(());
                };
                self.in_block = false;
                stmt = stmt[at + 1..].trim();
            }
            if stmt.starts_with("%%{") {
                return self.directive(number, stmt);
            }
            if stmt.is_empty() || stmt.starts_with("%%") {
                return Ok(());
            }
            match accessibility(stmt) {
                Some(true) if self.started => {
                    self.in_block = true;
                    self.block_line = number;
                    stmt = &stmt[stmt.find('{').map_or(stmt.len(), |at| at + 1)..];
                }
                Some(false) if self.started => return Ok(()),
                _ => break,
            }
        }
        if !self.started {
            let header: String = stmt.split_whitespace().collect::<Vec<_>>().join(" ");
            self.upward = match header.as_str() {
                "gitGraph" | "gitGraph:" | "gitGraph :" | "gitGraph LR:" | "gitGraph TB:"
                | "gitGraph LR :" | "gitGraph TB :" => false,
                "gitGraph BT:" | "gitGraph BT :" => true,
                _ => {
                    return Err(format!(
                        "Line {number}: a Git graph starts with gitGraph, gitGraph LR:, gitGraph TB: or gitGraph BT:"
                    ));
                }
            };
            self.started = true;
            return Ok(());
        }
        let (word, rest) = stmt
            .split_once([' ', '\t'])
            .map_or((stmt, ""), |(w, r)| (w, r.trim()));
        match word {
            "title" => {
                let title = rest.split("%%").next().unwrap_or("").trim();
                let title = decode(number, &title.replace('\t', " "))?;
                self.title = (!title.is_empty()).then_some(title);
                Ok(())
            }
            "commit" => self.commit(number, &tokens(number, rest)?),
            "branch" => self.branch(number, &tokens(number, rest)?),
            "checkout" | "switch" => {
                let tokens = tokens(number, rest)?;
                let name = branch_name(number, tokens.first())?;
                if tokens.len() > 1 {
                    return Err(format!("Line {number}: {word} takes only a branch name"));
                }
                self.current = self.find(&name).ok_or_else(|| {
                    format!("Line {number}: there is no branch {name} to check out; make it with branch {name}")
                })?;
                Ok(())
            }
            "merge" => self.merge(number, &tokens(number, rest)?),
            "cherry-pick" => self.cherry_pick(number, &tokens(number, rest)?),
            _ => Err(format!(
                "Line {number}: '{stmt}' is not a Git graph statement; use commit, branch, checkout, merge or cherry-pick"
            )),
        }
    }

    /// Read `mainBranchName` from a `%%{init: ...}%%` directive, before the first commit.
    fn directive(&mut self, number: usize, stmt: &str) -> Result<()> {
        let Some(at) = stmt.find("mainBranchName") else {
            return Ok(());
        };
        let after = stmt[at + "mainBranchName".len()..].trim_start_matches(['\'', '"', ' ', ':']);
        let name: String = after
            .chars()
            .take_while(|c| !matches!(c, '\'' | '"' | ',' | '}'))
            .collect();
        let name = decode(number, &name.trim().replace('\t', " "))?;
        if name.is_empty() || self.started {
            return Err(format!(
                "Line {number}: mainBranchName is read from a directive before gitGraph, as a quoted name"
            ));
        }
        self.branches[0].name = name;
        Ok(())
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.branches.iter().position(|b| b.name == name)
    }

    /// The attributes after a statement's keyword: each `key: value` once read, in any order.
    fn attributes<'a>(
        number: usize,
        tokens: &'a [Token],
        keys: &[&str],
    ) -> Result<Vec<(&'a str, &'a Token)>> {
        let mut pairs = Vec::new();
        let mut at = 0;
        while at < tokens.len() {
            match (&tokens[at], tokens.get(at + 1)) {
                (Token::Key(key), Some(value)) if keys.contains(&key.as_str()) => {
                    pairs.push((key.as_str(), value));
                    at += 2;
                }
                // A commit's message, which is not drawn.
                (Token::Text(_), _) if keys.contains(&"msg") => {
                    at += 1;
                }
                (Token::Key(key), _) | (Token::Word(key), _) | (Token::Text(key), _) => {
                    return Err(format!(
                        "Line {number}: '{key}' is not read here; this statement takes {}",
                        keys.iter()
                            .map(|k| format!("{k}:"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
        }
        Ok(pairs)
    }

    fn text(number: usize, key: &str, value: &Token) -> Result<String> {
        match value {
            Token::Text(text) => Ok(text.clone()),
            _ => Err(format!("Line {number}: {key}: takes quoted text")),
        }
    }

    fn kind(number: usize, value: &Token) -> Result<Kind> {
        match value {
            Token::Word(word) if word == "NORMAL" => Ok(Kind::Normal),
            Token::Word(word) if word == "REVERSE" => Ok(Kind::Reverse),
            Token::Word(word) if word == "HIGHLIGHT" => Ok(Kind::Highlight),
            _ => Err(format!(
                "Line {number}: type: is NORMAL, REVERSE or HIGHLIGHT"
            )),
        }
    }

    /// Id, tags and type from `pairs`; the id is refused if a commit has it already.
    fn details(
        &self,
        number: usize,
        pairs: &[(&str, &Token)],
    ) -> Result<(Option<String>, Vec<String>, Option<Kind>)> {
        let (mut id, mut tags, mut kind) = (None, Vec::new(), None);
        for &(key, value) in pairs {
            match key {
                "id" => id = Some(Self::text(number, key, value)?),
                "tag" => tags.push(Self::text(number, key, value)?),
                "type" => kind = Some(Self::kind(number, value)?),
                _ => {}
            }
        }
        if let Some(id) = &id
            && self.ids.contains_key(id)
        {
            return Err(format!(
                "Line {number}: a commit with id \"{id}\" already exists; use another id"
            ));
        }
        Ok((id, tags, kind))
    }

    /// Add `commit`; `merged` names the branch a merge brings in, for a parent with no id.
    fn push(
        &mut self,
        number: usize,
        id: Option<String>,
        mut commit: Commit,
        merged: Option<&str>,
    ) -> Result<()> {
        if self.commits.len() == MAX_HEIGHT {
            return Err(format!(
                "Line {number}: a Git graph has at most {MAX_HEIGHT} commits"
            ));
        }
        // A parent on this lane is joined by the lane from the commit above it. One on another
        // lane is joined at this row when it is the last on its lane, else from its own row
        // down this lane when that reaches this commit first. Any other parent is marked and
        // named, as Mermaid draws these histories too.
        let lane = &self.branches[self.current];
        for &parent in &commit.parents {
            let on = &self.branches[self.commits[parent].branch];
            let joined = on.last == Some(parent)
                || (commit.branch != self.commits[parent].branch
                    && lane.last.is_none_or(|last| last < parent));
            if joined {
                continue;
            }
            commit.marked.push(parent);
            let named = self
                .ids
                .iter()
                .find_map(|(id, &at)| (at == parent).then(|| id.clone()))
                .or_else(|| merged.map(str::to_owned))
                .unwrap_or_else(|| on.name.clone());
            if !commit.label.contains(&format!("cherry-pick:{named}")) {
                if !commit.label.is_empty() {
                    commit.label.push(' ');
                }
                commit.label.push_str(&format!("(from {named})"));
            }
        }
        let index = self.commits.len();
        if let Some(id) = id {
            self.ids.insert(id, index);
        }
        self.branches[self.current].head = Some(index);
        self.branches[self.current].last = Some(index);
        self.commits.push(commit);
        Ok(())
    }

    fn label(id: Option<&str>, tags: &[String]) -> String {
        id.into_iter()
            .map(str::to_owned)
            .chain(tags.iter().map(|tag| format!("[{tag}]")))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn commit(&mut self, number: usize, tokens: &[Token]) -> Result<()> {
        let pairs = Self::attributes(number, tokens, &["id", "msg", "tag", "type"])?;
        let (id, tags, kind) = self.details(number, &pairs)?;
        let commit = Commit {
            branch: self.current,
            parents: self.branches[self.current].head.into_iter().collect(),
            kind: kind.unwrap_or(Kind::Normal),
            merge: false,
            label: Self::label(id.as_deref(), &tags),
            marked: Vec::new(),
        };
        self.push(number, id, commit, None)
    }

    fn branch(&mut self, number: usize, tokens: &[Token]) -> Result<()> {
        let name = branch_name(number, tokens.first())?;
        let order = match &tokens[1..] {
            [] => None,
            [Token::Key(key), Token::Word(order)]
                if key == "order" && order.bytes().all(|b| b.is_ascii_digit()) =>
            {
                Some(
                    order
                        .parse::<f64>()
                        .map_err(|e| format!("Line {number}: {e}"))?,
                )
            }
            _ => {
                return Err(format!(
                    "Line {number}: a branch is written branch name, with an optional order: N"
                ));
            }
        };
        if self.find(&name).is_some() {
            return Err(format!(
                "Line {number}: branch {name} already exists; use checkout {name} to work on it"
            ));
        }
        if self.branches.len() == MAX_BRANCHES {
            return Err(format!(
                "Line {number}: a Git graph has at most {MAX_BRANCHES} branches"
            ));
        }
        let head = self.branches[self.current].head;
        self.branches.push(Branch {
            name,
            order,
            head,
            last: None,
        });
        self.current = self.branches.len() - 1;
        Ok(())
    }

    fn merge(&mut self, number: usize, tokens: &[Token]) -> Result<()> {
        let name = branch_name(number, tokens.first())?;
        let pairs = Self::attributes(number, &tokens[1..], &["id", "tag", "type"])?;
        let current = &self.branches[self.current];
        let other = self.find(&name);
        let other_head = other.and_then(|b| self.branches[b].head);
        let error = |text: String| Err(format!("Line {number}: {text}"));
        if let (Some(head), Some(_)) = (current.head, other_head)
            && self.commits[head].branch == other.unwrap_or(usize::MAX)
        {
            return error(format!("cannot merge branch {name} into itself"));
        }
        if other == Some(self.current) {
            return error(format!("cannot merge branch {name} into itself"));
        }
        let Some(head) = current.head else {
            return error(format!(
                "branch {} has no commits to merge into",
                current.name
            ));
        };
        if other.is_none() {
            return error(format!("there is no branch {name} to merge"));
        }
        let Some(other_head) = other_head else {
            return error(format!("branch {name} has no commits to merge"));
        };
        if head == other_head {
            return error(format!(
                "branch {name} has no commits that {} lacks",
                current.name
            ));
        }
        let (id, tags, kind) = self.details(number, &pairs)?;
        let commit = Commit {
            branch: self.current,
            parents: vec![head, other_head],
            kind: kind.unwrap_or(Kind::Merge),
            merge: true,
            label: Self::label(id.as_deref(), &tags),
            marked: Vec::new(),
        };
        self.push(number, id, commit, Some(&name))
    }

    fn cherry_pick(&mut self, number: usize, tokens: &[Token]) -> Result<()> {
        let pairs = Self::attributes(number, tokens, &["id", "parent", "tag"])?;
        let (mut source, mut parent, mut tags) = (None, None, None::<Vec<String>>);
        for &(key, value) in &pairs {
            let text = Self::text(number, key, value)?;
            match key {
                "id" => source = Some(text),
                "parent" => parent = Some(text),
                _ => tags.get_or_insert_default().push(text),
            }
        }
        let error = |text: &str| Err(format!("Line {number}: {text}"));
        let Some(source_index) = source.as_ref().and_then(|id| self.ids.get(id).copied()) else {
            return error("cherry-pick needs the id: of a commit already drawn");
        };
        let picked = &self.commits[source_index];
        let parent_index = parent.as_ref().and_then(|id| self.ids.get(id).copied());
        if parent.is_some() && !parent_index.is_some_and(|p| picked.parents.contains(&p)) {
            return error("the parent: given is not an immediate parent of the commit picked");
        }
        if picked.merge && parent.is_none() {
            return error("cherry-picking a merge needs the parent: it is picked against");
        }
        if picked.branch == self.current {
            return error("the commit picked is already on this branch");
        }
        let Some(head) = self.branches[self.current].head else {
            return error("cherry-pick needs a commit on this branch first");
        };
        let source = source.unwrap_or_default();
        // Given tags replace the default one, and empty ones are dropped, as in Mermaid.
        let tags = tags.map_or_else(
            || match (&parent, picked.merge) {
                (Some(parent), true) => {
                    vec![format!("cherry-pick:{source}|parent:{parent}")]
                }
                _ => vec![format!("cherry-pick:{source}")],
            },
            |mut tags| {
                tags.retain(|tag| !tag.is_empty());
                tags
            },
        );
        let commit = Commit {
            branch: self.current,
            parents: vec![head, source_index],
            kind: Kind::CherryPick,
            merge: false,
            label: Self::label(None, &tags),
            marked: Vec::new(),
        };
        self.push(number, None, commit, None)
    }
}

/// Lanes in Mermaid's order: an explicit `order`, else `0.<place>` read as a number, the main
/// branch's place being 0; a stable sort keeps ties in the order made.
fn lane_order(branches: &[Branch]) -> Vec<usize> {
    let keys: Vec<f64> = (0..branches.len())
        .map(|index| {
            branches[index]
                .order
                .unwrap_or_else(|| format!("0.{index}").parse().unwrap_or(0.0))
        })
        .collect();
    let mut order: Vec<usize> = (0..branches.len()).collect();
    order.sort_by(|&a, &b| keys[a].total_cmp(&keys[b]));
    let mut lane = vec![0; branches.len()];
    for (place, &branch) in order.iter().enumerate() {
        lane[branch] = place;
    }
    lane
}

fn too_tall() -> String {
    format!("The Git graph needs more than {MAX_HEIGHT} rows; a drawing has at most {MAX_HEIGHT}")
}

/// `text` wrapped to `width`; refused, before any row is made, when it needs more rows than a
/// drawing has.
fn wrapped(text: &str, width: usize) -> Result<Vec<String>> {
    let filled = text.bytes().filter(|&b| b != b' ').count();
    if filled.div_ceil(width) > MAX_HEIGHT {
        return Err(too_tall());
    }
    Ok(wrap_lines(text, width))
}

fn draw(reader: &Reader, max_width: Option<usize>) -> Result<Frame> {
    let lanes = lane_order(&reader.branches);
    let count = reader.branches.len();
    let grid = (count - 1) * LANE + 1;
    if max_width.is_some_and(|width| width < grid + 3) {
        return Err(format!(
            "This Git graph needs a --max-width of at least {} columns",
            grid + 3
        ));
    }
    let label_room = max_width.map_or(MAX_WIDTH - grid - 2, |width| width - grid - 2);
    let column = |commit: usize| lanes[reader.commits[commit].branch] * LANE;

    // Each row's lane cells, then the hops drawn across it.
    let mut rows: Vec<Vec<u8>> = reader
        .commits
        .iter()
        .map(|commit| {
            let mut row: Vec<u8> = (0..grid)
                .map(|x| if x % LANE == 0 { b'|' } else { b' ' })
                .collect();
            row[lanes[commit.branch] * LANE] = match commit.kind {
                Kind::Normal => b'o',
                Kind::Reverse => b'x',
                Kind::Highlight => b'#',
                Kind::Merge => b'@',
                Kind::CherryPick => b'%',
            };
            row
        })
        .collect();
    let mut last: Vec<Option<usize>> = vec![None; count];
    for (child, commit) in reader.commits.iter().enumerate() {
        for &parent in &commit.parents {
            let parent_branch = reader.commits[parent].branch;
            if parent_branch == commit.branch {
                continue;
            }
            if commit.marked.contains(&parent) {
                hop(&mut rows[child], column(child), column(parent), true);
                rows[child][column(parent)] = b'*';
                continue;
            }
            // Into the child from the parent's lane, or out of the parent to the child's lane.
            let (row, at, lane, into_commit) = if last[parent_branch] == Some(parent) {
                (child, column(child), column(parent), true)
            } else {
                (parent, column(parent), column(child), false)
            };
            hop(&mut rows[row], at, lane, into_commit);
        }
        last[commit.branch] = Some(child);
    }

    let blank: String = (0..grid)
        .map(|x| if x % LANE == 0 { '|' } else { ' ' })
        .collect();
    // Each entry is a commit's rows or a branch name's rows; an upward graph reverses entries,
    // never the lines inside one.
    let mut entries: Vec<Vec<String>> = Vec::new();
    let mut graph: Vec<Vec<String>> = Vec::new();
    let mut height = 0;
    for (commit, row) in reader.commits.iter().zip(&rows) {
        if height > MAX_HEIGHT {
            return Err(too_tall());
        }
        let cells = String::from_utf8_lossy(row).into_owned();
        let mut entry = Vec::new();
        for (index, part) in wrapped(&commit.label, label_room)?.into_iter().enumerate() {
            let lanes = if index == 0 { &cells } else { &blank };
            entry.push(if part.is_empty() {
                lanes.clone()
            } else {
                format!("{lanes}  {part}")
            });
        }
        height += entry.len();
        graph.push(entry);
    }

    // Branch names, one per row, each over its lane, as steps.
    let mut by_lane: Vec<usize> = (0..count).collect();
    by_lane.sort_by_key(|&b| lanes[b]);
    let width = max_width.unwrap_or(MAX_WIDTH);
    for (place, &branch) in by_lane.iter().enumerate() {
        let before = "|  ".repeat(place);
        entries.push(
            wrapped(&reader.branches[branch].name, width - place * LANE)?
                .into_iter()
                .map(|part| format!("{before}{part}"))
                .collect(),
        );
    }
    entries.extend(graph);
    if reader.upward {
        entries.reverse();
    }
    let body: Vec<String> = entries.into_iter().flatten().collect();

    let used = |kind: Kind| reader.commits.iter().any(|c| c.kind == kind);
    let legend: Vec<&str> = [
        (used(Kind::Normal), "o commit"),
        (used(Kind::Reverse), "x reverse"),
        (used(Kind::Highlight), "# highlight"),
        (used(Kind::Merge), "@ merge"),
        (used(Kind::CherryPick), "% cherry-pick"),
        (
            reader.commits.iter().any(|c| {
                c.marked
                    .iter()
                    .any(|&p| reader.commits[p].branch != c.branch)
            }),
            "* from the commit named",
        ),
    ]
    .into_iter()
    .filter_map(|(on, text)| on.then_some(text))
    .collect();
    // Legend entries are kept whole on a line where the width allows.
    let mut key: Vec<String> = Vec::new();
    for item in legend {
        match key.last_mut() {
            Some(last) if last.len() + 2 + item.len() <= width => {
                last.push_str("  ");
                last.push_str(item);
            }
            _ => key.extend(wrap_lines(item, width)),
        }
    }
    // A title wraps to the width of the rest of the drawing.
    let drawn = body
        .iter()
        .chain(&key)
        .map(String::len)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut lines: Vec<String> = Vec::new();
    if let Some(title) = &reader.title {
        lines.extend(wrapped(title, drawn.min(width))?);
        lines.push(String::new());
    }
    lines.extend(body);
    lines.extend(key);
    if lines.len() > MAX_HEIGHT {
        return Err(format!(
            "The Git graph needs {} rows; a drawing has at most {MAX_HEIGHT}",
            lines.len()
        ));
    }
    Frame::from_text_padded(&lines.join("\n"))
}

/// Draw a line along a row from the commit at column `at` to the lane at column `lane`, which
/// ends in `+`. Lanes it crosses stay `|`. The arrowhead points at the commit when the line
/// brings a parent into it, else at the lane, where room allows.
fn hop(row: &mut [u8], at: usize, lane: usize, into_commit: bool) {
    let (low, high) = (at.min(lane), at.max(lane));
    // An arrowhead another line left on this row is kept.
    for (x, cell) in row.iter_mut().enumerate().take(high).skip(low + 1) {
        if x % LANE != 0 && *cell == b' ' {
            *cell = b'-';
        }
    }
    row[lane] = b'+';
    let rightward = lane > at;
    let (cell, head) = match (into_commit, rightward) {
        (true, true) => (at + 1, b'<'),
        (true, false) => (at - 1, b'>'),
        (false, true) => (lane - 1, b'>'),
        (false, false) => (lane + 1, b'<'),
    };
    if row[cell] == b'-' {
        row[cell] = head;
    }
}
