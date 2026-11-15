//! Section 3, ray queries: a bottom- and top-level structure over a checkerboard of a few thousand
//! triangles, traced from a grid of rays in compute and checked ray by ray; then the build and
//! trace times of a half-million-triangle heightfield and of a top level of many instances.

use std::time::Instant;

use crate::gpu::{self, Gpu};
use crate::report::{Report, Status};

const S: &str = "3. ray queries";
const RUNS: u32 = 10;

const TRACE: &str = "
enable wgpu_ray_query;
@group(0) @binding(0) var scene: acceleration_structure;
@group(0) @binding(1) var<storage, read_write> out: array<u32>;
override W: u32;
override EXTENT: f32;
override MODE: u32;
fn hash(x: u32) -> u32 {
    var h = x * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    return (h >> 22u) ^ h;
}
fn rnd(s: ptr<function, u32>) -> f32 { *s = hash(*s); return f32(*s) / 4294967296.0; }
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= W || id.y >= W) { return; }
    let idx = id.y * W + id.x;
    var origin = vec3<f32>((vec2<f32>(id.xy) + 0.5) / f32(W) * EXTENT, 10.0);
    var dir = vec3<f32>(0.0, 0.0, -1.0);
    if (MODE == 1u) {
        var s = idx;
        origin = vec3<f32>(rnd(&s) * EXTENT, rnd(&s) * EXTENT, 2.0 + rnd(&s) * 4.0);
        dir = normalize(vec3<f32>(rnd(&s) * 2.0 - 1.0, rnd(&s) * 2.0 - 1.0, -rnd(&s) - 0.05));
    }
    var rq: ray_query;
    rayQueryInitialize(&rq, scene, RayDesc(0u, 0xFFu, 0.0, 1000.0, origin, dir));
    while (rayQueryProceed(&rq)) {}
    let hit = rayQueryGetCommittedIntersection(&rq);
    out[idx] = select(0u, hit.primitive_index + 1u, hit.kind != RAY_QUERY_INTERSECTION_NONE);
}";

struct Mesh {
    vertices: wgpu::Buffer,
    size: wgpu::BlasTriangleGeometrySizeDescriptor,
    blas: wgpu::Blas,
    triangles: u32,
}

