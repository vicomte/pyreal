//! Sections 4 to 8 and 10: timestamps, limits and large buffers, immediates, subgroups, texture
//! formats, and the other features a modern renderer leans on.

use crate::bindless::groups_2d;
use crate::gpu::{self, Gpu};
use crate::report::{Report, Status};

const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;

// ---- 4. timestamps ------------------------------------------------------------------------------

const FILL: &str = "
@group(0) @binding(0) var<storage, read_write> data: array<vec4<u32>>;
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = id.x + id.y * nw.x * 256u;
    if (i < arrayLength(&data)) { data[i] = vec4<u32>(i, i ^ 0x5555u, i * 3u, 7u); }
}";

pub fn timestamps(gpu: &Gpu, r: &mut Report) {
    use wgpu::Features as F;
    const S: &str = "4. timestamp queries";
    let ts = r.feature(S, gpu, F::TIMESTAMP_QUERY);
    let enc_ts = r.feature(S, gpu, F::TIMESTAMP_QUERY_INSIDE_ENCODERS);
    let pass_ts = r.feature(S, gpu, F::TIMESTAMP_QUERY_INSIDE_PASSES);
    r.feature(S, gpu, F::PIPELINE_STATISTICS_QUERY);
    r.row(
        S,
        "timestamp period",
        Status::Info,
        format!("{} ns per tick", gpu.queue.get_timestamp_period()),
    );
    if !ts {
        r.row(S, "timed pass", Status::Skip, "feature missing");
        return;
    }
    let size = 256 * MIB;
    let res = gpu.scoped(|| {
        let data = gpu.buffer(
            "fill",
            size,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        );
        let copy = gpu.buffer("copy", size, wgpu::BufferUsages::COPY_DST);
        let pipeline = gpu.compute("fill", FILL);
        let group = gpu.bind_buffers(&pipeline, &[&data]);
        let timer = gpu.timer(4).ok_or("no timer")?;
        gpu.warm();
        let (x, y) = groups_2d(gpu::u((size / 16) as usize));
        let mut enc = gpu.encoder();
        // Pass timestamps: one compute pass writing 256 MiB.
        for run in 0..2 {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("fill"),
                timestamp_writes: Some(timer.compute_pass(run)),
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(x, y, 1);
        }
        // Encoder timestamps around a 256 MiB copy.
        if enc_ts {
            enc.write_timestamp(&timer.set, 4);
            enc.copy_buffer_to_buffer(&data, 0, &copy, 0, size);
            enc.write_timestamp(&timer.set, 5);
        }
        // Timestamps inside a pass, between two dispatches.
        if pass_ts {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.write_timestamp(&timer.set, 6);
            pass.dispatch_workgroups(x, y, 1);
            pass.write_timestamp(&timer.set, 7);
        }
        gpu.submit_wait(enc);
        let ms = timer.read(gpu);
        #[allow(clippy::cast_precision_loss)]
        let gbps = |ms: f64| size as f64 / 1e6 / ms;
        r.time(
            S,
            "compute pass writing 256 MiB (pass timestamps)",
            None,
            Some(ms[1]),
            format!("{:.0} GB/s", gbps(ms[1])),
        );
        if enc_ts {
            r.time(
                S,
                "copy 256 MiB (encoder timestamps)",
                None,
                Some(ms[2]),
                format!("{:.0} GB/s read+write", gbps(ms[2])),
            );
        }
        if pass_ts {
            r.time(
                S,
                "dispatch writing 256 MiB (in-pass timestamps)",
                None,
                Some(ms[3]),
                "",
            );
        }
        Ok(format!("pass {:.3} ms ({:.0} GB/s)", ms[1], gbps(ms[1])))
    });
    r.result(S, "timed compute pass", res);
}

// ---- 5. limits ----------------------------------------------------------------------------------

