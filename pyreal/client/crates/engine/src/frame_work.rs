//! What the engine made on this thread since it was last asked: textures and their bytes, meshes,
//! pipelines and shader modules and their time, ray-tracing structures built, and bindless groups
//! rebuilt. The hitch reports read it once a frame ([`take`]).
//!
//! The counts are the calling thread's own, so the frame's thread sees only what it made itself
//! and not what a pool made beside it. Counting costs a thread-local add.

use std::cell::Cell;

/// What was made on this thread since the last [`take`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameWork {
    pub textures: u32,
    pub texture_bytes: u64,
    pub meshes: u32,
    pub pipelines: u32,
    pub shaders: u32,
    /// Making the pipelines and shader modules, ms.
    pub pipeline_ms: f32,
    /// Bottom-level ray-tracing structures built, and top levels.
    pub blas: u32,
    pub tlas: u32,
    /// The ray-tracing structures' upkeep on the processor, ms.
    pub rays_ms: f32,
    /// Bindless texture groups rebuilt.
    pub bind_groups: u32,
    /// Writing the frame's changed instances, materials and pictures to the device, ms.
    pub upload_ms: f32,
}

thread_local! {
    static WORK: Cell<FrameWork> = const { Cell::new(FrameWork {
        textures: 0,
        texture_bytes: 0,
        meshes: 0,
        pipelines: 0,
        shaders: 0,
        pipeline_ms: 0.0,
        blas: 0,
        tlas: 0,
        rays_ms: 0.0,
        bind_groups: 0,
        upload_ms: 0.0,
    }) };
}

/// Adds to this thread's counts.
pub fn note(f: impl FnOnce(&mut FrameWork)) {
    WORK.with(|w| {
        let mut v = w.get();
        f(&mut v);
        w.set(v);
    });
}

/// This thread's counts since the last call, which start again from nothing.
#[must_use]
pub fn take() -> FrameWork {
    WORK.with(|w| w.replace(FrameWork::default()))
}

/// Milliseconds since `t`.
#[must_use]
pub fn ms_since(t: std::time::Instant) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let ms = (t.elapsed().as_secs_f64() * 1e3) as f32;
    ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_the_threads_own_and_start_again_once_taken() {
        let _ = take();
        note(|w| w.textures += 2);
        note(|w| w.texture_bytes += 1024);
        std::thread::spawn(|| note(|w| w.textures += 100))
            .join()
            .unwrap();
        let w = take();
        assert_eq!((w.textures, w.texture_bytes), (2, 1024));
        assert_eq!(take(), FrameWork::default());
    }
}
