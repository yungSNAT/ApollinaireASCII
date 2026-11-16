// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Shape-matched characters from pictures and GIFs.
mod common;

use apollinaire_ascii::{
    Glyphs, RenderOptions, decode_media_as, image_renderer::RAMPS, render_image, render_image_as,
    video,
};
use common::{GifFrame, gif};
use image::{DynamicImage, GrayImage, Luma};

/// font8x8's `A`, as its README draws it: one byte a row, the lowest bit leftmost.
const A: [u8; 8] = [0x0C, 0x1E, 0x33, 0x33, 0x3F, 0x33, 0x33, 0x00];

/// One 8 by 16 cell, white where `ink` says, black elsewhere.
fn cell(ink: impl Fn(u32, u32) -> bool) -> DynamicImage {
    DynamicImage::ImageLuma8(GrayImage::from_fn(8, 16, |x, y| {
        Luma([if ink(x, y) { 255 } else { 0 }])
    }))
}

fn letter_a(x: u32, y: u32) -> bool {
    A[(y / 2) as usize] >> x & 1 == 1
}

fn one() -> RenderOptions {
    RenderOptions {
        width: 1,
        height: Some(1),
        ..RenderOptions::default()
    }
}

fn shaped(picture: &DynamicImage, options: &RenderOptions) -> String {
    render_image_as(picture, options, Glyphs::Shapes)
        .unwrap()
        .text()
}

#[test]
fn shapes_draw_a_letter_as_itself_and_dark_and_light_as_space_and_ink() {
    assert_eq!(shaped(&cell(letter_a), &one()), "A");
    assert_eq!(shaped(&cell(|_, _| false), &one()), " ");
    let light = shaped(&cell(|_, _| true), &one());
    assert_ne!(light, " ");
    // Inverted, white is paper.
    let inverted = RenderOptions {
        invert: true,
        ..one()
    };
    assert_eq!(shaped(&cell(|_, _| true), &inverted), " ");
    // A line drawing: a vertical stroke 2 pixels wide down the middle.
    let stroke = shaped(&cell(|x, _| x == 3 || x == 4), &one());
    assert!("|!:".contains(&stroke), "{stroke}");
    // The ramp is unchanged and still the default.
    for picture in [cell(letter_a), cell(|_, _| true)] {
        assert_eq!(
            render_image_as(&picture, &one(), Glyphs::Ramp).unwrap(),
            render_image(&picture, &one()).unwrap()
        );
    }
    assert_eq!(Glyphs::default(), Glyphs::Ramp);
}

#[test]
fn video_frames_take_shapes_from_their_rgba_pixels() {
    let rgba: Vec<u8> = (0..16)
        .flat_map(|y| (0..8).map(move |x| letter_a(x, y)))
        .flat_map(|ink| if ink { [255; 4] } else { [0, 0, 0, 255] })
        .collect();
    let frame = video::render_rgba_as(8, 16, rgba, &one(), Glyphs::Shapes).unwrap();
    assert_eq!(frame.text(), "A");
}

#[test]
fn shapes_refuse_ramp_options_and_follow_the_size_rules() {
    for options in [
        RenderOptions {
            dither: true,
            ..one()
        },
        RenderOptions {
            edges: true,
            ..one()
        },
        RenderOptions {
            edge_glyphs: true,
            ..one()
        },
        RenderOptions {
            ramp: RAMPS[1].1.into(),
            ..one()
        },
    ] {
        let error = render_image_as(&cell(letter_a), &options, Glyphs::Shapes).unwrap_err();
        assert!(error.starts_with("Shape matching"), "{error}");
    }
    let wide = RenderOptions {
        width: 513,
        ..one()
    };
    assert!(render_image_as(&cell(letter_a), &wide, Glyphs::Shapes).is_err());
    // Rows follow the picture's shape as the ramp's do: 3 cells across a 24 by 16 picture.
    let row = DynamicImage::ImageLuma8(GrayImage::from_fn(24, 16, |x, y| {
        Luma([if (8..16).contains(&x) && letter_a(x - 8, y) {
            255
        } else {
            0
        }])
    }));
    let options = RenderOptions {
        width: 3,
        cell_aspect: 1.0,
        ..RenderOptions::default()
    };
    let frame = render_image_as(&row, &options, Glyphs::Shapes).unwrap();
    assert_eq!((frame.width(), frame.height()), (3, 2));
    assert!(frame.cells().iter().all(|b| (32..=126).contains(b)));
}

#[test]
fn gif_frames_and_pictures_take_shapes_through_decode_media() {
    // 2 frames of 8 by 16: the letter, then a blank.
    let letter: Vec<u8> = (0..16 * 8)
        .map(|i| u8::from(letter_a(i % 8, i / 8)))
        .collect();
    let bytes = gif(
        8,
        16,
        Some(&[[0, 0, 0], [255, 255, 255]]),
        &[
            GifFrame::full(8, 16, letter, 5),
            GifFrame::full(8, 16, vec![0; 128], 5),
        ],
    );
    let project = decode_media_as(&bytes, &one(), Glyphs::Shapes).unwrap();
    let texts: Vec<String> = project.frames().iter().map(|f| f.frame.text()).collect();
    assert_eq!(texts, ["A", " "]);
    let mut png = Vec::new();
    cell(letter_a)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let still = decode_media_as(&png, &one(), Glyphs::Shapes).unwrap();
    assert_eq!(still.frames()[0].frame.text(), "A");
}