pub fn limits(gpu: &Gpu, r: &mut Report) {
    const S: &str = "5. limits";
    let l = &gpu.limits;
    let rows: [(&str, u64); 22] = [
        ("max_buffer_size", l.max_buffer_size),
        (
            "max_storage_buffer_binding_size",
            l.max_storage_buffer_binding_size,
        ),
        (
            "max_uniform_buffer_binding_size",
            l.max_uniform_buffer_binding_size,
        ),
        (
            "max_texture_dimension_2d",
            l.max_texture_dimension_2d.into(),
        ),
        (
            "max_texture_dimension_3d",
            l.max_texture_dimension_3d.into(),
        ),
        (
            "max_texture_array_layers",
            l.max_texture_array_layers.into(),
        ),
        (
            "max_compute_workgroup_storage_size",
            l.max_compute_workgroup_storage_size.into(),
        ),
        (
            "max_compute_invocations_per_workgroup",
            l.max_compute_invocations_per_workgroup.into(),
        ),
        (
            "max_compute_workgroups_per_dimension",
            l.max_compute_workgroups_per_dimension.into(),
        ),
        ("max_bind_groups", l.max_bind_groups.into()),
        (
            "max_bindings_per_bind_group",
            l.max_bindings_per_bind_group.into(),
        ),
        (
            "max_storage_buffers_per_shader_stage",
            l.max_storage_buffers_per_shader_stage.into(),
        ),
        (
            "max_storage_textures_per_shader_stage",
            l.max_storage_textures_per_shader_stage.into(),
        ),
        (
            "max_sampled_textures_per_shader_stage",
            l.max_sampled_textures_per_shader_stage.into(),
        ),
        (
            "max_samplers_per_shader_stage",
            l.max_samplers_per_shader_stage.into(),
        ),
        (
            "max_uniform_buffers_per_shader_stage",
            l.max_uniform_buffers_per_shader_stage.into(),
        ),
        (
            "max_non_sampler_bindings",
            l.max_non_sampler_bindings.into(),
        ),
        ("max_color_attachments", l.max_color_attachments.into()),
        (
            "max_color_attachment_bytes_per_sample",
            l.max_color_attachment_bytes_per_sample.into(),
        ),
        ("max_vertex_buffers", l.max_vertex_buffers.into()),
        (
            "max_inter_stage_shader_variables",
            l.max_inter_stage_shader_variables.into(),
        ),
        (
            "min_storage_buffer_offset_alignment",
            l.min_storage_buffer_offset_alignment.into(),
        ),
    ];
    for (name, v) in rows {
        r.row(S, name, Status::Info, pretty(v));
    }
}

fn pretty(v: u64) -> String {
    if v >= GIB && v.is_multiple_of(MIB) {
        #[allow(clippy::cast_precision_loss)]
        let g = v as f64 / GIB as f64;
        format!("{v} ({g:.2} GiB)")
    } else if v >= MIB && v.is_multiple_of(MIB) {
        format!("{v} ({} MiB)", v / MIB)
    } else {
        v.to_string()
    }
}

const MARK: &str = "
@group(0) @binding(0) var<storage, read_write> data: array<u32>;
@compute @workgroup_size(1)
fn main() { data[arrayLength(&data) - 1u] = 0xC0FFEEu; data[0] = 0xBEEFu; }";

