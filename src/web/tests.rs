// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Headless-browser tests of the editor. The body of `web/index.html` and `web/style.css` are
//! embedded once and the editor started; tests then drive the page only through DOM events and
//! controls. Run them with `cargo test-editor` (see `.cargo/config.toml`).

use super::{document, now};
use std::cell::RefCell;
use std::sync::Once;
use wasm_bindgen::{JsCast, prelude::Closure};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{
    ClipboardEvent, ClipboardEventInit, DataTransfer, Element, HtmlAnchorElement, HtmlElement,
    HtmlInputElement, HtmlProgressElement, HtmlSelectElement, HtmlTextAreaElement, KeyboardEvent,
    KeyboardEventInit, PointerEvent, PointerEventInit,
};

use crate::gif::common as gif_writer;

wasm_bindgen_test_configure!(run_in_browser);

const PAGE: &str = include_str!("../../web/index.html");
const STYLE: &str = include_str!("../../web/style.css");
/// A 3-second, 32 x 24 pixel, 25 fps grey VP8 clip from FFmpeg's `color` source.
const CLIP: &[u8] = include_bytes!("../../tests/fixtures/video/grey-3s.webm");

fn element<T: JsCast>(id: &str) -> T {
    document()
        .get_element_by_id(id)
        .unwrap_or_else(|| panic!("missing #{id}"))
        .dyn_into()
        .unwrap_or_else(|_| panic!("#{id} has the wrong type"))
}
fn set_value(id: &str, value: &str) {
    element::<HtmlInputElement>(id).set_value(value);
}
fn click(id: &str) {
    let button = element::<HtmlElement>(id);
    assert!(!button.has_attribute("disabled"), "#{id} is disabled");
    button.click();
}
/// Click a checkbox only when it must change, so its change listener runs as for a user.
fn check(id: &str, on: bool) {
    let input = element::<HtmlInputElement>(id);
    if input.checked() != on {
        input.click();
    }
}
fn tool(name: &str) {
    let input: HtmlInputElement = document()
        .query_selector(&format!("input[name=tool][value={name}]"))
        .unwrap()
        .unwrap_or_else(|| panic!("missing tool {name}"))
        .dyn_into()
        .unwrap();
    input.click();
    assert!(input.checked());
}
fn status() -> String {
    element::<Element>("status")
        .text_content()
        .unwrap_or_default()
}
fn rows(lines: &[&str]) -> String {
    lines.join("\n")
}

/// The artwork text, checked against the ASCII-only contract on every read: printable bytes
/// 32..=126 in equal-width rows separated by LF.
fn art() -> String {
    let text = element::<Element>("art").text_content().unwrap_or_default();
    assert!(
        text.bytes().all(|b| b == b'\n' || (32..=126).contains(&b)),
        "#art holds a byte outside printable ASCII: {text:?}"
    );
    let widths: Vec<usize> = text.split('\n').map(str::len).collect();
    assert!(
        widths.iter().all(|w| *w == widths[0]),
        "#art rows differ in width: {widths:?}"
    );
    text
}
fn grid_size() -> (usize, usize) {
    let text = art();
    (
        text.split('\n').next().unwrap().len(),
        text.split('\n').count(),
    )
}

/// Load the page once, start the editor, then reset to a blank canvas with default tool
/// options. History and the area clipboard carry over between tests, as in one session.
fn blank(width: usize, height: usize) {
    static LOAD: Once = Once::new();
    LOAD.call_once(|| {
        let body = PAGE
            .split_once("<body>")
            .and_then(|(_, rest)| rest.rsplit_once("</body>"))
            .expect("index.html has a body")
            .0;
        let host = document().create_element("div").unwrap();
        // Scripts inserted as HTML do not run, so only `start` loads the editor.
        host.set_inner_html(&format!("<style>{STYLE}</style>{body}"));
        document().body().unwrap().append_child(&host).unwrap();
        super::start().expect("editor starts");
    });
    tool("select-text");
    check("diagram", false);
    check("arrow", true);
    check("onion-skin", false);
    check("mirror-glyphs", false);
    check("stamp-transparent", true);
    check("constrain", false);
    check("border", false);
    element::<HtmlSelectElement>("border-style").set_value("plain");
    set_value("brush", "#");
    set_value("stamp", "");
    element::<HtmlSelectElement>("mode").set_value("blank");
    set_value("width", &width.to_string());
    set_value("height", &height.to_string());
    click("generate");
    assert_eq!(art(), rows(&vec![" ".repeat(width).as_str(); height]));
}

fn pointer(kind: &str, (x, y): (usize, usize)) {
    let (columns, lines) = grid_size();
    let rect = element::<Element>("art").get_bounding_client_rect();
    let init = PointerEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_pointer_id(1);
    init.set_pointer_type("mouse");
    init.set_is_primary(true);
    init.set_button(if kind == "pointermove" { -1 } else { 0 });
    // Aim at the centre of the cell.
    init.set_client_x(
        (rect.left() + (x as f64 + 0.5) * rect.width() / columns as f64).round() as i32,
    );
    init.set_client_y(
        (rect.top() + (y as f64 + 0.5) * rect.height() / lines as f64).round() as i32,
    );
    let event = PointerEvent::new_with_event_init_dict(kind, &init).unwrap();
    element::<Element>("art").dispatch_event(&event).unwrap();
}
/// Press at the first cell, move through the others and release at the last.
fn drag(path: &[(usize, usize)]) {
    pointer("pointerdown", path[0]);
    for point in &path[1..] {
        pointer("pointermove", *point);
    }
    pointer("pointerup", *path.last().unwrap());
}
fn key_with(key: &str, ctrl: bool, shift: bool) -> bool {
    let init = KeyboardEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_key(key);
    init.set_ctrl_key(ctrl);
    init.set_shift_key(shift);
    let event = KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap();
    let art = element::<HtmlElement>("art");
    art.focus().unwrap();
    art.dispatch_event(&event).unwrap();
    event.default_prevented()
}
fn keys(sequence: &[&str]) {
    for key in sequence {
        key_with(key, false, false);
    }
}
fn repeat(key: &str, times: usize) {
    for _ in 0..times {
        key_with(key, false, false);
    }
}
/// Move the keyboard cursor to a cell from the top-left corner.
fn cursor_to((x, y): (usize, usize)) {
    let (columns, lines) = grid_size();
    repeat("ArrowUp", lines);
    repeat("ArrowLeft", columns);
    repeat("ArrowDown", y);
    repeat("ArrowRight", x);
    assert_eq!(
        element::<Element>("cursor-position")
            .text_content()
            .unwrap(),
        format!("Column {}, row {}", x + 1, y + 1)
    );
}
/// Dispatch a clipboard event on the artwork and return the text it left on the clipboard.
fn clipboard(kind: &str, text: &str) -> String {
    let data = DataTransfer::new().unwrap();
    data.set_data("text/plain", text).unwrap();
    let init = ClipboardEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_clipboard_data(Some(&data));
    let event = ClipboardEvent::new_with_event_init_dict(kind, &init).unwrap();
    element::<Element>("art").dispatch_event(&event).unwrap();
    data.get_data("text/plain").unwrap()
}

#[wasm_bindgen_test]
fn generate_draws_the_chosen_border_style() {
    blank(2, 1);
    check("border", true);
    for (style, expected) in [
        ("plain", ["+--+", "|  |", "+--+"]),
        ("rounded", [".--.", "|  |", "'--'"]),
        ("dotted", ["....", ":  :", ":..:"]),
        ("heavy", ["#==#", "#  #", "#==#"]),
    ] {
        element::<HtmlSelectElement>("border-style").set_value(style);
        click("generate");
        assert_eq!(art(), rows(&expected), "{style}");
    }
}

#[wasm_bindgen_test]
fn border_style_is_disabled_whilst_the_border_is_off() {
    blank(2, 1);
    let style = element::<HtmlSelectElement>("border-style");
    assert!(style.disabled(), "enabled without a border");
    check("border", true);
    assert!(!style.disabled(), "disabled with a border");
    style.set_value("rounded");
    check("border", false);
    assert!(style.disabled(), "enabled after the border was turned off");
    assert_eq!(style.value(), "rounded");
    check("border", true);
    click("generate");
    assert_eq!(art(), rows(&[".--.", "|  |", "'--'"]));
}

#[wasm_bindgen_test]
fn pencil_draws_by_pointer_and_keyboard() {
    blank(8, 4);
    tool("pencil");
    drag(&[(1, 0), (4, 0), (4, 2)]);
    assert_eq!(
        art(),
        rows(&[" ####   ", "    #   ", "    #   ", "        "])
    );
    set_value("brush", "*");
    cursor_to((0, 3));
    keys(&[" ", "ArrowRight", "ArrowRight", " "]);
    assert_eq!(
        art(),
        rows(&[" ####   ", "    #   ", "    #   ", "***     "])
    );
}

