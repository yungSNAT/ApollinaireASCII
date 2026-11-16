// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Video files for the CLI binary, decoded by an installed ffmpeg. ffprobe reads the size and
//! duration; one ffmpeg process then decodes the sampled frames as raw RGBA, read one frame at
//! a time. Both start without a shell, with the path as one `file:` argument, local files as
//! their only protocol and the demuxers of the video extensions as their only formats.
//!
//! The editor seeks to each sample time; ffmpeg's `fps` filter picks the frame showing at each
//! time instead, so a variable frame rate video can give a neighbouring frame at a few samples.

use apollinaire_ascii::{
    Animation, Glyphs, RenderOptions, Result, TimedFrame,
    video::{check_capture, frame_durations, render_rgba_as, sample_size, sample_times},
};
use std::{
    ffi::OsString,
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
};

/// Extensions read as video, in any letter case.
const EXTENSIONS: [&str; 9] = [
    "mp4", "m4v", "mov", "webm", "mkv", "avi", "ogv", "mpg", "mpeg",
];
/// Demuxers for those extensions only: no playlist or reference formats such as hls or concat,
/// which name other files or URLs.
const DEMUXERS: &str = "mov,mp4,m4a,3gp,3g2,mj2,matroska,webm,avi,mpeg,mpegts,ogg";
/// The most of ffmpeg's error output kept for a message.
const MAX_ERROR_BYTES: u64 = 64 * 1024;

/// The editor's video options.
pub struct Sampling {
    pub fps: u32,
    pub start_ms: f64,
    pub end_ms: Option<f64>,
}

pub fn is_video(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    EXTENSIONS.iter().any(|v| v.eq_ignore_ascii_case(extension))
}

/// Convert a video file to a timed project, sampled and limited as the editor does.
pub fn convert(
    path: &Path,
    sampling: &Sampling,
    options: &RenderOptions,
    glyphs: Glyphs,
) -> Result<Animation> {
    let mut input = OsString::from("file:");
    input.push(path);
    let shown = path.display();
    let mut probe = Command::new("ffprobe");
    probe
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            "file",
            "-format_whitelist",
            DEMUXERS,
        ])
        .args(["-select_streams", "v:0", "-of", "json", "-show_entries"])
        .arg("stream=width,height:stream_side_data=rotation:format=duration,format_name")
        .arg("-i")
        .arg(&input)
        .stdin(Stdio::null());
    let probe = probe.output().map_err(missing)?;
    if !probe.status.success() {
        return Err(failed("ffprobe", &shown, &probe.stderr));
    }
    let (width, height, duration_ms, demuxer) = probed(&probe.stdout).ok_or_else(|| {
        format!("ffprobe found no video stream with a size and duration in {shown}")
    })?;
    let end_ms = sampling.end_ms.unwrap_or(duration_ms);
    let times = sample_times(duration_ms, sampling.start_ms, end_ms, sampling.fps)?;
    let (pixels_wide, pixels_high) = sample_size(width, height, options.width)?;
    let rows = options.output_height(pixels_wide, pixels_high);
    check_capture(options.width, rows, times.len())?;

    let seconds = |ms: f64| format!("{:.3}", ms / 1000.0);
    let mut decode = Command::new("ffmpeg");
    decode
        .args([
            "-nostdin",
            "-xerror",
            "-v",
            "error",
            "-protocol_whitelist",
            "file",
        ])
        .args(["-format_whitelist", DEMUXERS, "-f", &demuxer]);
    if demuxer == "mov" {
        // Never follow a QuickTime file's references to other files.
        decode.args(["-enable_drefs", "0", "-use_absolute_path", "0"]);
    }
    decode
        .args(["-ss", &seconds(sampling.start_ms), "-i"])
        .arg(&input)
        .args(["-t", &seconds(end_ms - sampling.start_ms), "-an", "-vf"])
        .arg(format!(
            "fps={},scale={pixels_wide}:{pixels_high}",
            sampling.fps
        ))
        .args(["-frames:v", &times.len().to_string()])
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = decode.spawn().map_err(missing)?;
    // Errors are read alongside the frames, so a full error pipe cannot stall ffmpeg.
    let errors = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            let mut kept = Vec::new();
            let mut stderr = stderr;
            let _ = stderr.by_ref().take(MAX_ERROR_BYTES).read_to_end(&mut kept);
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
            kept
        })
    });
    let frames = read_frames(
        &mut child,
        (pixels_wide, pixels_high),
        &times,
        options,
        glyphs,
    );
    // Closing the pipe stops ffmpeg should any frames be left unread.
    drop(child.stdout.take());
    let status = child
        .wait()
        .map_err(|e| format!("ffmpeg did not finish: {e}"))?;
    let errors = errors.and_then(|e| e.join().ok()).unwrap_or_default();
    let frames = frames?;
    if !status.success() {
        return Err(failed("ffmpeg", &shown, &errors));
    }
    // A clean exit before every sample means the video's frames end before its duration: keep
    // what there is, and let the last frame last to the end.
    if frames.is_empty() {
        return Err(format!("ffmpeg decoded no frames from {shown}"));
    }
    let durations = frame_durations(&times[..frames.len()], end_ms)?;
    Animation::new(
        frames
            .into_iter()
            .zip(durations)
            .map(|(frame, duration_ms)| TimedFrame { frame, duration_ms })
            .collect(),
        true,
    )
}

