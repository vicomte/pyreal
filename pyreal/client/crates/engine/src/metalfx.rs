//! MetalFX's temporal upscaling: each frame the scaler takes the
//! picture drawn at fewer pixels, its depth and how far each pixel moved, and rebuilds it at the
//! output's size from those and the frames before it. Only Metal has it: anywhere else no scaler
//! is made ([`TemporalScaler::new`]) and the engine's own temporal resolve takes its place.
//!
//! `wgpu` does not allow raw commands and its own in one encoder, so a scale is a submission of its
//! own ([`TemporalScaler::encode`]); everything drawn before it must have been submitted.
//!
//! One scaler serves an output size at every render scale down to [`MOST_SCALE`]: it is made with
//! MetalFX's dynamic resolution and told each frame how large the picture was drawn. Rebuilding a
//! scaler per scale stalled each step, and leaked about 150 MB a time at smaller outputs.
//!
//! At large outputs MetalFX runs part of each scale on the Neural Engine, which waits for the GPU's
//! share; once Metal stops running a process's command buffers after GPU errors, those requests
//! never finish, and the 128th aborts the process. Each scale's command buffer is watched
//! ([`Watch`]): a failed one turns MetalFX off for the session, and too many unfinished hold it back.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// How MetalFX's scales are faring: how many are unfinished, and whether one failed.
#[derive(Debug, Default)]
pub struct Watch {
    pending: AtomicU32,
    failed: AtomicBool,
}

impl Watch {
    /// Unfinished scales past which none is added, well inside the Neural Engine's queue of 127.
    pub const MOST_PENDING: u32 = 8;

    /// A scale's command buffer ended in an error: MetalFX is off from here on.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    /// Scales encoded whose command buffers have not completed.
    #[must_use]
    pub fn pending(&self) -> u32 {
        self.pending.load(Ordering::Acquire)
    }

    /// Whether another scale may be encoded now.
    #[must_use]
    pub fn ready(&self) -> bool {
        !self.failed() && self.pending() < Self::MOST_PENDING
    }

    /// A scale was encoded.
    pub fn begin(&self) {
        self.pending.fetch_add(1, Ordering::AcqRel);
    }

    /// A scale's command buffer completed, `ok` or in an error.
    pub fn end(&self, ok: bool) {
        if !ok {
            self.failed.store(true, Ordering::Release);
        }
        let _ = self
            .pending
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1));
    }
}

/// What one frame's scale reads and writes.
#[derive(Debug, Clone, Copy)]
pub struct ScaleInputs<'a> {
    /// The picture as drawn, jittered, in [`COLOUR_FORMAT`].
    pub colour: &'a wgpu::Texture,
    /// Its depth, `Depth32Float`, reversed (near is one).
    pub depth: &'a wgpu::Texture,
    /// How far each pixel moved, in [`MOTION_FORMAT`], without the jitter.
    pub motion: &'a wgpu::Texture,
    /// How little each pixel should keep of the frames before it, `R8Unorm`.
    pub reactive: &'a wgpu::Texture,
    /// Where the picture goes, at the output size, in [`COLOUR_FORMAT`], with storage use.
    pub output: &'a wgpu::Texture,
    /// The frame's jitter, in the drawn picture's pixels.
    pub jitter: [f32; 2],
    /// What turns a `motion` texel into pixels toward where the surface last stood.
    pub motion_scale: [f32; 2],
    /// Forget the frames before this one.
    pub reset: bool,
    /// The size the picture was drawn at, each input's size.
    pub size: (u32, u32),
}

/// The most a scaler enlarges by: render scales down to half.
pub const MOST_SCALE: f32 = 2.0;

/// Whether a picture drawn at `input` may be scaled to `output`, enlarging by at most `most`.
#[must_use]
pub fn within(input: (u32, u32), output: (u32, u32), most: f32) -> bool {
    #[allow(clippy::cast_precision_loss)]
    let side = |i: u32, o: u32| i > 0 && i <= o && o as f32 <= i as f32 * most + 0.5;
    side(input.0, output.0) && side(input.1, output.1)
}

