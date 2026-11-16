// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mazes, night skies and patterns: sizes, determinism and each generator's own promises.
use apollinaire_ascii::Frame;
use apollinaire_ascii::generator::{
    Algorithm, MazeOptions, MazeStyle, PATTERNS, Phase, SkyOptions, maze, pattern, sky,
};
use std::collections::VecDeque;

const ALGORITHMS: [Algorithm; 4] = [
    Algorithm::DepthFirst,
    Algorithm::Prim,
    Algorithm::Wilson,
    Algorithm::Sidewinder,
];

fn grid(frame: &Frame) -> Vec<&[u8]> {
    frame.cells().chunks(frame.width()).collect()
}

/// Checks a maze is perfect: every cell reachable from the entrance, and exactly one fewer
/// opening between cells than there are cells, so there are no loops.
fn assert_perfect(frame: &Frame, style: MazeStyle) {
    let rows = grid(frame);
    let (width, height) = (frame.width(), frame.height());
    let (inner_width, inner_height) = (width - (width + 1) % 2, height - (height + 1) % 2);
    let wall = |byte: u8| match style {
        MazeStyle::Hash => byte == b'#',
        MazeStyle::Lines => b"+-|".contains(&byte),
    };
    for (y, row) in rows.iter().enumerate() {
        for (x, &byte) in row.iter().enumerate() {
            let inside = x < inner_width && y < inner_height;
            assert!(
                wall(byte) || byte == b' ' || byte == b'.',
                "unexpected {byte} at {x},{y}"
            );
            assert!(inside || byte == b' ', "outside the maze at {x},{y}");
            if x % 2 == 1 && y % 2 == 1 && inside {
                assert!(!wall(byte), "a cell is walled at {x},{y}");
            }
            if x % 2 == 0 && y % 2 == 0 && inside {
                assert!(wall(byte), "a corner is open at {x},{y}");
            }
        }
    }
    assert!(!wall(rows[0][1]), "no entrance");
    assert!(!wall(rows[inner_height - 1][inner_width - 2]), "no exit");
    let cells = (inner_width / 2) * (inner_height / 2);
    let openings = (1..inner_height - 1)
        .flat_map(|y| (1..inner_width - 1).map(move |x| (x, y)))
        .filter(|&(x, y)| (x + y) % 2 == 1 && !wall(rows[y][x]))
        .count();
    assert_eq!(openings, cells - 1, "a perfect maze is a tree");
    let mut seen = vec![false; width * height];
    let mut queue = VecDeque::from([(1, 0)]);
    seen[1] = true;
    let mut reached = 0;
    while let Some((x, y)) = queue.pop_front() {
        if x % 2 == 1 && y % 2 == 1 {
            reached += 1;
        }
        for (nx, ny) in [
            (x + 1, y),
            (x.wrapping_sub(1), y),
            (x, y + 1),
            (x, y.wrapping_sub(1)),
        ] {
            if nx < inner_width
                && ny < inner_height
                && !wall(rows[ny][nx])
                && !seen[ny * width + nx]
            {
                seen[ny * width + nx] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    assert_eq!(reached, cells, "every cell is reachable");
}

#[test]
fn every_algorithm_and_style_gives_a_perfect_maze_at_every_size() {
    for algorithm in ALGORITHMS {
        for style in [MazeStyle::Hash, MazeStyle::Lines] {
            for (width, height) in [(3, 3), (4, 4), (9, 5), (72, 24), (21, 40)] {
                for seed in [0, 1, 42, u32::MAX] {
                    let options = MazeOptions {
                        algorithm,
                        style,
                        solution: false,
                    };
                    let frame = maze(width, height, seed, &options).unwrap();
                    assert_eq!((frame.width(), frame.height()), (width, height));
                    assert_perfect(&frame, style);
                    assert_eq!(frame, maze(width, height, seed, &options).unwrap());
                }
            }
        }
        let options = MazeOptions {
            algorithm,
            ..Default::default()
        };
        assert_perfect(&maze(512, 256, 9, &options).unwrap(), MazeStyle::Hash);
        assert_ne!(
            maze(41, 21, 1, &options).unwrap(),
            maze(41, 21, 2, &options).unwrap(),
            "{algorithm:?} ignores the seed"
        );
    }
    let first = maze(41, 21, 5, &MazeOptions::default()).unwrap();
    for algorithm in &ALGORITHMS[1..] {
        let options = MazeOptions {
            algorithm: *algorithm,
            ..Default::default()
        };
        assert_ne!(first, maze(41, 21, 5, &options).unwrap());
    }
}

#[test]
fn a_sidewinder_maze_has_an_open_top_row() {
    let options = MazeOptions {
        algorithm: Algorithm::Sidewinder,
        ..Default::default()
    };
    let frame = maze(21, 9, 3, &options).unwrap();
    assert_eq!(&grid(&frame)[1][1..20], " ".repeat(19).as_bytes());
}

#[test]
fn the_solution_is_the_one_path_from_the_entrance_to_the_exit() {
    for algorithm in ALGORITHMS {
        let options = MazeOptions {
            algorithm,
            style: MazeStyle::Lines,
            solution: true,
        };
        let frame = maze(31, 15, 11, &options).unwrap();
        let rows = grid(&frame);
        assert_eq!(rows[0][1], b'.');
        assert_eq!(rows[14][29], b'.');
        // Walking the dots from the entrance never branches and ends at the exit.
        let (mut at, mut previous) = ((1, 0), None);
        let mut steps = 1;
        while at != (29, 14) {
            let (x, y): (usize, usize) = at;
            let next: Vec<_> = [
                (x + 1, y),
                (x.wrapping_sub(1), y),
                (x, y + 1),
                (x, y.wrapping_sub(1)),
            ]
            .into_iter()
            .filter(|&(nx, ny)| nx < 31 && ny < 15 && rows[ny][nx] == b'.')
            .filter(|&step| Some(step) != previous)
            .collect();
            assert_eq!(next.len(), 1, "the path branches or stops at {at:?}");
            (previous, at) = (Some(at), next[0]);
            steps += 1;
        }
        let dots = frame.cells().iter().filter(|&&b| b == b'.').count();
        assert_eq!(dots, steps, "{algorithm:?}: a dot off the path");
        let plain = MazeOptions {
            solution: false,
            ..options
        };
        let unmarked = maze(31, 15, 11, &plain).unwrap();
        assert_eq!(frame.text().replace('.', " "), unmarked.text());
    }
}

#[test]
fn a_maze_needs_3_by_3_and_fits_the_limits() {
    let options = MazeOptions::default();
    assert_eq!(maze(3, 3, 0, &options).unwrap().text(), "# #\n# #\n# #");
    for (width, height) in [(2, 9), (9, 2)] {
        assert!(
            maze(width, height, 0, &options)
                .unwrap_err()
                .contains("at least 3")
        );
    }
    assert!(
        maze(513, 9, 0, &options)
            .unwrap_err()
            .contains("Dimensions")
    );
}

fn count(frame: &Frame, glyphs: &[u8]) -> usize {
    frame.cells().iter().filter(|b| glyphs.contains(b)).count()
}

#[test]
fn a_sky_holds_what_was_asked_with_nothing_touching() {
    let only = |stars, big_stars, moons, asteroids| SkyOptions {
        stars,
        big_stars,
        moons,
        phase: Phase::Full,
        asteroids,
    };
    let stars = sky(40, 12, 3, &only(25, 0, 0, 0)).unwrap();
    assert_eq!(count(&stars, b".+*o'"), 25);
    assert_eq!(count(&stars, b" .+*o'"), 40 * 12);
    let rows = grid(&stars);
    for y in 0..12 {
        for x in 0..40 {
            if rows[y][x] == b' ' {
                continue;
            }
            for (dx, dy) in [(1, 0), (0, 1), (1, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 40 && ny < 12 {
                    assert_eq!(rows[ny][nx], b' ', "stars touch at {x},{y}");
                }
                if dx == 1 && dy == 1 && x > 0 && ny < 12 {
                    assert_eq!(rows[ny][x - 1], b' ', "stars touch at {x},{y}");
                }
            }
        }
    }
    for (phase, sprite) in [
        (Phase::Full, "/    \\"),
        (Phase::Waxing, "   ) \\"),
        (Phase::Waning, "/ (   "),
    ] {
        let options = SkyOptions {
            phase,
            ..only(0, 0, 1, 0)
        };
        let moon = sky(20, 8, 1, &options).unwrap();
        assert!(moon.text().contains(sprite), "{phase:?}\n{}", moon.text());
        assert_eq!(
            count(&moon, b"()/\\.-'"),
            count(&sky(6, 4, 9, &options).unwrap(), b"()/\\.-'")
        );
    }
    let big = sky(12, 5, 4, &only(0, 2, 0, 0)).unwrap();
    assert_eq!(count(&big, b"o"), 2);
    assert_eq!(count(&big, b"|-"), 8);
    let rocks = sky(12, 5, 4, &only(0, 0, 0, 3)).unwrap();
    assert_eq!(count(&rocks, b"^"), 3);
    let mixed = SkyOptions {
        phase: Phase::Random,
        ..only(30, 3, 4, 3)
    };
    let night = sky(72, 24, 8, &mixed).unwrap();
    assert_eq!(night, sky(72, 24, 8, &mixed).unwrap());
    assert_ne!(night, sky(72, 24, 9, &mixed).unwrap());
    assert_eq!(count(&night, b"^"), 3);
    assert_eq!(
        sky(4, 2, 0, &only(0, 0, 0, 0)).unwrap().text(),
        "    \n    "
    );
    let default = sky(72, 24, 42, &SkyOptions::default()).unwrap();
    assert_eq!(count(&default, b"^"), 2);
}

#[test]
fn a_sky_refuses_what_it_cannot_hold() {
    let options = |moons, big_stars, stars| SkyOptions {
        stars,
        big_stars,
        moons,
        phase: Phase::Full,
        asteroids: 0,
    };
    let crowded = sky(3, 3, 0, &options(0, 2, 0)).unwrap_err();
    assert!(crowded.contains("no room for big star 2"), "{crowded}");
    let wide = sky(5, 9, 0, &options(1, 0, 0)).unwrap_err();
    assert!(wide.contains("no room for moon 1"), "{wide}");
    let full = sky(3, 1, 0, &options(0, 0, 3)).unwrap_err();
    assert!(full.contains("no room for small star"), "{full}");
    assert!(
        sky(9, 9, 0, &options(21, 0, 0))
            .unwrap_err()
            .contains("at most 20 moons")
    );
    assert!(
        sky(9, 9, 0, &options(0, 201, 0))
            .unwrap_err()
            .contains("at most 200 big stars")
    );
    assert!(
        sky(9, 9, 0, &options(0, 0, 5001))
            .unwrap_err()
            .contains("at most 5000 small stars")
    );
    let rocks = SkyOptions {
        asteroids: 201,
        ..options(0, 0, 0)
    };
    assert!(
        sky(9, 9, 0, &rocks)
            .unwrap_err()
            .contains("at most 200 asteroids")
    );
    assert!(
        sky(0, 9, 0, &options(0, 0, 0))
            .unwrap_err()
            .contains("Dimensions")
    );
}

#[test]
fn patterns_repeat_their_tiles_to_the_size() {
    let text = |name: &str, width, height| pattern(width, height, name, "AB").unwrap().text();
    assert_eq!(
        text("checker", 10, 4),
        "####    ##\n####    ##\n    ####  \n    ####  "
    );
    assert_eq!(text("stripes", 4, 3), "====\n    \n====");
    assert_eq!(text("diagonal", 5, 4), "   / \n  /  \n /   \n/   /");
    assert_eq!(text("chevron", 8, 2), "\\  /\\  /\n \\/  \\/ ");
    assert_eq!(text("waves", 10, 1), "_.-'`'-._.");
    assert_eq!(text("dots", 6, 3), "o   o \n      \n  o   ");
    assert_eq!(text("bricks", 9, 2), "|___|___|\n__|___|__");
    assert_eq!(text("honeycomb", 12, 2), "\\__/  \\__/  \n/  \\__/  \\__");
    assert_eq!(text("text", 5, 2), "AB AB\n AB A");
    let zebra = text("zebra", 30, 10);
    assert!(zebra.bytes().all(|b| b"# \n".contains(&b)));
    let rows: Vec<&str> = zebra.lines().collect();
    assert_ne!(rows[0], rows[4], "zebra stripes waver");
    for name in PATTERNS {
        let frame = pattern(512, 256, name, "AB").unwrap();
        assert_eq!((frame.width(), frame.height()), (512, 256));
        assert_eq!(frame, pattern(512, 256, name, "AB").unwrap());
    }
}

#[test]
fn patterns_refuse_unknown_names_and_bad_text() {
    let unknown = pattern(4, 4, "plaid", "").unwrap_err();
    assert!(
        unknown.contains("Unknown pattern plaid; choose checker"),
        "{unknown}"
    );
    assert!(pattern(4, 4, "text", "").unwrap_err().contains("1 to 512"));
    assert!(
        pattern(4, 4, "text", &"x".repeat(513))
            .unwrap_err()
            .contains("1 to 512")
    );
    assert!(
        pattern(4, 4, "text", "caf\u{e9}")
            .unwrap_err()
            .contains("printable ASCII")
    );
    assert_eq!(pattern(4, 1, "dots", "\u{e9}").unwrap().text(), "o   ");
    assert!(
        pattern(4, 257, "dots", "")
            .unwrap_err()
            .contains("Dimensions")
    );
}
