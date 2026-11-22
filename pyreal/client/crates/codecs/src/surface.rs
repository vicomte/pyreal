//! The surface flags that select material and pass behavior.

/// Surface-type bits stored in the dat payload.
///
/// Surface flags that select material and pass behavior.
pub mod surface_type {
    /// The surface is a flat colour (`color_value`), no texture.
    pub const BASE1_SOLID: u32 = 0x0000_0001;
    /// The surface has a texture map.
    pub const BASE1_IMAGE: u32 = 0x0000_0002;
    /// The texture has a 1-bit cutout: alpha test on.
    pub const BASE1_CLIPMAP: u32 = 0x0000_0004;
    /// Use `translucency` as a constant alpha.
    pub const TRANSLUCENT: u32 = 0x0000_0010;
    /// (data-side) the `diffuse` scalar is meaningful.
    pub const DIFFUSE: u32 = 0x0000_0020;
    /// (data-side) the `luminosity` scalar is meaningful.
    pub const LUMINOUS: u32 = 0x0000_0040;
    /// src = `SRCALPHA`.
    pub const ALPHA: u32 = 0x0000_0100;
    /// src = `INVSRCALPHA`.
    pub const INVALPHA: u32 = 0x0000_0200;
    /// dst = `ONE` (and fog is suppressed).
    pub const ADDITIVE: u32 = 0x0001_0000;
    /// (data-side) marks a detail surface.
    pub const DETAIL: u32 = 0x0002_0000;
    /// Always forced on when a surface is installed.
    pub const GOURAUD: u32 = 0x1000_0000;
    /// Reused at runtime as "this texture is tiled (WRAP addressing)".
    pub const STIPPLED: u32 = 0x4000_0000;
    /// Legacy software-rasteriser flag; ignored by the D3D path.
    pub const PERSPECTIVE: u32 = 0x8000_0000;
}
