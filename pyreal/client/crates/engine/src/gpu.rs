//! The device the engine draws with: the features the design is built on (measured by `pyreal-gpu-probe`),
//! asked for by name, and the adapter's limits.

use std::sync::Arc;

use crate::EngineError;

/// What the engine cannot run without: bindless textures, the instance index carrying a draw's
/// identity, and the dats' DXT blocks uploaded as they are.
pub const REQUIRED: wgpu::Features = wgpu::Features::TEXTURE_BINDING_ARRAY
    .union(wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING)
    .union(wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY)
    .union(wgpu::Features::INDIRECT_FIRST_INSTANCE)
    .union(wgpu::Features::TEXTURE_COMPRESSION_BC);

/// Used where present: pass timings read zero without it; the lamps' shadows are ray traced
/// with ray queries (the desktop tier), and drawn without shadows elsewhere.
pub const OPTIONAL: wgpu::Features =
    wgpu::Features::TIMESTAMP_QUERY.union(wgpu::Features::EXPERIMENTAL_RAY_QUERY);

/// `PYREAL_NO_RAY_QUERIES=1` leaves ray queries off for a driver that mishandles them: lamps cast
/// no shadows and light no bounce, as on a device without them.
#[must_use]
pub fn ray_queries_turned_off() -> bool {
    std::env::var_os("PYREAL_NO_RAY_QUERIES").is_some_and(|v| !v.is_empty() && v != "0")
}

/// Why an adapter offering `features` and `limits` cannot run the engine, when it cannot: a
/// [`REQUIRED`] feature missing, or too few textures in a binding array for the material table.
#[must_use]
pub fn unfit(features: wgpu::Features, limits: &wgpu::Limits) -> Option<String> {
    let missing = REQUIRED - features;
    if !missing.is_empty() {
        return Some(format!("it lacks {missing:?}"));
    }
    let room = limits.max_binding_array_elements_per_shader_stage;
    if room < crate::materials::MAX_TEXTURES {
        return Some(format!(
            "it binds {room} textures in an array where the engine needs {}",
            crate::materials::MAX_TEXTURES
        ));
    }
    None
}

/// The adapter, its device and queue.
#[derive(Debug, Clone)]
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub features: wgpu::Features,
    pub limits: wgpu::Limits,
    /// The pipelines' cache, where the device keeps one ([`crate::pipeline_cache`]), and the file
    /// it is kept in between runs.
    pub cache: Option<wgpu::PipelineCache>,
    cache_file: Option<Arc<std::path::PathBuf>>,
    /// How the cache started: read from its file, empty, or its file refused.
    pub cache_start: crate::pipeline_cache::Start,
}

