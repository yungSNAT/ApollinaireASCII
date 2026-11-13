// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

/// The README's `[dependencies]` line must select this release series, or copying it installs
/// an older crate.
#[test]
fn readme_dependency_line_matches_the_crate_version() {
    let readme =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).unwrap();
    let requested = readme
        .lines()
        .find_map(|line| {
            line.strip_prefix("apollinaire-ascii = { version = \"")?
                .split_once('"')
                .map(|(version, _)| version)
        })
        .expect("README.md has no apollinaire-ascii dependency line");
    let series: Vec<_> = env!("CARGO_PKG_VERSION").split('.').take(2).collect();
    assert_eq!(requested, series.join("."));
}
