//! Static art, lettering and animation as printable ASCII text.
//!
//! ```
//! use apollinaire_ascii::{Frame, Animation};
//! let frame = Frame::from_text(" /\\\n/  ")?;
//! assert!(frame.text().is_ascii());
//! let project = Animation::still(frame);
//! assert_eq!(Animation::from_json(&project.to_json())?, project);
//! # Ok::<(), String>(())
//! ```

pub mod animation;
pub mod art;
pub mod bubble;
pub mod canvas;
pub mod diagram;
pub mod effects;
pub mod export;
pub mod frame;
mod gif;
pub mod image_renderer;
pub mod lettering;
mod project_json;
pub mod video;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
mod web;

pub use animation::{Animation, SceneOptions, TimedFrame};
pub use canvas::Canvas;
pub use effects::EffectOptions;
pub use frame::{BorderStyle, Frame};
pub use image_renderer::{RenderOptions, decode_media, render_image};

pub type Result<T> = std::result::Result<T, String>;
pub const MAX_WIDTH: usize = 512;
pub const MAX_HEIGHT: usize = 256;
pub const MAX_CELLS: usize = MAX_WIDTH * MAX_HEIGHT;
pub const MAX_FRAMES: usize = 600;
pub const MAX_PROJECT_CELLS: usize = 8_000_000;
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn printable(bytes: &[u8]) -> Result<()> {
    if let Some((index, byte)) = bytes
        .iter()
        .enumerate()
        .find(|(_, b)| !(32..=126).contains(*b))
    {
        return Err(format!(
            "Only printable ASCII is allowed: byte {index} is 0x{byte:02x}"
        ));
    }
    Ok(())
}

pub(crate) fn dimensions(width: usize, height: usize) -> Result<usize> {
    if width == 0 || height == 0 || width > MAX_WIDTH || height > MAX_HEIGHT {
        return Err(format!(
            "Dimensions must be 1..={MAX_WIDTH} columns and 1..={MAX_HEIGHT} rows"
        ));
    }
    width
        .checked_mul(height)
        .filter(|n| *n <= MAX_CELLS)
        .ok_or_else(|| "Too many cells".into())
}