impl Gpu {
    /// The high-performance adapter with the engine's features, for drawing to `surface` when
    /// one is given and offscreen otherwise; when that one is [`unfit`], the first other adapter
    /// that can draw there and is fit, a discrete one before an integrated one.
    ///
    /// # Errors
    /// No adapter, or none fit.
    pub fn new(
        instance: wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, EngineError> {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: surface,
            apply_limit_buckets: false,
        }))
        .map_err(|e| EngineError::Device(format!("no GPU adapter: {e}")))?;
        let adapter = match unfit(adapter.features(), &adapter.limits()) {
            None => adapter,
            Some(why) => {
                let rank = |a: &wgpu::Adapter| match a.get_info().device_type {
                    wgpu::DeviceType::DiscreteGpu => 0,
                    wgpu::DeviceType::IntegratedGpu => 1,
                    _ => 2,
                };
                let mut others: Vec<wgpu::Adapter> =
                    pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()))
                        .into_iter()
                        .filter(|a| unfit(a.features(), &a.limits()).is_none())
                        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
                        .collect();
                others.sort_by_key(rank);
                others.into_iter().next().ok_or_else(|| {
                    EngineError::Device(format!(
                        "the adapter {} cannot run the engine ({why}), nor can any other",
                        adapter.get_info().name
                    ))
                })?
            }
        };
        let offered = adapter.features();
        let optional = if ray_queries_turned_off() {
            OPTIONAL - wgpu::Features::EXPERIMENTAL_RAY_QUERY
        } else {
            OPTIONAL
        };
        // Vulkan keeps a pipeline cache the engine can hand back between runs.
        let features = REQUIRED | (optional & offered) | (wgpu::Features::PIPELINE_CACHE & offered);
        let limits = adapter.limits();
        let experimental = if features.contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY) {
            // SAFETY: the token only lets the device be asked for the adapter's experimental ray
            // queries; the engine uses them through wgpu's own validated API.
            #[allow(unsafe_code)]
            unsafe {
                wgpu::ExperimentalFeatures::enabled()
            }
        } else {
            wgpu::ExperimentalFeatures::default()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("engine"),
            required_features: features,
            required_limits: limits.clone(),
            experimental_features: experimental,
            ..Default::default()
        }))
        .map_err(|e| EngineError::Device(format!("no GPU device: {e}")))?;
        let (made, cache_start) = crate::pipeline_cache::make(&adapter, &device);
        let (cache, cache_file) = match made {
            Some((c, f)) => (Some(c), f.map(Arc::new)),
            None => (None, None),
        };
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            features,
            limits,
            cache,
            cache_file,
            cache_start,
        })
    }

    /// Writes the pipelines' cache to its file, when there is both: the file and how many bytes,
    /// or why it could not be written; `None` when there is nothing to write to. Cheap to call
    /// again: the device gives back what it holds now.
    #[must_use]
    pub fn save_pipeline_cache(&self) -> Option<(std::path::PathBuf, std::io::Result<usize>)> {
        let (cache, file) = (self.cache.as_ref()?, self.cache_file.as_ref()?);
        Some((
            file.as_ref().clone(),
            crate::pipeline_cache::save(cache, file),
        ))
    }

    /// A render pipeline from `desc`, made through the pipelines' cache when there is one.
    pub fn render_pipeline<'a>(
        &'a self,
        mut desc: wgpu::RenderPipelineDescriptor<'a>,
    ) -> wgpu::RenderPipeline {
        desc.cache = self.cache.as_ref();
        self.device.create_render_pipeline(&desc)
    }

    /// A compute pipeline from `desc`, made through the pipelines' cache when there is one.
    pub fn compute_pipeline<'a>(
        &'a self,
        mut desc: wgpu::ComputePipelineDescriptor<'a>,
    ) -> wgpu::ComputePipeline {
        desc.cache = self.cache.as_ref();
        self.device.create_compute_pipeline(&desc)
    }

    /// A device drawing to the window `display`/`window` names, and the window's surface (not
    /// yet configured).
    ///
    /// # Errors
    /// No surface for the window, or as [`Self::new`].
    pub fn for_window(
        display: wgpu::rwh::RawDisplayHandle,
        window: wgpu::rwh::RawWindowHandle,
    ) -> Result<(Self, wgpu::Surface<'static>), EngineError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        // SAFETY: the handles are the live window's, and the window outlives the surface: the
        // client drops its presentation before the window it was made from.
        #[allow(unsafe_code)]
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(display),
                raw_window_handle: window,
            })
        }
        .map_err(|e| EngineError::Device(format!("no surface: {e}")))?;
        let gpu = Self::new(instance, Some(&surface))?;
        Ok((gpu, surface))
    }

    /// An offscreen device, from an instance configured by the `WGPU_*` environment.
    ///
    /// # Errors
    /// As [`Self::new`].
    pub fn headless() -> Result<Self, EngineError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        Self::new(instance, None)
    }

    pub fn has(&self, f: wgpu::Features) -> bool {
        self.features.contains(f)
    }

    /// Whether the device traces rays in shaders (ray queries).
    #[must_use]
    pub fn rays(&self) -> bool {
        self.has(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
    }

    /// Blocks until the GPU has finished everything submitted.
    pub fn wait(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }

    /// A buffer's bytes, copied back once the GPU is done with it.
    pub fn read_buffer(&self, src: &wgpu::Buffer, offset: u64, size: u64) -> Vec<u8> {
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(src, offset, &staging, 0, size);
        self.queue.submit([enc.finish()]);
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.wait();
        let out = slice
            .get_mapped_range()
            .map(|v| v.to_vec())
            .unwrap_or_default();
        staging.unmap();
        out
    }

    /// A shader module from WGSL text.
    pub fn shader(&self, label: &str, wgsl: &str) -> wgpu::ShaderModule {
        self.device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            })
    }
}

/// A shared handle, for the parts that keep the device beside their own state.
pub type SharedGpu = Arc<Gpu>;
