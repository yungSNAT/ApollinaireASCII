// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! `apollinaire edit`: an interactive terminal editor. Drawing goes through the library's
//! `Canvas` and frame operations through `Animation`; this module maps keys and the mouse to
//! them and draws the result. Escape codes are terminal transport only, never artwork.

use apollinaire_ascii::{Animation, Canvas, Frame, Result, TimedFrame};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute, queue,
    style::{Attribute, Print, SetAttribute},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    fs::File,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// The same bounds as the browser editor's history.
const HISTORY_STATES: usize = 100;
const HISTORY_CELLS: usize = 16_000_000;

// tmux and screen can take Ctrl+B and Ctrl+S or Ctrl+Q before the editor sees them, so the
// brush, save and quit keys also have function keys.
const HELP: &[&str] = &[
    "Keys",
    "",
    "Type to place characters. Enter goes to the next row; Backspace and Delete erase.",
    "Arrow keys, Home and End move. The left mouse button draws with the brush.",
    "Ctrl+B or F3, then a key: choose the brush.",
    "PageUp and PageDown: previous and next frame. Ctrl+N: new blank frame after this one.",
    "Ctrl+D: duplicate this frame. Ctrl+K: delete it. Ctrl+P: play; any key stops.",
    "Ctrl+Z: undo. Ctrl+Y: redo.",
    "Ctrl+S or F2: save. Ctrl+Q or F10: quit.",
    "",
    "F1 or Escape closes this help.",
];
const TEXT_HOLDS_ONE_FRAME: &str = "A .txt file holds one frame; edit a .json file to animate";

/// What the terminal loop does after an event.
#[derive(Debug, PartialEq)]
pub enum Flow {
    Continue,
    Play,
    Quit,
}

/// Consecutive typing, or one mouse stroke, is one undo step.
#[derive(Clone, Copy, PartialEq)]
enum Run {
    Typing,
    Stroke,
}

pub struct Editor {
    path: PathBuf,
    project: Animation,
    frame: usize,
    cursor: (usize, usize),
    brush: u8,
    choosing_brush: bool,
    stroke_from: Option<(i32, i32)>,
    joining: Option<Run>,
    undo: Vec<(Animation, usize)>,
    redo: Vec<(Animation, usize)>,
    modified: bool,
    quit_warned: bool,
    help: bool,
    message: String,
    /// Top-left cell shown, and the rows of the screen showing the frame, from the last draw.
    scroll: (usize, usize),
    view_rows: usize,
}

/// `edit` saves to its file, so it takes only .txt and .json paths.
pub fn check_path(path: &Path) -> Result<()> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("txt" | "json") => Ok(()),
        _ => Err("Edit needs a .txt or .json file".into()),
    }
}

impl Editor {
    pub fn new(path: PathBuf, project: Animation) -> Self {
        Self {
            path,
            project,
            frame: 0,
            cursor: (0, 0),
            brush: b'#',
            choosing_brush: false,
            stroke_from: None,
            joining: None,
            undo: Vec::new(),
            redo: Vec::new(),
            modified: false,
            quit_warned: false,
            help: false,
            message: "F1 lists the keys. Ctrl+S or F2 saves; Ctrl+Q or F10 quits.".into(),
            scroll: (0, 0),
            view_rows: 0,
        }
    }

    fn current(&self) -> &TimedFrame {
        &self.project.frames()[self.frame]
    }

    fn at_cursor(&self) -> (i32, i32) {
        (self.cursor.0 as i32, self.cursor.1 as i32)
    }

    fn is_text(&self) -> bool {
        self.path.extension().is_some_and(|e| e == "txt")
    }

    /// Apply one change to the current frame. A change that leaves the frame as it was adds no
    /// undo step, does not mark the project modified and starts no run, so the next real change
    /// gets its own step.
    fn draw_with(&mut self, run: Option<Run>, f: impl FnOnce(&mut Canvas) -> Result<()>) {
        let mut canvas = Canvas::from_frame(&self.current().frame);
        if let Err(error) = f(&mut canvas) {
            self.message = error;
            return;
        }
        let frame = canvas.finish();
        if frame == self.current().frame {
            return;
        }
        let mut project = self.project.clone();
        let duration_ms = self.current().duration_ms;
        if let Err(error) = project.replace_frame(self.frame, TimedFrame { frame, duration_ms }) {
            self.message = error;
            return;
        }
        let joins = run.is_some() && run == self.joining;
        self.commit(project, self.frame, !joins);
        self.joining = run;
    }

