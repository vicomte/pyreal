//! Shared preference values, registration defaults and choice tables.
//!
//! Runtime configuration and host adapters read this store directly. The interface's option pages
//! bind to the same values; their widgets and drafts stay in the interface crate.

pub mod cloud_detail;
pub mod config;
pub mod depth_mapping;
pub mod draw_distance;
pub mod dynamic_resolution;
pub mod fidelity;
pub mod fog_density;
pub mod god_rays;
pub mod grass_height;
pub mod ground_coloring;
pub mod landscape;
pub mod landscape_smoothing;
pub mod light_distance;
pub mod names;
pub mod object_detail;
pub mod performance;
pub mod portal_style;
pub mod preferences;
pub mod render_scale;
pub mod shadow_distance;
pub mod sheet;
pub mod sound_style;
pub mod store;
pub mod update_check;
pub mod water_detail;
pub mod weather_sound;

/// The title of the error box shown when opening a support URL fails (the shell's result is 32
/// or less). The Options page's support buttons raise the request; the host makes the call.
pub const SHELL_EXECUTE_ERROR_TITLE: &str = "Asheron's Call Error";
