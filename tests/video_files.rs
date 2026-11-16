// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Video files in `apollinaire image`, decoded by ffmpeg. The tests that run ffmpeg need it on
//! PATH: CI installs it on Linux and fails without it; elsewhere they are skipped.

#![cfg(feature = "cli")]
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_apollinaire"))
        .args(args)
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn video_options_need_a_video_and_video_needs_ffmpeg() {
    for args in [
        &["image", "picture.png", "--fps", "5"][..],
        &["image", "picture.png", "--start", "1"],
        &["image", "picture.png", "--end", "1"],
        // A URL is fetched as an image, whatever its name.
        &["image", "https://example.com/clip.mp4", "--fps", "5"],
    ] {
        let output = cli(args);
        assert!(!output.status.success());
        assert_eq!(
            stderr(&output),
            "apollinaire: --fps, --start and --end need a video input\n",
            "{args:?}"
        );
    }
    assert!(!cli(&["image", "clip.mp4", "--fps", "61"]).status.success());
    let output = Command::new(env!("CARGO_BIN_EXE_apollinaire"))
        .args(["image", "clip.MP4"])
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("Video input needs ffmpeg and ffprobe on PATH (",)
            && stderr(&output)
                .ends_with("); install ffmpeg, or convert the video in the browser editor\n"),
        "{}",
        stderr(&output)
    );
}