pub fn run(gpu: &Gpu, r: &mut Report) {
    use wgpu::Features as F;
    let rq = r.feature(S, gpu, F::EXPERIMENTAL_RAY_QUERY);
    r.feature(S, gpu, F::EXPERIMENTAL_RAY_HIT_VERTEX_RETURN);
    r.feature(S, gpu, F::EXPERIMENTAL_RAY_TRACING_PIPELINES);
    r.feature(S, gpu, F::EXTENDED_ACCELERATION_STRUCTURE_VERTEX_FORMATS);
    r.feature(S, gpu, F::ACCELERATION_STRUCTURE_BINDING_ARRAY);
    let l = &gpu.limits;
    r.row(
        S,
        "limits",
        Status::Info,
        format!(
            "blas primitives {}, blas geometries {}, tlas instances {}, structures/stage {}",
            l.max_blas_primitive_count,
            l.max_blas_geometry_count,
            l.max_tlas_instance_count,
            l.max_acceleration_structures_per_shader_stage
        ),
    );
    if !rq {
        r.row(S, "ray query workloads", Status::Skip, "feature missing");
        return;
    }

    // The checkerboard: 64 x 64 cells of side 1, every other one solid, two triangles each.
    let res = gpu.scoped(|| {
        let mut solid = vec![u32::MAX; 64 * 64];
        let mut tris = Vec::new();
        let mut s = 0;
        for cy in 0..64u32 {
            for cx in 0..64u32 {
                if (cx + cy) % 2 == 0 {
                    solid[(cy * 64 + cx) as usize] = s;
                    s += 1;
                    quad(&mut tris, cx as f32, cy as f32, [0.0; 4]);
                }
            }
        }
        let mesh = mesh(gpu, &tris);
        let tlas = tlas(gpu, &mesh.blas, 1, |_| [0.0, 0.0]);
        gpu.warm();
        let t = Instant::now();
        let mut enc = gpu.encoder();
        build(&mut enc, &mesh, Some(&tlas));
        gpu.submit_wait(enc);
        let build_ms = t.elapsed().as_secs_f64() * 1e3;
        let w = 512;
        let (out, gpu_ms) = trace(gpu, &tlas, w, 64.0, 0)?;
        let hits = gpu.read_u32s(&out, 0, (w * w) as usize);
        let mut bad = 0;
        for y in 0..w {
            for x in 0..w {
                let cell = solid[((y / 8) * 64 + x / 8) as usize];
                let got = hits[(y * w + x) as usize];
                let ok = if cell == u32::MAX {
                    got == 0
                } else {
                    got != 0 && (got - 1) / 2 == cell
                };
                bad += usize::from(!ok);
            }
        }
        r.time(
            S,
            format!("build: {} triangles + 1-instance top level", mesh.triangles),
            Some(build_ms),
            None,
            "cpu = submit to done (wall)",
        );
        r.time(
            S,
            format!("trace {w}x{w} coherent rays, checkerboard"),
            None,
            gpu_ms,
            "",
        );
        if bad == 0 {
            Ok(format!(
                "{} triangles, {} rays, every hit and miss correct",
                mesh.triangles,
                w * w
            ))
        } else {
            Err(format!("{bad} of {} rays wrong", w * w))
        }
    });
    r.result(S, "BLAS/TLAS build and trace (checkerboard)", res);

    // A heightfield of half a million triangles, traced with a million rays.
    let res = gpu.scoped(|| {
        let side = 512u32.min(isqrt(gpu.limits.max_blas_primitive_count / 2));
        let mut tris = Vec::with_capacity((side * side * 18) as usize);
        let h = |x: u32, y: u32| (x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)) % 1000;
        for cy in 0..side {
            for cx in 0..side {
                let z = [h(cx, cy), h(cx + 1, cy), h(cx, cy + 1), h(cx + 1, cy + 1)]
                    .map(|v| v as f32 / 1000.0);
                quad(&mut tris, cx as f32, cy as f32, z);
            }
        }
        let mesh = mesh(gpu, &tris);
        let tlas = tlas(gpu, &mesh.blas, 1, |_| [0.0, 0.0]);
        gpu.warm();
        let t = Instant::now();
        let mut enc = gpu.encoder();
        build(&mut enc, &mesh, Some(&tlas));
        gpu.submit_wait(enc);
        let build_ms = t.elapsed().as_secs_f64() * 1e3;
        r.time(
            S,
            format!("build: {} triangles + 1-instance top level", mesh.triangles),
            Some(build_ms),
            None,
            "cpu = submit to done (wall)",
        );
        let w = 1024;
        let extent = side as f32;
        let (_, coherent) = trace(gpu, &tlas, w, extent, 0)?;
        r.time(
            S,
            format!("trace {w}x{w} coherent rays, heightfield"),
            None,
            coherent,
            "straight down",
        );
        let (out, random) = trace(gpu, &tlas, w, extent, 1)?;
        r.time(
            S,
            format!("trace {w}x{w} random rays, heightfield"),
            None,
            random,
            "random origins and directions",
        );
        let hits = gpu.read_u32s(&out, 0, (w * w) as usize);
        let n = hits.iter().filter(|&&h| h != 0).count();
        Ok(format!(
            "{} triangles, {} random rays, {n} hit",
            mesh.triangles,
            w * w
        ))
    });
    r.result(S, "heightfield build and trace", res);

    // A top level of many instances of the checkerboard.
    let res = gpu.scoped(|| {
        let n = 4096u32.min(gpu.limits.max_tlas_instance_count);
        let mut tris = Vec::new();
        for cy in 0..64u32 {
            for cx in 0..64u32 {
                if (cx + cy) % 2 == 0 {
                    quad(&mut tris, cx as f32, cy as f32, [0.0; 4]);
                }
            }
        }
        let mesh = mesh(gpu, &tris);
        let mut enc = gpu.encoder();
        build(&mut enc, &mesh, None);
        gpu.submit_wait(enc);
        let side = isqrt(n);
        let tlas = tlas(gpu, &mesh.blas, n, |i| {
            [(i % side) as f32 * 64.0, (i / side) as f32 * 64.0]
        });
        let mut enc = gpu.encoder();
        enc.build_acceleration_structures(std::iter::empty(), std::iter::once(&tlas));
        let ms = gpu.submit_wait(enc);
        r.time(
            S,
            format!("build: top level of {n} instances"),
            Some(ms),
            None,
            "cpu = submit to done (wall); instance upload included",
        );
        let (_, gpu_ms) = trace(gpu, &tlas, 1024, 64.0 * side as f32, 1)?;
        r.time(
            S,
            format!("trace 1024x1024 random rays, {n} instances"),
            None,
            gpu_ms,
            "",
        );
        Ok(format!("{n} instances of {} triangles", mesh.triangles))
    });
    r.result(S, "many-instance top level", res);
}

