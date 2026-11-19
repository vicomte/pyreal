//! The game's picture codecs with no device: DXT and BC blocks, JPEG, palettes, mip chains, surface decodes and the cursor's pictures.
//!
//! The modules: DXT and the BC formats (`dxt`, `d3dx_bc`, `block_codec`), JPEG (`jpeg`), palettes
//! (`palette`), mip chains (`mip`), the pixel formats and a surface's decode to the formats a GPU
//! takes (`pixel_format`, `texture`, `surface`, `sampler`) and the cursor's pictures (`cursor`).
//!
//! **Depends on** `pyreal-primitives` and the contract (`pyreal-client-contract`). **Used by** the
//! engine (`pyreal-engine`), the art pack's reader (`pyreal-art-pack`), the scene
//! (`pyreal-scene`), the shell and the desktop host (the cursor) and the art tool.
//!
//! **Must never** touch a graphics API or use `unsafe`, and never reaches the client runtime
//! (`cargo xtask seams`, `seam: client crates`). Its arithmetic goes through
//! `pyreal_primitives::num` wherever the original's float-to-int truncation is load-bearing.

#![doc(html_no_source)]

pub mod block_codec;
pub mod cursor;
mod d3dx_bc;
pub mod dxt;
pub mod jpeg;
pub mod mip;
pub mod palette;
pub mod pixel_format;
pub mod sampler;
pub mod surface;
pub mod texture;

pub use palette::ExpandedPalette;
pub use pixel_format::{PixelFormatDesc, PixelFormatId};
pub use surface::{Surface, SurfaceHandler};
pub use texture::{decode_surface, select_surface_format, Caps, SourcePixels};

#[derive(Debug, thiserror::Error)]
/// A picture that will not decode.
pub enum RenderError {
    /// A source pixel format this build does not decode.
    #[error("unsupported pixel format {0:?}")]
    UnsupportedFormat(PixelFormatId),
    /// The source bits are shorter than `width * height * bytes_per_pixel` requires.
    #[error(
        "source data is {actual} bytes; {expected} are needed for {width}x{height} {format:?}"
    )]
    ShortSourceData {
        format: PixelFormatId,
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    /// A dimension the client would have rejected (zero, or above the 2048 surface limit).
    #[error("bad dimensions {width}x{height}: {reason}")]
    BadDimensions {
        width: u32,
        height: u32,
        reason: &'static str,
    },
    /// The JPEG decoder rejected the stream, or produced something the client would not accept.
    #[error("jpeg decode failed: {0}")]
    Jpeg(String),
    /// A path the decoder deliberately does not implement; see the message for which and why.
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}
