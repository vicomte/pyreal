//! The presentation, as this executable draws through it: the runtime's device seam
//! (`Presentation`, `NullPresentation`, `Scene`) and what the interface adds to it
//! (`ClientPresentation`): where tracked objects project on screen, the marks drawn in the world
//! and the depth probes under the plugins' labels.
//!
//! Backend-neutral by construction: no method, type or argument here names a graphics API, and no
//! backend type appears in a signature. The device implementation is `crate::gpu::Renderer`,
//! behind the device features; `NullPresentation` is the headless one, which performs the device
//! steps as **counted no-ops**, so a headless `App` still records all fourteen `FrameStep`s in
//! order and a test can assert that the step it cares about was asked for.

use pyreal_primitives::ObjectId;

pub use pyreal_client_runtime::present::{
    NullPresentation, NullPresentationCounts, PresentError, Presentation, Scene, SceneCensus,
    SceneMut,
};
/// The world state the application owns; every world method is handed it.
pub use pyreal_client_runtime::world_state::WorldState;

/// Which preview space: the scene's, which holds the spaces (`crate::gpu` re-exports it too).
pub use pyreal_client_contract::overlay::PreviewSpace as PreviewId;

/// A mark placed in the world, drawn after it and against its depth (the scene's).
pub use pyreal_scene::marks::WorldMark;

/// A device error as the application reports it: the device's own message, unchanged.
#[must_use]
pub fn present_error(e: pyreal_codecs::RenderError) -> PresentError {
    PresentError(e.to_string())
}

/// What the interface adds to the device: where tracked objects project on screen, the marks
/// drawn in the world and the depth probes under the plugins' labels.
pub trait ClientPresentation: Presentation {
    /// Where a tracked object projects on screen, for the target reticule.
    fn target_projection(
        &self,
        id: ObjectId,
        world: Option<&WorldState>,
    ) -> Option<pyreal_client_contract::target::Projection>;
    /// The marks drawn in the world this frame, replacing the last frame's.
    fn set_world_marks(&mut self, marks: Vec<WorldMark>);
    /// Whether this presentation answers depth probes; one that does not shows every label.
    fn depth_probes_supported(&self) -> bool;
    /// Read the world's depth (0 at the near plane, 1 at the far one) at these back-buffer pixels
    /// once this frame is drawn; the answer comes back, with `tag`, through
    /// [`Self::take_depth_probes`] a frame or two later.
    fn request_depth_probes(&mut self, tag: u64, pixels: Vec<(u32, u32)>);
    /// The newest answer to [`Self::request_depth_probes`] since the last call: its tag and one
    /// depth a pixel, in the order asked.
    fn take_depth_probes(&mut self) -> Option<(u64, Vec<f32>)>;

    /// Where a tracked object's own origin stands on screen, at the height of its selection
    /// sphere's centre: the point to centre a name or a marker on.
    fn target_origin(&self, _id: ObjectId, _world: Option<&WorldState>) -> Option<(i32, i32)> {
        None
    }

    /// Where the point a name over a tracked object stands on is on screen, in pixels, and its
    /// depth as the world is drawn this frame (`z / w`: 0 at the near plane, 1 at the far), where
    /// the world keeps a depth of its own for it.
    fn target_top(
        &self,
        _id: ObjectId,
        _world: Option<&WorldState>,
    ) -> Option<(f32, f32, Option<f32>)> {
        None
    }

    /// The objects the last world draw could see (those a click could pick): `None` when no
    /// world has been drawn.
    fn drawn_objects(&self) -> Option<std::collections::BTreeSet<ObjectId>> {
        None
    }
}

macro_rules! counted_ui {
    ($presentation:ty) => {
        impl ClientPresentation for $presentation {
            fn target_projection(
                &self,
                _id: ObjectId,
                _world: Option<&WorldState>,
            ) -> Option<pyreal_client_contract::target::Projection> {
                None
            }
            fn set_world_marks(&mut self, marks: Vec<WorldMark>) {
                self.counts_mut().world_marks = u64::try_from(marks.len()).unwrap_or(u64::MAX);
            }
            fn depth_probes_supported(&self) -> bool {
                false
            }
            fn request_depth_probes(&mut self, _tag: u64, _pixels: Vec<(u32, u32)>) {}
            fn take_depth_probes(&mut self) -> Option<(u64, Vec<f32>)> {
                None
            }
        }
    };
}

counted_ui!(NullPresentation);
counted_ui!(pyreal_client_runtime::sim_present::SimPresentation);

#[cfg(test)]
mod tests {
    use super::{present_error, PresentError};
    use pyreal_codecs::{PixelFormatId, RenderError};

    /// A present error reads exactly as the device error it came from.
    #[test]
    fn a_present_error_reads_exactly_as_the_device_error_it_came_from() {
        let errors = [
            RenderError::BadDimensions {
                width: 0,
                height: 600,
                reason: "zero width",
            },
            RenderError::ShortSourceData {
                format: PixelFormatId::A8R8G8B8,
                width: 4,
                height: 4,
                expected: 64,
                actual: 3,
            },
            RenderError::UnsupportedFormat(PixelFormatId::Unknown),
        ];
        for device in errors {
            let text = device.to_string();
            let e = present_error(device);
            assert_eq!(e.to_string(), text);
            assert_eq!(format!("{e}"), text);
            assert_eq!(e, PresentError(text));
        }
    }

    /// A device error converts on the way out of a presentation method, and the result is an
    /// ordinary error with nothing underneath it.
    #[test]
    fn a_device_error_converts_on_the_way_out() {
        fn present() -> Result<(), PresentError> {
            let device: Result<(), RenderError> = Err(RenderError::BadDimensions {
                width: 4096,
                height: 1,
                reason: "too wide",
            });
            device.map_err(present_error)?;
            Ok(())
        }
        let e = present().unwrap_err();
        assert_eq!(e.to_string(), "bad dimensions 4096x1: too wide");
        let as_error: &dyn std::error::Error = &e;
        assert!(as_error.source().is_none());
    }
}
