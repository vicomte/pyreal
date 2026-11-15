//! The device, and the small helpers every workload uses: buffers, readback, error scopes and
//! timestamp queries.

use std::time::Instant;

use wgpu::util::DeviceExt as _;

/// The adapter and a device on it with everything the adapter offers.
#[derive(Debug)]
pub struct Gpu {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub features: wgpu::Features,
    pub limits: wgpu::Limits,
    pub instance_flags: wgpu::InstanceFlags,
    warm: (wgpu::ComputePipeline, wgpu::BindGroup),
}

/// Busy work that brings the GPU's clocks up before a timed run.
const WARM: &str = "
@group(0) @binding(0) var<storage, read_write> data: array<vec4<f32>>;
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = id.x + id.y * nw.x * 256u;
    var v = data[i];
    for (var k = 0; k < 64; k++) { v = v * 0.999 + vec4<f32>(0.001); }
    data[i] = v;
}";

impl Gpu {
    pub fn new() -> Result<Self, String> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        // Direct3D 12 and Vulkan refuse a buffer past 90% of the card's memory budget, so the
        // large-buffer tests skip on a smaller card rather than overcommitting it.
        desc.memory_budget_thresholds.for_resource_creation = Some(90);
        let instance_flags = desc.flags;
        let instance = wgpu::Instance::new(desc);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let features = adapter.features();
        let limits = adapter.limits();
        let experimental = if features.intersects(
            wgpu::Features::EXPERIMENTAL_RAY_QUERY
                | wgpu::Features::EXPERIMENTAL_MESH_SHADER
                | wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX
                | wgpu::Features::EXPERIMENTAL_RAY_HIT_VERTEX_RETURN
                | wgpu::Features::EXPERIMENTAL_RAY_TRACING_PIPELINES
                | wgpu::Features::EXPERIMENTAL_MESH_SHADER_MULTIVIEW
                | wgpu::Features::EXPERIMENTAL_MESH_SHADER_POINTS,
        ) {
            // SAFETY: the token only lets the device be asked for the adapter's experimental
            // features; the probe uses them through wgpu's validated API.
            #[allow(unsafe_code)]
            unsafe {
                wgpu::ExperimentalFeatures::enabled()
            }
        } else {
            wgpu::ExperimentalFeatures::default()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gpu-probe"),
            required_features: features,
            required_limits: limits.clone(),
            experimental_features: experimental,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU device: {e}"))?;
        device.on_uncaptured_error(std::sync::Arc::new(|e| eprintln!("uncaptured: {e}")));
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("warm"),
            source: wgpu::ShaderSource::Wgsl(WARM.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("warm"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let data = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("warm"),
            size: 64 << 20,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("warm"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: data.as_entire_binding(),
            }],
        });
        Ok(Self {
            adapter,
            device,
            queue,
            features,
            limits,
            instance_flags,
            warm: (pipeline, group),
        })
    }

    /// Keeps the GPU busy for about 200 ms, so a timed run that follows starts at full clocks.
    pub fn warm(&self) {
        let t = Instant::now();
        while t.elapsed().as_millis() < 200 {
            let mut enc = self.encoder();
            {
                let mut pass = enc.begin_compute_pass(&Default::default());
                pass.set_pipeline(&self.warm.0);
                pass.set_bind_group(0, &self.warm.1, &[]);
                pass.dispatch_workgroups(4096, 4, 1);
            }
            self.queue.submit([enc.finish()]);
            self.wait();
        }
    }

    pub fn has(&self, f: wgpu::Features) -> bool {
        self.features.contains(f)
    }

    /// Blocks until the GPU has finished everything submitted.
    pub fn wait(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }

    /// Runs `f` inside validation, out-of-memory and internal error scopes, so a failure is a
    /// result rather than a panic.
    pub fn scoped<T>(&self, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let oom = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let out = f();
        self.wait();
        let errors = [
            pollster::block_on(validation.pop()),
            pollster::block_on(oom.pop()),
            pollster::block_on(internal.pop()),
        ];
        if let Some(e) = errors.into_iter().flatten().next() {
            return Err(one_line(&e.to_string()));
        }
        out
    }

    pub fn buffer(&self, label: &str, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage,
            mapped_at_creation: false,
        })
    }

    pub fn buffer_init(
        &self,
        label: &str,
        bytes: &[u8],
        usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytes,
                usage,
            })
    }

    pub fn shader(&self, label: &str, wgsl: &str) -> wgpu::ShaderModule {
        self.device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            })
    }

    /// A compute pipeline with a layout derived from the shader.
    pub fn compute(&self, label: &str, wgsl: &str) -> wgpu::ComputePipeline {
        let module = self.shader(label, wgsl);
        self.device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            })
    }

    /// A bind group of whole buffers at bindings 0, 1, ... of `pipeline`'s group 0.
    pub fn bind_buffers(
        &self,
        pipeline: &wgpu::ComputePipeline,
        buffers: &[&wgpu::Buffer],
    ) -> wgpu::BindGroup {
        let entries: Vec<wgpu::BindGroupEntry> = buffers
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: u32::try_from(i).unwrap_or(0),
                resource: b.as_entire_binding(),
            })
            .collect();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        })
    }

    /// Copies `size` bytes at `offset` of `src` back to the CPU.
    pub fn read(&self, src: &wgpu::Buffer, offset: u64, size: u64) -> Vec<u8> {
        let staging = self.buffer(
            "readback",
            size,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        let mut enc = self.encoder();
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

    pub fn read_u32s(&self, src: &wgpu::Buffer, offset: u64, count: usize) -> Vec<u32> {
        let bytes = self.read(src, offset, count as u64 * 4);
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect()
    }

    pub fn encoder(&self) -> wgpu::CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None })
    }

    /// Submits `enc` and waits for it: the wall-clock milliseconds from submit to done.
    pub fn submit_wait(&self, enc: wgpu::CommandEncoder) -> f64 {
        let t = Instant::now();
        self.queue.submit([enc.finish()]);
        self.wait();
        t.elapsed().as_secs_f64() * 1e3
    }

    /// A timer for `pairs` begin/end pairs, when the device has timestamp queries.
    pub fn timer(&self, pairs: u32) -> Option<Timer> {
        if !self.has(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let count = pairs * 2;
        let set = self.device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("timer"),
            ty: wgpu::QueryType::Timestamp,
            count,
        });
        let resolve = self.buffer(
            "timer resolve",
            u64::from(count) * 8,
            wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        );
        Some(Timer {
            set,
            resolve,
            count,
            period: f64::from(self.queue.get_timestamp_period()),
        })
    }
}