#[wasm_bindgen_test]
fn saved_art_stamps_preview_cancel_clip_and_commit_as_one_undo_step() {
    blank(8, 4);
    tool("type");
    drag(&[(0, 0)]);
    keys(&["A", " ", "B", "Enter", " ", "C", " "]);
    let original = rows(&["A B     ", " C      ", "        ", "        "]);
    assert_eq!(art(), original);
    tool("area");
    drag(&[(0, 0), (2, 1)]);
    click("save-stamp");
    let picker = element::<HtmlSelectElement>("art-stamp");
    let count = picker.child_element_count();
    click("save-stamp");
    assert_eq!(
        picker.child_element_count(),
        count,
        "same grid is not saved twice"
    );
    tool("stamp-art");
    pointer("pointerdown", (3, 1));
    assert_eq!(
        art(),
        rows(&["A B     ", " C A B  ", "    C   ", "        "])
    );
    pointer("pointermove", (5, 2));
    let placed = rows(&["A B     ", " C      ", "     A B", "      C "]);
    assert_eq!(art(), placed, "moving a preview leaves no trail");
    pointer("pointercancel", (5, 2));
    assert_eq!(art(), original);
    drag(&[(5, 2)]);
    assert_eq!(art(), placed);
    click("undo");
    assert_eq!(art(), original);
    click("redo");
    assert_eq!(art(), placed);
    // A stamp survives frame and project changes, but is not part of their history.
    blank(4, 2);
    tool("fill");
    drag(&[(0, 0)]);
    tool("stamp-art");
    cursor_to((1, 0));
    keys(&[" ", "Enter"]);
    assert_eq!(art(), rows(&["#A#B", "##C#"]));
    click("undo");
    check("stamp-transparent", false);
    cursor_to((1, 0));
    keys(&[" ", "Enter"]);
    assert_eq!(art(), rows(&["#A B", "# C "]));
    click("undo");
    cursor_to((3, 1));
    keys(&[" ", "Enter"]);
    assert_eq!(
        art(),
        rows(&["####", "###A"]),
        "edge placement clips safely"
    );
    click("undo");
    cursor_to((0, 0));
    keys(&[" ", "ArrowRight", "Escape"]);
    assert_eq!(art(), rows(&["####", "####"]));
    click("delete-stamp");
}

#[wasm_bindgen_test]
fn saved_art_stamp_library_is_bounded_and_removal_keeps_artwork() {
    blank(1, 1);
    // Each test starts with no saved entries, without inspecting application state.
    while !element::<Element>("delete-stamp").has_attribute("disabled") {
        click("delete-stamp");
    }
    for i in 0..16u8 {
        tool("type");
        cursor_to((0, 0));
        keys(&[&(char::from(b'A' + i)).to_string()]);
        tool("area");
        drag(&[(0, 0)]);
        click("save-stamp");
    }
    assert_eq!(element::<Element>("art-stamp").child_element_count(), 16);
    tool("type");
    cursor_to((0, 0));
    keys(&["Z"]);
    tool("area");
    drag(&[(0, 0)]);
    click("save-stamp");
    assert!(status().contains("limited to 16"));
    assert_eq!(element::<Element>("art-stamp").child_element_count(), 16);
    let picker = element::<HtmlSelectElement>("art-stamp");
    picker.set_value("0");
    picker
        .dispatch_event(&web_sys::Event::new("change").unwrap())
        .unwrap();
    tool("stamp-art");
    drag(&[(0, 0)]);
    assert_eq!(art(), "A", "choosing an earlier stamp uses that grid");
    click("undo");
    tool("area");
    drag(&[(0, 0)]);
    click("delete-stamp");
    assert_eq!(art(), "Z");
    click("save-stamp");
    assert_eq!(element::<Element>("art-stamp").child_element_count(), 16);
    while !element::<Element>("delete-stamp").has_attribute("disabled") {
        click("delete-stamp");
    }
    tool("stamp-art");
    drag(&[(0, 0)]);
    assert!(status().contains("save it as a stamp first"));
    assert_eq!(art(), "Z");
    assert_eq!(
        element::<Element>("art-stamp").text_content().unwrap(),
        "No stamps yet"
    );
}

#[wasm_bindgen_test]
fn saved_art_stamps_limit_total_cells_before_the_entry_limit() {
    blank(1, 1);
    while !element::<Element>("delete-stamp").has_attribute("disabled") {
        click("delete-stamp");
    }
    for i in 0..8u8 {
        blank(512, 256);
        tool("type");
        drag(&[(0, 0)]);
        keys(&[&char::from(b'A' + i).to_string()]);
        tool("area");
        drag(&[(0, 0), (511, 255)]);
        click("save-stamp");
        assert_eq!(
            element::<Element>("art-stamp").child_element_count(),
            usize::from((i + 1).min(7)) as u32
        );
    }
    assert!(status().contains("1,000,000 cells"));
    // A duplicate remains selectable even when a new full-size entry cannot fit.
    tool("type");
    drag(&[(0, 0)]);
    keys(&["A"]);
    tool("area");
    drag(&[(0, 0), (511, 255)]);
    click("save-stamp");
    assert!(status().contains("saved as a stamp"));
    assert_eq!(element::<HtmlSelectElement>("art-stamp").value(), "0");
    while !element::<Element>("delete-stamp").has_attribute("disabled") {
        click("delete-stamp");
    }
}

#[wasm_bindgen_test]
fn directional_mirror_is_optional_and_undoable() {
    blank(4, 2);
    tool("type");
    drag(&[(0, 0)]);
    keys(&["/", "(", "<", "v", "Enter", "\\", ")", ">", "^"]);
    let original = rows(&["/(<v", "\\)>^"]);
    assert_eq!(art(), original);
    click("flip-horizontal");
    assert_eq!(art(), rows(&["v<(/", "^>)\\"]));
    click("undo");
    check("mirror-glyphs", true);
    click("flip-horizontal");
    assert_eq!(art(), rows(&["v>)\\", "^<(/"]));
    click("undo");
    assert_eq!(art(), original);
    click("flip-vertical");
    assert_eq!(art(), rows(&["/)>v", "\\(<^"]));
    click("undo");
    assert_eq!(art(), original);
}

#[wasm_bindgen_test]
fn eraser_clears_by_pointer_and_keyboard() {
    blank(6, 3);
    tool("fill");
    drag(&[(0, 0)]);
    assert_eq!(art(), rows(&["######"; 3]));
    tool("eraser");
    drag(&[(0, 0), (3, 0)]);
    cursor_to((5, 2));
    keys(&[" ", "ArrowLeft", "ArrowUp", " "]);
    assert_eq!(art(), rows(&["    ##", "#### #", "####  "]));
}

#[wasm_bindgen_test]
fn line_previews_from_its_start_and_draws_by_keyboard() {
    blank(7, 4);
    tool("line");
    set_value("brush", "-");
    // The preview redraws from the start each move, so the detour leaves no trace.
    pointer("pointerdown", (0, 0));
    pointer("pointermove", (3, 3));
    assert_eq!(art(), rows(&["-      ", " -     ", "  -    ", "   -   "]));
    pointer("pointermove", (6, 0));
    assert_eq!(art(), rows(&["-------", "       ", "       ", "       "]));
    pointer("pointerup", (6, 0));
    assert_eq!(art(), rows(&["-------", "       ", "       ", "       "]));
    set_value("brush", "|");
    cursor_to((3, 1));
    keys(&[" ", "ArrowDown", "ArrowDown"]);
    let drawn = rows(&["-------", "   |   ", "   |   ", "   |   "]);
    assert_eq!(art(), drawn);
    keys(&[" "]);
    assert_eq!(art(), drawn);
    // Escape abandons a keyboard line and its preview without changing the frame.
    keys(&[" ", "ArrowLeft"]);
    assert_eq!(art(), rows(&["-------", "   |   ", "   |   ", "  ||   "]));
    keys(&["Escape"]);
    assert_eq!(art(), drawn);
    click("undo");
    assert_eq!(art(), rows(&["-------", "       ", "       ", "       "]));
}

#[wasm_bindgen_test]
fn rectangle_draws_by_pointer_and_keyboard() {
    blank(9, 4);
    tool("rectangle");
    // Corners in either order give the same outline.
    drag(&[(3, 2), (0, 0)]);
    set_value("brush", "o");
    cursor_to((5, 1));
    keys(&[
        " ",
        "ArrowRight",
        "ArrowRight",
        "ArrowRight",
        "ArrowDown",
        "ArrowDown",
        " ",
    ]);
    assert_eq!(
        art(),
        rows(&["####     ", "#  # oooo", "#### o  o", "     oooo"])
    );
}