/// Large storage buffers: made and cleared, then written at both ends through a binding of the
/// whole buffer and through one of its last 256 bytes, and read back.
pub fn buffers(gpu: &Gpu, r: &mut Report) {
    const S: &str = "5. limits";
    let pipeline = gpu.compute("mark", MARK);
    for size in [GIB, 2 * GIB, 4 * GIB, 8 * GIB] {
        let name = format!("{} GiB storage buffer", size / GIB);
        if size > gpu.limits.max_buffer_size {
            r.row(S, name, Status::Skip, "over max_buffer_size");
            continue;
        }
        // Made alone first: out of memory here is a card too small for it, not a failure.
        let oom = gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let buf = gpu.buffer(
            "large",
            size,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        );
        if let Some(e) = pollster::block_on(oom.pop()) {
            let why = format!("over this card's memory: {}", gpu::one_line(&e.to_string()));
            r.row(S, name, Status::Skip, why);
            continue;
        }
        let made = gpu.scoped(|| {
            // The first clear also commits the memory; the second is the clear alone.
            let mut enc = gpu.encoder();
            enc.clear_buffer(&buf, 0, None);
            let first_ms = gpu.submit_wait(enc);
            let mut enc = gpu.encoder();
            enc.clear_buffer(&buf, 0, None);
            let clear_ms = gpu.submit_wait(enc);
            #[allow(clippy::cast_precision_loss)]
            let gbps = size as f64 / 1e6 / clear_ms;
            r.time(
                S,
                format!("first clear of a new {name}"),
                Some(first_ms),
                None,
                "wall; commits the memory",
            );
            r.time(
                S,
                format!("clear {name} again"),
                Some(clear_ms),
                None,
                format!("wall; {gbps:.0} GB/s"),
            );
            Ok(buf)
        });
        let buf = match made {
            Ok(b) => {
                r.row(S, name.clone(), Status::Pass, "made and cleared");
                b
            }
            Err(e) => {
                r.row(S, name, Status::Fail, e);
                continue;
            }
        };
        let whole = size <= gpu.limits.max_storage_buffer_binding_size;
        for (offset, len, how) in [
            (0, size, "whole buffer bound"),
            (size - 256, 256, "last 256 bytes bound"),
        ] {
            let item = format!("{name}, {how}");
            if len == size && !whole {
                r.row(
                    S,
                    item,
                    Status::Skip,
                    "over max_storage_buffer_binding_size",
                );
                continue;
            }
            let res = gpu.scoped(|| {
                let mut enc = gpu.encoder();
                enc.clear_buffer(&buf, 0, None);
                gpu.submit_wait(enc);
                let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buf,
                            offset,
                            size: wgpu::BufferSize::new(len),
                        }),
                    }],
                });
                let mut enc = gpu.encoder();
                {
                    let mut pass = enc.begin_compute_pass(&Default::default());
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &group, &[]);
                    pass.dispatch_workgroups(1, 1, 1);
                }
                gpu.submit_wait(enc);
                let first = gpu.read_u32s(&buf, offset, 1)[0];
                let last = gpu.read_u32s(&buf, size - 4, 1)[0];
                if first == 0xBEEF && last == 0xC0FFEE {
                    Ok("written at both ends of the binding".to_owned())
                } else {
                    Err(format!(
                        "first word {first:#x}, last word {last:#x}: the shader's arrayLength \
                         did not reach the end"
                    ))
                }
            });
            r.result(S, item, res);
        }
    }
}

// ---- 6. immediates ------------------------------------------------------------------------------

pub fn immediates(gpu: &Gpu, r: &mut Report) {
    const S: &str = "6. immediates (push constants)";
    let have = r.feature(S, gpu, wgpu::Features::IMMEDIATES);
    r.row(
        S,
        "max_immediate_size",
        Status::Info,
        format!("{} bytes", gpu.limits.max_immediate_size),
    );
    if !have {
        r.row(S, "immediate read", Status::Skip, "feature missing");
        return;
    }
    let res = gpu.scoped(|| {
        let module = gpu.shader(
            "immediates",
            "struct Imm { a: vec4<u32>, b: vec4<u32> };
var<immediate> imm: Imm;
@group(0) @binding(0) var<storage, read_write> out: array<vec4<u32>, 2>;
@compute @workgroup_size(1)
fn main() { out[0] = imm.a; out[1] = imm.b; }",
        );
        let layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[crate::bindless::storage_entry(0)],
            });
        let pl = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 32,
            });
        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("immediates"),
                layout: Some(&pl),
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let out = gpu.buffer(
            "imm out",
            32,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: out.as_entire_binding(),
            }],
        });
        let want: Vec<u32> = (1..=8).map(|i| i * 0x0101_0101).collect();
        let mut enc = gpu.encoder();
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.set_immediates(0, &gpu::bytes_u32(&want));
            pass.dispatch_workgroups(1, 1, 1);
        }
        gpu.submit_wait(enc);
        let got = gpu.read_u32s(&out, 0, 8);
        if got == want {
            Ok("32 bytes read back in a compute shader".to_owned())
        } else {
            Err(format!("read {got:x?}"))
        }
    });
    r.result(S, "immediate read (compute)", res);
}

