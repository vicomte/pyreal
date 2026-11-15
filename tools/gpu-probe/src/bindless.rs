//! Section 1, bindless textures: a binding array of many small textures, each invocation sampling one
//! chosen by a hash of its index (so neighbouring invocations read different textures), against
//! the same reads from one texture array.

use std::num::NonZeroU32;
use std::time::Instant;

use crate::gpu::{self, Gpu};
use crate::report::{Report, Status};

const S: &str = "1. bindless textures";
/// How many distinct textures the workloads make.
const TEXTURES: u32 = 1024;
/// Invocations in a timed dispatch.
const TIMED: u32 = 1 << 22;
const RUNS: u32 = 10;

pub fn run(gpu: &Gpu, r: &mut Report) {
    use wgpu::Features as F;
    let arrays = r.feature(S, gpu, F::TEXTURE_BINDING_ARRAY);
    let nonuniform = r.feature(
        S,
        gpu,
        F::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
    );
    let partial = r.feature(S, gpu, F::PARTIALLY_BOUND_BINDING_ARRAY);
    r.feature(S, gpu, F::BUFFER_BINDING_ARRAY);
    r.feature(S, gpu, F::STORAGE_RESOURCE_BINDING_ARRAY);
    r.feature(S, gpu, F::STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING);
    r.feature(S, gpu, F::UNIFORM_BUFFER_BINDING_ARRAYS);
    let max = gpu.limits.max_binding_array_elements_per_shader_stage;
    r.row(
        S,
        "max_binding_array_elements_per_shader_stage",
        Status::Info,
        max.to_string(),
    );
    r.row(
        S,
        "max_binding_array_sampler_elements_per_shader_stage",
        Status::Info,
        gpu.limits
            .max_binding_array_sampler_elements_per_shader_stage
            .to_string(),
    );

    let (textures, views) = make_textures(gpu);
    let view_refs: Vec<&wgpu::TextureView> = views.iter().collect();
    let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("nearest"),
        ..Default::default()
    });

    if arrays && nonuniform {
        for (shift, how) in [
            (0, "a different texture per invocation"),
            (6, "one texture per 64 invocations"),
        ] {
            let res = gpu.scoped(|| {
                let out = bindless(gpu, &view_refs, &sampler, TEXTURES, TEXTURES, TIMED, shift)?;
                r.time(
                    S,
                    format!("binding array: {TIMED} samples over {TEXTURES} textures, {how}"),
                    None,
                    out.gpu_ms,
                    "",
                );
                Ok(out.detail)
            });
            r.result(
                S,
                format!("{TEXTURES} textures, non-uniform sample, {how}"),
                res,
            );
        }

        // A material table's size: the same textures bound many times over.
        let big = max.min(1 << 16);
        if big > TEXTURES {
            let res = gpu.scoped(|| {
                let refs: Vec<&wgpu::TextureView> = (0..big as usize)
                    .map(|i| view_refs[i % TEXTURES as usize])
                    .collect();
                let out = bindless(gpu, &refs, &sampler, big, big, TIMED, 0)?;
                r.time(
                    S,
                    format!("binding array: {TIMED} samples over {big} bindings"),
                    Some(out.bind_ms),
                    out.gpu_ms,
                    "cpu = bind group creation",
                );
                Ok(out.detail)
            });
            r.result(S, format!("{big}-element binding array"), res);
        }
        if partial {
            let res = gpu.scoped(|| {
                bindless(
                    gpu,
                    &view_refs,
                    &sampler,
                    TEXTURES * 2,
                    TEXTURES,
                    1 << 16,
                    0,
                )
                .map(|o| o.detail)
            });
            r.result(
                S,
                format!("partially bound ({TEXTURES} of {})", TEXTURES * 2),
                res,
            );
        } else {
            r.row(S, "partially bound", Status::Skip, "feature missing");
        }
    } else {
        r.row(S, "binding array workload", Status::Skip, "feature missing");
    }

    // The fallback: one texture array holding the same pictures as layers.
    for (shift, how) in [
        (0, "a different layer per invocation"),
        (6, "one layer per 64 invocations"),
    ] {
        let res = gpu.scoped(|| {
            let out = texture_array(gpu, &textures, &sampler, shift)?;
            r.time(
                S,
                format!("texture array: {TIMED} samples over {TEXTURES} layers, {how}"),
                None,
                out.gpu_ms,
                "the fallback, for comparison",
            );
            Ok(out.detail)
        });
        r.result(
            S,
            format!("texture array of {TEXTURES} layers (fallback), {how}"),
            res,
        );
    }
}

