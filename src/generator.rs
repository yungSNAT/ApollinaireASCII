// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Still pictures built to a size: mazes, night skies and repeating patterns. The same size,
//! options and seed always give the same picture. The animated `starfield` scene is
//! [`animation::generate`](crate::animation::generate); a sky here is a still.
//!
//! ```
//! use apollinaire_ascii::generator::{self, MazeOptions};
//! let maze = generator::maze(9, 5, 7, &MazeOptions::default())?;
//! assert_eq!(maze.height(), 5);
//! assert_eq!(&maze.text()[..9], "# #######");
//! # Ok::<(), String>(())
//! ```
use crate::animation::random;
use crate::{Frame, Result, dimensions, printable};
use std::collections::VecDeque;

/// How a maze's passages are carved. Each gives a perfect maze: one path between any 2 cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum Algorithm {
    /// Depth-first search: long, winding passages.
    #[default]
    DepthFirst,
    /// Randomised Prim: many short dead ends.
    Prim,
    /// Wilson's loop-erased random walks: every perfect maze equally likely.
    #[cfg_attr(
        feature = "cli",
        value(help = "Wilson's loop-erased random walks: every maze equally likely")
    )]
    Wilson,
    /// Sidewinder: row by row, with an open top row.
    #[cfg_attr(feature = "cli", value(help = "Row by row, with an open top row"))]
    Sidewinder,
}

/// The characters a maze's walls are drawn with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum MazeStyle {
    /// `#` for every wall.
    #[default]
    #[cfg_attr(feature = "cli", value(help = "# walls"))]
    Hash,
    /// `+` at corners, `-` and `|` between them.
    #[cfg_attr(feature = "cli", value(help = "+, - and | walls"))]
    Lines,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MazeOptions {
    pub algorithm: Algorithm,
    pub style: MazeStyle,
    /// Mark the path from the entrance to the exit with `.`.
    pub solution: bool,
}

/// A deterministic stream of choices from a seed.
struct Choices {
    seed: u32,
    index: u32,
}
impl Choices {
    fn new(seed: u32) -> Self {
        Self { seed, index: 0 }
    }
    /// A number from 0 to `count - 1`; `count` is at least 1.
    fn below(&mut self, count: usize) -> usize {
        let value = random(self.seed, self.index);
        self.index = self.index.wrapping_add(1);
        ((value * count as f64) as usize).min(count - 1)
    }
}