#[cfg(target_os = "linux")]
mod with_ffmpeg {
    use super::{Output, cli, stderr};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apollinaire-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Whether ffmpeg can run here; CI must have it.
    fn ffmpeg_present() -> bool {
        let found = Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|o| o.status.success());
        assert!(
            found || std::env::var_os("CI").is_none(),
            "ffmpeg is required on CI"
        );
        if !found {
            eprintln!("skipped: ffmpeg is not on PATH");
        }
        found
    }

    /// Make a test file with ffmpeg's own sources.
    fn make(output: &Path, args: &[&str]) {
        let made = Command::new("ffmpeg")
            .args(["-y", "-v", "error"])
            .args(args)
            .arg(output)
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", stderr(&made));
    }

    fn project(output: &Output) -> apollinaire_ascii::Animation {
        assert!(output.status.success(), "{}", stderr(output));
        apollinaire_ascii::Animation::from_json(std::str::from_utf8(&output.stdout).unwrap())
            .unwrap()
    }

    #[test]
    fn videos_convert_to_timed_projects_sampled_as_the_editor_samples() {
        use apollinaire_ascii::{RenderOptions, video::render_rgba};
        if !ffmpeg_present() {
            return;
        }
        let dir = scratch("video");
        // One second of black, then one of white, 64 x 48 pixels.
        let halves = [
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x48:r=25:d=1",
            "-f",
            "lavfi",
            "-i",
            "color=c=white:s=64x48:r=25:d=1",
            "-filter_complex",
            "[0][1]concat=n=2:v=1",
        ];
        let clip = dir.join("clip.mp4");
        make(
            &clip,
            &[&halves[..], &["-c:v", "mpeg4", "-q:v", "2"][..]].concat(),
        );
        let solid = |level: u8| {
            render_rgba(
                64,
                48,
                [level, level, level, 255].repeat(64 * 48),
                &RenderOptions::default(),
            )
            .unwrap()
        };
        let (black, white) = (solid(0), solid(255));
        let path = clip.to_str().unwrap();
        let whole = project(&cli(&["image", path]));
        assert_eq!(whole.frames().len(), 20);
        assert!(whole.frames().iter().all(|f| f.duration_ms == 100));
        assert_eq!((whole.width(), whole.height()), (80, 30));
        for (index, frame) in whole.frames().iter().enumerate() {
            match index {
                0..=8 => assert_eq!(frame.frame, black, "frame {index}"),
                11.. => assert_eq!(frame.frame, white, "frame {index}"),
                _ => {}
            }
        }
        let cut = project(&cli(&[
            "image", path, "--fps", "5", "--start", "0.5", "--end", "1.5",
        ]));
        assert_eq!(cut.frames().len(), 5);
        assert!(cut.frames().iter().all(|f| f.duration_ms == 200));
        assert_eq!(cut.frames()[0].frame, black);
        assert_eq!(cut.frames()[4].frame, white);

        // WebM goes through another demuxer; a name like an option or protocol is still a file.
        let webm = dir.join("-concat:a|b.webm");
        make(
            &webm,
            &[&halves[..], &["-c:v", "libvpx", "-b:v", "1M"][..]].concat(),
        );
        let named = project(&cli(&[
            "image",
            "--width",
            "8",
            "--",
            webm.to_str().unwrap(),
        ]));
        assert_eq!(named.frames().len(), 20);

        // Several inputs, images and videos alike, each to its own file.
        let picture = dir.join("still.png");
        make(
            &picture,
            &["-f", "lavfi", "-i", "color=c=white:s=8x8", "-frames:v", "1"],
        );
        let out = dir.join("out");
        let batch = cli(&[
            "image",
            picture.to_str().unwrap(),
            path,
            "--fps",
            "2",
            "--output-dir",
            out.to_str().unwrap(),
        ]);
        assert!(batch.status.success(), "{}", stderr(&batch));
        assert!(out.join("still.png.txt").is_file());
        let json = std::fs::read_to_string(out.join("clip.mp4.json")).unwrap();
        assert_eq!(
            apollinaire_ascii::Animation::from_json(&json)
                .unwrap()
                .frames()
                .len(),
            4
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn videos_that_end_early_or_cannot_be_read_are_handled_as_ffmpeg_reports() {
        if !ffmpeg_present() {
            return;
        }
        let dir = scratch("video-ends");
        // One second of picture in a file 2 seconds long: the frames there are keep their
        // samples, and the last lasts to the end.
        let short = dir.join("short.mov");
        make(
            &short,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=s=32x24:r=25:d=1",
                "-f",
                "lavfi",
                "-i",
                "sine=d=2",
                "-c:v",
                "mpeg4",
                "-c:a",
                "pcm_s16le",
            ],
        );
        let path = short.to_str().unwrap();
        let ended = project(&cli(&["image", path]));
        let durations: Vec<u32> = ended.frames().iter().map(|f| f.duration_ms).collect();
        assert!((10..=11).contains(&durations.len()), "{durations:?}");
        assert_eq!(durations.iter().sum::<u32>(), 2000);
        assert!(durations[..durations.len() - 1].iter().all(|d| *d == 100));
        let output = cli(&["image", path, "--start", "1.5"]);
        assert_eq!(
            stderr(&output),
            format!("apollinaire: ffmpeg decoded no frames from {path}\n")
        );

        // Sound alone has no picture to draw.
        let sound = dir.join("sound.mp4");
        make(&sound, &["-f", "lavfi", "-i", "sine=d=1", "-c:a", "aac"]);
        let output = cli(&["image", sound.to_str().unwrap()]);
        assert_eq!(
            stderr(&output),
            format!(
                "apollinaire: ffprobe found no video stream with a size and duration in {}\n",
                sound.display()
            )
        );

        // A playlist named like a video is not read: its format is not one of the video formats,
        // so the clip it names is never opened.
        let playlist = dir.join("list.mp4");
        std::fs::write(
            &playlist,
            "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\nshort.mov\n#EXT-X-ENDLIST\n",
        )
        .unwrap();
        let output = cli(&["image", playlist.to_str().unwrap()]);
        assert!(
            stderr(&output).starts_with(&format!(
                "apollinaire: ffprobe could not read {}: ",
                playlist.display()
            )),
            "{}",
            stderr(&output)
        );

        // A file cut short part-way through its pictures is an error, not an early end.
        let whole = dir.join("whole.mp4");
        make(
            &whole,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=s=320x240:r=25:d=2",
                "-c:v",
                "mpeg4",
                "-q:v",
                "2",
                "-movflags",
                "+faststart",
            ],
        );
        let bytes = std::fs::read(&whole).unwrap();
        let cut = dir.join("cut.mp4");
        std::fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
        let output = cli(&["image", cut.to_str().unwrap()]);
        assert!(
            stderr(&output).starts_with(&format!(
                "apollinaire: ffmpeg could not read {}: ",
                cut.display()
            )),
            "{}",
            stderr(&output)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