fn isqrt(n: u32) -> u32 {
    let mut s = 0u32;
    while (s + 1) * (s + 1) <= n {
        s += 1;
    }
    s
}

/// Two triangles over the unit cell at `(x, y)`, with its corners' heights `z`.
fn quad(out: &mut Vec<f32>, x: f32, y: f32, z: [f32; 4]) {
    let p = [
        [x, y, z[0]],
        [x + 1.0, y, z[1]],
        [x, y + 1.0, z[2]],
        [x + 1.0, y + 1.0, z[3]],
    ];
    for i in [0, 1, 2, 2, 1, 3] {
        out.extend_from_slice(&p[i]);
    }
}

fn mesh(gpu: &Gpu, positions: &[f32]) -> Mesh {
    let vertices = gpu.buffer_init(
        "traced",
        &gpu::bytes_f32(positions),
        wgpu::BufferUsages::BLAS_INPUT,
    );
    let count = gpu::u(positions.len() / 3);
    let size = wgpu::BlasTriangleGeometrySizeDescriptor {
        vertex_format: wgpu::VertexFormat::Float32x3,
        vertex_count: count,
        index_format: None,
        index_count: None,
        flags: wgpu::AccelerationStructureGeometryFlags::OPAQUE,
    };
    let blas = gpu.device.create_blas(
        &wgpu::CreateBlasDescriptor {
            label: Some("probe"),
            flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
            update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        },
        wgpu::BlasGeometrySizeDescriptors::Triangles {
            descriptors: vec![size.clone()],
        },
    );
    Mesh {
        vertices,
        size,
        blas,
        triangles: count / 3,
    }
}

/// A top level of `n` instances of `blas`, instance `i` moved by `offset(i)` in x and y.
fn tlas(gpu: &Gpu, blas: &wgpu::Blas, n: u32, offset: impl Fn(u32) -> [f32; 2]) -> wgpu::Tlas {
    let mut tlas = gpu.device.create_tlas(&wgpu::CreateTlasDescriptor {
        label: Some("probe"),
        max_instances: n.max(1),
        flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
        update_mode: wgpu::AccelerationStructureUpdateMode::Build,
    });
    for i in 0..n {
        let [x, y] = offset(i);
        tlas[i as usize] = Some(wgpu::TlasInstance::new(
            blas,
            [1.0, 0.0, 0.0, x, 0.0, 1.0, 0.0, y, 0.0, 0.0, 1.0, 0.0],
            0,
            0xFF,
        ));
    }
    tlas
}

fn build(enc: &mut wgpu::CommandEncoder, mesh: &Mesh, tlas: Option<&wgpu::Tlas>) {
    let entry = wgpu::BlasBuildEntry {
        blas: &mesh.blas,
        geometry: wgpu::BlasGeometries::TriangleGeometries(vec![wgpu::BlasTriangleGeometry {
            size: &mesh.size,
            vertex_buffer: &mesh.vertices,
            first_vertex: 0,
            vertex_stride: 12,
            index_buffer: None,
            first_index: None,
            transform_buffer: None,
            transform_buffer_offset: None,
        }]),
    };
    enc.build_acceleration_structures(std::iter::once(&entry), tlas);
}

/// Traces a `w` x `w` grid of rays [`RUNS`] times: the hits and the median GPU time.
fn trace(
    gpu: &Gpu,
    tlas: &wgpu::Tlas,
    w: u32,
    extent: f32,
    mode: u32,
) -> Result<(wgpu::Buffer, Option<f64>), String> {
    let module = gpu.shader("trace", TRACE);
    let pipeline = gpu
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("trace"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[
                    ("W", f64::from(w)),
                    ("EXTENT", f64::from(extent)),
                    ("MODE", f64::from(mode)),
                ],
                ..Default::default()
            },
            cache: None,
        });
    let out = gpu.buffer(
        "hits",
        u64::from(w * w) * 4,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::AccelerationStructure(tlas),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: out.as_entire_binding(),
            },
        ],
    });
    let timer = gpu.timer(RUNS);
    gpu.warm();
    let mut enc = gpu.encoder();
    for run in 0..RUNS {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("trace"),
            timestamp_writes: timer.as_ref().map(|t| t.compute_pass(run)),
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(w.div_ceil(8), w.div_ceil(8), 1);
    }
    gpu.submit_wait(enc);
    Ok((out, timer.and_then(|t| gpu::median(t.read(gpu)))))
}