/// Timestamp queries in begin/end pairs, read back in milliseconds.
#[derive(Debug)]
pub struct Timer {
    pub set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    count: u32,
    period: f64,
}

impl Timer {
    pub fn compute_pass(&self, pair: u32) -> wgpu::ComputePassTimestampWrites<'_> {
        wgpu::ComputePassTimestampWrites {
            query_set: &self.set,
            beginning_of_pass_write_index: Some(pair * 2),
            end_of_pass_write_index: Some(pair * 2 + 1),
        }
    }

    pub fn render_pass(&self, pair: u32) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.set,
            beginning_of_pass_write_index: Some(pair * 2),
            end_of_pass_write_index: Some(pair * 2 + 1),
        }
    }

    /// Each pair's end minus begin, in milliseconds, once the passes have run.
    ///
    /// The resolve is a submission of its own: on Metal, a resolve in the command buffer that
    /// wrote the queries reads zeros for its last pass.
    pub fn read(&self, gpu: &Gpu) -> Vec<f64> {
        let mut enc = gpu.encoder();
        enc.resolve_query_set(&self.set, 0..self.count, &self.resolve, 0);
        gpu.submit_wait(enc);
        let bytes = gpu.read(&self.resolve, 0, u64::from(self.count) * 8);
        let ticks: Vec<u64> = bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| u64::from_le_bytes(*c))
            .collect();
        ticks
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| tick_ms(p[1].saturating_sub(p[0]), self.period))
            .collect()
    }
}

#[allow(clippy::cast_precision_loss)]
pub fn tick_ms(ticks: u64, period_ns: f64) -> f64 {
    ticks as f64 * period_ns / 1e6
}

pub fn bytes_u32(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub fn bytes_f32(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// The median of `v`, or `None` when it is empty.
pub fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(v[v.len() / 2])
}

/// A wgpu error message on one line, without its trailing blank lines.
pub fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn u(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}
