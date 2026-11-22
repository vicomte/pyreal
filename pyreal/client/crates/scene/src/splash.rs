//! The picture the window shows while the device's pipelines are made.
//!
//! A driver whose shader cache is cold (the first start after an update, a driver update or a
//! cleaned cache) takes many seconds over the engine's pipelines, and nothing is drawn until they
//! are made: the window would stand empty -- white, on Windows -- for all of that time. This is
//! drawn first instead, with one small pipeline of its own: a still of the screens before the
//! world (the portal lights, the logo and a panel saying the game is loading), fitted to the window
//! whole, the rest of it the picture's own edge colour. The picture is the product's
//! (`Host::SPLASH_PNG`); on Windows the desktop has painted the same one on the window already,
//! with no device at all, and this takes over from it.

use pyreal_engine::Gpu;

const SHADER: &str = r"
struct Fit { scale: vec4<f32> };
@group(0) @binding(0) var<uniform> fit: Fit;
@group(0) @binding(1) var picture: texture_2d<f32>;
@group(0) @binding(2) var picture_sampler: sampler;

struct Corner {
    @builtin(position) at: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_picture(@builtin(vertex_index) i: u32) -> Corner {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let c = corners[i];
    var out: Corner;
    out.at = vec4<f32>((c.x * 2.0 - 1.0) * fit.scale.x, (1.0 - c.y * 2.0) * fit.scale.y, 0.0, 1.0);
    out.uv = c;
    return out;
}

@fragment
fn fs_picture(c: Corner) -> @location(0) vec4<f32> {
    return textureSample(picture, picture_sampler, c.uv);
}
";

/// The still, on the device, and what draws it.
pub struct Splash {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    fit: wgpu::Buffer,
    /// The picture's size.
    size: (u32, u32),
    /// The colour round the picture where the window is not its shape, linear.
    round: wgpu::Color,
}

impl std::fmt::Debug for Splash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Splash").field("size", &self.size).finish()
    }
}

impl Splash {
    /// The still `png` on `gpu`, for a window of `format`; `None` (logged) when the picture will
    /// not decode, which only a broken build could do.
    #[must_use]
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, png: &[u8]) -> Option<Self> {
        let (w, h, rgba) = pyreal_art_pack::images::decode_png(png)
            .map_err(|e| tracing::warn!("the splash picture: {e}"))
            .ok()?;
        let device = &gpu.device;
        let extent = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("splash"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            texture.as_image_copy(),
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            extent,
        );
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("splash"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let fit = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("splash fit"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("splash"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("splash"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: fit.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let module = gpu.shader("splash", SHADER);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("splash"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
            label: Some("splash"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_picture"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_picture"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Some(Self {
            pipeline,
            group,
            fit,
            size: (w, h),
            round: border_colour(w, h, &rgba),
        })
    }

    /// Draws the still to `surface`, configured `size`, and presents it. Whether it was shown (a
    /// window being made, minimised or lost shows nothing, and the caller tries again later).
    pub fn draw(&self, gpu: &Gpu, surface: &wgpu::Surface<'_>, size: (u32, u32)) -> bool {
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => return false,
        };
        let scale = fit(self.size, size);
        let bytes: Vec<u8> = [scale.0, scale.1, 0.0, 0.0]
            .iter()
            .flat_map(|f| f.to_ne_bytes())
            .collect();
        gpu.queue.write_buffer(&self.fit, 0, &bytes);
        let view = frame.texture.create_view(&Default::default());
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("splash"),
            });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("splash"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.round),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.draw(0..6, 0..1);
        }
        gpu.queue.submit([enc.finish()]);
        gpu.queue.present(frame);
        true
    }
}

/// How much of the window, across and down (1 the whole of it), a picture `picture` fills drawn
/// as large as it goes whole in a window `window`.
#[must_use]
fn fit(picture: (u32, u32), window: (u32, u32)) -> (f32, f32) {
    #[allow(clippy::cast_precision_loss)]
    let aspect = |(w, h): (u32, u32)| w.max(1) as f32 / h.max(1) as f32;
    let (p, w) = (aspect(picture), aspect(window));
    if w > p {
        (p / w, 1.0)
    } else {
        (1.0, w / p)
    }
}

/// The mean colour of the picture's outermost texels, linear: what the window shows round it.
/// The mean is taken of the bytes, as the desktop's own painting of the picture takes it, so the
/// two show the same colour.
#[must_use]
fn border_colour(w: u32, h: u32, rgba: &[u8]) -> wgpu::Color {
    let (w, h) = (w as usize, h as usize);
    let mut sum = [0u64; 3];
    let mut n = 0u64;
    for y in 0..h {
        for x in 0..w {
            if !(x == 0 || y == 0 || x + 1 == w || y + 1 == h) {
                continue;
            }
            let Some(t) = rgba.get((y * w + x) * 4..(y * w + x) * 4 + 3) else {
                continue;
            };
            for (s, c) in sum.iter_mut().zip(t) {
                *s += u64::from(*c);
            }
            n += 1;
        }
    }
    let linear = |s: u64| {
        let byte = u8::try_from(s / n.max(1)).unwrap_or(u8::MAX);
        f64::from(pyreal_engine::asset::material::srgb_to_linear(byte))
    };
    wgpu::Color {
        r: linear(sum[0]),
        g: linear(sum[1]),
        b: linear(sum[2]),
        a: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_still_fills_a_window_of_its_own_shape_and_stands_whole_in_any_other() {
        assert_eq!(fit((800, 600), (800, 600)), (1.0, 1.0));
        assert_eq!(fit((800, 600), (1600, 1200)), (1.0, 1.0));
        // Wider: bars either side.
        let (x, y) = fit((800, 600), (1920, 1080));
        assert!((x - 0.75).abs() < 1e-6 && (y - 1.0).abs() < 1e-6);
        // Taller: bars above and below.
        let (x, y) = fit((800, 600), (600, 800));
        assert!((x - 1.0).abs() < 1e-6 && (y - 0.5625).abs() < 1e-6);
    }

    #[test]
    fn the_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(SHADER).expect("the splash shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("the splash shader validates");
    }

    /// The client's own splash (`Dereth::SPLASH_PNG`).
    const PICTURE: &[u8] = include_bytes!("../../../assets/splash.png");

    #[test]
    fn the_picture_decodes_at_the_login_size() {
        let (w, h, rgba) =
            pyreal_art_pack::images::decode_png(PICTURE).expect("the splash decodes");
        assert_eq!((w, h), (800, 600));
        let round = border_colour(w, h, &rgba);
        // The portal lights' dark blue: blue the most, and dark.
        assert!(round.b > round.r && round.b < 0.1, "{round:?}");
    }
}