/// Each whole frame ffmpeg writes, rendered as it arrives, up to one per sample time.
fn read_frames(
    child: &mut Child,
    (width, height): (u32, u32),
    times: &[f64],
    options: &RenderOptions,
    glyphs: Glyphs,
) -> Result<Vec<apollinaire_ascii::Frame>> {
    let stdout = child.stdout.as_mut().ok_or("ffmpeg gave no output")?;
    let mut frames = Vec::with_capacity(times.len());
    let mut pixels = vec![0; width as usize * height as usize * 4];
    while frames.len() < times.len() && stdout.read_exact(&mut pixels).is_ok() {
        let frame = render_rgba_as(width, height, pixels.clone(), options, glyphs)?;
        frames.push(frame);
    }
    Ok(frames)
}

/// Width, height (turned with the video's rotation, as ffmpeg turns its frames), duration in
/// milliseconds and the demuxer from ffprobe's JSON.
fn probed(json: &[u8]) -> Option<(u32, u32, f64, String)> {
    let value: serde_json::Value = serde_json::from_slice(json).ok()?;
    let stream = value.get("streams")?.get(0)?;
    let size = |key: &str| u32::try_from(stream.get(key)?.as_u64()?).ok();
    let (width, height) = (size("width")?, size("height")?);
    let format = value.get("format")?;
    let duration: f64 = format.get("duration")?.as_str()?.parse().ok()?;
    let demuxer = format
        .get("format_name")?
        .as_str()?
        .split(',')
        .next()?
        .to_string();
    let rotation = stream
        .get("side_data_list")
        .and_then(|list| {
            list.as_array()?
                .iter()
                .find_map(|d| d.get("rotation")?.as_i64())
        })
        .unwrap_or(0);
    let (width, height) = if rotation % 180 == 0 {
        (width, height)
    } else {
        (height, width)
    };
    Some((width, height, duration * 1000.0, demuxer))
}

fn missing(error: std::io::Error) -> String {
    format!(
        "Video input needs ffmpeg and ffprobe on PATH ({error}); install ffmpeg, or convert the video in the browser editor"
    )
}

fn failed(program: &str, shown: &impl std::fmt::Display, errors: &[u8]) -> String {
    let errors = String::from_utf8_lossy(errors);
    let detail: Vec<&str> = errors
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(3)
        .collect();
    format!("{program} could not read {shown}: {}", detail.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_video_demuxers_are_named_to_ffmpeg() {
        assert_eq!(allowed("matroska"), Ok("matroska"));
        assert_eq!(
            allowed("hls").unwrap_err(),
            "ffprobe chose the hls format, which video input does not read"
        );
        assert!(allowed("mov,mp4").is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_program_that_stalls_is_stopped_and_reaped() {
        let started = Instant::now();
        let mut stalls = Command::new("sh");
        stalls.args(["-c", "echo $$; exec sleep 30"]);
        let mut run = Run::start(&mut stalls, Duration::from_millis(200)).unwrap();
        let mut pid = String::new();
        run.stdout.read_to_string(&mut pid).unwrap();
        assert_eq!(
            run.finish("ffmpeg", &"clip.mp4").unwrap_err(),
            "ffmpeg made no progress on clip.mp4 for 0.2 seconds, so it was stopped"
        );
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(!Path::new(&format!("/proc/{}", pid.trim())).exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_slow_program_that_keeps_making_progress_runs_to_the_end() {
        let mut slow = Command::new("sh");
        slow.args(["-c", "for i in 1 2 3 4; do sleep 1; echo; done"]);
        let mut run = Run::start(&mut slow, Duration::from_secs(2)).unwrap();
        let mut byte = [0];
        while run.stdout.read_exact(&mut byte).is_ok() {
            run.progress.fetch_add(1, Ordering::Relaxed);
        }
        let (status, errors) = run.finish("ffmpeg", &"clip.mp4").unwrap();
        assert!(status.success() && errors.is_empty());
    }
}