/// A perfect maze filling `width` by `height` characters: its cells sit on odd columns and rows
/// with walls between them, the entrance is in the top wall at the top-left cell and the exit in
/// the bottom wall at the bottom-right cell. An even width or height leaves its last column or
/// row blank. Needs at least 3 columns and 3 rows.
pub fn maze(width: usize, height: usize, seed: u32, options: &MazeOptions) -> Result<Frame> {
    dimensions(width, height)?;
    if width < 3 || height < 3 {
        return Err("A maze needs at least 3 columns and 3 rows".into());
    }
    let (columns, rows) = ((width - 1) / 2, (height - 1) / 2);
    let cells = columns * rows;
    let (inner_width, inner_height) = (2 * columns + 1, 2 * rows + 1);
    let mut open = vec![false; width * height];
    for row in 0..rows {
        for column in 0..columns {
            open[(2 * row + 1) * width + 2 * column + 1] = true;
        }
    }
    // Open the wall between 2 neighbouring cells.
    let mut join = |a: usize, b: usize| {
        let (ax, ay) = (a % columns, a / columns);
        let (bx, by) = (b % columns, b / columns);
        open[(ay + by + 1) * width + ax + bx + 1] = true;
    };
    let neighbours = |cell: usize| {
        let (x, y) = (cell % columns, cell / columns);
        [
            (y > 0).then(|| cell - columns),
            (y + 1 < rows).then(|| cell + columns),
            (x > 0).then(|| cell - 1),
            (x + 1 < columns).then(|| cell + 1),
        ]
        .into_iter()
        .flatten()
    };
    let mut choices = Choices::new(seed);
    let mut visited = vec![false; cells];
    match options.algorithm {
        Algorithm::DepthFirst => {
            visited[0] = true;
            let mut stack = vec![0];
            while let Some(&current) = stack.last() {
                let next: Vec<usize> = neighbours(current).filter(|n| !visited[*n]).collect();
                if next.is_empty() {
                    stack.pop();
                    continue;
                }
                let chosen = next[choices.below(next.len())];
                visited[chosen] = true;
                join(current, chosen);
                stack.push(chosen);
            }
        }
        Algorithm::Prim => {
            visited[0] = true;
            let mut frontier: Vec<(usize, usize)> = neighbours(0).map(|n| (0, n)).collect();
            while !frontier.is_empty() {
                let (from, to) = frontier.swap_remove(choices.below(frontier.len()));
                if visited[to] {
                    continue;
                }
                visited[to] = true;
                join(from, to);
                frontier.extend(neighbours(to).filter(|n| !visited[*n]).map(|n| (to, n)));
            }
        }
        Algorithm::Wilson => {
            visited[choices.below(cells)] = true;
            let mut next = vec![0; cells];
            for start in 0..cells {
                // Walk until the maze is reached; overwriting each cell's step erases loops.
                let mut current = start;
                while !visited[current] {
                    let steps: Vec<usize> = neighbours(current).collect();
                    next[current] = steps[choices.below(steps.len())];
                    current = next[current];
                }
                current = start;
                while !visited[current] {
                    visited[current] = true;
                    join(current, next[current]);
                    current = next[current];
                }
            }
        }
        Algorithm::Sidewinder => {
            for row in 0..rows {
                let mut run = row * columns;
                for cell in row * columns..(row + 1) * columns {
                    let last = cell + 1 == (row + 1) * columns;
                    if row == 0 {
                        if !last {
                            join(cell, cell + 1);
                        }
                    } else if last || choices.below(2) == 0 {
                        let up = run + choices.below(cell - run + 1);
                        join(up, up - columns);
                        run = cell + 1;
                    } else {
                        join(cell, cell + 1);
                    }
                }
            }
        }
    }
    let entrance = 1;
    let exit = (inner_height - 1) * width + inner_width - 2;
    open[entrance] = true;
    open[exit] = true;
    let mut path = vec![false; width * height];
    if options.solution {
        // Breadth-first search from the entrance; a perfect maze has one path to the exit.
        let mut from = vec![usize::MAX; width * height];
        from[entrance] = entrance;
        let mut queue = VecDeque::from([entrance]);
        while let Some(at) = queue.pop_front() {
            let (x, y) = (at % width, at / width);
            let steps = [
                (y > 0).then(|| at - width),
                (y + 1 < inner_height).then(|| at + width),
                (x > 0).then(|| at - 1),
                (x + 1 < inner_width).then(|| at + 1),
            ];
            for step in steps.into_iter().flatten() {
                if open[step] && from[step] == usize::MAX {
                    from[step] = at;
                    queue.push_back(step);
                }
            }
        }
        let mut at = exit;
        while at != entrance {
            path[at] = true;
            at = from[at];
        }
        path[entrance] = true;
    }
    let cells: Vec<u8> = (0..width * height)
        .map(|index| {
            let (x, y) = (index % width, index / width);
            if x >= inner_width || y >= inner_height || open[index] {
                return if path[index] { b'.' } else { b' ' };
            }
            match options.style {
                MazeStyle::Hash => b'#',
                MazeStyle::Lines if y % 2 == 0 && x % 2 == 0 => b'+',
                MazeStyle::Lines if y % 2 == 0 => b'-',
                MazeStyle::Lines => b'|',
            }
        })
        .collect();
    Frame::new(width, height, cells)
}

/// How the moons in a sky are lit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum Phase {
    #[default]
    Full,
    /// Lit on the right.
    Waxing,
    /// Lit on the left.
    Waning,
    /// Each moon full, waxing or waning by the seed.
    Random,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkyOptions {
    /// Single-character stars, each `.`, `+`, `*`, `o` or `'`.
    pub stars: usize,
    /// Stars 3 by 3 characters, with rays.
    pub big_stars: usize,
    /// Moons 6 by 4 characters.
    pub moons: usize,
    pub phase: Phase,
    /// Rocks 3 by 2 characters.
    pub asteroids: usize,
}
impl Default for SkyOptions {
    fn default() -> Self {
        Self {
            stars: 60,
            big_stars: 4,
            moons: 1,
            phase: Phase::Full,
            asteroids: 2,
        }
    }
}

const FULL: &[&str] = &[" .--. ", "/    \\", "\\    /", " '--' "];
const WAXING: &[&str] = &["  .-. ", "   ) \\", "   ) /", "  '-' "];
const WANING: &[&str] = &[" .-.  ", "/ (   ", "\\ (   ", " '-'  "];
const BIG_STAR: &[&str] = &[" | ", "-o-", " | "];
const ASTEROID: &[&str] = &["/^\\", "\\_/"];
const STARS: &[u8] = b".+*o'";