#[wasm_bindgen_test]
fn ellipse_draws_by_pointer_and_keyboard() {
    blank(11, 5);
    tool("ellipse");
    // Corners in either order give the same outline.
    drag(&[(4, 2), (0, 0)]);
    let first = rows(&[
        " ###       ",
        "#   #      ",
        " ###       ",
        "           ",
        "           ",
    ]);
    assert_eq!(art(), first);
    set_value("brush", "o");
    cursor_to((6, 2));
    keys(&[
        " ",
        "ArrowRight",
        "ArrowRight",
        "ArrowRight",
        "ArrowRight",
        "ArrowDown",
        "ArrowDown",
        " ",
    ]);
    assert_eq!(
        art(),
        rows(&[
            " ###       ",
            "#   #      ",
            " ###   ooo ",
            "      o   o",
            "       ooo ",
        ])
    );
    click("undo");
    assert_eq!(art(), first);
}

#[wasm_bindgen_test]
fn constraint_keeps_shapes_square_inside_the_drag_by_pointer_and_keyboard() {
    blank(12, 5);
    check("constrain", true);
    tool("ellipse");
    drag(&[(0, 0), (9, 4)]);
    assert_eq!(
        art(),
        rows(&[
            "  #####     ",
            "##     ##   ",
            "#       #   ",
            "##     ##   ",
            "  #####     ",
        ])
    );
    click("undo");
    // The shape stays anchored at the start, whichever way the drag leaves it.
    tool("rectangle");
    drag(&[(11, 4), (4, 0)]);
    assert_eq!(
        art(),
        rows(&[
            "            ",
            "     #######",
            "     #     #",
            "     #     #",
            "     #######",
        ])
    );
    click("undo");
    cursor_to((1, 0));
    keys(&[
        " ",
        "ArrowRight",
        "ArrowRight",
        "ArrowRight",
        "ArrowDown",
        " ",
    ]);
    assert_eq!(
        art(),
        rows(&[
            " ###        ",
            " ###        ",
            "            ",
            "            ",
            "            ",
        ])
    );
    click("undo");
    // A diagram box judges its label against the box it drew, not the drag.
    check("diagram", true);
    set_value("stamp", "Hello");
    drag(&[(0, 0), (11, 2)]);
    assert!(status().contains("Make it larger"), "{}", status());
}

#[wasm_bindgen_test]
fn fill_replaces_the_touching_run_by_pointer_and_keyboard() {
    blank(7, 5);
    tool("rectangle");
    drag(&[(1, 1), (5, 3)]);
    tool("fill");
    set_value("brush", ".");
    drag(&[(3, 2)]);
    assert_eq!(
        art(),
        rows(&["       ", " ##### ", " #...# ", " ##### ", "       "])
    );
    set_value("brush", ":");
    cursor_to((0, 4));
    keys(&[" "]);
    assert_eq!(
        art(),
        rows(&[":::::::", ":#####:", ":#...#:", ":#####:", ":::::::"])
    );
}

#[wasm_bindgen_test]
fn type_writes_rows_by_pointer_keyboard_and_stamp() {
    blank(8, 3);
    tool("type");
    drag(&[(2, 0)]);
    keys(&["H", "i", "x", "Backspace", "Enter", "y", "o"]);
    assert_eq!(art(), rows(&["  Hi    ", "  yo    ", "        "]));
    // The keyboard alone chooses the cell; Stamp text needs no keyboard typing.
    cursor_to((5, 2));
    set_value("stamp", "ok!");
    click("stamp-text");
    assert_eq!(art(), rows(&["  Hi    ", "  yo    ", "     ok!"]));
    // At the right edge typing stops instead of overwriting the last cell, and Backspace
    // erases the character just typed.
    cursor_to((5, 1));
    keys(&["a", "b", "c", "d"]);
    assert_eq!(art(), rows(&["  Hi    ", "  yo abc", "     ok!"]));
    assert!(status().contains("row is full"), "{}", status());
    keys(&["Backspace", "Backspace"]);
    assert_eq!(art(), rows(&["  Hi    ", "  yo a  ", "     ok!"]));
    // The same holds when the first key of a run lands on the last column.
    cursor_to((7, 0));
    keys(&["x", "y"]);
    assert_eq!(art(), rows(&["  Hi   x", "  yo a  ", "     ok!"]));
    // Moving the cursor or starting the next row takes typing again.
    keys(&["ArrowLeft", "w"]);
    assert_eq!(art(), rows(&["  Hi  wx", "  yo a  ", "     ok!"]));
    cursor_to((6, 1));
    keys(&["b", "c", "Enter", "d"]);
    assert_eq!(art(), rows(&["  Hi  wx", "  yo abc", "     od!"]));
}

#[wasm_bindgen_test]
fn enter_on_the_bottom_row_keeps_typing_where_it_was() {
    blank(4, 2);
    tool("type");
    cursor_to((0, 1));
    keys(&["a", "b", "Enter", "c"]);
    assert_eq!(art(), rows(&["    ", "abc "]));
    assert!(status().contains("no row below"), "{}", status());
    // A full bottom row stops typing without pointing to Enter.
    keys(&["d", "e", "Enter", "f"]);
    assert_eq!(art(), rows(&["    ", "abcd"]));
    keys(&["g"]);
    assert!(
        status().contains("row is full") && !status().contains("Enter"),
        "{}",
        status()
    );
}

#[wasm_bindgen_test]
async fn a_full_row_does_not_stop_typing_on_the_frame_playback_moves_to() {
    blank(1, 1);
    let project = super::Animation::new(
        ["ab", "cd"]
            .iter()
            .map(|text| super::TimedFrame {
                frame: super::Frame::from_text(text).unwrap(),
                duration_ms: 10,
            })
            .collect(),
        false,
    )
    .unwrap();
    open("playback-typing.json", &project.to_json()).await;
    tool("type");
    cursor_to((1, 0));
    keys(&["x"]);
    assert_eq!(art(), "ax");
    click("play");
    wait_for(|| {
        art() == "cd" && element::<Element>("play").text_content().as_deref() == Some("Play")
    })
    .await;
    keys(&["y"]);
    assert_eq!(art(), "cy");
}

#[wasm_bindgen_test]
async fn area_selects_moves_copies_pastes_and_clears() {
    blank(10, 5);
    tool("type");
    drag(&[(1, 1)]);
    keys(&["A", "B", "Enter", "C", "D"]);
    tool("area");
    drag(&[(1, 1), (2, 2)]);
    assert!(!element::<Element>("selection").has_attribute("hidden"));
    assert_eq!(clipboard("copy", ""), "AB\nCD");
    // Dragging inside the selection moves its contents.
    drag(&[(1, 1), (5, 2)]);
    let moved = rows(&[
        "          ",
        "          ",
        "     AB   ",
        "     CD   ",
        "          ",
    ]);
    assert_eq!(art(), moved);
    // Pasting at the keyboard cursor selects the pasted area; Delete clears it.
    cursor_to((7, 0));
    clipboard("paste", "xy");
    assert_eq!(&art()[..10], "       xy ");
    keys(&["Delete"]);
    assert_eq!(art(), moved);
    // Keyboard selection and move, then cut, then paste and clear with the buttons.
    cursor_to((5, 2));
    keys(&[" ", "ArrowRight", "ArrowDown", " "]);
    // Space inside the selection lifts it; the arrow keys carry it.
    keys(&[" ", "ArrowLeft", "ArrowLeft", " "]);
    assert_eq!(
        art(),
        rows(&[
            "          ",
            "          ",
            "   AB     ",
            "   CD     ",
            "          ",
        ])
    );
    assert_eq!(clipboard("cut", ""), "AB\nCD");
    assert_eq!(art(), rows(&["          "; 5]));
    cursor_to((0, 0));
    click("area-paste");
    assert_eq!(&art()[..21], "AB        \nCD        ");
    click("area-clear");
    assert_eq!(art(), rows(&["          "; 5]));
    // The Copy area button fills the editor clipboard even where the system one is blocked.
    tool("type");
    drag(&[(0, 4)]);
    keys(&["z"]);
    tool("area");
    drag(&[(0, 4), (0, 4)]);
    click("area-copy");
    wait_for(|| {
        matches!(
            status().as_str(),
            "Area copied as ASCII text."
                | "Error: Copy was blocked. Select the artwork and copy it with your keyboard."
        )
    })
    .await;
    cursor_to((9, 0));
    click("area-paste");
    assert_eq!(
        art(),
        rows(&[
            "         z",
            "          ",
            "          ",
            "          ",
            "z         "
        ])
    );
}

#[wasm_bindgen_test]
fn diagram_boxes_and_connectors_join_with_plus() {
    blank(21, 7);
    check("diagram", true);
    tool("rectangle");
    set_value("stamp", "CLI");
    drag(&[(0, 0), (6, 2)]);
    set_value("stamp", "LIB");
    drag(&[(14, 0), (20, 2)]);
    // By keyboard, a box below the first.
    set_value("stamp", "OUT");
    cursor_to((0, 4));
    keys(&[" "]);
    repeat("ArrowRight", 6);
    keys(&["ArrowDown", "ArrowDown", " "]);
    tool("line");
    // Ending on the far box's edge keeps the edge; the arrowhead stops before it.
    drag(&[(6, 1), (14, 1)]);
    cursor_to((3, 2));
    keys(&[" ", "ArrowDown", "ArrowDown", " "]);
    assert_eq!(
        art(),
        rows(&[
            "+-----+       +-----+",
            "| CLI +------>| LIB |",
            "+--+--+       +-----+",
            "   v                 ",
            "+-----+              ",
            "| OUT |              ",
            "+-----+              ",
        ])
    );
}

