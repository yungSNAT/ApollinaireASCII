// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Release packaging and the conversion speed benchmark.
//! Run from the repository with `cargo run --locked --manifest-path xtask/Cargo.toml -- <command>`.
use std::{path::Path, process::ExitCode};

mod deb;
mod manifests;
mod speed;

const USAGE: &str = "Usage:
  xtask deb <release folder> <version> <amd64|arm64> <output .deb>
  xtask manifests <tag> <SHA256SUMS.txt> <output folder>
  xtask speed <apollinaire> <ascii-image-converter> <chafa> [work folder]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["deb", src, version, arch, out] => {
            deb::build(Path::new(src), version, arch, Path::new(out))
        }
        ["manifests", tag, sums, out] => manifests::write(tag, Path::new(sums), Path::new(out)),
        ["speed", apo, aic, chafa] => speed::run(apo, aic, chafa, None),
        ["speed", apo, aic, chafa, work] => speed::run(apo, aic, chafa, Some(Path::new(work))),
        _ => Err(USAGE.into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
