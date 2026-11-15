//! Sections 2 and 9, multi-draw indirect and indirect first-instance: a compute pass culls a grid of 10,000
//! quads and writes the draws, a render pass draws them into an offscreen target, and the
//! covered pixels are counted. Each way of issuing the draws is timed against the others.

use std::time::Instant;

use crate::gpu::{self, Gpu};
use crate::report::{Report, Status};

const S: &str = "2. multi-draw indirect";
const GRID: u32 = 100;
const N: u32 = GRID * GRID;
/// Pixels per cell side, and the target's side.
const CELL: u32 = 8;
const SIZE: u32 = GRID * CELL;
const ROW: u32 = 1024;
const RUNS: u32 = 10;

/// Whether quad `i` survives the cull: about three in four do.
fn visible(i: u32) -> bool {
    !(i.wrapping_mul(2_654_435_761) >> 16).is_multiple_of(4)
}

const CULL: &str = "
struct Args { index_count: u32, instance_count: u32, first_index: u32, base_vertex: i32, first_instance: u32 };
struct One { index_count: u32, instance_count: atomic<u32>, first_index: u32, base_vertex: i32, first_instance: u32 };
@group(0) @binding(0) var<storage, read_write> args: array<Args>;
@group(0) @binding(1) var<storage, read_write> count: atomic<u32>;
@group(0) @binding(2) var<storage, read_write> one: One;
@group(0) @binding(3) var<storage, read_write> ids: array<u32>;
override MODE: u32;
fn visible(i: u32) -> bool { return ((i * 2654435761u) >> 16u) % 4u != 0u; }
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= arrayLength(&args)) { return; }
    let v = visible(i);
    if (MODE == 0u) {
        // Every slot written; a culled draw has no instances.
        args[i] = Args(6u, select(0u, 1u, v), 0u, 0, i);
    } else if (MODE == 1u) {
        // Compacted, with a count.
        if (v) { let s = atomicAdd(&count, 1u); args[s] = Args(6u, 1u, 0u, 0, i); }
    } else {
        // One instanced draw over a compacted id list.
        if (v) { let s = atomicAdd(&one.instance_count, 1u); ids[s] = i; }
    }
}";