#[wasm_bindgen_test]
fn onion_skin_stays_out_of_the_frame_and_selected_text() {
    blank(6, 2);
    tool("pencil");
    drag(&[(0, 0), (5, 0)]);
    click("add-frame");
    drag(&[(2, 1)]);
    check("onion-skin", true);
    let onion = element::<Element>("onion");
    assert!(!onion.has_attribute("hidden"));
    // The ghost shows only where the current frame is blank.
    assert_eq!(onion.text_content().unwrap(), rows(&["######", "      "]));
    assert_eq!(art(), rows(&["      ", "  #   "]));
    assert_eq!(
        element::<HtmlTextAreaElement>("editor").value(),
        rows(&["      ", "  #   "])
    );
    tool("select-text");
    let selection = web_sys::window().unwrap().get_selection().unwrap().unwrap();
    selection
        .select_all_children(&element::<Element>("grid"))
        .unwrap();
    let selected = String::from(js_sys::Object::to_string(&selection));
    selection.remove_all_ranges().unwrap();
    assert_eq!(selected.matches('#').count(), 1, "{selected:?}");
    // The first frame has no previous frame, so the ghost hides.
    click("previous");
    assert!(onion.has_attribute("hidden"));
    // Playback hides the ghost; pausing shows it again.
    click("next");
    assert!(!onion.has_attribute("hidden"));
    click("play");
    assert!(onion.has_attribute("hidden"));
    click("play");
    assert!(!onion.has_attribute("hidden"));
}

#[wasm_bindgen_test]
fn undo_and_redo_a_stroke_by_button_and_keyboard() {
    blank(5, 1);
    tool("pencil");
    drag(&[(0, 0), (2, 0)]);
    assert_eq!(art(), "###  ");
    click("undo");
    assert_eq!(art(), "     ");
    click("redo");
    assert_eq!(art(), "###  ");
    assert!(key_with("z", true, false));
    assert_eq!(art(), "     ");
    assert!(key_with("y", true, false));
    assert_eq!(art(), "###  ");
    key_with("z", true, false);
    assert!(key_with("Z", true, true));
    assert_eq!(art(), "###  ");
}

#[wasm_bindgen_test]
fn undo_reverts_a_typing_run_as_one_step() {
    blank(6, 1);
    tool("type");
    drag(&[(0, 0)]);
    keys(&["a", "b", "c"]);
    // Moving the cursor ends the run, so this character is a step of its own.
    keys(&["ArrowRight", "d"]);
    assert_eq!(art(), "abc d ");
    key_with("z", true, false);
    assert_eq!(art(), "abc   ");
    key_with("z", true, false);
    assert_eq!(art(), "      ");
    key_with("y", true, false);
    assert_eq!(art(), "abc   ");
}

#[wasm_bindgen_test]
fn non_ascii_input_never_reaches_the_artwork() {
    blank(4, 2);
    tool("type");
    drag(&[(0, 0)]);
    keys(&["é", "\u{1F600}", "\t", "a"]);
    assert_eq!(art(), rows(&["a   ", "    "]));
    // Choosing a tool replaces the status, so each error below is that step's own.
    tool("select-text");
    tool("type");
    clipboard("paste", "é");
    assert!(status().contains("printable ASCII"), "{}", status());
    tool("select-text");
    tool("type");
    set_value("stamp", "café");
    click("stamp-text");
    assert!(status().contains("printable ASCII"), "{}", status());
    assert_eq!(art(), rows(&["a   ", "    "]));
    tool("pencil");
    set_value("brush", "é");
    drag(&[(0, 1), (3, 1)]);
    assert!(status().contains("one printable ASCII"), "{}", status());
    set_value("brush", "\t");
    drag(&[(0, 1), (3, 1)]);
    assert_eq!(art(), rows(&["a   ", "    "]));
}

fn frame_count() -> String {
    element::<Element>("frame-count").text_content().unwrap()
}
fn choose_range(first: usize, last: usize) {
    set_value("range-first", &first.to_string());
    set_value("range-last", &last.to_string());
    click("select-range");
}
fn selected_range() -> (String, String) {
    (
        element::<HtmlInputElement>("range-first").value(),
        element::<HtmlInputElement>("range-last").value(),
    )
}
fn seek_frame(index: usize) {
    input_event("timeline", &index.to_string());
}
fn input_event(id: &str, value: &str) {
    let field = element::<HtmlElement>(id);
    if let Some(input) = field.dyn_ref::<HtmlInputElement>() {
        input.set_value(value);
    } else {
        element::<HtmlTextAreaElement>(id).set_value(value);
    }
    field
        .dispatch_event(&web_sys::Event::new("input").unwrap())
        .unwrap();
}
async fn sleep(milliseconds: i32) {
    let tick = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, milliseconds)
            .unwrap();
    });
    JsFuture::from(tick).await.unwrap();
}
async fn wait_for(condition: impl FnMut() -> bool) {
    wait_up_to(5_000, condition).await;
}
async fn wait_up_to(milliseconds: i32, mut condition: impl FnMut() -> bool) {
    for _ in 0..milliseconds / 10 {
        if condition() {
            return;
        }
        sleep(10).await;
    }
    panic!("Timed out waiting for the editor: {}", status());
}
/// Put one file in a file control, as choosing it in the browser's picker would.
fn choose(id: &str, name: &str, bytes: &[u8]) {
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let file = web_sys::File::new_with_u8_array_sequence(&parts, name).unwrap();
    let data = DataTransfer::new().unwrap();
    data.items().add_with_file(&file).unwrap();
    element::<HtmlInputElement>(id).set_files(data.files().as_ref());
}
/// Convert a picture with the image control as a user would, then wait for the read to finish.
async fn convert(contents: &str) {
    choose("media-file", "step.pgm", contents.as_bytes());
    click("convert");
    wait_for(|| !status().starts_with("Reading")).await;
    assert!(status().contains("File opened"), "{}", status());
}
fn preset(name: &str) {
    let picker = element::<HtmlSelectElement>("preset");
    picker.set_value(name);
    picker
        .dispatch_event(&web_sys::Event::new("change").unwrap())
        .unwrap();
}
/// Choose a file in the Open control as a user would, then wait for the read to finish.
async fn open(name: &str, contents: &str) -> String {
    choose("open-file", name, contents.as_bytes());
    element::<HtmlInputElement>("open-file")
        .dispatch_event(&web_sys::Event::new("change").unwrap())
        .unwrap();
    for _ in 0..500 {
        // Clipboard messages can replace the status whilst the file is still loading.
        if enabled("generate") {
            return status();
        }
        sleep(10).await;
    }
    panic!("{name} is still loading: {}", status());
}

thread_local! {
    /// Downloads the page asked for, as (file name, object URL), oldest first.
    static DOWNLOADS: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
}
/// Click a Save button and return the file name and text the page offered. The click on the
/// download link is stopped before the browser acts on it, and its object URL read instead.
async fn save(id: &str) -> (String, String) {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let hook = Closure::<dyn FnMut(web_sys::Event)>::new(|event: web_sys::Event| {
            if let Some(link) = event
                .target()
                .and_then(|target| target.dyn_into::<HtmlAnchorElement>().ok())
                .filter(|link| link.has_attribute("download"))
            {
                event.prevent_default();
                DOWNLOADS.with(|list| list.borrow_mut().push((link.download(), link.href())));
            }
        });
        document()
            .add_event_listener_with_callback_and_bool("click", hook.as_ref().unchecked_ref(), true)
            .unwrap();
        hook.forget();
    });
    let earlier = DOWNLOADS.with(|list| list.borrow().len());
    click(id);
    let (name, url) = DOWNLOADS
        .with(|list| list.borrow().get(earlier).cloned())
        .unwrap_or_else(|| panic!("#{id} offered no download: {}", status()));
    let response: web_sys::Response =
        JsFuture::from(web_sys::window().unwrap().fetch_with_str(&url))
            .await
            .unwrap()
            .dyn_into()
            .unwrap();
    let text = JsFuture::from(response.text().unwrap()).await.unwrap();
    (name, text.as_string().unwrap())
}
/// Everything a refused or cancelled import must leave alone: the whole project as Save would
/// write it, the frame on show and the unsaved-work prompt on leaving.
async fn project_now() -> (String, String, String, bool) {
    (
        save("download-json").await.1,
        frame_count(),
        art(),
        leaving_warns(),
    )
}
/// Whether leaving the page now would raise the browser's unsaved-work prompt.
fn leaving_warns() -> bool {
    let init = web_sys::EventInit::new();
    init.set_cancelable(true);
    let event = web_sys::Event::new_with_event_init_dict("beforeunload", &init).unwrap();
    web_sys::window().unwrap().dispatch_event(&event).unwrap();
    event.default_prevented()
}
fn enabled(id: &str) -> bool {
    !element::<Element>(id).has_attribute("disabled")
}