/// The 4x4 textures: texel (x, y) of texture k holds (k & 255, k >> 8, x + 4y, 255).
fn make_textures(gpu: &Gpu) -> (Vec<wgpu::Texture>, Vec<wgpu::TextureView>) {
    let mut textures = Vec::new();
    let mut views = Vec::new();
    for k in 0..TEXTURES {
        let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bindless"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            t.as_image_copy(),
            &texels(k),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16),
                rows_per_image: Some(4),
            },
            wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
        );
        views.push(t.create_view(&Default::default()));
        textures.push(t);
    }
    (textures, views)
}

fn texels(k: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity(64);
    for i in 0..16u8 {
        let [lo, hi, ..] = k.to_le_bytes();
        v.extend_from_slice(&[lo, hi, i, 255]);
    }
    v
}

/// What texel the shader reads for invocation `i` over `range` indices.
fn expected(i: u32, range: u32, shift: u32) -> u32 {
    let k = (i >> shift).wrapping_mul(2_654_435_761) % range % TEXTURES;
    let texel = (i & 3) + 4 * ((i >> 2) & 3);
    (k & 255) | ((k >> 8) << 8) | (texel << 16)
}

struct Outcome {
    detail: String,
    gpu_ms: Option<f64>,
    bind_ms: f64,
}

/// The shared body: invocation `i` samples texel `(i & 3, (i >> 2) & 3)` of the texture `pick`
/// chooses, and stores what it read.
fn body(pick: &str) -> String {
    format!(
        "@group(0) @binding(2) var<storage, read_write> out: array<u32>;
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {{
    let i = id.x + id.y * nw.x * 256u;
    if (i >= arrayLength(&out)) {{ return; }}
    let t = ((i >> SHIFT) * 2654435761u) % RANGE;
    let px = vec2<u32>(i & 3u, (i >> 2u) & 3u);
    let uv = (vec2<f32>(px) + 0.5) / 4.0;
    let c = {pick};
    out[i] = u32(c.r * 255.0 + 0.5) | (u32(c.g * 255.0 + 0.5) << 8u) | (u32(c.b * 255.0 + 0.5) << 16u);
}}"
    )
}

fn bindless(
    gpu: &Gpu,
    views: &[&wgpu::TextureView],
    sampler: &wgpu::Sampler,
    len: u32,
    range: u32,
    n: u32,
    shift: u32,
) -> Result<Outcome, String> {
    let wgsl = format!(
        "enable wgpu_binding_array;
const RANGE: u32 = {range}u;
const SHIFT: u32 = {shift}u;
@group(0) @binding(0) var textures: binding_array<texture_2d<f32>, {len}>;
@group(0) @binding(1) var samp: sampler;
{}",
        body("textureSampleLevel(textures[t], samp, uv, 0.0)")
    );
    let layout = gpu
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bindless"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: NonZeroU32::new(len),
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                storage_entry(2),
            ],
        });
    let out = gpu.buffer(
        "bindless out",
        u64::from(n) * 4,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let t = Instant::now();
    let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bindless"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureViewArray(views),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: out.as_entire_binding(),
            },
        ],
    });
    let bind_ms = t.elapsed().as_secs_f64() * 1e3;
    let pipeline = pipeline(gpu, &wgsl, &layout);
    let gpu_ms = dispatch(gpu, &pipeline, &group, n);
    check(gpu, &out, n, range, shift).map(|detail| Outcome {
        detail,
        gpu_ms,
        bind_ms,
    })
}

