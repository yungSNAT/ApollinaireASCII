// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Runs every effect through the CLI with each of its options and input routes. Every run must
//! succeed, print only printable ASCII and line feeds, give the documented frame count and finish
//! (or, reversed, start) on the effect's end state for the chosen source frame.
//!
//! With `APOLLINAIRE_SHOWROOM=<new directory>`, the same runs become the showroom that the Pages
//! build publishes beside the editor: an index of mid-effect frames with a timing table, and each
//! effect's standalone player. The CLI writes all of it, so the page cannot drift from the code.
#![cfg(feature = "cli")]

use apollinaire_ascii::{Animation, Frame, art, effects::EFFECTS};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

/// One input route and set of options, applied to every effect.
struct Variant {
    label: &'static str,
    args: &'static [&'static str],
    /// The input given as a file, `FILE` in `args`, or on stdin, `-` in `args`.
    input: Input,
    duration_ms: u32,
    fps: u32,
    reverse: bool,
}
#[derive(Clone, Copy)]
enum Input {
    Catalogue,
    LetteringFile,
    ProjectOnStdin,
}
const VARIANTS: &[Variant] = &[
    Variant {
        label: "catalogue ghost, defaults (2,000 ms at 20 fps, seed 42)",
        args: &["--art", "ghost"],
        input: Input::Catalogue,
        duration_ms: 2000,
        fps: 20,
        reverse: false,
    },
    Variant {
        label: "catalogue ghost, reversed, 500 ms at 60 fps, seed 7",
        args: &[
            "--art",
            "ghost",
            "--reverse",
            "--duration",
            "500",
            "--fps",
            "60",
            "--seed",
            "7",
        ],
        input: Input::Catalogue,
        duration_ms: 500,
        fps: 60,
        reverse: true,
    },
    Variant {
        label: "block lettering from a .txt file, 100 ms at 1 fps",
        args: &["FILE", "--duration", "100", "--fps", "1"],
        input: Input::LetteringFile,
        duration_ms: 100,
        fps: 1,
        reverse: false,
    },
    Variant {
        label: "second frame of a JSON project on stdin, 60,000 ms at 1 fps",
        args: &[
            "-",
            "--source-frame",
            "1",
            "--duration",
            "60000",
            "--fps",
            "1",
        ],
        input: Input::ProjectOnStdin,
        duration_ms: 60000,
        fps: 1,
        reverse: false,
    },
];

/// Run the CLI, failing on a nonzero exit, any output byte other than 32 to 126 and LF, or
/// anything on stderr, where the CLI writes only errors.
/// Returns stdout and the run's wall-clock milliseconds, process start included.
fn apollinaire(args: &[&str], stdin: Option<&[u8]>) -> (String, f64) {
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_apollinaire"))
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = stdin {
        // A CLI that refuses its arguments may close stdin first; its exit status says why.
        let _ = child.stdin.take().unwrap().write_all(input);
    }
    let output = child.wait_with_output().unwrap();
    let milliseconds = started.elapsed().as_secs_f64() * 1000.0;
    let command = format!("apollinaire {}", args.join(" "));
    assert!(
        output.status.success(),
        "{command} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{command} wrote {} bytes to stderr: {:?}",
        output.stderr.len(),
        String::from_utf8_lossy(&output.stderr)
    );
    if let Some(byte) = output
        .stdout
        .iter()
        .find(|b| **b != b'\n' && !(32..=126).contains(*b))
    {
        panic!("{command} printed byte {byte}, outside 32 to 126");
    }
    (String::from_utf8(output.stdout).unwrap(), milliseconds)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Markup that would show anything but text, which the ASCII contract rules out: picture and
/// media elements, image inputs, framed pages, script-made elements and CSS images.
fn non_text_elements(html: &str) -> Vec<&'static str> {
    let lower = html.to_ascii_lowercase();
    let mut found: Vec<_> = [
        "<canvas",
        "<svg",
        "<img",
        "<image",
        "<picture",
        "<video",
        "<object",
        "<embed",
        "<iframe",
        "<frame",
        "createelement(",
        "url(",
    ]
    .into_iter()
    .filter(|tag| lower.contains(tag))
    .collect();
    // An image input: within each input tag, the type may be quoted either way or spaced out, as
    // in `type = 'image'`. The tag ends at the first `>` outside a quoted value, so `title="x > y"`
    // does not end it early. Text outside input tags is not read as an attribute.
    let image_input = lower.split("<input").skip(1).any(|rest| {
        let mut quote = None;
        let end = rest.find(|c| match quote {
            Some(open) => {
                if c == open {
                    quote = None;
                }
                false
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                false
            }
            None => c == '>',
        });
        let tag = &rest[..end.unwrap_or(rest.len())];
        tag.chars()
            .filter(|c| !c.is_ascii_whitespace() && *c != '"' && *c != '\'')
            .collect::<String>()
            .contains("type=image")
    });
    if image_input {
        found.push("type=image");
    }
    found
}