#[wasm_bindgen_test]
async fn frame_ranges_duplicate_move_time_and_restore_the_view_with_history() {
    blank(1, 1);
    let project = super::Animation::new(
        "ABCDE"
            .chars()
            .enumerate()
            .map(|(i, c)| super::TimedFrame {
                frame: super::Frame::from_text(&c.to_string()).unwrap(),
                duration_ms: (i as u32 + 1) * 100,
            })
            .collect(),
        true,
    )
    .unwrap();
    open("range.json", &project.to_json()).await;
    seek_frame(2);
    choose_range(2, 3);
    assert_eq!(art(), "C");
    click("duplicate");
    assert_eq!(frame_count(), "Frame 5 / 7");
    assert_eq!(selected_range(), ("4".into(), "5".into()));
    assert_eq!(art(), "C");
    click("undo");
    assert_eq!(frame_count(), "Frame 3 / 5");
    assert_eq!(selected_range(), ("2".into(), "3".into()));
    click("redo");
    assert_eq!(frame_count(), "Frame 5 / 7");
    assert_eq!(selected_range(), ("4".into(), "5".into()));
    click("undo");
    click("move-left");
    assert_eq!(frame_count(), "Frame 2 / 5");
    assert_eq!(selected_range(), ("1".into(), "2".into()));
    assert!(element::<Element>("move-left").has_attribute("disabled"));
    click("move-right");
    assert_eq!(frame_count(), "Frame 3 / 5");
    assert_eq!(selected_range(), ("2".into(), "3".into()));
    set_value("range-duration", "170");
    click("apply-range-duration");
    assert_eq!(element::<HtmlInputElement>("duration").value(), "170");
    click("undo");
    assert_eq!(element::<HtmlInputElement>("duration").value(), "300");
    assert_eq!(selected_range(), ("2".into(), "3".into()));
    click("delete-frame");
    assert_eq!(frame_count(), "Frame 2 / 3");
    assert_eq!(art(), "D");
    click("undo");
    assert_eq!(art(), "C");
    assert_eq!(frame_count(), "Frame 3 / 5");
    assert_eq!(selected_range(), ("2".into(), "3".into()));
    // A failed duration change must not add a history entry.
    set_value("range-duration", "9");
    click("apply-range-duration");
    assert!(status().contains("duration"), "{}", status());
    click("undo");
    assert_eq!(frame_count(), "Frame 2 / 5");
    assert_eq!(selected_range(), ("1".into(), "2".into()));
    choose_range(1, 5);
    assert!(element::<Element>("delete-frame").has_attribute("disabled"));
    assert!(element::<Element>("move-right").has_attribute("disabled"));
    set_value("range-first", "4");
    set_value("range-last", "2");
    click("select-range");
    assert!(status().contains("range"), "{}", status());
    // A manual seek collapses the range.
    seek_frame(4);
    assert_eq!(selected_range(), ("5".into(), "5".into()));
    // Replacing the project retains the previous view in undo history.
    choose_range(3, 5);
    element::<HtmlSelectElement>("mode").set_value("blank");
    click("generate");
    click("undo");
    assert_eq!(art(), "E");
    assert_eq!(frame_count(), "Frame 5 / 5");
    assert_eq!(selected_range(), ("3".into(), "5".into()));
}

#[wasm_bindgen_test]
async fn playback_keeps_the_frame_range_at_completion_and_restart() {
    blank(1, 1);
    let project = super::Animation::new(
        "ABC"
            .chars()
            .map(|c| super::TimedFrame {
                frame: super::Frame::from_text(&c.to_string()).unwrap(),
                duration_ms: 10,
            })
            .collect(),
        false,
    )
    .unwrap();
    open("playback-range.json", &project.to_json()).await;
    choose_range(2, 3);
    click("play");
    wait_for(|| {
        art() == "C" && element::<Element>("play").text_content().as_deref() == Some("Play")
    })
    .await;
    assert_eq!(selected_range(), ("2".into(), "3".into()));
    click("play");
    assert_eq!(selected_range(), ("2".into(), "3".into()));
    wait_for(|| element::<Element>("play").text_content().as_deref() == Some("Play")).await;
    assert_eq!(selected_range(), ("2".into(), "3".into()));
}

fn recovery_phase() -> String {
    element::<Element>("recovery-status")
        .get_attribute("data-state")
        .unwrap_or_default()
}
async fn recovery_settled() {
    wait_for(|| {
        !element::<HtmlInputElement>("recovery-enabled").disabled()
            && !matches!(recovery_phase().as_str(), "checking" | "saving" | "pending")
    })
    .await;
}
async fn saved_project() -> Option<super::Animation> {
    let value = super::recovery::read_record().await.unwrap();
    if value.is_undefined() {
        return None;
    }
    let raw = value.as_string().unwrap();
    let (_, json) = raw.split_once('\n').unwrap();
    if json.is_empty() {
        None
    } else {
        Some(super::Animation::from_json(json).unwrap())
    }
}

#[wasm_bindgen_test]
async fn local_recovery_is_opt_in_serialised_restoreable_and_undoable() {
    blank(4, 1);
    recovery_settled().await;
    check("recovery-enabled", false);
    if !element::<Element>("discard-recovery").has_attribute("disabled") {
        click("discard-recovery");
        recovery_settled().await;
    }
    let initial = super::recovery::read_record().await.unwrap();
    tool("type");
    keys(&["A"]);
    assert_eq!(
        super::recovery::read_record().await.unwrap(),
        initial,
        "No artwork is stored without opt-in"
    );
    check("recovery-enabled", true);
    // Editing and pausing while the first save is in flight must not lose the
    // captured revision or silently schedule a later write after pausing.
    keys(&["B", "C"]);
    check("recovery-enabled", false);
    assert!(element::<HtmlInputElement>("recovery-enabled").disabled());
    recovery_settled().await;
    assert_eq!(
        saved_project().await.unwrap().frames()[0].frame.text(),
        "A   "
    );
    check("recovery-enabled", true);
    recovery_settled().await;
    assert_eq!(recovery_phase(), "saved");
    assert_eq!(
        saved_project().await.unwrap().frames()[0].frame.text(),
        "ABC "
    );
    // This character follows the existing grouped-typing path, not commit().
    keys(&["D"]);
    wait_for(|| recovery_phase() == "saved").await;
    assert_eq!(
        saved_project().await.unwrap().frames()[0].frame.text(),
        "ABCD"
    );
    check("recovery-enabled", false);
    element::<HtmlSelectElement>("mode").set_value("blank");
    click("generate");
    assert_eq!(art(), "    ");
    tool("pencil");
    pointer("pointerdown", (0, 0));
    click("restore-recovery");
    assert!(element::<Element>("apply").has_attribute("disabled"));
    pointer("pointerup", (3, 0));
    assert_eq!(art(), "    ", "Recovery cancels unfinished stroke previews");
    // Mutation entry points must reject late synthetic events too.
    element::<Element>("apply")
        .dispatch_event(&web_sys::Event::new("click").unwrap())
        .unwrap();
    assert!(status().contains("recovery operation"), "{}", status());
    recovery_settled().await;
    assert_eq!(art(), "ABCD");
    click("undo");
    assert_eq!(art(), "    ");
    wait_for(|| recovery_phase() == "saved").await;
    assert_eq!(
        saved_project().await.unwrap().frames()[0].frame.text(),
        "    "
    );
    click("redo");
    wait_for(|| recovery_phase() == "saved").await;
    assert_eq!(
        saved_project().await.unwrap().frames()[0].frame.text(),
        "ABCD"
    );
    // Identical JSON with a new revision still represents a competing claim.
    let previous = super::recovery::read_record().await.unwrap();
    let json = saved_project().await.unwrap().to_json();
    let other: wasm_bindgen::JsValue = format!("other-tab\n{json}").into();
    assert!(
        super::recovery::replace_record(previous, other.clone())
            .await
            .is_ok()
    );
    tool("type");
    cursor_to((0, 0));
    keys(&["X"]);
    wait_for(|| recovery_phase() == "conflict").await;
    assert_eq!(art(), "XBCD");
    assert!(!element::<HtmlInputElement>("recovery-enabled").checked());
    assert_eq!(super::recovery::read_record().await.unwrap(), other);
    check("recovery-enabled", true);
    assert_eq!(recovery_phase(), "error");
    assert_eq!(super::recovery::read_record().await.unwrap(), other);
    tool("pencil");
    pointer("pointerdown", (0, 0));
    click("discard-recovery");
    pointer("pointerup", (3, 0));
    assert_eq!(art(), "XBCD");
    recovery_settled().await;
    assert!(saved_project().await.is_none());
    assert!(!element::<HtmlInputElement>("recovery-enabled").checked());
}