// ---- 7. subgroups -------------------------------------------------------------------------------

/// Sums `data` per workgroup into `total`, by subgroups or by a shared-memory tree.
const REDUCE: &str = "
@group(0) @binding(0) var<storage, read> data: array<u32>;
@group(0) @binding(1) var<storage, read_write> total: atomic<u32>;
@group(0) @binding(2) var<storage, read_write> size: u32;
var<workgroup> partial: array<u32, 256>;
override SUBGROUPS: bool;
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(local_invocation_index) li: u32,
        @builtin(num_workgroups) nw: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>,
        @builtin(subgroup_invocation_id) lane: u32, @builtin(subgroup_size) sg: u32,
        @builtin(subgroup_id) sid: u32) {
    let i = gid.x + gid.y * nw.x * 256u;
    var v = 0u;
    if (i < arrayLength(&data)) { v = data[i]; }
    if (SUBGROUPS) {
        let s = subgroupAdd(v);
        if (lane == 0u) { partial[sid] = s; }
        workgroupBarrier();
        if (li == 0u) {
            var t = 0u;
            for (var k = 0u; k < (256u + sg - 1u) / sg; k++) { t += partial[k]; }
            atomicAdd(&total, t);
        }
        if (i == 0u) { size = sg; }
    } else {
        partial[li] = v;
        workgroupBarrier();
        for (var step = 128u; step > 0u; step >>= 1u) {
            if (li < step) { partial[li] += partial[li + step]; }
            workgroupBarrier();
        }
        if (li == 0u) { atomicAdd(&total, partial[0]); }
    }
}";

pub fn subgroups(gpu: &Gpu, r: &mut Report) {
    const S: &str = "7. subgroups";
    let have = r.feature(S, gpu, wgpu::Features::SUBGROUP);
    r.feature(S, gpu, wgpu::Features::SUBGROUP_BARRIER);
    r.feature(S, gpu, wgpu::Features::SUBGROUP_VERTEX);
    let info = gpu.adapter.get_info();
    r.row(
        S,
        "subgroup size",
        Status::Info,
        format!(
            "{}..{} lanes",
            info.subgroup_min_size, info.subgroup_max_size
        ),
    );
    if !have {
        r.row(S, "subgroup reduction", Status::Skip, "feature missing");
        return;
    }
    let n: u32 = 1 << 24;
    let res = gpu.scoped(|| {
        let values: Vec<u32> = (0..n).map(|i| i & 0xF).collect();
        let want: u32 = values.iter().sum();
        let data = gpu.buffer_init(
            "reduce",
            &gpu::bytes_u32(&values),
            wgpu::BufferUsages::STORAGE,
        );
        let total = gpu.buffer(
            "total",
            4,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        );
        let size = gpu.buffer(
            "size",
            4,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let module = gpu.shader("reduce", REDUCE);
        let mut detail = String::new();
        for sub in [true, false] {
            let pipeline = gpu
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("reduce"),
                    layout: None,
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[("SUBGROUPS", if sub { 1.0 } else { 0.0 })],
                        ..Default::default()
                    },
                    cache: None,
                });
            let group = gpu.bind_buffers(&pipeline, &[&data, &total, &size]);
            let timer = gpu.timer(10);
            gpu.warm();
            let (x, y) = groups_2d(n);
            let mut results = Vec::new();
            let mut ms = Vec::new();
            for run in 0..10 {
                let mut enc = gpu.encoder();
                enc.clear_buffer(&total, 0, None);
                {
                    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("reduce"),
                        timestamp_writes: timer.as_ref().map(|t| t.compute_pass(run)),
                    });
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &group, &[]);
                    pass.dispatch_workgroups(x, y, 1);
                }
                gpu.submit_wait(enc);
                results.push(gpu.read_u32s(&total, 0, 1)[0]);
            }
            if let Some(t) = &timer {
                ms = t.read(gpu);
            }
            let label = if sub {
                "subgroupAdd"
            } else {
                "shared-memory tree"
            };
            r.time(
                S,
                format!("reduce 16M u32: {label}"),
                None,
                gpu::median(ms),
                "",
            );
            if let Some(bad) = results.iter().find(|&&g| g != want) {
                return Err(format!("{label}: sum {bad}, want {want}"));
            }
            if sub {
                let sg = gpu.read_u32s(&size, 0, 1)[0];
                detail = format!("sums correct both ways; subgroup_size builtin = {sg}");
            }
        }
        Ok(detail)
    });
    r.result(S, "subgroup reduction", res);
}

