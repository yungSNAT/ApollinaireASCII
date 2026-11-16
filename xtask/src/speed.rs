// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Time picture-to-ASCII conversion at 200 columns, best of 5 runs, against
//! ascii-image-converter and chafa, and check each converter's output is plain ASCII text.
//! docs/VALIDATION.md records a run. ImageMagick 7 (`magick`) draws the 3 test pictures.
//! Without a work folder it uses a temporary one and removes it afterwards.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

const PICTURES: [&str; 3] = ["big.jpg", "hd.png", "big.png"];

fn absolute(program: &str) -> Result<PathBuf, String> {
    fs::canonicalize(program).map_err(|error| format!("{program}: {error}"))
}

/// Run a command in `work` and return its output, failing on a non-zero exit.
fn call(work: &Path, program: &Path, args: &[&str], stdout: Stdio) -> Result<Vec<u8>, String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(work)
        .stdout(stdout)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("Cannot run {}: {error}", program.display()))?;
    if !out.status.success() {
        return Err(format!("{} failed: {}", program.display(), out.status));
    }
    Ok(out.stdout)
}

/// Print a command's first line of output.
fn first_line(work: &Path, program: &Path, args: &[&str]) -> Result<(), String> {
    let out = call(work, program, args, Stdio::piped())?;
    println!(
        "{}",
        String::from_utf8_lossy(&out).lines().next().unwrap_or("")
    );
    Ok(())
}

/// The bytes outside line feeds and printable ASCII.
fn outside_ascii(out: &[u8]) -> usize {
    out.iter()
        .filter(|&&b| b != b'\n' && !(b' '..=b'~').contains(&b))
        .count()
}

fn run_in(work: &Path, apo: &Path, aic: &Path, chafa: &Path) -> Result<(), String> {
    let magick = Path::new("magick");
    for program in [apo, aic] {
        call(work, program, &["--version"], Stdio::inherit())?;
    }
    first_line(work, chafa, &["--version"])?;
    first_line(work, magick, &["-version"])?;
    for args in [
        [
            "-size",
            "4000x3000",
            "plasma:fractal",
            "-seed",
            "7",
            "-quality",
            "90",
            "big.jpg",
        ]
        .as_slice(),
        &[
            "-size",
            "1920x1080",
            "plasma:fractal",
            "-seed",
            "7",
            "hd.png",
        ],
        &[
            "-size",
            "4000x3000",
            "plasma:fractal",
            "-seed",
            "7",
            "big.png",
        ],
    ] {
        call(work, magick, args, Stdio::inherit())?;
    }
    for picture in PICTURES {
        let path = work.join(picture);
        let size = fs::metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        println!("{picture}: {} bytes", size.len());
    }
    for picture in PICTURES {
        for (name, program, args) in [
            ("apollinaire", apo, vec!["image", picture, "--width", "200"]),
            (
                "apollinaire-dither",
                apo,
                vec!["image", picture, "--width", "200", "--dither"],
            ),
            ("ascii-image-converter", aic, vec![picture, "-W", "200"]),
            (
                "chafa",
                chafa,
                vec![
                    "--format",
                    "symbols",
                    "--symbols",
                    "ascii",
                    "--colors",
                    "none",
                    "--size",
                    "200x",
                    "--animate",
                    "off",
                    picture,
                ],
            ),
        ] {
            let name = format!("{name} {picture}");
            // One run: the bytes outside line feeds and printable ASCII, or the refusal.
            let out = Command::new(program)
                .args(&args)
                .current_dir(work)
                .output()
                .map_err(|error| format!("Cannot run {}: {error}", program.display()))?;
            if !out.status.success() {
                let error = String::from_utf8_lossy(&out.stderr);
                println!("{name}: refused: {}", error.lines().next().unwrap_or(""));
                continue;
            }
            println!(
                "{name}: {} bytes outside printable ASCII",
                outside_ascii(&out.stdout)
            );
            // Milliseconds for the fastest of 5 runs, including process start.
            let mut fastest = u128::MAX;
            for _ in 0..5 {
                let start = Instant::now();
                call(work, program, &args, Stdio::null())?;
                fastest = fastest.min(start.elapsed().as_millis());
            }
            println!("{name}: {fastest} ms");
        }
    }
    Ok(())
}

pub fn run(apo: &str, aic: &str, chafa: &str, work: Option<&Path>) -> Result<(), String> {
    let (apo, aic, chafa) = (absolute(apo)?, absolute(aic)?, absolute(chafa)?);
    match work {
        Some(work) => run_in(work, &apo, &aic, &chafa),
        None => {
            let work =
                std::env::temp_dir().join(format!("conversion-speed-{}", std::process::id()));
            fs::create_dir(&work).map_err(|error| format!("{}: {error}", work.display()))?;
            let result = run_in(&work, &apo, &aic, &chafa);
            let _ = fs::remove_dir_all(&work);
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::outside_ascii;

    #[test]
    fn counts_bytes_outside_line_feeds_and_printable_ascii() {
        assert_eq!(outside_ascii(b" ~az\n"), 0);
        assert_eq!(outside_ascii(b"\r\t\x1b\x7f\x1f\xc3\xa9"), 7);
    }
}
