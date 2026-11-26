// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use super::{git_graph, mermaid_with_max_width};

#[test]
fn git_graph_refuses_text_too_tall_to_draw_before_wrapping_it() {
    let long = format!("gitGraph\ncommit id: \"{}\"", "x".repeat(1 << 20));
    assert_eq!(
        mermaid_with_max_width(&long, 7).unwrap_err(),
        "The Git graph needs more than 256 rows; a drawing has at most 256"
    );
    let name = format!("gitGraph\nbranch \"{}\"", "x".repeat(1 << 20));
    assert_eq!(
        mermaid_with_max_width(&name, 10).unwrap_err(),
        "The Git graph needs more than 256 rows; a drawing has at most 256"
    );
    assert_eq!(
        git_graph("gitGraph\nbranch \"\"").unwrap_err(),
        "Line 2: a branch name is expected"
    );
}

#[test]
fn git_graph_bounds_inputs_counts_and_total_height() {
    assert_eq!(
        git_graph(&"x".repeat((16 << 20) + 1)).unwrap_err(),
        "The Git graph exceeds the 16 MiB input limit"
    );
    assert_eq!(
        git_graph("%% comment").unwrap_err(),
        "A Git graph starts with gitGraph"
    );
    assert_eq!(
        git_graph(&format!("gitGraph\n{}", "commit\n".repeat(257))).unwrap_err(),
        "Line 258: a Git graph has at most 256 commits"
    );
    let branches: String = (1..=170).map(|n| format!("branch b{n}\n")).collect();
    assert_eq!(
        git_graph(&format!("gitGraph\n{branches}")).unwrap_err(),
        "Line 171: a Git graph has at most 170 branches"
    );
    assert_eq!(
        git_graph(&format!("gitGraph\n{}", "commit\n".repeat(256))).unwrap_err(),
        "The Git graph needs 258 rows; a drawing has at most 256"
    );
    let mut source = String::from("gitGraph\n");
    for i in 0..10 {
        source.push_str(&format!("commit id: \"{i}{}\"\n", "x".repeat(200)));
    }
    assert_eq!(
        mermaid_with_max_width(&source, 7).unwrap_err(),
        "The Git graph needs more than 256 rows; a drawing has at most 256"
    );
}