struct Shown {
    effect: &'static str,
    frame: String,
    timings: Vec<f64>,
}

fn index(shown: &[Shown], context: &str) -> String {
    let mut sections = String::new();
    for entry in shown {
        sections.push_str(&format!(
            "<section><h2 id=\"{0}\">{0}</h2>\n<figure><pre tabindex=\"0\">{1}</pre>\n<figcaption><code>apollinaire effect {0} --art ghost</code>, \
             halfway through. <a href=\"{0}.html\">Play {0}</a></figcaption></figure></section>\n",
            entry.effect,
            escape(&entry.frame)
        ));
    }
    let mut table = String::from("<tr><th scope=\"col\">Effect</th>");
    for variant in VARIANTS {
        table.push_str(&format!("<th scope=\"col\">{}</th>", escape(variant.label)));
    }
    table.push_str("</tr>\n");
    for entry in shown {
        table.push_str(&format!("<tr><th scope=\"row\">{}</th>", entry.effect));
        for milliseconds in &entry.timings {
            table.push_str(&format!("<td>{milliseconds:.1}</td>"));
        }
        table.push_str("</tr>\n");
    }
    format!(
        r#"<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>ApollinaireASCII effect showroom</title>
<style>
:root{{color-scheme:dark}}body{{margin:24px;background:#000000;color:#FFFFF0;font:16px system-ui}}
a{{color:#FFD32A}}a:focus-visible{{outline:3px solid #FFD32A;outline-offset:4px}}
figure{{margin:0}}pre{{font:14px/1.2 ui-monospace,Consolas,monospace;white-space:pre;overflow:auto;max-width:100%}}
pre:focus-visible{{outline:3px solid #FFD32A;outline-offset:4px}}caption{{text-align:left;padding:4px 0}}
table{{border-collapse:collapse}}th,td{{border:1px solid #FFFFF0;padding:4px 8px;text-align:right}}th[scope=row]{{text-align:left}}
</style>
<main>
<h1>Effect showroom</h1>
<p>Every effect, rendered by the <code>apollinaire</code> CLI when this page was built. Each frame is printable ASCII text. <a href="../">Open the editor</a>.</p>
{sections}<h2 id="timing">Timing</h2>
<table>
<caption>Milliseconds for each CLI run, process start included, {context}.</caption>
{table}</table>
</main>
</html>
"#
    )
}

#[test]
fn every_effect_runs_through_the_cli_with_each_option_in_printable_ascii() {
    let dir = std::env::temp_dir().join(format!("apollinaire-showroom-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let lettering = dir.join("hello.txt");
    let (letters, _) = apollinaire(&["text", "HELLO", "--font", "block"], None);
    std::fs::write(&lettering, &letters).unwrap();
    let (project, _) = apollinaire(
        &[
            "animate", "ghost", "--frames", "2", "--width", "30", "--height", "8",
        ],
        None,
    );
    let ghost = art::template("ghost").unwrap();
    let lettered = Frame::from_text_padded(&letters).unwrap();
    let second = Animation::from_json(&project).unwrap().frames()[1]
        .frame
        .clone();
    assert_ne!(
        second,
        Animation::from_json(&project).unwrap().frames()[0].frame
    );
    let mut seed_changes = 0;
    let mut shown = Vec::new();
    for effect in EFFECTS {
        let mut timings = Vec::new();
        let mut clip = String::new();
        for variant in VARIANTS {
            let mut args = vec!["effect", effect];
            args.extend(variant.args.iter().map(|arg| match *arg {
                "FILE" => lettering.to_str().unwrap(),
                other => other,
            }));
            let stdin =
                matches!(variant.input, Input::ProjectOnStdin).then_some(project.as_bytes());
            let (json, milliseconds) = apollinaire(&args, stdin);
            let animation = Animation::from_json(&json).unwrap();
            let frames = animation.frames();
            let expected = (u64::from(variant.duration_ms) * u64::from(variant.fps))
                .div_ceil(1000)
                .max(2) as usize;
            assert_eq!(frames.len(), expected, "{effect}: {}", variant.label);
            assert_eq!(animation.duration_ms(), u64::from(variant.duration_ms));
            // Every effect but burn arrives at its source frame; burn ends blank. Reversed, the
            // end state comes first.
            let source = match variant.input {
                Input::Catalogue => &ghost,
                Input::LetteringFile => &lettered,
                Input::ProjectOnStdin => &second,
            };
            let end = if *effect == "burn" {
                Frame::blank(source.width(), source.height()).unwrap()
            } else {
                source.clone()
            };
            let finish = if variant.reverse {
                &frames[0]
            } else {
                &frames[frames.len() - 1]
            };
            assert_eq!(finish.frame, end, "{effect}: {}", variant.label);
            if args.contains(&"--seed") {
                let same_with_default_seed: Vec<_> = args
                    .iter()
                    .map(|arg| if *arg == "7" { "42" } else { *arg })
                    .collect();
                seed_changes += usize::from(apollinaire(&same_with_default_seed, stdin).0 != json);
            }
            if matches!(variant.input, Input::Catalogue) && clip.is_empty() {
                clip = json;
            }
            timings.push(milliseconds);
        }
        let halfway = (Animation::from_json(&clip).unwrap().frames().len() / 2).to_string();
        let (frame, _) = apollinaire(
            &["import", "-", "--format", "text", "--frame", &halfway],
            Some(clip.as_bytes()),
        );
        let (player, _) = apollinaire(&["import", "-", "--format", "html"], Some(clip.as_bytes()));
        assert_eq!(
            non_text_elements(&player),
            Vec::<&str>::new(),
            "{effect} player"
        );
        shown.push((
            Shown {
                effect,
                frame: frame.trim_end_matches('\n').to_string(),
                timings,
            },
            player,
        ));
    }
    // Seeded effects must change with the seed, or --seed is not reaching them.
    assert!(seed_changes > 0, "no effect changed with --seed");
    let context = format!(
        "{} build of {} on {} {}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        std::env::var("GITHUB_SHA").map_or("a local checkout".into(), |sha| sha
            .get(..12)
            .unwrap_or(&sha)
            .to_string()),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let (entries, players): (Vec<_>, Vec<_>) = shown.into_iter().unzip();
    let page = index(&entries, &context);
    assert_eq!(
        non_text_elements(&page),
        Vec::<&str>::new(),
        "showroom index"
    );
    if let Some(out) = std::env::var_os("APOLLINAIRE_SHOWROOM") {
        let out = std::path::Path::new(&out);
        std::fs::create_dir(out).expect("APOLLINAIRE_SHOWROOM must name a new directory");
        std::fs::write(out.join("index.html"), page).unwrap();
        for (entry, player) in entries.iter().zip(players) {
            std::fs::write(out.join(format!("{}.html", entry.effect)), player).unwrap();
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_text_only_check_finds_each_non_text_element() {
    let page = index(&[], "a test");
    assert_eq!(non_text_elements(&page), Vec::<&str>::new());
    for tag in [
        "<CANVAS id=a>",
        "<svg>",
        "<img src=x>",
        "<image src=x>",
        "<picture>",
        "<video>",
        "<object>",
        "<embed>",
        "<iframe>",
        "<frameset>",
        "<input type=image>",
        "<input type=\"image\">",
        "<input type='image' src=x>",
        "<input type = \"Image\">",
        "<input title=\"x > y\" type=image>",
        "<script>document.createElement('canvas')</script>",
        "<p style=\"background:url(x.png)\">",
    ] {
        assert_eq!(non_text_elements(&format!("{page}{tag}")).len(), 1, "{tag}");
    }
    // Text that only looks like an image input's attribute is not one.
    for text in [
        "<pre>type = \"image\"</pre>",
        "<script>let kind = 'type=image';</script>",
        "<input type=text><p>type=image</p>",
    ] {
        assert_eq!(
            non_text_elements(&format!("{page}{text}")),
            Vec::<&str>::new(),
            "{text}"
        );
    }
}
