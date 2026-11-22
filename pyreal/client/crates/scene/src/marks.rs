//! Marks a front end places in the world: lines, outlines and dots, drawn after the world and
//! against its depth, so a wall or a hill in front of a mark hides it. A mark that asks to stay
//! on top is drawn over everything instead.
//!
//! A line is a ribbon that faces the camera and keeps its width in pixels at any distance; a
//! single point is a square dot of that side.

/// One mark: points in the world's own space (Z up), joined in order. A mark of one point is a
/// dot.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldMark {
    pub points: Vec<[f32; 3]>,
    /// `0xAARRGGBB`.
    pub color: u32,
    /// The line's width, or the dot's side, in back-buffer pixels.
    pub width: f32,
    /// Drawn over whatever stands in front of it.
    pub on_top: bool,
}