fn texture_array(
    gpu: &Gpu,
    textures: &[wgpu::Texture],
    sampler: &wgpu::Sampler,
    shift: u32,
) -> Result<Outcome, String> {
    let layers = TEXTURES.min(gpu.limits.max_texture_array_layers);
    let array = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("texture array"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut enc = gpu.encoder();
    for (k, t) in textures.iter().take(layers as usize).enumerate() {
        enc.copy_texture_to_texture(
            t.as_image_copy(),
            wgpu::TexelCopyTextureInfo {
                texture: &array,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: gpu::u(k),
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
        );
    }
    gpu.queue.submit([enc.finish()]);
    let view = array.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let wgsl = format!(
        "const RANGE: u32 = {layers}u;
const SHIFT: u32 = {shift}u;
@group(0) @binding(0) var layers: texture_2d_array<f32>;
@group(0) @binding(1) var samp: sampler;
{}",
        body("textureSampleLevel(layers, samp, uv, t, 0.0)")
    );
    let module = gpu.shader("texture array", &wgsl);
    let pipeline = gpu
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("texture array"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
    let out = gpu.buffer(
        "texture array out",
        u64::from(TIMED) * 4,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: out.as_entire_binding(),
            },
        ],
    });
    let gpu_ms = dispatch(gpu, &pipeline, &group, TIMED);
    check(gpu, &out, TIMED, layers, shift).map(|detail| Outcome {
        detail,
        gpu_ms,
        bind_ms: 0.0,
    })
}

pub fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn pipeline(gpu: &Gpu, wgsl: &str, layout: &wgpu::BindGroupLayout) -> wgpu::ComputePipeline {
    let module = gpu.shader("bindless", wgsl);
    let pl = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(layout)],
            immediate_size: 0,
        });
    gpu.device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("bindless"),
            layout: Some(&pl),
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        })
}

/// Workgroups for `n` invocations of 256: at most 4096 across, as many rows as needed.
pub fn groups_2d(n: u32) -> (u32, u32) {
    let groups = n.div_ceil(256);
    let x = groups.min(4096);
    (x, groups.div_ceil(x))
}

/// Runs the dispatch [`RUNS`] times, each its own timed pass: the median GPU time.
fn dispatch(
    gpu: &Gpu,
    pipeline: &wgpu::ComputePipeline,
    group: &wgpu::BindGroup,
    n: u32,
) -> Option<f64> {
    gpu.warm();
    let timer = gpu.timer(RUNS);
    let (x, y) = groups_2d(n);
    let mut enc = gpu.encoder();
    for run in 0..RUNS {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("bindless"),
            timestamp_writes: timer.as_ref().map(|t| t.compute_pass(run)),
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.dispatch_workgroups(x, y, 1);
    }
    gpu.submit_wait(enc);
    timer.and_then(|t| gpu::median(t.read(gpu)))
}

fn check(gpu: &Gpu, out: &wgpu::Buffer, n: u32, range: u32, shift: u32) -> Result<String, String> {
    let got = gpu.read_u32s(out, 0, n as usize);
    let bad = (0..n)
        .filter(|&i| got[i as usize] != expected(i, range, shift))
        .count();
    if bad == 0 {
        Ok(format!("{n} reads correct"))
    } else {
        let first = (0..n)
            .find(|&i| got[i as usize] != expected(i, range, shift))
            .unwrap_or(0);
        Err(format!(
            "{bad} of {n} reads wrong (first at {first}: got {:#x}, want {:#x})",
            got[first as usize],
            expected(first, range, shift)
        ))
    }
}