const DRAW: &str = "
const GRID: u32 = 100u;
@group(0) @binding(0) var<storage, read> ids: array<u32>;
var<immediate> imm: u32;
fn corner(cell: u32, v: u32) -> vec4<f32> {
    let c = vec2<f32>(f32(cell % GRID), f32(cell / GRID)) + vec2<f32>(f32(v & 1u), f32(v >> 1u));
    return vec4<f32>(c * (2.0 / f32(GRID)) - 1.0, 0.0, 1.0);
}
@vertex fn by_instance(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> @builtin(position) vec4<f32> {
    return corner(i, v);
}
@vertex fn by_list(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> @builtin(position) vec4<f32> {
    return corner(ids[i], v);
}
@vertex fn by_immediate(@builtin(vertex_index) v: u32) -> @builtin(position) vec4<f32> {
    return corner(imm, v);
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }
";

struct Scene {
    target: wgpu::Texture,
    view: wgpu::TextureView,
    index: wgpu::Buffer,
    args: wgpu::Buffer,
    count: wgpu::Buffer,
    one: wgpu::Buffer,
    one_template: wgpu::Buffer,
    cull: [wgpu::ComputePipeline; 3],
    cull_group: [wgpu::BindGroup; 3],
    draw_group: wgpu::BindGroup,
    by_instance: wgpu::RenderPipeline,
    by_list: wgpu::RenderPipeline,
    by_immediate: wgpu::RenderPipeline,
}

pub fn run(gpu: &Gpu, r: &mut Report) {
    use wgpu::Features as F;
    r.row(
        S,
        "multi_draw_indirect (DownlevelFlags::INDIRECT_EXECUTION)",
        if gpu
            .adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::INDIRECT_EXECUTION)
        {
            Status::Yes
        } else {
            Status::No
        },
        "core in wgpu 30; no separate feature",
    );
    let count = r.feature(S, gpu, F::MULTI_DRAW_INDIRECT_COUNT);
    let first = gpu.has(F::INDIRECT_FIRST_INSTANCE);
    r.feature(S, gpu, F::SHADER_DRAW_INDEX);
    r.row(
        S,
        "indirect call validation",
        Status::Info,
        if gpu
            .instance_flags
            .contains(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL)
        {
            "on (WGPU_VALIDATION_INDIRECT_CALL=0 turns it off)"
        } else {
            "off"
        },
    );
    let want = gpu::u((0..N).filter(|&i| visible(i)).count()) * CELL * CELL;

    let scene = match gpu.scoped(|| Ok(scene(gpu))) {
        Ok(s) => s,
        Err(e) => {
            r.row(S, "set-up", Status::Fail, e);
            return;
        }
    };

    if first {
        variant(
            gpu,
            r,
            &scene,
            want,
            "multi_draw_indexed_indirect, 10k slots (culled = 0 instances)",
            |enc, timer, run| {
                cull(enc, &scene, 0, timer, run);
                let mut pass = target_pass(enc, &scene, timer, run);
                pass.set_pipeline(&scene.by_instance);
                pass.set_bind_group(0, &scene.draw_group, &[]);
                pass.set_index_buffer(scene.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.multi_draw_indexed_indirect(&scene.args, 0, N);
            },
        );
        if count {
            variant(
                gpu,
                r,
                &scene,
                want,
                "multi_draw_indexed_indirect_count, GPU count",
                |enc, timer, run| {
                    enc.clear_buffer(&scene.count, 0, None);
                    cull(enc, &scene, 1, timer, run);
                    let mut pass = target_pass(enc, &scene, timer, run);
                    pass.set_pipeline(&scene.by_instance);
                    pass.set_bind_group(0, &scene.draw_group, &[]);
                    pass.set_index_buffer(scene.index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.multi_draw_indexed_indirect_count(&scene.args, 0, &scene.count, 0, N);
                },
            );
        } else {
            r.row(
                S,
                "multi_draw_indexed_indirect_count",
                Status::Skip,
                "feature missing",
            );
        }
    } else {
        r.row(
            S,
            "multi-draw workloads",
            Status::Skip,
            "INDIRECT_FIRST_INSTANCE missing",
        );
    }
    variant(
        gpu,
        r,
        &scene,
        want,
        "one draw_indexed_indirect, GPU-compacted instance list",
        |enc, timer, run| {
            enc.copy_buffer_to_buffer(&scene.one_template, 0, &scene.one, 0, 20);
            cull(enc, &scene, 2, timer, run);
            let mut pass = target_pass(enc, &scene, timer, run);
            pass.set_pipeline(&scene.by_list);
            pass.set_bind_group(0, &scene.draw_group, &[]);
            pass.set_index_buffer(scene.index.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed_indirect(&scene.one, 0);
        },
    );
    variant(
        gpu,
        r,
        &scene,
        want,
        "CPU draw_indexed per quad (first_instance)",
        |enc, timer, run| {
            let mut pass = target_pass(enc, &scene, timer, run);
            pass.set_pipeline(&scene.by_instance);
            pass.set_bind_group(0, &scene.draw_group, &[]);
            pass.set_index_buffer(scene.index.slice(..), wgpu::IndexFormat::Uint32);
            for i in (0..N).filter(|&i| visible(i)) {
                pass.draw_indexed(0..6, 0, i..i + 1);
            }
        },
    );
    if gpu.has(F::IMMEDIATES) {
        variant(
            gpu,
            r,
            &scene,
            want,
            "CPU draw_indexed per quad (immediate per draw)",
            |enc, timer, run| {
                let mut pass = target_pass(enc, &scene, timer, run);
                pass.set_pipeline(&scene.by_immediate);
                pass.set_bind_group(0, &scene.draw_group, &[]);
                pass.set_index_buffer(scene.index.slice(..), wgpu::IndexFormat::Uint32);
                for i in (0..N).filter(|&i| visible(i)) {
                    pass.set_immediates(0, &i.to_le_bytes());
                    pass.draw_indexed(0..6, 0, 0..1);
                }
            },
        );
    }
    mesh(gpu, r, &scene, want);
    r.feature(
        "9. indirect first instance",
        gpu,
        F::INDIRECT_FIRST_INSTANCE,
    );
    r.row(
        "9. indirect first instance",
        "first_instance from an indirect draw",
        if first { Status::Pass } else { Status::Skip },
        if first {
            "the 10k-slot multi-draw above places every quad by its first_instance"
        } else {
            "feature missing"
        },
    );
}

const MESH: &str = "
enable wgpu_mesh_shader;
const GRID: u32 = 100u;
struct V { @builtin(position) position: vec4<f32> };
struct P { @builtin(triangle_indices) indices: vec3<u32> };
struct M {
    @builtin(vertices) vertices: array<V, 4>,
    @builtin(primitives) primitives: array<P, 2>,
    @builtin(vertex_count) vertex_count: u32,
    @builtin(primitive_count) primitive_count: u32,
};
var<workgroup> m: M;
fn visible(i: u32) -> bool { return ((i * 2654435761u) >> 16u) % 4u != 0u; }
@mesh(m) @workgroup_size(1)
fn ms(@builtin(workgroup_id) wg: vec3<u32>) {
    let cell = wg.y * GRID + wg.x;
    if (visible(cell)) {
        m.vertex_count = 4u;
        m.primitive_count = 2u;
        for (var v = 0u; v < 4u; v++) {
            let c = vec2<f32>(f32(cell % GRID), f32(cell / GRID)) + vec2<f32>(f32(v & 1u), f32(v >> 1u));
            m.vertices[v].position = vec4<f32>(c * (2.0 / f32(GRID)) - 1.0, 0.0, 1.0);
        }
        m.primitives[0].indices = vec3<u32>(0u, 1u, 2u);
        m.primitives[1].indices = vec3<u32>(2u, 1u, 3u);
    } else {
        m.vertex_count = 0u;
        m.primitive_count = 0u;
    }
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }
";

/// The same grid from a mesh shader: one workgroup per quad, the cull inside it.
fn mesh(gpu: &Gpu, r: &mut Report, scene: &Scene, want: u32) {
    let name = "mesh shader, one workgroup per quad (WGSL through naga)";
    if !gpu.has(wgpu::Features::EXPERIMENTAL_MESH_SHADER) {
        r.row(S, name, Status::Skip, "feature missing");
        return;
    }
    let pipeline = gpu.scoped(|| {
        let module = gpu.shader("mesh", MESH);
        Ok(gpu
            .device
            .create_mesh_pipeline(&wgpu::MeshPipelineDescriptor {
                label: Some("mesh"),
                layout: None,
                task: None,
                mesh: wgpu::MeshState {
                    module: &module,
                    entry_point: Some("ms"),
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::TextureFormat::R8Unorm.into())],
                }),
                multiview: None,
                cache: None,
            }))
    });
    match pipeline {
        Ok(p) => variant(gpu, r, scene, want, name, |enc, timer, run| {
            let mut pass = target_pass(enc, scene, timer, run);
            pass.set_pipeline(&p);
            pass.draw_mesh_tasks(GRID, GRID, 1);
        }),
        Err(e) => r.row(S, name, Status::Fail, e),
    }
}

fn scene(gpu: &Gpu) -> Scene {
    use wgpu::BufferUsages as B;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("indirect target"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let index = gpu.buffer_init("quad", &gpu::bytes_u32(&[0, 1, 2, 2, 1, 3]), B::INDEX);
    let args = gpu.buffer("args", u64::from(N) * 20, B::STORAGE | B::INDIRECT);
    let count = gpu.buffer("count", 4, B::STORAGE | B::INDIRECT | B::COPY_DST);
    let one = gpu.buffer("one", 20, B::STORAGE | B::INDIRECT | B::COPY_DST);
    let one_template = gpu.buffer_init(
        "one template",
        &gpu::bytes_u32(&[6, 0, 0, 0, 0]),
        B::COPY_SRC,
    );
    let ids = gpu.buffer("ids", u64::from(N) * 4, B::STORAGE);

    let cull_module = gpu.shader("cull", CULL);
    let cull_layout = gpu
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cull"),
            entries: &(0..4)
                .map(crate::bindless::storage_entry)
                .collect::<Vec<_>>(),
        });
    let cull_pl = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&cull_layout)],
            immediate_size: 0,
        });
    let cull = [0.0, 1.0, 2.0].map(|mode| {
        gpu.device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("cull"),
                layout: Some(&cull_pl),
                module: &cull_module,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[("MODE", mode)],
                    ..Default::default()
                },
                cache: None,
            })
    });
    let cull_group = [0, 1, 2].map(|_| {
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cull"),
            layout: &cull_layout,
            entries: &[
                entire(0, &args),
                entire(1, &count),
                entire(2, &one),
                entire(3, &ids),
            ],
        })
    });

    let draw_src = if gpu.has(wgpu::Features::IMMEDIATES) {
        DRAW.to_owned()
    } else {
        DRAW.replace("var<immediate> imm: u32;", "const imm: u32 = 0u;")
    };
    let draw_module = gpu.shader("draw", &draw_src);
    let draw_layout = gpu
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
    let immediates = if gpu.has(wgpu::Features::IMMEDIATES) {
        4
    } else {
        0
    };
    let draw_pl = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&draw_layout)],
            immediate_size: immediates,
        });
    let render = |entry: &str| {
        gpu.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&draw_pl),
                vertex: wgpu::VertexState {
                    module: &draw_module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &draw_module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::TextureFormat::R8Unorm.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
    };
    let draw_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("draw"),
        layout: &draw_layout,
        entries: &[entire(0, &ids)],
    });
    Scene {
        target,
        view,
        index,
        args,
        count,
        one,
        one_template,
        cull,
        cull_group,
        draw_group,
        by_instance: render("by_instance"),
        by_list: render("by_list"),
        by_immediate: if immediates > 0 {
            render("by_immediate")
        } else {
            render("by_instance")
        },
    }
}