// ---- 8. texture formats -------------------------------------------------------------------------

pub fn formats(gpu: &Gpu, r: &mut Report) {
    use wgpu::Features as F;
    use wgpu::TextureFormat as T;
    const S: &str = "8. texture formats";
    let bc = r.feature(S, gpu, F::TEXTURE_COMPRESSION_BC);
    r.feature(S, gpu, F::TEXTURE_COMPRESSION_BC_SLICED_3D);
    r.feature(S, gpu, F::TEXTURE_COMPRESSION_ASTC);
    r.feature(S, gpu, F::TEXTURE_COMPRESSION_ASTC_HDR);
    r.feature(S, gpu, F::TEXTURE_COMPRESSION_ETC2);
    r.feature(S, gpu, F::FLOAT32_FILTERABLE);
    r.feature(S, gpu, F::FLOAT32_BLENDABLE);
    r.feature(S, gpu, F::RG11B10UFLOAT_RENDERABLE);
    r.feature(S, gpu, F::DEPTH32FLOAT_STENCIL8);
    r.feature(S, gpu, F::TEXTURE_FORMAT_16BIT_NORM);
    r.feature(S, gpu, F::BGRA8UNORM_STORAGE);
    r.feature(S, gpu, F::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES);
    for f in [
        T::Bc7RgbaUnormSrgb,
        T::Bc6hRgbUfloat,
        T::Bc5RgUnorm,
        T::Astc {
            block: wgpu::AstcBlock::B4x4,
            channel: wgpu::AstcChannel::UnormSrgb,
        },
        T::Etc2Rgba8UnormSrgb,
        T::R32Float,
        T::Rg32Float,
        T::Rgba32Float,
        T::R32Uint,
        T::R64Uint,
        T::Rgba16Float,
        T::Rg16Float,
        T::R16Float,
        T::Rg11b10Ufloat,
        T::Rgb10a2Unorm,
        T::Rgba8Unorm,
        T::Bgra8Unorm,
        T::Depth32Float,
        T::Depth32FloatStencil8,
    ] {
        let ff = gpu.adapter.get_texture_format_features(f);
        let mut caps = Vec::new();
        let flags = ff.flags;
        use wgpu::TextureFormatFeatureFlags as Q;
        for (q, name) in [
            (Q::FILTERABLE, "filter"),
            (Q::BLENDABLE, "blend"),
            (Q::MULTISAMPLE_X4, "msaa4"),
            (Q::STORAGE_READ_ONLY, "st-r"),
            (Q::STORAGE_WRITE_ONLY, "st-w"),
            (Q::STORAGE_READ_WRITE, "st-rw"),
            (Q::STORAGE_ATOMIC, "st-atomic"),
        ] {
            if flags.contains(q) {
                caps.push(name);
            }
        }
        let u = ff.allowed_usages;
        if u.contains(wgpu::TextureUsages::RENDER_ATTACHMENT) {
            caps.push("render");
        }
        if u.contains(wgpu::TextureUsages::TEXTURE_BINDING) {
            caps.push("sample");
        }
        r.row(S, format!("{f:?}"), Status::Info, caps.join(" "));
    }

    // Hi-Z: an r32float storage texture read and written in place.
    let rw = gpu
        .adapter
        .get_texture_format_features(T::R32Float)
        .flags
        .contains(wgpu::TextureFormatFeatureFlags::STORAGE_READ_WRITE);
    if rw {
        let res = gpu.scoped(|| hiz(gpu));
        r.result(S, "r32float read-write storage (Hi-Z)", res);
    } else {
        r.row(
            S,
            "r32float read-write storage (Hi-Z)",
            Status::Skip,
            "format lacks it",
        );
    }
    if bc {
        let res = gpu.scoped(|| bc1(gpu));
        r.result(S, "BC1 block sampled", res);
    }
}