    /// Change the frames, returning the frame to show afterwards.
    fn restructure(&mut self, f: impl FnOnce(&mut Animation, usize) -> Result<usize>) {
        let mut project = self.project.clone();
        match f(&mut project, self.frame) {
            Ok(frame) => self.commit(project, frame, true),
            Err(error) => self.message = error,
        }
    }

    fn commit(&mut self, project: Animation, frame: usize, snapshot: bool) {
        let before = std::mem::replace(&mut self.project, project);
        if snapshot {
            self.undo.push((before, self.frame));
            self.redo.clear();
            self.limit_history();
        }
        self.frame = frame;
        self.modified = true;
        // A change made after the unsaved-changes warning needs a new warning.
        self.quit_warned = false;
    }

    fn limit_history(&mut self) {
        let cells = |states: &[(Animation, usize)]| {
            states
                .iter()
                .map(|(p, _)| p.width() * p.height() * p.frames().len())
                .sum::<usize>()
        };
        while self.undo.len() + self.redo.len() > HISTORY_STATES
            || cells(&self.undo) + cells(&self.redo) > HISTORY_CELLS
        {
            if self.undo.is_empty() {
                self.redo.remove(0);
            } else {
                self.undo.remove(0);
            }
        }
    }

    fn step_history(&mut self, back: bool) {
        let (from, to) = if back {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let Some((project, frame)) = from.pop() else {
            self.message = if back {
                "Nothing to undo".into()
            } else {
                "Nothing to redo".into()
            };
            return;
        };
        to.push((std::mem::replace(&mut self.project, project), self.frame));
        self.frame = frame;
        self.modified = true;
    }

    fn move_cursor(&mut self, dx: isize, dy: isize) {
        let clamp = |value: usize, delta: isize, len: usize| {
            value.saturating_add_signed(delta).min(len - 1)
        };
        self.cursor = (
            clamp(self.cursor.0, dx, self.project.width()),
            clamp(self.cursor.1, dy, self.project.height()),
        );
    }

    pub fn save(&mut self) -> Result<()> {
        let content = if self.is_text() {
            if self.project.frames().len() > 1 {
                return Err(TEXT_HOLDS_ONE_FRAME.into());
            }
            format!("{}\n", self.current().frame.text())
        } else {
            format!("{}\n", self.project.to_json())
        };
        replace_file(&self.path, &content)?;
        self.modified = false;
        Ok(())
    }

    pub fn handle(&mut self, event: Event) -> Flow {
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => self.key(key),
            Event::Mouse(mouse) => {
                self.mouse(mouse);
                Flow::Continue
            }
            _ => Flow::Continue,
        }
    }

