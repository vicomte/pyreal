//! The sound device: the default output endpoint through `cpal` (`pyreal-audio-out`), which the
//! mixer in [`pyreal_client_runtime::audio`] plays through. Everything else is the runtime's, at
//! its old path.

pub use pyreal_audio_out::CpalOutput;
pub use pyreal_client_runtime::audio::*;

/// Install [`CpalOutput`] as the mixer's default endpoint. Idempotent.
pub fn install_default_output() {
    pyreal_client_runtime::audio::install_default_output(|| Box::new(CpalOutput::default()));
}