fn hiz(gpu: &Gpu) -> Result<String, String> {
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hiz"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let init: Vec<f32> = (0..64 * 64).map(|i| i as f32).collect();
    gpu.queue.write_texture(
        tex.as_image_copy(),
        &gpu::bytes_f32(&init),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(64),
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    let pipeline = gpu.compute(
        "hiz",
        "@group(0) @binding(0) var t: texture_storage_2d<r32float, read_write>;
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let v = textureLoad(t, id.xy).r;
    textureBarrier();
    textureStore(t, id.xy, vec4<f32>(v * 2.0 + 1.0));
}",
    );
    let view = tex.create_view(&Default::default());
    let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    let buf = gpu.buffer(
        "hiz read",
        64 * 256,
        wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
    );
    let mut enc = gpu.encoder();
    {
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(8, 8, 1);
    }
    enc.copy_texture_to_buffer(
        tex.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    gpu.submit_wait(enc);
    let got = gpu.read(&buf, 0, 64 * 256);
    let bad = got
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(i, c)| f32::from_le_bytes(**c) != *i as f32 * 2.0 + 1.0)
        .count();
    if bad == 0 {
        Ok("4096 texels read and rewritten in place".to_owned())
    } else {
        Err(format!("{bad} texels wrong"))
    }
}

/// One 4x4 BC1 block, all red, sampled back.
fn bc1(gpu: &Gpu) -> Result<String, String> {
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bc1"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bc1RgbaUnorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    // color0 = pure red (565), color1 = black, every index 0.
    let block = [0x00, 0xF8, 0x00, 0x00, 0, 0, 0, 0];
    gpu.queue.write_texture(
        tex.as_image_copy(),
        &block,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
    );
    let pipeline = gpu.compute(
        "bc1",
        "@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> out: array<f32, 4>;
@compute @workgroup_size(1)
fn main() { let c = textureLoad(t, vec2<u32>(2u, 1u), 0); out = array<f32, 4>(c.r, c.g, c.b, c.a); }",
    );
    let view = tex.create_view(&Default::default());
    let out = gpu.buffer(
        "bc1 out",
        16,
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
                resource: out.as_entire_binding(),
            },
        ],
    });
    let mut enc = gpu.encoder();
    {
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    gpu.submit_wait(enc);
    let c: Vec<f32> = gpu
        .read(&out, 0, 16)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect();
    if c == [1.0, 0.0, 0.0, 1.0] {
        Ok("decoded to opaque red".to_owned())
    } else {
        Err(format!("decoded to {c:?}"))
    }
}

// ---- 10. others ---------------------------------------------------------------------------------

const ATOMIC64: &str = "
@group(0) @binding(0) var<storage, read_write> vis: array<atomic<u64>, 1>;
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = gid.x + gid.y * nw.x * 256u;
    if (i >= 4194304u) { return; }
    // A visibility-buffer write: depth in the high word, the triangle id in the low.
    let depth = (i * 2654435761u) >> 8u;
    atomicMax(&vis[0], (u64(depth) << 32u) | u64(i));
}";