    fn key(&mut self, key: KeyEvent) -> Flow {
        // Windows reports AltGr as Ctrl+Alt, so that pair types its character.
        let control = key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT);
        if control || !matches!(key.code, KeyCode::Char(_)) || self.choosing_brush {
            self.joining = None;
        }
        self.message.clear();
        if self.help {
            if matches!(key.code, KeyCode::F(1) | KeyCode::Esc) {
                self.help = false;
            }
            return Flow::Continue;
        }
        if self.choosing_brush {
            self.choosing_brush = false;
            match key.code {
                KeyCode::Char(c) if is_printable(c) && !control => {
                    self.brush = c as u8;
                    self.message = format!("Brush: {c}");
                }
                KeyCode::Esc => self.message = "Brush unchanged".into(),
                _ => self.message = "The brush must be a printable ASCII character".into(),
            }
            return Flow::Continue;
        }
        let command = match key.code {
            KeyCode::Char(c) if control => Some(c.to_ascii_lowercase()),
            KeyCode::F(2) => Some('s'),
            KeyCode::F(3) => Some('b'),
            KeyCode::F(10) => Some('q'),
            _ => None,
        };
        if let Some(command) = command {
            return self.command(command);
        }
        self.quit_warned = false;
        match key.code {
            KeyCode::Char(c) if is_printable(c) => {
                let (x, y) = self.at_cursor();
                self.draw_with(Some(Run::Typing), |canvas| canvas.set(x, y, c as u8));
                self.move_cursor(1, 0);
            }
            KeyCode::Char(_) => {
                self.message = "Only printable ASCII characters can be typed".into();
            }
            KeyCode::Enter => {
                if self.cursor.1 + 1 < self.project.height() {
                    self.cursor = (0, self.cursor.1 + 1);
                } else {
                    self.message = "This is the bottom row".into();
                }
            }
            KeyCode::Backspace if self.cursor.0 > 0 => {
                self.move_cursor(-1, 0);
                let (x, y) = self.at_cursor();
                self.draw_with(None, |canvas| canvas.set(x, y, b' '));
            }
            KeyCode::Delete => {
                let (x, y) = self.at_cursor();
                self.draw_with(None, |canvas| canvas.set(x, y, b' '));
            }
            KeyCode::Left => self.move_cursor(-1, 0),
            KeyCode::Right => self.move_cursor(1, 0),
            KeyCode::Up => self.move_cursor(0, -1),
            KeyCode::Down => self.move_cursor(0, 1),
            KeyCode::Home => self.cursor.0 = 0,
            KeyCode::End => self.cursor.0 = self.project.width() - 1,
            KeyCode::PageUp if self.frame > 0 => self.frame -= 1,
            KeyCode::PageDown if self.frame + 1 < self.project.frames().len() => self.frame += 1,
            KeyCode::PageUp | KeyCode::PageDown => {
                self.message = "No frame in that direction".into();
            }
            KeyCode::F(1) => self.help = true,
            _ => {}
        }
        Flow::Continue
    }

    fn command(&mut self, c: char) -> Flow {
        if c != 'q' {
            self.quit_warned = false;
        }
        match c {
            'q' if self.modified && !self.quit_warned => {
                self.quit_warned = true;
                self.message =
                    "Unsaved changes: save with Ctrl+S or F2, or quit again to discard them".into();
            }
            'q' => return Flow::Quit,
            's' => match self.save() {
                Ok(()) => self.message = format!("Saved {}", self.path.display()),
                Err(error) => self.message = error,
            },
            'z' => self.step_history(true),
            'y' => self.step_history(false),
            'b' => {
                self.choosing_brush = true;
                self.message = "Type the brush character".into();
            }
            // Every edit stays saveable: a text file never gains a second frame.
            'n' | 'd' if self.is_text() => self.message = TEXT_HOLDS_ONE_FRAME.into(),
            'n' => self.restructure(|project, frame| {
                let blank = TimedFrame {
                    frame: Frame::blank(project.width(), project.height())?,
                    duration_ms: project.frames()[frame].duration_ms,
                };
                project.insert_frame(frame + 1, blank)?;
                Ok(frame + 1)
            }),
            'd' => self.restructure(|project, frame| {
                project.duplicate_frames(frame..frame + 1)?;
                Ok(frame + 1)
            }),
            'k' => self.restructure(|project, frame| {
                project.remove_frame(frame)?;
                Ok(frame.min(project.frames().len() - 1))
            }),
            'p' if self.project.frames().len() > 1 => return Flow::Play,
            'p' => self.message = "Add a frame to play an animation".into(),
            _ => {}
        }
        Flow::Continue
    }

    fn mouse(&mut self, mouse: MouseEvent) {
        let (column, row) = (usize::from(mouse.column), usize::from(mouse.row));
        let point = (column + self.scroll.0, row + self.scroll.1);
        let inside = !self.help
            && row < self.view_rows
            && point.0 < self.project.width()
            && point.1 < self.project.height();
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) if inside => {
                self.cursor = point;
                let (at, brush) = (self.at_cursor(), self.brush);
                self.joining = None;
                self.draw_with(Some(Run::Stroke), |canvas| canvas.set(at.0, at.1, brush));
                self.stroke_from = Some(at);
            }
            MouseEventKind::Drag(MouseButton::Left) if inside => {
                if let Some(from) = self.stroke_from {
                    self.cursor = point;
                    let (to, brush) = (self.at_cursor(), self.brush);
                    self.draw_with(Some(Run::Stroke), |canvas| canvas.line(from, to, brush));
                    self.stroke_from = Some(to);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.stroke_from = None;
                self.joining = None;
            }
            _ => {}
        }
    }

    /// Draw the frame, scrolled to keep the cursor in view, with 2 status lines below it.
    pub fn draw(&mut self, out: &mut impl Write, columns: u16, rows: u16) -> io::Result<()> {
        let (columns, rows) = (usize::from(columns), usize::from(rows));
        queue!(out, Hide, MoveTo(0, 0), Clear(ClearType::All))?;
        if rows < 3 || columns < 20 {
            self.view_rows = 0;
            queue!(out, Print("Enlarge the terminal to edit"))?;
            return out.flush();
        }
        let view_rows = rows - 2;
        self.view_rows = view_rows;
        self.scroll = (
            scrolled(self.scroll.0, self.cursor.0, columns),
            scrolled(self.scroll.1, self.cursor.1, view_rows),
        );
        let frame = &self.current().frame;
        for screen_row in 0..view_rows {
            let line = if self.help {
                HELP.get(screen_row).map(|line| line.as_bytes())
            } else {
                frame.row(self.scroll.1 + screen_row).map(|row| {
                    let start = self.scroll.0.min(row.len());
                    &row[start..(start + columns).min(row.len())]
                })
            };
            if let Some(line) = line {
                let line = &line[..line.len().min(columns)];
                queue!(
                    out,
                    MoveTo(0, screen_row as u16),
                    Print(String::from_utf8_lossy(line))
                )?;
            }
        }
        let name = self
            .path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let status = format!(
            "{name}{}  frame {}/{}  column {} row {}  brush {}",
            if self.modified { " (modified)" } else { "" },
            self.frame + 1,
            self.project.frames().len(),
            self.cursor.0 + 1,
            self.cursor.1 + 1,
            self.brush as char,
        );
        // File names and error text need not be ASCII, so they are cut by character, and a
        // control character in them is shown as `?` rather than sent to the terminal.
        let fit = |text: &str| {
            text.chars()
                .take(columns)
                .map(|c| if c.is_control() { '?' } else { c })
                .collect::<String>()
        };
        queue!(
            out,
            MoveTo(0, view_rows as u16),
            SetAttribute(Attribute::Reverse),
            Print(format!("{:<columns$}", fit(&status))),
            SetAttribute(Attribute::Reset),
            MoveTo(0, (view_rows + 1) as u16),
            Print(fit(&self.message)),
        )?;
        if !self.help {
            queue!(
                out,
                MoveTo(
                    (self.cursor.0 - self.scroll.0) as u16,
                    (self.cursor.1 - self.scroll.1) as u16
                ),
                Show
            )?;
        }
        out.flush()
    }
}

