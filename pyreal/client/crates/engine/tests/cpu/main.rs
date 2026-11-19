//! The engine's CPU half, with no device and no dats: mesh conversion, the material fallbacks,
//! the pools' allocator, the frame graph's checks, the record layouts, the land field, the
//! frame's jitter, upscaler and dynamic resolution, and every shader.

mod adapter;
mod backends;
mod cells;
mod clusters;
mod conversion;
mod cover;
mod far;
mod graph;
mod heavens;
mod land;
mod layout;
mod materials;
mod puff_runs;
mod shaders;
mod shadows;
mod shore;
mod skin;
mod temporal;
mod water;
mod water_heights;
mod water_patches;