pub fn others(gpu: &Gpu, r: &mut Report) {
    use wgpu::Features as F;
    const S: &str = "10. other features";
    r.feature(S, gpu, F::SHADER_INT64);
    let a64 = r.feature(S, gpu, F::SHADER_INT64_ATOMIC_MIN_MAX);
    r.feature(S, gpu, F::SHADER_INT64_ATOMIC_ALL_OPS);
    r.feature(S, gpu, F::TEXTURE_INT64_ATOMIC);
    r.feature(S, gpu, F::TEXTURE_ATOMIC);
    r.feature(S, gpu, F::SHADER_FLOAT32_ATOMIC);
    r.feature(S, gpu, F::SHADER_F16);
    r.feature(S, gpu, F::SHADER_I16);
    r.feature(S, gpu, F::SHADER_F64);
    r.feature(S, gpu, F::DUAL_SOURCE_BLENDING);
    r.feature(S, gpu, F::CLIP_DISTANCES);
    r.feature(S, gpu, F::DEPTH_CLIP_CONTROL);
    r.feature(S, gpu, F::PRIMITIVE_INDEX);
    r.feature(S, gpu, F::SHADER_BARYCENTRICS);
    r.feature(S, gpu, F::SHADER_PER_VERTEX);
    r.feature(S, gpu, F::SHADER_EARLY_DEPTH_TEST);
    r.feature(S, gpu, F::CONSERVATIVE_RASTERIZATION);
    r.feature(S, gpu, F::MULTIVIEW);
    r.feature(S, gpu, F::SELECTIVE_MULTIVIEW);
    r.feature(S, gpu, F::MULTISAMPLE_ARRAY);
    r.feature(S, gpu, F::EXPERIMENTAL_MESH_SHADER);
    r.feature(S, gpu, F::EXPERIMENTAL_COOPERATIVE_MATRIX);
    r.feature(S, gpu, F::PIPELINE_CACHE);
    r.feature(S, gpu, F::ADDRESS_MODE_CLAMP_TO_BORDER);
    r.feature(S, gpu, F::MAPPABLE_PRIMARY_BUFFERS);
    r.feature(S, gpu, F::VERTEX_WRITABLE_STORAGE);
    r.feature(S, gpu, F::POLYGON_MODE_LINE);
    let down = gpu.adapter.get_downlevel_capabilities();
    r.row(
        S,
        "downlevel",
        Status::Info,
        if down.is_webgpu_compliant() {
            "fully WebGPU-compliant".to_owned()
        } else {
            format!("{:?}", down.flags)
        },
    );
    if a64 {
        let res = gpu.scoped(|| {
            let vis = gpu.buffer(
                "vis",
                8,
                wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
            );
            let pipeline = gpu.compute("atomic64", ATOMIC64);
            let group = gpu.bind_buffers(&pipeline, &[&vis]);
            let n = 1u32 << 22;
            let (x, y) = groups_2d(n);
            let timer = gpu.timer(1);
            gpu.warm();
            let mut enc = gpu.encoder();
            enc.clear_buffer(&vis, 0, None);
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("atomic64"),
                    timestamp_writes: timer.as_ref().map(|t| t.compute_pass(0)),
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.dispatch_workgroups(x, y, 1);
            }
            gpu.submit_wait(enc);
            let ms = timer.and_then(|t| t.read(gpu).first().copied());
            r.time(
                S,
                "4M 64-bit atomicMax on one address",
                None,
                ms,
                "worst-case contention",
            );
            let w = gpu.read_u32s(&vis, 0, 2);
            let got = u64::from(w[0]) | (u64::from(w[1]) << 32);
            let want = (0..n)
                .map(|i| (u64::from(i.wrapping_mul(2_654_435_761) >> 8) << 32) | u64::from(i))
                .max()
                .unwrap_or(0);
            if got == want {
                Ok(format!("{n} packed depth|id writes, max {want:#x}"))
            } else {
                Err(format!("max {got:#x}, want {want:#x}"))
            }
        });
        r.result(S, "64-bit atomicMax (visibility buffer)", res);
    }
}