/// The first visible cell, moved only as far as needed to show `cursor` in `span` cells.
fn scrolled(first: usize, cursor: usize, span: usize) -> usize {
    if cursor < first {
        cursor
    } else if cursor >= first + span {
        cursor + 1 - span
    } else {
        first
    }
}

fn is_printable(c: char) -> bool {
    (' '..='~').contains(&c)
}

/// Refuse a symbolic link whose target is missing, which a save would replace with a file.
pub fn check_link(path: &Path) -> Result<()> {
    let link = std::fs::symlink_metadata(path).is_ok_and(|m| m.is_symlink());
    if link && !path.exists() {
        return Err(format!("{} is a link to a missing file", path.display()));
    }
    Ok(())
}

/// Write `content` to a new file beside the file `path` names, following a symbolic link, then
/// rename it over that file, so a failed write or rename leaves the original as it was. An
/// existing file's permission bits carry over; its ACLs, extended attributes and owner do not.
fn replace_file(path: &Path, content: &str) -> Result<()> {
    check_link(path).map_err(|e| format!("Cannot save: {e}"))?;
    let failed = |e: io::Error| format!("Cannot save {}: {e}", path.display());
    let target = match std::fs::canonicalize(path) {
        Ok(target) => target,
        // Only a path that names nothing yet is used as it is; a link is never replaced.
        Err(e)
            if e.kind() == io::ErrorKind::NotFound
                && std::fs::symlink_metadata(path)
                    .is_err_and(|e| e.kind() == io::ErrorKind::NotFound) =>
        {
            path.to_path_buf()
        }
        Err(e) => return Err(failed(e)),
    };
    let permissions = std::fs::metadata(&target).map(|m| m.permissions()).ok();
    // A short name, so a file name near the length limit still has room beside it; the next
    // name is tried when one is taken, and only a file this save created is ever removed.
    let id = std::process::id();
    let (file, temporary) = (0..100)
        .map(|n| match n {
            0 => target.with_file_name(format!(".apollinaire-{id}.tmp")),
            n => target.with_file_name(format!(".apollinaire-{id}-{n}.tmp")),
        })
        .find_map(
            |name| match File::options().write(true).create_new(true).open(&name) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                opened => Some(opened.map(|file| (file, name))),
            },
        )
        .unwrap_or_else(|| Err(io::ErrorKind::AlreadyExists.into()))
        .map_err(failed)?;
    // The file is closed before the rename, which Windows needs.
    let write = |mut file: File| {
        file.write_all(content.as_bytes())?;
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }
        file.sync_all()
    };
    write(file)
        .and_then(|()| std::fs::rename(&temporary, &target))
        .map_err(|e| {
            let _ = std::fs::remove_file(&temporary);
            failed(e)
        })
}

