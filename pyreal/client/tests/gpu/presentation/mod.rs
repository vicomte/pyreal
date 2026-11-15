//! Presentation: the window, resolution and display preferences, sync to refresh, screenshots and the headless capture,
//! frame pacing without a physics tick, the shell's input and sound, shutdown, the world clock,
//! and what stays bounded over a long session.

mod engine_frames;
mod headless_capture_determinism;
mod headless_settings;
mod pre_tod_capture;
mod screenshot_file;
mod selected_preferences_profile;
mod shell;
mod shutdown;
mod window_position;
mod world_clock_sync;
