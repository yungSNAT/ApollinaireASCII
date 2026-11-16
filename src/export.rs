// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use crate::Animation;

/// An asciicast v2 recording that `asciinema play`, its web player and `agg` can replay. Each
/// frame homes the cursor and overwrites the whole grid, so the only escape sequences are
/// cursor movement, one screen clear and hiding the cursor. A last event at the end of the
/// final frame keeps that frame on screen for its duration, and the header's idle time limit
/// is the longest a frame may last, 60 seconds, so players that shorten pauses keep each
/// frame whole. Recordings play once; looping is the player's option.
pub fn asciicast(project: &Animation) -> String {
    let mut result = format!(
        "{{\"version\":2,\"width\":{},\"height\":{},\"idle_time_limit\":60}}\n",
        project.width(),
        project.height()
    );
    let mut event = |ms: u64, output: &str| {
        let output = serde_json::to_string(output).expect("a string always serialises");
        result.push_str(&format!(
            "[{}.{:03},\"o\",{output}]\n",
            ms / 1000,
            ms % 1000
        ));
    };
    let mut start = 0;
    for (index, entry) in project.frames().iter().enumerate() {
        let setup = if index == 0 { "\x1b[?25l\x1b[2J" } else { "" };
        let rows = entry.frame.text().replace('\n', "\r\n");
        event(start, &format!("{setup}\x1b[H{rows}"));
        start += u64::from(entry.duration_ms);
    }
    event(start, "\x1b[?25h");
    result
}

/// The frames, their durations in milliseconds and the loop flag as JavaScript declarations.
/// JSON strings are valid JavaScript and Go literals, and frames hold only printable ASCII and
/// line feeds.
fn script_data(project: &Animation) -> String {
    let frames: Vec<String> = project.frames().iter().map(|e| e.frame.text()).collect();
    let durations: Vec<u32> = project.frames().iter().map(|e| e.duration_ms).collect();
    format!(
        "const frames = {};\nconst durations = {};\nconst loop = {};\n",
        serde_json::to_string(&frames).expect("strings always serialise"),
        serde_json::to_string(&durations).expect("numbers always serialise"),
        project.looping()
    )
}

/// The React hook both JavaScript components play with: the frame index, advanced by a timer
/// that stops while paused and, unless the artwork loops, on its last frame.
const USE_FRAME: &str = "
function useFrame(paused) {
  const [index, setIndex] = useState(0);
  useEffect(() => {
    if (paused || frames.length < 2 || (!loop && index === frames.length - 1)) return undefined;
    const timer = setTimeout(() => setIndex((index + 1) % frames.length), durations[index]);
    return () => clearTimeout(timer);
  }, [index, paused]);
  return index;
}
";

/// A React component, as an ES module that needs no JSX transform:
/// `<ApollinaireArt paused={false} className="art" label="ASCII artwork" />`. It renders one
/// `<pre>` with the image role and plays the frames, starting paused when the reader asks for
/// reduced motion.
pub fn react(project: &Animation) -> String {
    format!(
        "// ApollinaireASCII artwork as a React component.
import {{ createElement, useEffect, useState }} from 'react';

{}{USE_FRAME}
const motionQuery = '(prefers-reduced-motion: reduce)';
function prefersReducedMotion() {{
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function'
    && window.matchMedia(motionQuery).matches;
}}

export default function ApollinaireArt({{ paused = false, className, label = 'ASCII artwork' }}) {{
  const [still, setStill] = useState(prefersReducedMotion);
  useEffect(() => {{
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return undefined;
    const query = window.matchMedia(motionQuery);
    const change = () => setStill(query.matches);
    query.addEventListener('change', change);
    // The preference may have changed since rendering, with no event to say so.
    change();
    return () => query.removeEventListener('change', change);
  }}, []);
  const index = useFrame(paused || still);
  return createElement('pre', {{ className, role: 'img', 'aria-label': label }}, frames[index]);
}}
",
        script_data(project)
    )
}

/// An Ink component for terminal apps, as an ES module: `render(<ApollinaireArt />)`. Each row
/// is its own `<Text>`, truncated rather than wrapped in a narrow terminal.
pub fn ink(project: &Animation) -> String {
    format!(
        "// ApollinaireASCII artwork as an Ink component.
import {{ createElement, useEffect, useState }} from 'react';
import {{ Box, Text }} from 'ink';

{}{USE_FRAME}
export default function ApollinaireArt({{ paused = false }}) {{
  const index = useFrame(paused);
  return createElement(
    Box,
    {{ flexDirection: 'column' }},
    frames[index].split('\\n').map((row, key) => createElement(Text, {{ key, wrap: 'truncate' }}, row)),
  );
}}
",
        script_data(project)
    )
}

/// An OpenTUI function, as an ES module: `addApollinaireArt(renderer)` adds one unwrapped
/// `TextRenderable` to `renderer.root`, plays the frames and returns `{ renderable, stop }`.
pub fn opentui(project: &Animation) -> String {
    format!(
        "// ApollinaireASCII artwork for OpenTUI.
import {{ TextRenderable }} from '@opentui/core';

{}
export function addApollinaireArt(renderer) {{
  const renderable = new TextRenderable(renderer, {{ content: frames[0], wrapMode: 'none' }});
  renderer.root.add(renderable);
  let index = 0;
  let timer;
  const next = () => {{
    if (frames.length < 2 || (!loop && index === frames.length - 1)) return;
    timer = setTimeout(() => {{
      index = (index + 1) % frames.length;
      renderable.content = frames[index];
      next();
    }}, durations[index]);
  }};
  next();
  return {{ renderable, stop: () => clearTimeout(timer) }};
}}
",
        script_data(project)
    )
}