/// Raw mode, the alternate screen and mouse capture, undone on drop whatever ends the session.
struct Screen;
impl Screen {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode().map_err(|e| format!("Cannot use the terminal: {e}"))?;
        let screen = Self;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| format!("Cannot use the terminal: {e}"))?;
        Ok(screen)
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        restore(&mut io::stdout());
    }
}
fn restore(out: &mut impl Write) {
    let _ = execute!(out, DisableMouseCapture, LeaveAlternateScreen, Show);
    let _ = terminal::disable_raw_mode();
}

/// crossterm 0.29 reads a closed input terminal for ever at full CPU, so on Unix the editor
/// ends through the hangup signal instead: its input must be the terminal that controls its
/// session, which signals the editor when it closes, and the signal ends the editor even when
/// it was set to be ignored (as `nohup` does). A thread restores the terminal, in case it is
/// still open, then exits; holding stdout keeps the editor from drawing after the restore.
#[cfg(unix)]
fn end_on_hangup() -> Result<()> {
    let controlling = matches!(
        (rustix::termios::tcgetsid(io::stdin()), rustix::process::getsid(None)),
        (Ok(terminal), Ok(own)) if terminal == own
    );
    if !controlling {
        return Err("Editing needs input from the terminal that controls this session".into());
    }
    let mut hangups = signal_hook::iterator::Signals::new([signal_hook::consts::SIGHUP])
        .map_err(|e| format!("Cannot use the terminal: {e}"))?;
    std::thread::spawn(move || {
        if hangups.forever().next().is_some() {
            restore(&mut io::stdout().lock());
            std::process::exit(1);
        }
    });
    Ok(())
}

pub fn run(path: PathBuf, project: Animation) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("Editing needs a terminal for input and output".into());
    }
    #[cfg(unix)]
    end_on_hangup()?;
    let mut editor = Editor::new(path, project);
    let _screen = Screen::enter()?;
    let failed = |e: io::Error| format!("Terminal failed: {e}");
    let mut out = io::stdout();
    loop {
        let (columns, rows) = terminal::size().map_err(failed)?;
        editor.draw(&mut out, columns, rows).map_err(failed)?;
        match editor.handle(event::read().map_err(failed)?) {
            Flow::Continue => {}
            Flow::Quit => return Ok(()),
            Flow::Play => {
                let shown = editor.frame;
                editor.message = "Playing; any key stops".into();
                // Loop through the frames until a key is pressed; mouse and resize events only
                // redraw, and each frame keeps its full duration.
                'playing: for index in (0..editor.project.frames().len()).cycle().skip(shown) {
                    editor.frame = index;
                    let (columns, rows) = terminal::size().map_err(failed)?;
                    editor.draw(&mut out, columns, rows).map_err(failed)?;
                    let until = Instant::now()
                        + Duration::from_millis(u64::from(editor.current().duration_ms));
                    loop {
                        let left = until.saturating_duration_since(Instant::now());
                        if left.is_zero() || !event::poll(left).map_err(failed)? {
                            break;
                        }
                        if let Event::Key(key) = event::read().map_err(failed)?
                            && key.kind != KeyEventKind::Release
                        {
                            break 'playing;
                        }
                    }
                }
                editor.frame = shown;
                editor.message.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests;
