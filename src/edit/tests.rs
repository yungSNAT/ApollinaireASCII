// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use super::*;

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn ctrl(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}
fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}
fn opened(name: &str, text: &str) -> Editor {
    let mut editor = Editor::new(
        PathBuf::from(name),
        Animation::still(Frame::from_text(text).unwrap()),
    );
    // Mouse events map through the last drawn view.
    editor.draw(&mut Vec::new(), 80, 24).unwrap();
    editor
}
fn editor(text: &str) -> Editor {
    opened("art.json", text)
}
fn typed(editor: &mut Editor, text: &str) {
    for c in text.chars() {
        assert_eq!(editor.handle(press(KeyCode::Char(c))), Flow::Continue);
    }
}
fn shown(editor: &Editor) -> String {
    editor.current().frame.text()
}
fn texts(editor: &Editor) -> Vec<String> {
    let frames = editor.project.frames();
    frames.iter().map(|f| f.frame.text()).collect()
}
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("apollinaire-edit-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn navigation_and_ignored_commands_preserve_the_artwork_and_history() {
    let mut editor = editor("abc\ndef");
    typed(&mut editor, "x");
    let project = editor.project.clone();
    let history = (editor.undo.clone(), editor.redo.clone());
    for key in [
        press(KeyCode::Right),
        press(KeyCode::Down),
        press(KeyCode::Up),
        press(KeyCode::Tab),
        ctrl('a'),
    ] {
        assert_eq!(editor.handle(key), Flow::Continue);
    }
    assert_eq!(editor.cursor, (2, 0));
    assert_eq!(editor.project, project);
    assert_eq!((editor.undo.clone(), editor.redo.clone()), history);
}

#[test]
fn rendering_stops_at_the_first_failed_write() {
    struct FailingWriter {
        remaining: usize,
        failed: bool,
        writes_after_failure: usize,
        flushed: bool,
        bytes: Vec<u8>,
    }
    impl Write for FailingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.failed {
                self.writes_after_failure += 1;
            }
            if self.remaining == 0 {
                self.failed = true;
                return Err(io::Error::other("fixture write failed"));
            }
            let written = bytes.len().min(self.remaining);
            self.bytes.extend_from_slice(&bytes[..written]);
            self.remaining -= written;
            Ok(written)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flushed = true;
            Ok(())
        }
    }

    let mut editor = editor("unique artwork");
    typed(&mut editor, "X");
    let project = editor.project.clone();
    let history = (editor.undo.clone(), editor.redo.clone());
    let mut complete = Vec::new();
    editor.draw(&mut complete, 80, 24).unwrap();
    let complete = String::from_utf8(complete).unwrap();
    // Fail in artwork, status and the final cursor command, using the real drawing output.
    for limit in [
        complete.find("Xnique artwork").unwrap() + 2,
        complete.find("art.json (modified)").unwrap() + 2,
        complete.rfind("\u{1b}[1;2H").unwrap() + 2,
    ] {
        let mut writer = FailingWriter {
            remaining: limit,
            failed: false,
            writes_after_failure: 0,
            flushed: false,
            bytes: Vec::new(),
        };
        let error = editor.draw(&mut writer, 80, 24).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.to_string(), "fixture write failed");
        assert_eq!(writer.bytes, complete.as_bytes()[..limit]);
        assert!(writer.failed);
        assert_eq!(writer.writes_after_failure, 0);
        assert!(!writer.flushed);
        assert_eq!(editor.project, project);
        assert_eq!((editor.undo.clone(), editor.redo.clone()), history);
    }
}