#[wasm_bindgen_test]
async fn recovery_compare_and_write_rejects_stale_claims_and_discards() {
    use std::{future::poll_fn, pin::pin, task::Poll};
    let expected = super::recovery::read_record().await.unwrap();
    let project = super::Animation::still(super::Frame::from_text("claim").unwrap());
    let a: wasm_bindgen::JsValue = format!("claim-a\n{}", project.to_json()).into();
    let b: wasm_bindgen::JsValue = format!("claim-b\n{}", project.to_json()).into();
    let mut first = pin!(super::recovery::replace_record(expected.clone(), a.clone()));
    let mut second = pin!(super::recovery::replace_record(expected.clone(), b.clone()));
    let (mut first_result, mut second_result) = (None, None);
    let (first_result, second_result) = poll_fn(|cx| {
        if first_result.is_none()
            && let Poll::Ready(result) = first.as_mut().poll(cx)
        {
            first_result = Some(result);
        }
        if second_result.is_none()
            && let Poll::Ready(result) = second.as_mut().poll(cx)
        {
            second_result = Some(result);
        }
        if first_result.is_some() && second_result.is_some() {
            Poll::Ready((first_result.take().unwrap(), second_result.take().unwrap()))
        } else {
            Poll::Pending
        }
    })
    .await;
    assert_ne!(
        first_result.is_ok(),
        second_result.is_ok(),
        "Only one competing claim may win"
    );
    let winner = super::recovery::read_record().await.unwrap();
    assert!(winner == a || winner == b);
    // Stale discard cannot erase the winner.
    assert!(
        super::recovery::replace_record(expected.clone(), "discarded\n".into())
            .await
            .is_err()
    );
    assert_eq!(super::recovery::read_record().await.unwrap(), winner);
    assert!(
        super::recovery::replace_record(winner, "discarded\n".into())
            .await
            .is_ok()
    );
    // A discard marker also fences an old empty-slot writer.
    assert!(
        super::recovery::replace_record(wasm_bindgen::JsValue::UNDEFINED, a)
            .await
            .is_err()
    );
    // Leave the editor's observed slot unchanged for the other browser cases.
    let marker = super::recovery::read_record().await.unwrap();
    assert!(
        super::recovery::replace_record(marker, expected)
            .await
            .is_ok()
    );
}

/// Enable recovery from a slot this tab may claim and wait until the tab is idle at 'saved'.
async fn recovery_saved_and_idle() {
    recovery_settled().await;
    check("recovery-enabled", false);
    if !element::<Element>("discard-recovery").has_attribute("disabled") {
        click("discard-recovery");
        recovery_settled().await;
    }
    check("recovery-enabled", true);
    recovery_settled().await;
    assert_eq!(recovery_phase(), "saved");
}
/// Stand in for another tab, or browser clearing, replacing this tab's saved record.
async fn replace_saved_record(value: wasm_bindgen::JsValue) {
    let ours = super::recovery::read_record().await.unwrap();
    assert!(super::recovery::replace_record(ours, value).await.is_ok());
}

#[wasm_bindgen_test]
async fn idle_recovery_stops_when_the_tab_returns_after_another_tab_or_clearing() {
    blank(3, 1);
    assert!(!document().hidden());
    recovery_saved_and_idle().await;
    // Another tab discards; this tab has nothing to write, so only returning to it can tell.
    replace_saved_record("other-tab\n".into()).await;
    assert_eq!(recovery_phase(), "saved");
    web_sys::window()
        .unwrap()
        .dispatch_event(&web_sys::Event::new("focus").unwrap())
        .unwrap();
    wait_for(|| recovery_phase() == "conflict").await;
    assert!(!element::<HtmlInputElement>("recovery-enabled").checked());
    // Clearing site data leaves an empty slot; showing the page again must notice that too.
    recovery_saved_and_idle().await;
    replace_saved_record(wasm_bindgen::JsValue::UNDEFINED).await;
    document()
        .dispatch_event(&web_sys::Event::new("visibilitychange").unwrap())
        .unwrap();
    wait_for(|| recovery_phase() == "conflict").await;
    assert!(!element::<HtmlInputElement>("recovery-enabled").checked());
}

#[wasm_bindgen_test]
async fn recovery_conflict_reaches_the_main_status_with_the_panel_closed() {
    blank(3, 1);
    recovery_saved_and_idle().await;
    assert!(!element::<Element>("recovery-panel").has_attribute("open"));
    replace_saved_record("other-tab\n".into()).await;
    tool("type");
    keys(&["Z"]);
    wait_for(|| recovery_phase() == "conflict").await;
    assert!(
        status().contains("another tab changed the saved record"),
        "{}",
        status()
    );
}

#[wasm_bindgen_test]
async fn discard_removes_a_corrupt_object_record_but_not_a_newer_one() {
    blank(3, 1);
    recovery_saved_and_idle().await;
    // Another program leaves a non-text record; each read returns a fresh copy of it.
    replace_saved_record(js_sys::Object::new().into()).await;
    web_sys::window()
        .unwrap()
        .dispatch_event(&web_sys::Event::new("focus").unwrap())
        .unwrap();
    wait_for(|| recovery_phase() == "conflict").await;
    assert!(element::<Element>("restore-recovery").has_attribute("disabled"));
    click("discard-recovery");
    recovery_settled().await;
    assert_eq!(recovery_phase(), "off", "{}", status());
    assert!(saved_project().await.is_none());
    // A stale view of a corrupt record must not erase a project saved since, or an emptied slot.
    let marker = super::recovery::read_record().await.unwrap();
    replace_saved_record(js_sys::Object::new().into()).await;
    let stale = super::recovery::read_record().await.unwrap();
    let project = super::Animation::still(super::Frame::from_text("newer").unwrap());
    let newer: wasm_bindgen::JsValue = format!("other-tab\n{}", project.to_json()).into();
    replace_saved_record(newer.clone()).await;
    assert!(
        super::recovery::replace_record(stale.clone(), "discarded\n".into())
            .await
            .is_err()
    );
    assert_eq!(super::recovery::read_record().await.unwrap(), newer);
    replace_saved_record(wasm_bindgen::JsValue::UNDEFINED).await;
    assert!(
        super::recovery::replace_record(stale, "discarded\n".into())
            .await
            .is_err()
    );
    assert!(super::recovery::read_record().await.unwrap().is_undefined());
    // Leave the slot as this tab last observed it for the other browser cases.
    replace_saved_record(marker).await;
}

#[wasm_bindgen_test]
async fn presets_fill_the_conversion_settings_and_reach_the_output() {
    const STEP: &str = "P2\n5 1\n255\n0 0 0 255 255\n";
    let settings = || {
        (
            element::<HtmlInputElement>("ramp").value(),
            element::<HtmlInputElement>("dither").checked(),
            element::<HtmlInputElement>("edges").checked(),
            element::<HtmlInputElement>("edge-glyphs").checked(),
            element::<HtmlInputElement>("gamma").value(),
            element::<HtmlInputElement>("contrast").value(),
        )
    };
    let chosen = || element::<HtmlSelectElement>("preset").value();
    blank(5, 1);
    let standard = settings();
    assert_eq!(chosen(), "standard", "the page starts on its own defaults");
    convert(STEP).await;
    assert_eq!(art(), "   @@");

    preset("line-art");
    assert!(status().contains("Preset applied"), "{}", status());
    convert(STEP).await;
    assert_eq!(art(), "  || ");
    // The options stay independent: direction alone overlays the picture, and edges alone use
    // the ramp.
    check("edges", false);
    assert_eq!(chosen(), "custom", "a changed setting leaves the preset");
    convert(STEP).await;
    assert_eq!(art(), "  ||@");
    check("edges", true);
    check("edge-glyphs", false);
    convert(STEP).await;
    assert_eq!(art(), "  @@ ");

    preset("photo");
    assert_eq!(settings().0, crate::image_renderer::RAMPS[1].1);
    assert!(settings().1 && !settings().2 && !settings().3);
    preset("poster");
    assert_eq!(settings().0, crate::image_renderer::RAMPS[2].1);
    assert_eq!(settings().5, "1.5");
    // Choosing Custom keeps what is there; every other preset is a full set of values.
    input_event("ramp", " .#");
    assert_eq!(chosen(), "custom");
    let edited = settings();
    preset("custom");
    assert_eq!(settings(), edited);
    preset("standard");
    assert_eq!(settings(), standard);
}