fn entire(binding: u32, b: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: b.as_entire_binding(),
    }
}

fn cull(
    enc: &mut wgpu::CommandEncoder,
    s: &Scene,
    mode: usize,
    timer: Option<&gpu::Timer>,
    run: u32,
) {
    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("cull"),
        timestamp_writes: timer.map(|t| t.compute_pass(run * 2)),
    });
    pass.set_pipeline(&s.cull[mode]);
    pass.set_bind_group(0, &s.cull_group[mode], &[]);
    pass.dispatch_workgroups(N.div_ceil(64), 1, 1);
}

fn target_pass<'e>(
    enc: &'e mut wgpu::CommandEncoder,
    s: &Scene,
    timer: Option<&gpu::Timer>,
    run: u32,
) -> wgpu::RenderPass<'e> {
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("draw"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &s.view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: timer.map(|t| t.render_pass(run * 2 + 1)),
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// Records `record` [`RUNS`] times, each submitted and waited for, then counts the covered
/// pixels of the last run.
fn variant(
    gpu: &Gpu,
    r: &mut Report,
    s: &Scene,
    want: u32,
    name: &str,
    record: impl Fn(&mut wgpu::CommandEncoder, Option<&gpu::Timer>, u32),
) {
    let timer = gpu.timer(RUNS * 2);
    gpu.warm();
    let res = gpu.scoped(|| {
        let mut cpu = Vec::new();
        for run in 0..RUNS {
            let t = Instant::now();
            let mut enc = gpu.encoder();
            record(&mut enc, timer.as_ref(), run);
            gpu.queue.submit([enc.finish()]);
            cpu.push(t.elapsed().as_secs_f64() * 1e3);
            gpu.wait();
        }
        let gpu_ms = timer.as_ref().and_then(|t| {
            let pairs = t.read(gpu);
            gpu::median(
                pairs
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| p[0] + p[1])
                    .collect(),
            )
        });
        let got = covered(gpu, s);
        r.time(
            S,
            name,
            gpu::median(cpu),
            gpu_ms,
            "cpu = record + submit; gpu = cull + draw passes",
        );
        if got == want {
            Ok(format!("{got} pixels covered, as expected"))
        } else {
            Err(format!("{got} pixels covered, want {want}"))
        }
    });
    r.result(S, name, res);
}

fn covered(gpu: &Gpu, s: &Scene) -> u32 {
    let buf = gpu.buffer(
        "coverage",
        u64::from(ROW * SIZE),
        wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
    );
    let mut enc = gpu.encoder();
    enc.copy_texture_to_buffer(
        s.target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    gpu.submit_wait(enc);
    let bytes = gpu.read(&buf, 0, u64::from(ROW * SIZE));
    let mut n = 0;
    for row in bytes.as_chunks::<{ ROW as usize }>().0 {
        n += row[..SIZE as usize].iter().filter(|&&b| b != 0).count();
    }
    gpu::u(n)
}
