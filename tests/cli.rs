// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

#![cfg(feature = "cli")]
mod common;
use std::io::Write;
use std::process::Command;
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_apollinaire"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn native_commands_emit_real_text_and_validate_input() {
    let art = cli(&["art", "ghost"]);
    assert!(art.status.success());
    assert!(
        art.stdout
            .iter()
            .all(|b| *b == b'\n' || (32..=126).contains(b))
    );
    assert!(!art.stdout.contains(&27));
    // `--border` alone, before or after the name, keeps the plain style.
    let plain = cli(&["art", "ghost", "--border=plain"]);
    assert!(plain.stdout.starts_with(b"+-"), "{}", text(&plain.stderr));
    for args in [["art", "ghost", "--border"], ["art", "--border", "ghost"]] {
        assert_eq!(cli(&args).stdout, plain.stdout, "{args:?}");
    }
    let rounded = cli(&["art", "ghost", "--border=rounded"]);
    assert!(
        rounded.stdout.starts_with(b".-"),
        "{}",
        text(&rounded.stderr)
    );
    assert!(!cli(&["art", "ghost", "--border=wavy"]).status.success());
    assert!(text(&cli(&["art", "--help"]).stdout).contains("--border[=<STYLE>]"));
    let text = cli(&["text", "Hello", "--font", "small"]);
    assert!(text.status.success());
    let unicode = cli(&["text", "👻"]);
    assert!(!unicode.status.success());
    assert!(unicode.stdout.is_empty());
    let animation = cli(&[
        "animate", "wave", "--frames", "3", "--width", "20", "--height", "8",
    ]);
    assert!(animation.status.success());
    let project =
        apollinaire_ascii::Animation::from_json(std::str::from_utf8(&animation.stdout).unwrap())
            .unwrap();
    assert_eq!(project.frames().len(), 3);
    let ambiguous = cli(&["animate", "ghost", "--format", "text"]);
    assert!(!ambiguous.status.success());
    let selected = cli(&["animate", "ghost", "--format", "text", "--frame", "1"]);
    assert!(selected.status.success());
}
#[test]
fn output_files_are_never_overwritten_and_playback_refuses_redirection() {
    let path = std::env::temp_dir().join(format!("apollinaire-cli-{}.txt", std::process::id()));
    std::fs::write(&path, "keep this").unwrap();
    let output = cli(&["art", "cat", "--output", path.to_str().unwrap()]);
    assert!(!output.status.success());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep this");
    let play = cli(&["play", path.to_str().unwrap()]);
    assert!(!play.status.success());
    assert!(play.stdout.is_empty());
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn stdin_ascii_can_start_with_an_opening_brace() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_apollinaire"))
        .args(["import", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"{ a }").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"{ a }\n");
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("apollinaire-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

#[test]
fn check_and_transform_report_dimensions_and_preserve_frame_content() {
    let dir = scratch("inspect");
    let input = dir.join("drawing.txt");
    std::fs::write(&input, "ab\ncd\n").unwrap();
    let path = input.to_str().unwrap();
    let check = cli(&["check", path]);
    assert!(check.status.success(), "{}", text(&check.stderr));
    assert!(text(&check.stdout).starts_with("Valid ASCII: 2 x 2, 1 frame(s), "));

    for (operation, expected) in [
        ("horizontal", "ba\ndc\n"),
        ("vertical", "cd\nab\n"),
        ("border", "+--+\n|ab|\n|cd|\n+--+\n"),
    ] {
        let result = cli(&["transform", path, operation]);
        assert!(result.status.success(), "{}", text(&result.stderr));
        assert_eq!(text(&result.stdout), expected);
    }
    for (style, expected) in [
        ("plain", "+--+\n|ab|\n|cd|\n+--+\n"),
        ("rounded", ".--.\n|ab|\n|cd|\n'--'\n"),
        ("dotted", "....\n:ab:\n:cd:\n:..:\n"),
        ("heavy", "#==#\n#ab#\n#cd#\n#==#\n"),
    ] {
        let result = cli(&["transform", path, "border", "--style", style]);
        assert!(result.status.success(), "{}", text(&result.stderr));
        assert_eq!(text(&result.stdout), expected);
    }
    let flip = cli(&["transform", path, "horizontal", "--style", "heavy"]);
    assert!(!flip.status.success());
    assert!(text(&flip.stderr).contains("Only the border operation takes --style"));
    let outside = cli(&[
        "transform",
        path,
        "crop",
        "--x",
        "2",
        "--width",
        "1",
        "--height",
        "1",
    ]);
    assert!(!outside.status.success());
    assert!(text(&outside.stderr).contains("Crop"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_and_export_report_incompatible_options_and_bad_inputs() {
    let dir = scratch("bad-io");
    let input = dir.join("invalid.txt");
    let path = input.to_str().unwrap();
    std::fs::write(&input, [0xff]).unwrap();
    let bad_utf8 = cli(&["import", path]);
    assert!(!bad_utf8.status.success());
    assert!(text(&bad_utf8.stderr).contains("Text input must be UTF-8"));

    let file = std::fs::File::create(&input).unwrap();
    file.set_len(apollinaire_ascii::MAX_INPUT_BYTES as u64 + 1)
        .unwrap();
    drop(file);
    let too_large = cli(&["import", path]);
    assert!(!too_large.status.success());
    assert!(text(&too_large.stderr).contains("Input exceeds 16 MiB"));

    let options = cli(&["completions", "bash", "--format", "json"]);
    assert!(!options.status.success());
    assert!(text(&options.stderr).contains("Completions does not accept export options"));
    let frame = cli(&["art", "cat", "--format", "html", "--frame", "0"]);
    assert!(!frame.status.success());
    assert!(text(&frame.stderr).contains("--frame applies only to text output"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn animation_commands_report_invalid_scene_and_effect_options() {
    let scene = cli(&["animate", "rain", "--fps", "0"]);
    assert!(!scene.status.success());
    assert!(text(&scene.stderr).contains("FPS must be 1..=60"));
    let effect = cli(&["effect", "scatter", "--art", "cat", "--duration", "0"]);
    assert!(!effect.status.success());
    assert!(text(&effect.stderr).contains("duration"));
}

#[test]
fn compose_clips_a_transparent_stamp_and_rejects_animations() {
    let dir = scratch("compose");
    let background = dir.join("background.txt");
    let stamp = dir.join("stamp.txt");
    std::fs::write(&background, "abc\ndef").unwrap();
    std::fs::write(&stamp, " XZ").unwrap();
    let output = cli(&[
        "compose",
        background.to_str().unwrap(),
        stamp.to_str().unwrap(),
        "--x",
        "1",
        "--y",
        "1",
        "--transparent-spaces",
    ]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "abc\ndeX\n");

    let animation = cli(&["animate", "wave", "--frames", "2"]);
    assert!(animation.status.success(), "{}", text(&animation.stderr));
    let project = dir.join("animation.json");
    std::fs::write(&project, animation.stdout).unwrap();
    let refused = cli(&[
        "compose",
        background.to_str().unwrap(),
        project.to_str().unwrap(),
    ]);
    assert!(!refused.status.success());
    assert!(text(&refused.stderr).contains("Compose needs 2 static frames"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn transform_crops_every_frame_and_preserves_timing_and_looping() {
    use apollinaire_ascii::{Animation, Frame, TimedFrame};

    let dir = scratch("transform");
    let original = Animation::new(
        vec![
            TimedFrame {
                frame: Frame::from_text("abcd\nefgh\nijkl").unwrap(),
                duration_ms: 90,
            },
            TimedFrame {
                frame: Frame::from_text("mnop\nqrst\nuvwx").unwrap(),
                duration_ms: 150,
            },
        ],
        false,
    )
    .unwrap();
    let path = dir.join("animation.json");
    std::fs::write(&path, original.to_json()).unwrap();

    let cropped = cli(&[
        "transform",
        path.to_str().unwrap(),
        "crop",
        "--x",
        "1",
        "--y",
        "1",
        "--width",
        "2",
        "--height",
        "2",
    ]);
    assert!(cropped.status.success(), "{}", text(&cropped.stderr));
    let cropped = Animation::from_json(text(&cropped.stdout)).unwrap();
    assert!(!cropped.looping());
    let frames: Vec<_> = cropped
        .frames()
        .iter()
        .map(|f| (f.duration_ms, f.frame.text()))
        .collect();
    assert_eq!(
        frames,
        [(90, String::from("fg\njk")), (150, String::from("rs\nvw"))]
    );

    let incomplete = cli(&["transform", path.to_str().unwrap(), "crop", "--width", "2"]);
    assert!(!incomplete.status.success());
    assert!(text(&incomplete.stderr).contains("Crop needs --height"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn list_searches_tags_and_effects_animate_files_and_stdin() {
    let birds = cli(&["list", "--tag", "bird"]);
    assert!(birds.status.success());
    let names: Vec<_> = text(&birds.stdout)
        .lines()
        .map(|l| l.split(' ').next().unwrap())
        .collect();
    assert!(names.contains(&"duck") && names.contains(&"owl") && !names.contains(&"cat"));
    assert!(!cli(&["list", "--tag", "no-such-tag"]).status.success());
    assert!(text(&cli(&["list"]).stdout).contains("Effects: reveal"));

    let dir = scratch("effect");
    let art = dir.join("fox.txt");
    assert!(
        cli(&["art", "fox", "--output", art.to_str().unwrap()])
            .status
            .success()
    );
    let output = cli(&[
        "effect",
        "scatter",
        art.to_str().unwrap(),
        "--duration",
        "500",
        "--fps",
        "10",
    ]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    let project = apollinaire_ascii::Animation::from_json(text(&output.stdout)).unwrap();
    assert_eq!(project.frames().len(), 5);
    let fox = apollinaire_ascii::art::template("fox").unwrap();
    assert_eq!(project.frames()[4].frame, fox);
    let leaving = cli(&[
        "effect",
        "scatter",
        "--art",
        "fox",
        "--reverse",
        "--duration",
        "500",
        "--fps",
        "10",
    ]);
    assert!(leaving.status.success(), "{}", text(&leaving.stderr));
    let leaving = apollinaire_ascii::Animation::from_json(text(&leaving.stdout)).unwrap();
    assert!(
        leaving.frames().iter().map(|f| &f.frame).eq(project
            .frames()
            .iter()
            .rev()
            .map(|f| &f.frame))
    );
    assert!(!cli(&["effect", "scatter"]).status.success());
    assert!(
        !cli(&["effect", "scatter", art.to_str().unwrap(), "--art", "fox"])
            .status
            .success()
    );
    assert!(
        !cli(&["effect", "scatter", "--art", "unicorn"])
            .status
            .success()
    );
    assert!(
        !cli(&["effect", "sparkle", art.to_str().unwrap()])
            .status
            .success()
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_apollinaire"))
        .args(["effect", "decrypt", "-", "--format", "text", "--frame", "0"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"HELLO\nFRIEND\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output
            .stdout
            .iter()
            .all(|b| *b == b'\n' || (32..=126).contains(b))
    );
    assert_ne!(output.stdout, b"HELLO \nFRIEND\n");
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn image_edge_glyphs_overlay_the_picture_or_the_traced_edges() {
    let dir = scratch("edge-glyphs");
    let path = dir.join("step.png");
    image::RgbaImage::from_fn(4, 1, |x, _| {
        image::Rgba(if x < 2 { [0, 0, 0, 255] } else { [255; 4] })
    })
    .save(&path)
    .unwrap();
    let run = |flags: &[&str]| {
        let image = path.to_str().unwrap();
        let mut args = vec!["image", image, "--width", "4", "--height", "1"];
        args.extend(flags);
        let output = cli(&args);
        assert!(output.status.success());
        text(&output.stdout).to_string()
    };
    assert_eq!(run(&["--edges"]), " @@ \n");
    assert_eq!(run(&["--edge-glyphs"]), " ||@\n");
    assert_eq!(run(&["--edges", "--edge-glyphs"]), " || \n");
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn image_batches_create_one_file_per_input_and_never_overwrite() {
    let dir = scratch("batch");
    let picture = |name: &str| {
        let path = dir.join(name);
        image::DynamicImage::new_rgba8(16, 8).save(&path).unwrap();
        path.to_str().unwrap().to_string()
    };
    let (a, b) = (picture("a.png"), picture("b.png"));
    std::fs::create_dir(dir.join("sub")).unwrap();
    let nested = picture("sub/a.png");
    let bad = dir.join("notes.png");
    std::fs::write(&bad, "not an image").unwrap();
    let bad = bad.to_str().unwrap();
    let out = dir.join("out");
    let out_arg = out.to_str().unwrap();

    let first = cli(&["image", &a, &b, "--width", "8", "--output-dir", out_arg]);
    assert!(first.status.success(), "{}", text(&first.stderr));
    let a_text = std::fs::read(out.join("a.png.txt")).unwrap();
    assert!(a_text.iter().all(|b| *b == b'\n' || (32..=126).contains(b)));
    assert!(out.join("b.png.txt").exists());
    assert_eq!(text(&first.stdout).lines().count(), 2);

    std::fs::write(out.join("b.png.txt"), "keep this").unwrap();
    let again = cli(&["image", &a, &b, "--width", "8", "--output-dir", out_arg]);
    assert!(!again.status.success());
    assert!(text(&again.stderr).contains("2 of 2 inputs failed"));
    assert_eq!(std::fs::read(out.join("a.png.txt")).unwrap(), a_text);
    assert_eq!(
        std::fs::read_to_string(out.join("b.png.txt")).unwrap(),
        "keep this"
    );

    let partial = dir.join("partial");
    let mixed = cli(&[
        "image",
        bad,
        &a,
        "--width",
        "8",
        "--output-dir",
        partial.to_str().unwrap(),
    ]);
    assert!(!mixed.status.success());
    assert!(text(&mixed.stderr).contains("notes.png"));
    assert!(partial.join("a.png.txt").exists() && !partial.join("notes.png.txt").exists());

    let clash = dir.join("clash");
    let same = cli(&[
        "image",
        &a,
        &nested,
        "--output-dir",
        clash.to_str().unwrap(),
    ]);
    assert!(!same.status.success());
    assert!(!clash.exists());
    let upper = picture("A.PNG");
    let case = cli(&["image", &a, &upper, "--output-dir", clash.to_str().unwrap()]);
    assert!(text(&case.stderr).contains("Several inputs would be named"));
    assert!(!clash.exists());
    let gif = dir.join("a.gif");
    let frame = common::GifFrame::full(16, 8, vec![0; 128], 0);
    std::fs::write(&gif, common::gif(16, 8, Some(&[[0, 0, 0]]), &[frame])).unwrap();
    let gif = gif.to_str().unwrap().to_string();
    let kinds = dir.join("kinds");
    let both = cli(&[
        "image",
        &a,
        &gif,
        "--width",
        "8",
        "--output-dir",
        kinds.to_str().unwrap(),
    ]);
    assert!(both.status.success(), "{}", text(&both.stderr));
    assert!(kinds.join("a.png.txt").exists() && kinds.join("a.gif.txt").exists());
    assert!(!cli(&["image", &a, &b]).status.success());
    assert!(
        !cli(&["image", "-", &a, "--output-dir", out_arg])
            .status
            .success()
    );

    let plain = cli(&["image", "http://127.0.0.1:9/a.png"]);
    assert!(!plain.status.success());
    assert!(text(&plain.stderr).contains("Only https:// URLs are accepted"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn image_batches_use_safe_names_and_the_requested_export_format() {
    let dir = scratch("batch-formats");
    let input = dir.join("two words.pgm");
    std::fs::write(&input, b"P2\n2 1\n255\n0 255\n").unwrap();
    let input = input.to_str().unwrap();
    for (format, signature) in [("json", "{"), ("html", "<!doctype html>")] {
        let output = dir.join(format);
        let result = cli(&[
            "image",
            input,
            "--format",
            format,
            "--width",
            "2",
            "--output-dir",
            output.to_str().unwrap(),
        ]);
        assert!(result.status.success(), "{}", text(&result.stderr));
        let contents =
            std::fs::read_to_string(output.join(format!("two_words.pgm.{format}"))).unwrap();
        assert!(contents.starts_with(signature));
    }

    let output = dir.join("conflict");
    let out_arg = output.to_str().unwrap();
    let mixed = cli(&["image", input, "--output-dir", out_arg, "--output", input]);
    assert!(!mixed.status.success());
    assert!(text(&mixed.stderr).contains("Use either --output or --output-dir"));

    for urls in [
        [
            "http://127.0.0.1:1/photo.png?q=1",
            "http://127.0.0.1:1/photo.png#crop",
        ],
        ["http://127.0.0.1:1/", "http://127.0.0.1:1/image"],
    ] {
        let collision = cli(&["image", urls[0], urls[1], "--output-dir", out_arg]);
        assert!(!collision.status.success());
        assert!(text(&collision.stderr).contains("Several inputs would be named"));
    }
    assert!(!output.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn text_cli_applies_full_layout_and_alignment_to_the_requested_width() {
    use apollinaire_ascii::lettering::{self, Alignment, Layout, Options};

    let width = 24;
    let sample = "Ab";
    for (align, alignment) in [("center", Alignment::Center), ("right", Alignment::Right)] {
        let result = cli(&[
            "text",
            sample,
            "--align",
            align,
            "--width",
            "24",
            "--horizontal",
            "full",
        ]);
        assert!(result.status.success(), "{}", text(&result.stderr));
        let mut options = Options {
            alignment,
            width: Some(width),
            horizontal: Layout::Full,
            ..Default::default()
        };
        let expected = lettering::render_with(sample, "standard", None, &options)
            .unwrap()
            .text();
        options.horizontal = Layout::Default;
        let default = lettering::render_with(sample, "standard", None, &options).unwrap();
        assert_ne!(expected, default.text());
        assert_eq!(text(&result.stdout), format!("{expected}\n"));
    }
}

#[test]
fn text_accepts_layout_wrap_and_ascii_toilet_fonts_only() {
    let wrapped = cli(&[
        "text",
        "Present day",
        "--wrap",
        "40",
        "--horizontal",
        "fitted",
        "--vertical",
        "smush",
    ]);
    assert!(wrapped.status.success());
    let rows: Vec<&[u8]> = wrapped
        .stdout
        .trim_ascii_end()
        .split(|b| *b == b'\n')
        .collect();
    assert!(rows.len() > 6 && rows.iter().all(|r| r.len() <= 40));
    let mut font = String::from("tlf2a$ 1 1 4 -1 0\n");
    for byte in 32..=126u8 {
        let marker = if byte == b'@' { '#' } else { '@' };
        font.push_str(&format!("{}{marker}\n", char::from(byte)));
    }
    let path = std::env::temp_dir().join(format!("apollinaire-cli-{}.tlf", std::process::id()));
    std::fs::write(&path, &font).unwrap();
    let ok = cli(&["text", "Hi", "--font-file", path.to_str().unwrap()]);
    assert_eq!(ok.stdout, b"Hi\n");
    std::fs::write(&path, font.replace("H@\n", "\u{2588}@\n")).unwrap();
    let refused = cli(&["text", "Hi", "--font-file", path.to_str().unwrap()]);
    assert!(!refused.status.success() && refused.stdout.is_empty());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("glyph 'H' row 1 has byte 0xe2"));
    std::fs::remove_file(&path).unwrap();
}

/// Run a checker on a generated file. A missing checker fails on CI, which installs them all,
/// and is reported and skipped elsewhere.
#[cfg(unix)]
fn check_with(program: &str, args: &[&str], env: &[(&str, &str)]) -> Option<(bool, String)> {
    match Command::new(program)
        .args(args)
        .envs(env.iter().copied())
        .output()
    {
        Ok(output) => Some((
            output.status.success() && output.stderr.is_empty(),
            format!("{}{}", text(&output.stdout), text(&output.stderr)),
        )),
        Err(error) if std::env::var_os("CI").is_none() => {
            eprintln!("skipped {program}: {error}");
            None
        }
        Err(error) => panic!("{program} is required on CI: {error}"),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn playback_draws_each_frame_for_each_requested_loop_in_a_terminal() {
    use apollinaire_ascii::{Animation, Frame, TimedFrame};

    let Some((success, version)) = check_with("script", &["--version"], &[]) else {
        return;
    };
    if !success || !version.contains("util-linux") {
        assert!(
            std::env::var_os("CI").is_none(),
            "expected util-linux script: {version}"
        );
        eprintln!("skipped terminal playback: util-linux script is unavailable");
        return;
    }
    let project = Animation::new(
        ["first ", "second"]
            .into_iter()
            .map(|text| TimedFrame {
                frame: Frame::from_text(text).unwrap(),
                duration_ms: 10,
            })
            .collect(),
        true,
    )
    .unwrap();
    let dir = scratch("terminal");
    let path = dir.join("frames.json");
    std::fs::write(&path, project.to_json()).unwrap();
    let env = [
        ("APOLLINAIRE_TEST_BIN", env!("CARGO_BIN_EXE_apollinaire")),
        ("APOLLINAIRE_TEST_PROJECT", path.to_str().unwrap()),
        ("SHELL", "/bin/sh"),
    ];
    let run = r#"exec "$APOLLINAIRE_TEST_BIN" play "$APOLLINAIRE_TEST_PROJECT" --loops 2"#;
    if let Some((success, output)) =
        check_with("script", &["-q", "-e", "-c", run, "/dev/null"], &env)
    {
        assert!(success, "{output:?}");
        assert_eq!(
            output.replace("\r\n", "\n"),
            "\x1b[2J\x1b[H\x1b[Hfirst \x1b[Hsecond\x1b[Hfirst \x1b[Hsecond\n"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn completions_parse_in_each_shell_and_the_manual_renders() {
    // pwsh -Command appends any later argument to its command text, so the file comes by
    // environment variable.
    const PARSE_PS1: &str = "$errors = $null; \
        [System.Management.Automation.Language.Parser]::ParseFile($env:COMPLETIONS, [ref]$null, [ref]$errors) > $null; \
        if ($errors) { $errors; exit 1 }";
    let dir = scratch("completions");
    for (shell, file, program, check) in [
        ("bash", "apollinaire.bash", "bash", &["-n", "FILE"][..]),
        ("zsh", "_apollinaire", "zsh", &["-n", "FILE"][..]),
        (
            "fish",
            "apollinaire.fish",
            "fish",
            &["--no-execute", "FILE"][..],
        ),
        (
            "powershell",
            "apollinaire.ps1",
            "pwsh",
            &["-NoProfile", "-NonInteractive", "-Command", PARSE_PS1][..],
        ),
    ] {
        let script = cli(&["completions", shell]);
        assert!(script.status.success(), "{}", text(&script.stderr));
        assert!(text(&script.stdout).contains("apollinaire"));
        let path = dir.join(file);
        std::fs::write(&path, &script.stdout).unwrap();
        let path = path.to_str().unwrap();
        let args: Vec<_> = check
            .iter()
            .map(|arg| if *arg == "FILE" { path } else { *arg })
            .collect();
        if let Some((parsed, said)) = check_with(program, &args, &[("COMPLETIONS", path)]) {
            assert!(parsed, "{program} rejects the {shell} completions: {said}");
        }
    }
    let page = cli(&["man"]);
    assert!(page.status.success(), "{}", text(&page.stderr));
    let path = dir.join("apollinaire.1");
    std::fs::write(&path, &page.stdout).unwrap();
    // `man -l` is man-db's; other systems' man may not read a local file.
    let env = [("MANPAGER", "cat"), ("MANWIDTH", "80")];
    if cfg!(target_os = "linux")
        && let Some((rendered, shown)) = check_with("man", &["-l", path.to_str().unwrap()], &env)
    {
        assert!(rendered, "man cannot render the page: {shown}");
        assert!(
            shown.contains("SUBCOMMANDS") && shown.contains("completions"),
            "{shown}"
        );
    }
    let refused = cli(&["man", "--output", dir.join("copy.1").to_str().unwrap()]);
    assert!(!refused.status.success() && refused.stdout.is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}