#[wasm_bindgen_test]
async fn open_pads_text_restores_projects_and_refuses_unicode() {
    blank(3, 1);
    assert!(
        open("ragged.txt", "ab\r\ncd e")
            .await
            .contains("File opened")
    );
    assert_eq!(art(), rows(&["ab  ", "cd e"]));
    assert_eq!(element::<HtmlInputElement>("width").value(), "4");
    let project = crate::Animation::new(
        vec![
            crate::TimedFrame {
                frame: crate::Frame::from_text("ab").unwrap(),
                duration_ms: 70,
            },
            crate::TimedFrame {
                frame: crate::Frame::from_text("cd").unwrap(),
                duration_ms: 20,
            },
        ],
        false,
    )
    .unwrap();
    assert!(
        open("project.json", &project.to_json())
            .await
            .contains("File opened")
    );
    assert_eq!(
        (frame_count().as_str(), art().as_str()),
        ("Frame 1 / 2", "ab")
    );
    assert_eq!(element::<HtmlInputElement>("duration").value(), "70");
    assert!(!element::<HtmlInputElement>("loop").checked());
    click("next");
    assert_eq!(art(), "cd");
    assert_eq!(element::<HtmlInputElement>("duration").value(), "20");
    // A refused file leaves the current project in place.
    assert!(
        open("bad.txt", "caf\u{e9}")
            .await
            .contains("printable ASCII")
    );
    assert_eq!(
        (frame_count().as_str(), art().as_str()),
        ("Frame 2 / 2", "cd")
    );
}

#[wasm_bindgen_test]
async fn refused_and_cancelled_picture_imports_leave_the_project_and_history() {
    const PICTURE: &[u8] = b"P2\n4 1\n255\n0 0 255 255\n";
    blank(4, 1);
    tool("pencil");
    drag(&[(0, 0), (1, 0)]);
    let before = project_now().await;
    assert_eq!(before.2, "##  ");
    assert!(before.3, "the stroke is unsaved, so leaving warns");

    choose(
        "media-file",
        "broken.png",
        b"\x89PNG\r\n\x1a\nnot a picture",
    );
    click("convert");
    wait_for(|| !status().starts_with("Reading")).await;
    assert!(status().starts_with("Error: Image header"), "{}", status());
    assert_eq!(project_now().await, before);

    // 62 frames of 512 x 256 cells exceed the 8,000,000 cell project budget.
    check("auto-height", false);
    set_value("width", "512");
    set_value("height", "256");
    let frames: Vec<_> = (0..62)
        .map(|_| gif_writer::GifFrame::full(2, 2, vec![0, 1, 1, 0], 10))
        .collect();
    let gif = gif_writer::gif(2, 2, Some(&[[255, 255, 255], [0, 0, 0]]), &frames);
    choose("media-file", "long.gif", &gif);
    click("convert");
    wait_for(|| !status().starts_with("Reading")).await;
    assert!(status().contains("output cell"), "{}", status());
    check("auto-height", true);
    set_value("width", "4");
    assert_eq!(project_now().await, before);

    // Cancel releases the controls at once and the read is discarded when it arrives.
    choose("media-file", "step.pgm", PICTURE);
    click("convert");
    click("media-cancel");
    assert_eq!(status(), "Cancelled. The artwork is unchanged.");
    assert!(enabled("convert"));
    assert!(element::<Element>("media-busy").has_attribute("hidden"));
    sleep(250).await;
    assert_eq!(status(), "Cancelled. The artwork is unchanged.");
    assert_eq!(project_now().await, before);

    // Left alone, the same picture replaces the artwork. Undo then reaches the stroke and the
    // blank canvas, so none of the refused or cancelled imports left a step.
    choose("media-file", "step.pgm", PICTURE);
    click("convert");
    wait_for(|| !status().starts_with("Reading")).await;
    assert_eq!(art(), "  @@");
    click("undo");
    assert_eq!(project_now().await, before);
    click("undo");
    assert_eq!(art(), "    ");
}

#[wasm_bindgen_test]
async fn refused_and_cancelled_video_conversions_leave_the_project_and_history() {
    blank(4, 1);
    tool("pencil");
    drag(&[(0, 0), (1, 0)]);
    let before = project_now().await;
    assert!(before.3, "the stroke is unsaved, so leaving warns");
    check("auto-height", true);
    set_value("media-fps", "30");
    set_value("video-start", "0");
    set_value("video-end", "");

    // Left alone, the clip becomes 90 frames; how long that takes bounds the cancel check below.
    let started = now();
    choose("video-file", "clip.webm", CLIP);
    click("video-convert");
    wait_up_to(30_000, || !status().starts_with("Reading")).await;
    let whole = now() - started;
    assert!(
        status().starts_with("Video converted to 90"),
        "{}",
        status()
    );
    click("undo");
    assert_eq!(project_now().await, before);

    for (name, video, end, width, refusal) in [
        ("broken.webm", &b"not a video"[..], "", "4", "cannot play"),
        (
            "clip.webm",
            CLIP,
            "4",
            "4",
            "within the video's 3.00 seconds",
        ),
        // 90 frames of 512 x 192 cells exceed the 8,000,000 cell project budget.
        ("clip.webm", CLIP, "", "512", "8,000,000 cell"),
    ] {
        set_value("video-end", end);
        set_value("width", width);
        choose("video-file", name, video);
        click("video-convert");
        wait_up_to(30_000, || !status().starts_with("Reading")).await;
        assert!(status().contains(refusal), "{refusal}: {}", status());
        set_value("width", "4");
        assert_eq!(project_now().await, before, "{refusal}");
    }
    set_value("video-end", "");

    // Cancel part-way, then wait twice as long as the whole conversion took: nothing arrives.
    choose("video-file", "clip.webm", CLIP);
    click("video-convert");
    wait_up_to(30_000, || {
        element::<HtmlProgressElement>("media-progress").value() >= 2.0
            || !status().starts_with("Reading")
    })
    .await;
    assert!(
        status().starts_with("Reading"),
        "the conversion finished before it could be cancelled: {}",
        status()
    );
    click("media-cancel");
    assert_eq!(status(), "Cancelled. The artwork is unchanged.");
    assert!(enabled("video-convert") && enabled("convert"));
    assert!(element::<Element>("media-busy").has_attribute("hidden"));
    sleep(2 * whole as i32 + 100).await;
    assert_eq!(status(), "Cancelled. The artwork is unchanged.");
    assert_eq!(project_now().await, before);
    click("undo");
    assert_eq!(art(), "    ");
}

#[wasm_bindgen_test]
async fn playback_and_saves_keep_unsaved_edits() {
    blank(3, 1);
    let still = || super::TimedFrame {
        frame: super::Frame::from_text("   ").unwrap(),
        duration_ms: 50,
    };
    let project = super::Animation::new(vec![still(), still()], true).unwrap();
    assert!(
        open("two.json", &project.to_json())
            .await
            .contains("File opened")
    );
    tool("pencil");
    drag(&[(0, 0)]);
    click("next");
    cursor_to((2, 0));
    keys(&[" ", "Enter"]);
    assert_eq!(art(), "  #");

    // Unapplied frame-editor text holds playback back rather than being lost to it.
    input_event("editor", "x");
    click("play");
    assert!(status().contains("Apply or discard"), "{}", status());
    assert_eq!(element::<HtmlTextAreaElement>("editor").value(), "x");
    click("discard");

    // Playback shows each edited frame in turn and leaves them in place when paused.
    click("play");
    wait_for(|| art() == "#  ").await;
    wait_for(|| art() == "  #").await;
    click("play");
    seek_frame(1);
    assert_eq!(art(), "  #");

    // Each save carries both edits: the project with its timing, the frame on show and the player.
    let (name, json) = save("download-json").await;
    assert_eq!(name, "artwork.ascii.json");
    let saved = super::Animation::from_json(&json).unwrap();
    let frames: Vec<_> = saved
        .frames()
        .iter()
        .map(|entry| (entry.frame.text(), entry.duration_ms))
        .collect();
    assert_eq!(frames, [("#  ".to_string(), 50), ("  #".to_string(), 50)]);
    assert_eq!(
        save("download-text").await,
        ("frame.txt".to_string(), "  #\n".to_string())
    );
    let (name, player) = save("download-html").await;
    assert_eq!(name, "artwork.html");
    assert!(player.contains(&json));

    // A save is only a request, so leaving still warns, and undo still reaches both edits.
    assert!(leaving_warns());
    click("undo");
    click("undo");
    let reverted = super::Animation::from_json(&save("download-json").await.1).unwrap();
    assert_eq!(reverted.to_json(), project.to_json());
}

/// Wait for a file read begun at `started` to finish and return the longest gap between timer
/// turns since then, the click's own work included: the longest time the page could not respond,
/// in milliseconds. Nested timers are clamped to about 4 ms, which sets the floor.
async fn longest_freeze(started: f64) -> f64 {
    let (mut last, mut longest) = (started, 0.0_f64);
    while status().starts_with("Reading") {
        sleep(1).await;
        let time = now();
        longest = longest.max(time - last);
        last = time;
    }
    longest
}
/// Median and slowest of the runs, in milliseconds.
fn spread(mut runs: Vec<f64>) -> String {
    runs.sort_by(f64::total_cmp);
    format!("{:.0} | {:.0}", runs[runs.len() / 2], runs[runs.len() - 1])
}