/// The picture's format, in and out.
pub const COLOUR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The motion texture's format.
pub const MOTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;

#[cfg(target_os = "macos")]
pub use metal::TemporalScaler;

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod metal {
    use std::ptr::NonNull;
    use std::sync::Arc;

    use block2::RcBlock;
    use objc2::rc::{autoreleasepool, Retained};
    use objc2::runtime::{NSObjectProtocol, ProtocolObject};
    use objc2::{sel, ClassType, Message};
    use objc2_metal::{MTLCommandBuffer, MTLCommandBufferStatus, MTLPixelFormat, MTLTexture};
    use objc2_metal_fx::{
        MTLFXTemporalScaler, MTLFXTemporalScalerBase, MTLFXTemporalScalerDescriptor,
    };
    use wgpu::hal::api::Metal;

    use super::{within, ScaleInputs, Watch, MOST_SCALE};

    /// A MetalFX temporal scaler for one output size, at any drawn size it accepts.
    #[derive(Debug)]
    pub struct TemporalScaler {
        scaler: Retained<ProtocolObject<dyn MTLFXTemporalScaler>>,
        pub output: (u32, u32),
        /// The most it enlarges by.
        most: f32,
        reactive: bool,
    }

    /// The texture's Metal object, on a device that is Metal.
    fn raw(texture: &wgpu::Texture) -> Option<Retained<ProtocolObject<dyn MTLTexture>>> {
        // SAFETY: the object is only retained here, never destroyed; `wgpu` keeps owning it.
        let hal = unsafe { texture.as_hal::<Metal>() }?;
        Some(hal.raw_handle().retain())
    }

    impl TemporalScaler {
        /// A scaler to `output` pixels from any drawn size down to [`MOST_SCALE`] smaller (or
        /// what the GPU allows); `None` where the device is not Metal or has no MetalFX.
        #[must_use]
        pub fn new(device: &wgpu::Device, output: (u32, u32)) -> Option<Self> {
            // SAFETY: the device is only read here, and outlives the call.
            let hal = unsafe { device.as_hal::<Metal>() }?;
            let raw = hal.raw_device();
            // SAFETY: plain property writes on an object made here, with values in the ranges
            // the header gives, and a device that is alive; the reactive mask's are asked of the
            // object first, as they are newer than the framework's first release, and so is the
            // GPU's largest scale.
            let (scaler, most, reactive) = unsafe {
                if !MTLFXTemporalScalerDescriptor::supportsDevice(raw) {
                    return None;
                }
                let class = MTLFXTemporalScalerDescriptor::class();
                let most = if class.responds_to(sel!(supportedInputContentMaxScaleForDevice:)) {
                    MTLFXTemporalScalerDescriptor::supportedInputContentMaxScaleForDevice(raw)
                        .min(MOST_SCALE)
                } else {
                    MOST_SCALE
                };
                if most < 1.0 {
                    return None;
                }
                let d = MTLFXTemporalScalerDescriptor::new();
                d.setColorTextureFormat(MTLPixelFormat::RGBA16Float);
                d.setDepthTextureFormat(MTLPixelFormat::Depth32Float);
                d.setMotionTextureFormat(MTLPixelFormat::RG16Float);
                d.setOutputTextureFormat(MTLPixelFormat::RGBA16Float);
                // The largest picture drawn is the output's size; each frame names its own.
                d.setInputWidth(output.0 as usize);
                d.setInputHeight(output.1 as usize);
                d.setOutputWidth(output.0 as usize);
                d.setOutputHeight(output.1 as usize);
                // The picture is tone mapped already: no exposure of its own.
                d.setAutoExposureEnabled(false);
                d.setInputContentPropertiesEnabled(true);
                d.setInputContentMinScale(1.0);
                d.setInputContentMaxScale(most);
                let reactive = d.respondsToSelector(sel!(setReactiveMaskTextureEnabled:));
                if reactive {
                    d.setReactiveMaskTextureEnabled(true);
                    d.setReactiveMaskTextureFormat(MTLPixelFormat::R8Unorm);
                }
                (d.newTemporalScalerWithDevice(raw)?, most, reactive)
            };
            // SAFETY: a plain property write on the scaler just made.
            unsafe { scaler.setDepthReversed(true) };
            Some(Self {
                scaler,
                output,
                most,
                reactive,
            })
        }

        /// Whether it scales a picture drawn at `input` to its output.
        #[must_use]
        pub fn accepts(&self, input: (u32, u32)) -> bool {
            within(input, self.output, self.most)
        }

        /// Scales one frame, as a submission of its own after everything already submitted, its
        /// command buffer reported to `watch`. False when a texture is not Metal's, and nothing was
        /// scaled.
        #[must_use]
        pub fn encode(
            &self,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            inputs: &ScaleInputs<'_>,
            watch: &Arc<Watch>,
        ) -> bool {
            let (Some(colour), Some(depth), Some(motion), Some(reactive), Some(output)) = (
                raw(inputs.colour),
                raw(inputs.depth),
                raw(inputs.motion),
                raw(inputs.reactive),
                raw(inputs.output),
            ) else {
                return false;
            };
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("MetalFX upscale"),
            });
            // SAFETY: this encoder records nothing through `wgpu`, and the command buffer is
            // neither ended nor destroyed here. Each texture was written by a submission before
            // this one and is alive for the call; the scaler holds its own references to them
            // after it. Only the scaler encodes into the command buffer, which has no encoder open.
            let encoded = unsafe {
                encoder.as_hal_mut::<Metal, _, _>(|hal| {
                    let Some(buffer) = hal.and_then(|h| h.raw_command_buffer()) else {
                        return false;
                    };
                    let s = &self.scaler;
                    s.setColorTexture(Some(&colour));
                    s.setDepthTexture(Some(&depth));
                    s.setMotionTexture(Some(&motion));
                    if self.reactive {
                        s.setReactiveMaskTexture(Some(&reactive));
                    }
                    s.setOutputTexture(Some(&output));
                    s.setInputContentWidth(inputs.size.0 as usize);
                    s.setInputContentHeight(inputs.size.1 as usize);
                    s.setJitterOffsetX(inputs.jitter[0]);
                    s.setJitterOffsetY(inputs.jitter[1]);
                    s.setMotionVectorScaleX(inputs.motion_scale[0]);
                    s.setMotionVectorScaleY(inputs.motion_scale[1]);
                    s.setReset(inputs.reset);
                    autoreleasepool(|_| s.encodeToCommandBuffer(buffer));
                    watch.begin();
                    let w = Arc::clone(watch);
                    let done =
                        RcBlock::new(move |b: NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
                            // Metal hands the completed command buffer, alive for the call.
                            let status = b.as_ref().status();
                            w.end(status != MTLCommandBufferStatus::Error);
                        });
                    // Metal copies the block; the buffer is not yet committed.
                    buffer.addCompletedHandler(RcBlock::as_ptr(&done));
                    true
                })
            };
            queue.submit([encoder.finish()]);
            encoded
        }
    }
}

/// A scaler no platform but macOS can make.
#[cfg(not(target_os = "macos"))]
#[derive(Debug)]
pub struct TemporalScaler {
    pub output: (u32, u32),
}

#[cfg(not(target_os = "macos"))]
impl TemporalScaler {
    /// There is no MetalFX here.
    #[must_use]
    pub fn new(_device: &wgpu::Device, _output: (u32, u32)) -> Option<Self> {
        None
    }

    /// It scales nothing.
    #[must_use]
    pub fn accepts(&self, _input: (u32, u32)) -> bool {
        false
    }

    /// Nothing is scaled.
    #[must_use]
    pub fn encode(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _inputs: &ScaleInputs<'_>,
        _watch: &std::sync::Arc<Watch>,
    ) -> bool {
        false
    }
}