/// A night sky of `width` by `height` characters: moons, then big stars, then asteroids, then
/// small stars, each placed by the seed with at least one blank character between any 2. Asks
/// for at most 20 moons, 200 big stars, 200 asteroids and 5,000 small stars, and refuses a sky
/// too small to hold them all.
pub fn sky(width: usize, height: usize, seed: u32, options: &SkyOptions) -> Result<Frame> {
    dimensions(width, height)?;
    for (count, limit, name) in [
        (options.moons, 20, "moons"),
        (options.big_stars, 200, "big stars"),
        (options.asteroids, 200, "asteroids"),
        (options.stars, 5000, "small stars"),
    ] {
        if count > limit {
            return Err(format!("A sky has at most {limit} {name}"));
        }
    }
    let mut choices = Choices::new(seed);
    let mut cells = vec![b' '; width * height];
    let mut taken = vec![false; width * height];
    let mut place = |sprite: &[&str], name: &str, number: usize, choices: &mut Choices| {
        let (w, h) = (sprite[0].len(), sprite.len());
        let fits = |x: usize, y: usize| {
            (y.saturating_sub(1)..(y + h + 1).min(height)).all(|row| {
                (x.saturating_sub(1)..(x + w + 1).min(width)).all(|col| !taken[row * width + col])
            })
        };
        let refused =
            || format!("The sky has no room for {name} {number}; make it larger or ask for fewer");
        // Top-left corners that keep the sprite inside the sky.
        let corners =
            |size: usize, sprite: usize| (size + 1).checked_sub(sprite).filter(|n| *n > 0);
        let column_spots = corners(width, w).ok_or_else(refused)?;
        let spots = column_spots * corners(height, h).ok_or_else(refused)?;
        let at = |spot: usize| (spot % column_spots, spot / column_spots);
        // A few random tries, then every spot in turn from a random start.
        let tried = (0..64)
            .map(|_| at(choices.below(spots)))
            .find(|&(x, y)| fits(x, y));
        let start = choices.below(spots);
        let spot = tried
            .or_else(|| {
                (0..spots)
                    .map(|offset| at((start + offset) % spots))
                    .find(|&(x, y)| fits(x, y))
            })
            .ok_or_else(refused)?;
        for (row, line) in sprite.iter().enumerate() {
            for (column, glyph) in line.bytes().enumerate() {
                let index = (spot.1 + row) * width + spot.0 + column;
                taken[index] = true;
                if glyph != b' ' {
                    cells[index] = glyph;
                }
            }
        }
        Ok::<(), String>(())
    };
    for number in 1..=options.moons {
        let phase = match options.phase {
            Phase::Random => [Phase::Full, Phase::Waxing, Phase::Waning][choices.below(3)],
            phase => phase,
        };
        let sprite = match phase {
            Phase::Waxing => WAXING,
            Phase::Waning => WANING,
            _ => FULL,
        };
        place(sprite, "moon", number, &mut choices)?;
    }
    for number in 1..=options.big_stars {
        place(BIG_STAR, "big star", number, &mut choices)?;
    }
    for number in 1..=options.asteroids {
        place(ASTEROID, "asteroid", number, &mut choices)?;
    }
    for number in 1..=options.stars {
        let glyph = [STARS[choices.below(STARS.len())]];
        let star = std::str::from_utf8(&glyph).expect("ASCII");
        place(&[star], "small star", number, &mut choices)?;
    }
    Frame::new(width, height, cells)
}

/// The patterns [`pattern`] draws.
pub const PATTERNS: &[&str] = &[
    "checker",
    "stripes",
    "diagonal",
    "chevron",
    "waves",
    "dots",
    "bricks",
    "honeycomb",
    "zebra",
    "text",
];

/// A repeating pattern filling `width` by `height` characters. `text` fills the area with
/// `text` and a space, repeated row after row; it is printable ASCII of 1 to 512 characters.
/// The other patterns ignore it.
pub fn pattern(width: usize, height: usize, name: &str, text: &str) -> Result<Frame> {
    dimensions(width, height)?;
    let tile: &[&str] = match name {
        "checker" => &["####    ", "####    ", "    ####", "    ####"],
        "stripes" => &["====", "    "],
        "diagonal" => &["   /", "  / ", " /  ", "/   "],
        "chevron" => &["\\  /", " \\/ "],
        "waves" => &["_.-'`'-.", "        "],
        "dots" => &["o   ", "    ", "  o ", "    "],
        "bricks" => &["|___", "__|_"],
        "honeycomb" => &["\\__/  ", "/  \\__"],
        "zebra" | "text" => &[],
        _ => {
            return Err(format!(
                "Unknown pattern {name}; choose {}",
                PATTERNS.join(", ")
            ));
        }
    };
    let fill = format!("{text} ");
    if name == "text" {
        printable(text.as_bytes())?;
        if text.is_empty() || text.len() > 512 {
            return Err("Pattern text must be 1 to 512 characters".into());
        }
    }
    let cells = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| match name {
            // Wavy vertical stripes 3 characters wide.
            "zebra" => {
                let stripe = ((x as f64 + 2.5 * (y as f64 * 0.4).sin()) / 3.0).floor();
                if stripe.rem_euclid(2.0) == 0.0 {
                    b'#'
                } else {
                    b' '
                }
            }
            "text" => fill.as_bytes()[(y * width + x) % fill.len()],
            _ => tile[y % tile.len()].as_bytes()[x % tile[0].len()],
        })
        .collect();
    Frame::new(width, height, cells)
}