/// Not a check: measures how long the page freezes whilst the editor converts GIFs and opens
/// projects up to the accepted limits. Run it on demand with
/// `cargo test-editor --release -- --include-ignored --nocapture conversion_freeze` and record
/// the table in `docs/VALIDATION.md`.
#[wasm_bindgen_test]
#[ignore = "benchmark; run on demand"]
async fn conversion_freeze() {
    const RUNS: usize = 3;
    blank(1, 1);
    check("auto-height", true);
    let mut table = vec![
        "| Input | Size | Median freeze (ms) | Slowest freeze (ms) |".to_string(),
        "|---|---|--:|--:|".to_string(),
    ];
    let palette: Vec<[u8; 3]> = (0..16u8).map(|i| [i * 17; 3]).collect();
    // The last GIF is within 0.4% of the 64 million decoded pixel budget.
    for (count, side, columns) in [(60u16, 160u16, 80), (300, 320, 120), (600, 326, 160)] {
        let frames: Vec<_> = (0..count)
            .map(|f| {
                let side = usize::from(side);
                let indices = (0..side * side)
                    .map(|i| ((i % side / 8 + i / side / 8 + usize::from(f)) % 16) as u8)
                    .collect();
                gif_writer::GifFrame::full(side as u16, side as u16, indices, 4)
            })
            .collect();
        let gif = gif_writer::gif(side, side, Some(&palette), &frames);
        assert!(gif.len() <= crate::MAX_INPUT_BYTES, "{} bytes", gif.len());
        set_value("width", &columns.to_string());
        let mut runs = Vec::new();
        for _ in 0..RUNS {
            choose("media-file", "bench.gif", &gif);
            let started = now();
            click("convert");
            runs.push(longest_freeze(started).await);
            assert!(status().contains("File opened"), "{}", status());
        }
        table.push(format!(
            "| GIF to {columns} columns | {count} frames of {side} x {side} px, {:.1} MiB | {} |",
            gif.len() as f64 / 1_048_576.0,
            spread(runs)
        ));
    }
    // The last project is within 0.06% of the 8,000,000 cell budget.
    let mut largest = String::new();
    for (count, width, height) in [(60, 80, 24), (600, 160, 80), (61, 512, 256)] {
        let frames = (0..count)
            .map(|f| {
                let text = (0..height)
                    .map(|y| {
                        (0..width)
                            .map(|x| char::from(b'!' + ((x + y + f) % 94) as u8))
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                super::TimedFrame {
                    frame: super::Frame::from_text(&text).unwrap(),
                    duration_ms: 100,
                }
            })
            .collect();
        let json = super::Animation::new(frames, true).unwrap().to_json();
        let mut runs = Vec::new();
        for _ in 0..RUNS {
            choose("open-file", "bench.json", json.as_bytes());
            let started = now();
            element::<HtmlInputElement>("open-file")
                .dispatch_event(&web_sys::Event::new("change").unwrap())
                .unwrap();
            runs.push(longest_freeze(started).await);
            assert!(status().contains("File opened"), "{}", status());
        }
        table.push(format!(
            "| Open project | {count} frames of {width} x {height}, {:.1} MiB | {} |",
            json.len() as f64 / 1_048_576.0,
            spread(runs)
        ));
        largest = json;
    }
    // Saving is synchronous, so the click's own duration is the freeze.
    save("download-json").await;
    let runs = (0..RUNS)
        .map(|_| {
            let started = now();
            click("download-json");
            now() - started
        })
        .collect();
    table.push(format!(
        "| Save project | the last project, {:.1} MiB | {} |",
        largest.len() as f64 / 1_048_576.0,
        spread(runs)
    ));
    let navigator = web_sys::window().unwrap().navigator();
    wasm_bindgen_test::console_log!(
        "{}, {} logical cores, {}\n{}",
        js_sys::Date::new_0().to_iso_string(),
        navigator.hardware_concurrency(),
        navigator.user_agent().unwrap(),
        table.join("\n")
    );
}

#[wasm_bindgen_test]
fn frames_edit_reorder_flip_and_delete_from_the_timeline() {
    blank(3, 1);
    tool("type");
    drag(&[(0, 0)]);
    keys(&["a"]);
    click("duplicate");
    assert_eq!(
        (frame_count().as_str(), art().as_str()),
        ("Frame 2 / 2", "a  ")
    );
    // Unapplied edits block other actions until applied or discarded.
    input_event("editor", "b");
    click("previous");
    assert!(status().contains("Apply or discard"), "{}", status());
    assert_eq!(frame_count(), "Frame 2 / 2");
    input_event("duration", "120");
    click("apply");
    assert_eq!(art(), "b  ");
    input_event("timeline", "0");
    assert_eq!(
        (frame_count().as_str(), art().as_str()),
        ("Frame 1 / 2", "a  ")
    );
    click("move-right");
    assert_eq!(
        (frame_count().as_str(), art().as_str()),
        ("Frame 2 / 2", "a  ")
    );
    click("previous");
    assert_eq!(art(), "b  ");
    assert_eq!(element::<HtmlInputElement>("duration").value(), "120");
    click("flip-horizontal");
    assert_eq!(art(), "  b");
    input_event("editor", "zzz");
    click("discard");
    assert_eq!(art(), "  b");
    assert_eq!(element::<HtmlTextAreaElement>("editor").value(), "  b");
    click("delete-frame");
    assert_eq!(
        (frame_count().as_str(), art().as_str()),
        ("Frame 1 / 1", "a  ")
    );
    click("undo");
    assert_eq!(
        (frame_count().as_str(), art().as_str()),
        ("Frame 1 / 2", "  b")
    );
}

#[wasm_bindgen_test]
fn effect_animates_the_selected_frame_and_undoes() {
    blank(6, 1);
    tool("type");
    drag(&[(0, 0)]);
    keys(&["h", "i"]);
    element::<HtmlSelectElement>("effect").set_value("reveal");
    set_value("effect-duration", "500");
    set_value("effect-fps", "10");
    check("effect-reverse", false);
    click("apply-effect");
    assert!(
        status().contains("Effect applied: 5 frames"),
        "{}",
        status()
    );
    let mut frames = vec![art()];
    for _ in 1..5 {
        click("next");
        frames.push(art());
    }
    assert_eq!(frame_count(), "Frame 5 / 5");
    assert_eq!(frames.last().unwrap(), "hi    ");
    assert!(frames.iter().any(|frame| frame != "hi    "), "{frames:?}");
    click("undo");
    assert_eq!(
        (frame_count().as_str(), art().as_str()),
        ("Frame 1 / 1", "hi    ")
    );
}

#[wasm_bindgen_test]
fn artwork_search_filters_the_catalogue() {
    blank(4, 1);
    input_event("art-search", "bird");
    let matches = element::<Element>("art-matches").text_content().unwrap();
    assert!(
        matches.ends_with(" pieces") && !matches.starts_with("0 "),
        "{matches}"
    );
    let choice = element::<HtmlSelectElement>("art-name").value();
    assert!(!choice.is_empty());
    element::<HtmlSelectElement>("mode").set_value("art");
    click("generate");
    assert!(art().bytes().any(|b| b != b' ' && b != b'\n'));
    input_event("art-search", "no-such-piece");
    assert!(
        element::<Element>("art-matches")
            .text_content()
            .unwrap()
            .starts_with("No artwork matches")
    );
    input_event("art-search", "");
}

#[wasm_bindgen_test]
fn errors_say_so_name_the_field_by_its_label_and_stay_in_view() {
    blank(4, 1);
    element::<HtmlSelectElement>("mode").set_value("scene");
    set_value("frames", "");
    click("generate");
    assert_eq!(status(), "Error: Frames must be a whole number, 0 or more");
    let region = element::<Element>("status");
    assert_eq!(region.get_attribute("data-kind").as_deref(), Some("error"));
    assert_eq!(region.get_attribute("role").as_deref(), Some("status"));
    assert_eq!(region.get_attribute("aria-live").as_deref(), Some("polite"));
    let style = web_sys::window()
        .unwrap()
        .get_computed_style(&region)
        .unwrap()
        .unwrap();
    assert_eq!(style.get_property_value("position").unwrap(), "sticky");
    set_value("frames", "80");

    choose("font-file", "large.flf", &vec![b'a'; 1024 * 1024 + 1]);
    element::<HtmlInputElement>("font-file")
        .dispatch_event(&web_sys::Event::new("change").unwrap())
        .unwrap();
    assert_eq!(status(), "Error: File exceeds the 1 MiB input limit");
    blank(4, 1);
    assert_eq!(region.get_attribute("data-kind").as_deref(), Some("info"));
}
