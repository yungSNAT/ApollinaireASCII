// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! REXPaint `.xp` images read as layers. `mltest.xp` is bracket-lib's 2-layer test image; the
//! other files are built here from the layout in REXPaint's manual.
use apollinaire_ascii::{Layered, rexpaint::read};
use flate2::{Compression, write::GzEncoder};
use std::io::Write;

const MAGENTA: [u8; 3] = [255, 0, 255];
const BLACK: [u8; 3] = [0, 0, 0];

/// One cell: a glyph code, a white foreground and the given background.
fn cell(code: u32, background: [u8; 3]) -> Vec<u8> {
    [&code.to_le_bytes()[..], &[255, 255, 255], &background].concat()
}

/// A layer of `width` by `height` cells, given column by column as REXPaint stores them.
fn layer(width: i32, height: i32, cells: &[Vec<u8>]) -> Vec<u8> {
    [
        width.to_le_bytes().to_vec(),
        height.to_le_bytes().to_vec(),
        cells.concat(),
    ]
    .concat()
}

/// Opaque cells for each code in `codes`.
fn opaque(codes: &[u8]) -> Vec<Vec<u8>> {
    codes.iter().map(|&code| cell(code.into(), BLACK)).collect()
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut file = GzEncoder::new(Vec::new(), Compression::default());
    file.write_all(data).unwrap();
    file.finish().unwrap()
}

/// A current-format file: version -1, the layer count, then the layers.
fn xp(layers: &[Vec<u8>]) -> Vec<u8> {
    let count = i32::try_from(layers.len()).unwrap();
    gzip(
        &[
            &(-1i32).to_le_bytes()[..],
            &count.to_le_bytes(),
            &layers.concat(),
        ]
        .concat(),
    )
}

fn texts(project: &Layered) -> Vec<String> {
    project
        .layers()
        .iter()
        .map(|layer| layer.art.frames()[0].frame.text())
        .collect()
}

fn flat(project: &Layered) -> String {
    project.flatten().frames()[0].frame.text()
}

#[test]
fn a_real_image_keeps_its_layers_bottom_first_and_shows_the_bottom_through_transparent_cells() {
    let project = read(include_bytes!("fixtures/rexpaint/mltest.xp")).unwrap();
    let names: Vec<_> = project.layers().iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Layer 1", "Layer 2"]);
    assert!(project.layers().iter().all(|layer| layer.visible));
    assert_eq!(
        texts(&project),
        [
            "AAAAAAAA\nAAAAAAAA\nAAAAAAAA\nAAAAAAAA",
            "        \n  BBBB  \n  BBBB  \n        "
        ]
    );
    assert_eq!(flat(&project), "AAAAAAAA\nAABBBBAA\nAABBBBAA\nAAAAAAAA");
}

#[test]
fn cells_fill_column_by_column_and_draw_as_cp437_draws_their_codes() {
    // 3 columns by 2 rows: a b in the first column, c d in the second, a heart and a full
    // block in the third.
    let project = read(&xp(&[layer(3, 2, &opaque(b"abcd\x03\xdb"))])).unwrap();
    assert_eq!(texts(&project), ["ac*\nbd#"]);
    // Codes 0 and 32 are both blank, and a transparent cell is blank whatever its code.
    let cells = [
        cell(0, BLACK),
        cell(32, BLACK),
        cell(u32::from(b'Z'), MAGENTA),
    ];
    assert_eq!(texts(&read(&xp(&[layer(3, 1, &cells)])).unwrap()), ["   "]);
}

#[test]
fn an_older_file_starts_with_its_layer_count() {
    let file = gzip(&[&1i32.to_le_bytes()[..], &layer(2, 1, &opaque(b"ok"))].concat());
    assert_eq!(texts(&read(&file).unwrap()), ["ok"]);
}

#[test]
fn an_opaque_space_shows_the_layer_below_unlike_in_rexpaint() {
    // REXPaint would hide the x under the top layer's space on a black background; a space in
    // a layer here always shows what is below.
    let bottom = layer(2, 1, &opaque(b"xy"));
    let top = layer(2, 1, &[cell(32, BLACK), cell(u32::from(b'z'), BLACK)]);
    let project = read(&xp(&[bottom, top])).unwrap();
    assert_eq!(flat(&project), "xz");
}

#[test]
fn malformed_files_are_refused_with_the_reason() {
    let refused = |bytes: &[u8]| read(bytes).unwrap_err();
    let one = || layer(1, 1, &opaque(b"a"));
    assert!(refused(b"not gzip").starts_with("Not a REXPaint .xp file"));
    assert!(refused(&xp(&[])).contains("0 layers"));
    assert!(refused(&xp(&vec![one(); 10])).contains("10 layers; REXPaint saves 1 to 9"));
    assert_eq!(texts(&read(&xp(&vec![one(); 9])).unwrap()).len(), 9);
    for (width, height) in [(513, 1), (1, 257), (0, 1), (-1, 1)] {
        let message = refused(&xp(&[layer(width, height, &[])]));
        assert!(
            message.contains(&format!(
                "{width} by {height} cells; the limit is 512 by 256"
            )),
            "{message}"
        );
    }
    let big = vec![cell(65, BLACK); 512 * 256];
    let largest = read(&xp(&[layer(512, 256, &big)])).unwrap();
    assert_eq!(largest.flatten().width(), 512);
    let mut short = [(-1i32).to_le_bytes(), 1i32.to_le_bytes()].concat();
    short.extend(&layer(2, 1, &opaque(b"a")));
    assert!(refused(&gzip(&short)).contains("ends inside its image"));
    assert!(refused(&gzip(&short[..10])).contains("ends inside its image"));
    let mut long = [(-1i32).to_le_bytes(), 1i32.to_le_bytes()].concat();
    long.extend(&one());
    long.extend([0, 0, 0]);
    assert!(refused(&gzip(&long)).contains("3 bytes after its last layer"));
    let extended = layer(
        2,
        2,
        &[
            cell(65, BLACK),
            cell(65, BLACK),
            cell(65, BLACK),
            cell(256, BLACK),
        ],
    );
    assert!(refused(&xp(&[extended])).contains(
        "Layer 1 of the .xp file has glyph code 256 at column 2, row 2; only codes 0 to 255"
    ));
    let sizes = refused(&xp(&[one(), layer(2, 1, &opaque(b"ab"))]));
    assert!(sizes.contains("same dimensions"), "{sizes}");
    let huge = gzip(&vec![0; 16 * 1024 * 1024 + 1]);
    assert!(refused(&huge).contains("more than the 16 MiB input limit"));
}