#[test]
fn saving_through_an_ordinary_file_keeps_the_parent_and_editor_unchanged() {
    let dir = scratch("save-file-parent");
    let parent = dir.join("parent");
    std::fs::write(&parent, "keep parent").unwrap();
    let mut editor = editor("old");
    editor.path = parent.join("art.txt");
    typed(&mut editor, "n");
    let project = editor.project.clone();
    let history = (editor.undo.clone(), editor.redo.clone());
    editor.handle(ctrl('s'));
    assert!(
        editor.message.starts_with("Cannot save"),
        "{}",
        editor.message
    );
    assert!(editor.modified);
    assert_eq!(editor.project, project);
    assert_eq!((editor.undo.clone(), editor.redo.clone()), history);
    assert_eq!(std::fs::read_to_string(&parent).unwrap(), "keep parent");
    assert_eq!(names(&dir), ["parent"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_destination_replaced_with_a_directory_survives_a_failed_save() {
    let dir = scratch("save-replaced-destination");
    let path = dir.join("art.txt");
    std::fs::write(&path, "old\n").unwrap();
    let mut editor = Editor::new(
        path.clone(),
        Animation::still(Frame::from_text("old").unwrap()),
    );
    typed(&mut editor, "n");
    let project = editor.project.clone();
    let history = (editor.undo.clone(), editor.redo.clone());
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("keep"), "keep child").unwrap();
    editor.handle(ctrl('s'));
    assert!(
        editor.message.starts_with("Cannot save"),
        "{}",
        editor.message
    );
    assert!(editor.modified);
    assert_eq!(editor.project, project);
    assert_eq!((editor.undo.clone(), editor.redo.clone()), history);
    assert_eq!(
        std::fs::read_to_string(path.join("keep")).unwrap(),
        "keep child"
    );
    assert_eq!(names(&path), ["keep"]);
    assert_eq!(names(&dir), ["art.txt"], "the temporary file is removed");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_rejected_canvas_operation_discards_its_partial_drawing() {
    let mut editor = editor("old");
    let project = editor.project.clone();
    // Exercise the drawing helper's transaction boundary with a real validation failure.
    editor.draw_with(Some(Run::Typing), |canvas| {
        canvas.set(0, 0, b'x')?;
        canvas.set(1, 0, b'\n')
    });
    assert_eq!(
        editor.message,
        "Only printable ASCII is allowed: byte 0 is 0x0a"
    );
    assert_eq!(editor.project, project);
    assert!(editor.undo.is_empty());
    assert!(editor.redo.is_empty());
    assert!(!editor.modified);
    assert!(editor.joining.is_none());
}

#[test]
fn a_drawing_with_incompatible_frame_dimensions_keeps_the_project_and_history() {
    let mut editor = editor("old");
    editor.handle(ctrl('d'));
    assert_eq!(editor.project.frames().len(), 2);
    let project = editor.project.clone();
    let history = (editor.undo.clone(), editor.redo.clone());
    let modified = editor.modified;
    let selected = editor.frame;
    // The mutable drawing surface accepts valid canvases; the project must still reject
    // one whose dimensions differ from its other frames. Current keys do not resize it.
    editor.draw_with(None, |canvas| {
        *canvas = Canvas::new(2, 1)?;
        canvas.set(0, 0, b'x')
    });
    assert_eq!(editor.message, "Every frame must have the same dimensions");
    assert_eq!(editor.project, project);
    assert_eq!((editor.undo.clone(), editor.redo.clone()), history);
    assert_eq!(editor.modified, modified);
    assert_eq!(editor.frame, selected);
}

#[test]
fn history_maintenance_trims_redo_states_created_by_undoing_frame_duplication() {
    let mut editor = Editor::new(
        PathBuf::from("history.json"),
        Animation::still(Frame::blank(512, 256).unwrap()),
    );
    for _ in 0..15 {
        editor.handle(ctrl('d'));
    }
    let cells = |states: &[(Animation, usize)]| {
        states
            .iter()
            .map(|(project, _)| project.width() * project.height() * project.frames().len())
            .sum::<usize>()
    };
    assert_eq!(cells(&editor.undo), 15_728_640);
    assert!(cells(&editor.undo) < HISTORY_CELLS);
    for _ in 0..15 {
        editor.handle(ctrl('z'));
    }
    assert!(editor.undo.is_empty());
    assert_eq!(cells(&editor.redo), 17_694_720);
    let project = editor.project.clone();
    // Undo retains larger projects than those it removes. Test the maintenance helper
    // against that event-produced history, without constructing or changing stack entries.
    editor.limit_history();
    assert_eq!(editor.project, project);
    assert!(editor.undo.is_empty());
    assert_eq!(cells(&editor.redo), 15_597_568);
    assert!(cells(&editor.redo) <= HISTORY_CELLS);
    assert_eq!(
        editor
            .redo
            .iter()
            .map(|(project, _)| project.frames().len())
            .collect::<Vec<_>>(),
        (2..=15).rev().collect::<Vec<_>>()
    );
    editor.handle(ctrl('y'));
    assert_eq!(editor.project.frames().len(), 2);
    assert_eq!(editor.frame, 1);
}

#[test]
fn typing_places_characters_and_undoes_as_one_step() {
    let mut editor = editor("     \n     ");
    typed(&mut editor, "Hi!");
    assert_eq!(shown(&editor), "Hi!  \n     ");
    assert_eq!(editor.cursor, (3, 0));
    editor.handle(press(KeyCode::Enter));
    typed(&mut editor, "ok");
    editor.handle(press(KeyCode::Enter));
    assert_eq!(editor.message, "This is the bottom row");
    typed(&mut editor, "?");
    assert_eq!(shown(&editor), "Hi!  \nok?  ");
    editor.handle(press(KeyCode::Backspace));
    assert_eq!(shown(&editor), "Hi!  \nok   ");
    // Typing stops at the last column, overwriting it.
    editor.handle(press(KeyCode::End));
    typed(&mut editor, "ab");
    assert_eq!(shown(&editor), "Hi!  \nok  b");
    editor.handle(press(KeyCode::Delete));
    assert_eq!(shown(&editor), "Hi!  \nok   ");

    for expected in [
        "Hi!  \nok  b",
        "Hi!  \nok   ",
        "Hi!  \nok?  ",
        "Hi!  \nok   ",
        "Hi!  \n     ",
        "     \n     ",
    ] {
        editor.handle(ctrl('z'));
        assert_eq!(
            shown(&editor),
            expected,
            "typing after any other key is a new step"
        );
    }
    editor.handle(ctrl('z'));
    assert_eq!(editor.message, "Nothing to undo");
    editor.handle(ctrl('y'));
    assert_eq!(shown(&editor), "Hi!  \n     ");
    typed(&mut editor, "x");
    editor.handle(ctrl('y'));
    assert_eq!(editor.message, "Nothing to redo", "an edit clears redo");
}

#[test]
fn only_printable_ascii_is_typed_and_key_releases_are_ignored() {
    let mut editor = editor("   ");
    editor.handle(press(KeyCode::Char('é')));
    assert_eq!(
        editor.message,
        "Only printable ASCII characters can be typed"
    );
    // Windows reports releases as well as presses; only presses and repeats act.
    let mut release = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    editor.handle(Event::Key(release));
    let mut repeat = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    repeat.kind = KeyEventKind::Repeat;
    editor.handle(Event::Key(repeat));
    assert_eq!(shown(&editor), "y  ");
    assert_eq!(editor.undo.len(), 1);
}

#[test]
fn the_brush_is_chosen_with_ctrl_b_or_f3_and_a_mouse_stroke_is_one_step() {
    let mut editor = editor(&["      "; 3].join("\n"));
    editor.handle(ctrl('b'));
    editor.handle(press(KeyCode::Tab));
    assert_eq!(
        editor.message,
        "The brush must be a printable ASCII character"
    );
    editor.handle(press(KeyCode::F(3)));
    assert_eq!(editor.message, "Type the brush character");
    editor.handle(press(KeyCode::Char('*')));
    assert_eq!(editor.message, "Brush: *");
    assert_eq!(
        shown(&editor),
        "      \n      \n      ",
        "choosing draws nothing"
    );

    editor.handle(mouse(MouseEventKind::Down(MouseButton::Left), 0, 0));
    editor.handle(mouse(MouseEventKind::Drag(MouseButton::Left), 5, 0));
    editor.handle(mouse(MouseEventKind::Drag(MouseButton::Left), 5, 2));
    // Outside the frame, and on the status lines, nothing is drawn.
    editor.handle(mouse(MouseEventKind::Drag(MouseButton::Left), 40, 2));
    editor.handle(mouse(MouseEventKind::Down(MouseButton::Left), 1, 22));
    editor.handle(mouse(MouseEventKind::Up(MouseButton::Left), 5, 2));
    assert_eq!(shown(&editor), "******\n     *\n     *");
    assert_eq!(editor.cursor, (5, 2));
    editor.handle(mouse(MouseEventKind::Drag(MouseButton::Left), 0, 2));
    assert_eq!(
        shown(&editor),
        "******\n     *\n     *",
        "no stroke after release"
    );
    editor.handle(ctrl('b'));
    editor.handle(press(KeyCode::Esc));
    assert_eq!(editor.message, "Brush unchanged");
    editor.handle(ctrl('z'));
    assert_eq!(shown(&editor), "      \n      \n      ");
}

#[test]
fn frames_are_added_duplicated_deleted_browsed_and_played() {
    let mut editor = editor("ab");
    assert_eq!(editor.handle(ctrl('p')), Flow::Continue);
    assert_eq!(editor.message, "Add a frame to play an animation");
    editor.handle(ctrl('d'));
    assert_eq!(editor.frame, 1);
    typed(&mut editor, "c");
    editor.handle(ctrl('n'));
    assert_eq!(texts(&editor), ["ab", "cb", "  "]);
    assert_eq!(editor.frame, 2);
    editor.handle(press(KeyCode::PageDown));
    assert_eq!(editor.message, "No frame in that direction");
    editor.handle(press(KeyCode::PageUp));
    assert_eq!(editor.frame, 1);
    assert_eq!(editor.handle(ctrl('p')), Flow::Play);
    editor.handle(ctrl('k'));
    editor.handle(ctrl('k'));
    assert_eq!(texts(&editor), ["ab"]);
    editor.handle(ctrl('k'));
    assert_eq!(editor.message, "Keep at least one frame");
    editor.handle(ctrl('z'));
    assert_eq!(texts(&editor), ["ab", "  "]);
}

#[test]
fn a_text_file_never_gains_a_second_frame() {
    let mut editor = opened("art.txt", "ab");
    for key in ['n', 'd'] {
        editor.handle(ctrl(key));
        assert_eq!(editor.message, TEXT_HOLDS_ONE_FRAME);
    }
    assert_eq!(texts(&editor), ["ab"]);
    assert!(!editor.modified);
}

#[test]
fn quitting_with_unsaved_changes_takes_a_second_quit() {
    let mut editor = editor("a");
    assert_eq!(editor.handle(ctrl('q')), Flow::Quit);
    typed(&mut editor, "b");
    assert_eq!(editor.handle(ctrl('q')), Flow::Continue);
    assert!(editor.message.starts_with("Unsaved changes"));
    editor.handle(press(KeyCode::Left));
    assert_eq!(
        editor.handle(press(KeyCode::F(10))),
        Flow::Continue,
        "a key in between resets the warning"
    );
    assert_eq!(editor.handle(ctrl('Q')), Flow::Quit);
}

#[test]
fn saving_replaces_the_file_and_failures_leave_it_as_it_was() {
    let dir = scratch("save");
    let text_path = dir.join("zihuatanejo.txt");
    std::fs::write(&text_path, "old\n").unwrap();
    let mut editor = Editor::new(
        text_path.clone(),
        Animation::still(Frame::from_text("old").unwrap()),
    );
    typed(&mut editor, "n");
    editor.handle(press(KeyCode::F(2)));
    assert_eq!(editor.message, format!("Saved {}", text_path.display()));
    assert!(!editor.modified);
    assert_eq!(std::fs::read_to_string(&text_path).unwrap(), "nld\n");

    // Opened as text but holding frames from elsewhere, the project cannot be saved as text.
    let mut project = editor.project.clone();
    project.duplicate_frames(0..1).unwrap();
    editor.project = project;
    editor.handle(ctrl('s'));
    assert_eq!(editor.message, TEXT_HOLDS_ONE_FRAME);
    assert_eq!(std::fs::read_to_string(&text_path).unwrap(), "nld\n");

    let json_path = dir.join("art.json");
    editor.path = json_path.clone();
    editor.save().unwrap();
    let saved = std::fs::read_to_string(&json_path).unwrap();
    assert_eq!(Animation::from_json(&saved).unwrap(), editor.project);
    assert!(saved.ends_with('\n'));

    editor.path = dir.join("missing").join("art.json");
    let error = editor.save().unwrap_err();
    assert!(error.starts_with("Cannot save"), "{error}");
    assert_eq!(
        names(&dir),
        ["art.json", "zihuatanejo.txt"],
        "no temporary file is left"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn saving_follows_a_link_and_keeps_the_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("link");
    let target = dir.join("real.txt");
    std::fs::write(&target, "a\n").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
    let link = dir.join("link.txt");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let mut editor = Editor::new(
        link.clone(),
        Animation::still(Frame::from_text("b").unwrap()),
    );
    editor.save().unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "b\n");
    let mode = std::fs::metadata(&target).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o640);
    assert_eq!(names(&dir), ["link.txt", "real.txt"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(windows)]
#[test]
fn a_save_refused_by_windows_leaves_the_file_unchanged() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = scratch("locked");
    let path = dir.join("art.txt");
    std::fs::write(&path, "a\n").unwrap();
    // Another program holding the file open without sharing it.
    let held = File::options()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    let mut editor = Editor::new(
        path.clone(),
        Animation::still(Frame::from_text("b").unwrap()),
    );
    let error = editor.save().unwrap_err();
    assert!(error.starts_with("Cannot save"), "{error}");
    drop(held);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\n");
    assert_eq!(names(&dir), ["art.txt"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_view_scrolls_to_the_cursor_and_the_status_lines_report_state() {
    let wide = (0..30)
        .map(|row| format!("{row:<3}{}", "-".repeat(97)))
        .collect::<Vec<_>>()
        .join("\n");
    let mut editor = editor(&wide);
    let mut screen = Vec::new();
    editor.draw(&mut screen, 60, 10).unwrap();
    let screen = String::from_utf8(screen).unwrap();
    assert!(screen.contains(&format!("0  {}", "-".repeat(57))));
    assert!(screen.contains("7  -"));
    assert!(
        !screen.contains("8  -"),
        "only 8 rows fit above the status lines"
    );
    assert!(screen.contains("art.json  frame 1/1  column 1 row 1  brush #"));

    editor.handle(press(KeyCode::End));
    for _ in 0..29 {
        editor.handle(press(KeyCode::Down));
    }
    let mut screen = Vec::new();
    editor.draw(&mut screen, 60, 10).unwrap();
    let screen = String::from_utf8(screen).unwrap();
    assert_eq!(editor.scroll, (40, 22));
    assert!(screen.contains("column 100 row 30"));
    // The cursor sits on the last visible column of the last frame row.
    assert!(screen.contains("\u{1b}[8;60H"), "{screen:?}");

    // A click maps through the scrolled view.
    editor.handle(mouse(MouseEventKind::Down(MouseButton::Left), 0, 0));
    assert_eq!(editor.cursor, (40, 22));

    editor.handle(press(KeyCode::Home));
    editor.handle(press(KeyCode::Up));
    let mut back = Vec::new();
    editor.draw(&mut back, 60, 10).unwrap();
    assert_eq!(editor.cursor, (0, 21));
    assert_eq!(editor.scroll, (0, 21));
    assert!(String::from_utf8(back).unwrap().contains("\u{1b}[1;1H"));

    let mut small = Vec::new();
    editor.draw(&mut small, 19, 10).unwrap();
    assert!(
        String::from_utf8(small)
            .unwrap()
            .contains("Enlarge the terminal")
    );
    editor.handle(mouse(MouseEventKind::Down(MouseButton::Left), 1, 1));
    assert_eq!(
        editor.cursor,
        (0, 21),
        "no frame is shown, so a click does nothing"
    );

    editor.handle(press(KeyCode::F(1)));
    let mut help = Vec::new();
    editor.draw(&mut help, 100, 30).unwrap();
    assert!(
        String::from_utf8(help)
            .unwrap()
            .contains("Ctrl+S or F2: save")
    );
    let steps = editor.undo.len();
    typed(&mut editor, "x");
    editor.handle(mouse(MouseEventKind::Down(MouseButton::Left), 1, 1));
    assert!(editor.help, "keys and clicks do not edit behind the help");
    assert_eq!(editor.undo.len(), steps);
    editor.handle(press(KeyCode::Esc));
    assert!(!editor.help);
}

#[test]
fn history_keeps_the_latest_100_states() {
    let mut editor = editor(" ");
    for n in 0..120u8 {
        typed(&mut editor, &char::from(b'a' + n % 26).to_string());
        editor.handle(press(KeyCode::Home));
    }
    assert_eq!(editor.undo.len(), HISTORY_STATES);
    while !editor.undo.is_empty() {
        editor.handle(ctrl('z'));
    }
    // States after edits 1 to 20 were dropped; the oldest kept is the one after edit 20.
    assert_eq!(shown(&editor), "t");
}

#[test]
fn only_text_and_json_paths_are_edited() {
    for good in ["a.txt", "dir/a.json"] {
        assert!(check_path(Path::new(good)).is_ok(), "{good}");
    }
    for bad in ["-", "a.html", "a"] {
        assert_eq!(
            check_path(Path::new(bad)).unwrap_err(),
            "Edit needs a .txt or .json file"
        );
    }
}

#[test]
fn an_edit_that_changes_nothing_does_not_join_the_next_edit_to_the_last_step() {
    let mut editor = editor("a ");
    typed(&mut editor, "x");
    editor.handle(press(KeyCode::Home));
    // Typing the character already there changes nothing.
    typed(&mut editor, "xy");
    assert_eq!(shown(&editor), "xy");
    editor.handle(ctrl('z'));
    assert_eq!(shown(&editor), "x ");

    // The same for a mouse stroke that starts on a cell already holding the brush.
    let mut stroke = opened("art.json", "#  ");
    stroke.handle(press(KeyCode::End));
    typed(&mut stroke, "z");
    stroke.handle(mouse(MouseEventKind::Down(MouseButton::Left), 0, 0));
    stroke.handle(mouse(MouseEventKind::Drag(MouseButton::Left), 1, 0));
    assert_eq!(shown(&stroke), "##z");
    stroke.handle(ctrl('z'));
    assert_eq!(shown(&stroke), "# z");
    stroke.handle(ctrl('y'));
    assert_eq!(shown(&stroke), "##z");
}

#[test]
fn drawing_after_a_quit_warning_needs_a_new_warning() {
    let mut editor = editor("   ");
    typed(&mut editor, "a");
    assert_eq!(editor.handle(ctrl('q')), Flow::Continue);
    editor.handle(mouse(MouseEventKind::Down(MouseButton::Left), 2, 0));
    assert_eq!(editor.handle(ctrl('q')), Flow::Continue);
    assert!(editor.message.starts_with("Unsaved changes"));
}

#[test]
fn altgr_characters_are_typed_not_taken_as_commands() {
    // Windows reports AltGr as Ctrl+Alt, which several layouts need for @, [, \ and ~.
    let mut editor = editor("   ");
    for c in ['@', '~', 'q'] {
        editor.handle(Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        )));
    }
    assert_eq!(shown(&editor), "@~q");
}

#[test]
fn control_characters_in_names_and_messages_are_not_sent_to_the_terminal() {
    let mut editor = opened("a\u{1b}[31mb.json", "x");
    editor.message = "Cannot save \u{1b}]0;title\u{7}".into();
    let mut screen = Vec::new();
    editor.draw(&mut screen, 80, 24).unwrap();
    let screen = String::from_utf8(screen).unwrap();
    assert!(screen.contains("a?[31mb.json"), "{screen:?}");
    assert!(screen.contains("Cannot save ?]0;title?"), "{screen:?}");
    assert!(!screen.contains('\u{7}'));
}

#[test]
fn a_file_name_near_the_length_limit_still_saves() {
    let dir = scratch("long");
    let path = dir.join(format!("{}.txt", "a".repeat(240)));
    let mut editor = Editor::new(
        path.clone(),
        Animation::still(Frame::from_text("b").unwrap()),
    );
    editor.save().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "b\n");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_file_with_the_temporary_name_is_kept_and_the_save_still_succeeds() {
    let dir = scratch("taken");
    let taken = dir.join(format!(".apollinaire-{}.tmp", std::process::id()));
    std::fs::write(&taken, "someone else's").unwrap();
    let path = dir.join("art.txt");
    let mut editor = Editor::new(
        path.clone(),
        Animation::still(Frame::from_text("b").unwrap()),
    );
    editor.save().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "b\n");
    assert_eq!(std::fs::read_to_string(&taken).unwrap(), "someone else's");
    assert_eq!(names(&dir).len(), 2, "{:?}", names(&dir));
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_link_to_a_missing_file_is_not_replaced() {
    let dir = scratch("dangling");
    let link = dir.join("link.txt");
    std::os::unix::fs::symlink(dir.join("missing.txt"), &link).unwrap();
    let mut editor = Editor::new(
        link.clone(),
        Animation::still(Frame::from_text("b").unwrap()),
    );
    let error = editor.save().unwrap_err();
    assert!(error.contains("a link to a missing file"), "{error}");
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(names(&dir), ["link.txt"]);
    std::fs::remove_dir_all(dir).unwrap();
}