/// A Bubble Tea v2 program in Go: the `Art` model and a `main` that plays it until `q` or
/// Ctrl+C. To embed `Art`, change the package line and delete `main`.
pub fn bubbletea(project: &Animation) -> String {
    let list = |items: Vec<String>| items.join(", ");
    let frames = list(
        project
            .frames()
            .iter()
            .map(|e| serde_json::to_string(&e.frame.text()).expect("a string always serialises"))
            .collect(),
    );
    let durations = list(
        project
            .frames()
            .iter()
            .map(|e| e.duration_ms.to_string())
            .collect(),
    );
    let looping = project.looping();
    format!(
        "// ApollinaireASCII artwork as a Bubble Tea program.
package main

import (
\t\"fmt\"
\t\"os\"
\t\"time\"

\ttea \"charm.land/bubbletea/v2\"
)

var frames = []string{{{frames}}}

// Milliseconds each frame shows for.
var durations = []int{{{durations}}}

const loop = {looping}

type artTick struct{{}}

// Art plays the frames.
type Art struct {{
\tindex int
}}

// NewArt starts on the first frame.
func NewArt() Art {{ return Art{{}} }}

func (a Art) next() tea.Cmd {{
\tif len(frames) < 2 || (!loop && a.index == len(frames)-1) {{
\t\treturn nil
\t}}
\treturn tea.Tick(time.Duration(durations[a.index])*time.Millisecond, func(time.Time) tea.Msg {{ return artTick{{}} }})
}}

// Init starts the timer.
func (a Art) Init() tea.Cmd {{ return a.next() }}

// Update moves to the next frame on each tick and quits on q or Ctrl+C.
func (a Art) Update(msg tea.Msg) (tea.Model, tea.Cmd) {{
\tswitch msg := msg.(type) {{
\tcase artTick:
\t\ta.index = (a.index + 1) % len(frames)
\t\treturn a, a.next()
\tcase tea.KeyPressMsg:
\t\tswitch msg.String() {{
\t\tcase \"q\", \"ctrl+c\":
\t\t\treturn a, tea.Quit
\t\t}}
\t}}
\treturn a, nil
}}

// View shows the current frame.
func (a Art) View() tea.View {{ return tea.NewView(frames[a.index]) }}

func main() {{
\tif _, err := tea.NewProgram(NewArt()).Run(); err != nil {{
\t\tfmt.Fprintln(os.Stderr, err)
\t\tos.Exit(1)
\t}}
}}
"
    )
}

/// Portable HTML uses a small JS text player because it has no WASM dependency.
pub fn html(project: &Animation) -> String {
    let data = project
        .to_json()
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e");
    let mut result = String::from(
        r#"<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>ApollinaireASCII text animation</title>
<style>
:root{color-scheme:dark}body{margin:24px;background:#000000;color:#FFFFF0;font:16px system-ui}
button{font:inherit;min-height:44px;padding:8px 16px;background:#66023C;color:#FFFFF0;border:1px solid #FFFFF0;cursor:pointer}
button:focus-visible,pre:focus-visible{outline:3px solid #FFD32A;outline-offset:4px}
pre{font:14px/1.2 ui-monospace,Consolas,monospace;white-space:pre;overflow:auto;max-width:100%;user-select:text}
</style>
<h1>ApollinaireASCII</h1><p>Selectable ASCII text. Copy it into any monospace editor.</p>
<button id="play" type="button">Play</button> <button id="step" type="button">Next frame</button>
<p id="count"></p><pre id="art" tabindex="0" aria-label="ASCII artwork"></pre>
<script type="application/json" id="project">"#,
    );
    result.push_str(&data);
    result.push_str(r#"</script><script>
'use strict';
const p=JSON.parse(document.getElementById('project').textContent);
const art=document.getElementById('art'),play=document.getElementById('play'),count=document.getElementById('count');
let index=0,playing=false,last=0,elapsed=0,completed=false;
function show(){art.textContent=p.frames[index].text;count.textContent='Frame '+(index+1)+' of '+p.frames.length;}
function pause(){playing=false;play.textContent='Play';}
play.onclick=()=>{if(playing){pause();return;}if(completed){index=0;elapsed=0;completed=false;show();}playing=true;play.textContent='Pause';last=0;};
document.getElementById('step').onclick=()=>{pause();index=(index+1)%p.frames.length;elapsed=0;completed=false;show();};
document.addEventListener('visibilitychange',()=>{if(document.hidden)pause();});
const motion=matchMedia('(prefers-reduced-motion: reduce)');
motion.addEventListener('change',()=>{if(motion.matches)pause();});
function tick(now){
 if(playing&&!document.hidden){
  if(last)elapsed+=now-last;
  while(playing&&elapsed>=p.frames[index].duration_ms){
   elapsed-=p.frames[index].duration_ms;
   if(index+1<p.frames.length)index++;else if(p.loop)index=0;else {pause();completed=true;}
   show();
  }
 }last=now;requestAnimationFrame(tick);
}
show();requestAnimationFrame(tick);
</script></html>"#);
    result
}
