//! The bindless material table: one binding array holding every texture (partially bound, so the
//! table grows without filling every slot) and one storage buffer of material records. Beside
//! them, each picture's companions: the maps that go with a ground picture wherever a cell lays
//! it (an art pack's normal and surface, and the picture its steep slopes take), and where its
//! height is kept.
//!
//! A texture or material no longer drawn can be freed ([`MaterialTable::free_texture`],
//! [`MaterialTable::free_material`]); its slot is reused, a freed texture's slot sampling white
//! until then. Pinned ones (the sky's, the cover's) are never freed.

use std::num::NonZeroU32;

use crate::asset::material::{MaterialRecord, MATERIAL_RECORD_SIZE, NO_TEXTURE};
use crate::asset::texture::TexturePixels;
use crate::gpu::Gpu;
use crate::EngineError;

/// Slots in the texture binding array.
pub const MAX_TEXTURES: u32 = 16_384;
/// Records the material buffer holds.
pub const MAX_MATERIALS: u32 = 65_536;
/// The texture every material without a base map samples: one white texel.
pub const WHITE: u32 = 0;

/// Where a ground picture's height is kept (its companions' fifth word); none of them: it has none
/// and blends as retail's.
pub mod height_in {
    /// In the picture's own alpha (an art pack's ground).
    pub const ALPHA: u32 = 1;
    /// In the blue of its surface companion (an art pack's material for a retail picture).
    pub const SURFACE: u32 = 2;
}

/// Every texture the materials sample, and the records that point at them.
#[derive(Debug)]
pub struct MaterialTable {
    views: Vec<wgpu::TextureView>,
    textures: Vec<Option<wgpu::Texture>>,
    /// Each slot's bytes.
    sizes: Vec<u64>,
    texture_bytes: u64,
    free_textures: Vec<u32>,
    records: Vec<MaterialRecord>,
    /// Whether each record is in use.
    live: Vec<bool>,
    free_records: Vec<u32>,
    /// Reused records to write again.
    rewritten: Vec<u32>,
    /// Records written since the last upload start here.
    uploaded: usize,
    /// Textures and materials never freed.
    pinned_textures: std::collections::BTreeSet<u32>,
    pinned_materials: std::collections::BTreeSet<u32>,
    pub buffer: wgpu::Buffer,
    /// Per texture slot: its normal, its surface, its steep slopes' picture, and that picture's
    /// metres a repeat ([`NO_TEXTURE`] where there is none); then where its height is kept
    /// ([`height_in`]), and its detail grain's slot, metres a repeat and strength (bits; no
    /// grain where the metres are nought).
    companions: Vec<[u32; 8]>,
    companions_dirty: bool,
    companion_buffer: wgpu::Buffer,
    pub layout: wgpu::BindGroupLayout,
    group: Option<wgpu::BindGroup>,
    repeat: wgpu::Sampler,
    clamp: wgpu::Sampler,
}

impl MaterialTable {
    pub fn new(gpu: &Gpu) -> Self {
        let device = &gpu.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material table"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: NonZeroU32::new(MAX_TEXTURES),
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let sampler = |label, mode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                address_mode_u: mode,
                address_mode_v: mode,
                address_mode_w: mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                anisotropy_clamp: 8,
                ..Default::default()
            })
        };
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("materials"),
            size: u64::from(MAX_MATERIALS) * MATERIAL_RECORD_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let companion_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("texture companions"),
            size: u64::from(MAX_TEXTURES) * 32,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut table = Self {
            views: Vec::new(),
            textures: Vec::new(),
            sizes: Vec::new(),
            texture_bytes: 0,
            free_textures: Vec::new(),
            records: Vec::new(),
            live: Vec::new(),
            free_records: Vec::new(),
            rewritten: Vec::new(),
            uploaded: 0,
            pinned_textures: std::collections::BTreeSet::new(),
            pinned_materials: std::collections::BTreeSet::new(),
            buffer,
            companions: Vec::new(),
            companions_dirty: true,
            companion_buffer,
            layout,
            group: None,
            repeat: sampler("repeat", wgpu::AddressMode::Repeat),
            clamp: sampler("clamp", wgpu::AddressMode::ClampToEdge),
        };
        // Slot 0 is white, so a lookup that finds nothing samples something harmless.
        let _ = table.add_texture(gpu, &TexturePixels::solid([255, 255, 255, 255]));
        table.pinned_textures.insert(WHITE);
        table
    }

    /// Frees texture `slot` (never white or a pinned one): its slot samples white until reused.
    pub fn free_texture(&mut self, slot: u32) {
        let i = slot as usize;
        if slot == WHITE
            || self.pinned_textures.contains(&slot)
            || self.textures.get(i).is_none_or(Option::is_none)
        {
            return;
        }
        self.textures[i] = None;
        self.views[i] = self.views[WHITE as usize].clone();
        self.texture_bytes -= self.sizes[i];
        self.sizes[i] = 0;
        self.companions[i] = [NO_TEXTURE, NO_TEXTURE, NO_TEXTURE, 0, 0, 0, 0, 0];
        self.companions_dirty = true;
        self.free_textures.push(slot);
        self.group = None;
    }

    /// Frees material `index` (never a pinned one); its record is reused.
    pub fn free_material(&mut self, index: u32) {
        let i = index as usize;
        if self.pinned_materials.contains(&index) || !self.live.get(i).copied().unwrap_or(false) {
            return;
        }
        self.live[i] = false;
        self.free_records.push(index);
    }

    /// Keeps texture `slot` whatever draws it.
    pub fn pin_texture(&mut self, slot: u32) {
        self.pinned_textures.insert(slot);
    }

    /// Keeps material `index` whatever draws it.
    pub fn pin_material(&mut self, index: u32) {
        self.pinned_materials.insert(index);
    }

    /// The pinned textures and materials.
    #[must_use]
    pub fn pinned(&self) -> (Vec<u32>, Vec<u32>) {
        (
            self.pinned_textures.iter().copied().collect(),
            self.pinned_materials.iter().copied().collect(),
        )
    }

    /// Whether texture `slot` holds a picture.
    #[must_use]
    pub fn texture_live(&self, slot: u32) -> bool {
        self.textures
            .get(slot as usize)
            .is_some_and(Option::is_some)
    }

    /// Whether material `index` is in use.
    #[must_use]
    pub fn material_live(&self, index: u32) -> bool {
        self.live.get(index as usize).copied().unwrap_or(false)
    }

    /// Textures in use.
    #[must_use]
    pub fn live_textures(&self) -> usize {
        self.views.len() - self.free_textures.len()
    }

    /// Materials in use.
    #[must_use]
    pub fn live_materials(&self) -> usize {
        self.records.len() - self.free_records.len()
    }

    /// Uploads `pixels` to a new slot.
    ///
    /// # Errors
    /// The binding array is full.
    pub fn add_texture(&mut self, gpu: &Gpu, pixels: &TexturePixels) -> Result<u32, EngineError> {
        let reused = self.free_textures.pop();
        let slot = reused.unwrap_or_else(|| u32::try_from(self.views.len()).unwrap_or(u32::MAX));
        if slot >= MAX_TEXTURES {
            return Err(EngineError::PoolFull("texture", 1));
        }
        let format = pixels.layout.wgpu_format();
        let levels = u32::try_from(pixels.levels.len()).unwrap_or(1);
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: pixels.width,
                height: pixels.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let block = if pixels.layout.is_block() { 4 } else { 1 };
        for (level, bytes) in pixels.levels.iter().enumerate() {
            let shift = u32::try_from(level).unwrap_or(0);
            let w = (pixels.width >> shift).max(1);
            let h = (pixels.height >> shift).max(1);
            let (bw, bh) = (w.div_ceil(block), h.div_ceil(block));
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: shift,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bw * pixels.layout.block_bytes()),
                    rows_per_image: Some(bh),
                },
                wgpu::Extent3d {
                    width: bw * block,
                    height: bh * block,
                    depth_or_array_layers: 1,
                },
            );
        }
        let bytes = pixels.byte_size() as u64;
        self.texture_bytes += bytes;
        crate::frame_work::note(|w| {
            w.textures += 1;
            w.texture_bytes += bytes;
        });
        let view = texture.create_view(&Default::default());
        let none = [NO_TEXTURE, NO_TEXTURE, NO_TEXTURE, 0, 0, 0, 0, 0];
        if reused.is_some() {
            let i = slot as usize;
            self.views[i] = view;
            self.textures[i] = Some(texture);
            self.sizes[i] = bytes;
            self.companions[i] = none;
        } else {
            self.views.push(view);
            self.textures.push(Some(texture));
            self.sizes.push(bytes);
            self.companions.push(none);
        }
        self.companions_dirty = true;
        self.group = None;
        Ok(slot)
    }

    /// The maps that go with picture `slot` wherever a cell lays it: `[normal, surface, steep
    /// picture, the steep picture's metres a repeat as bits]`.
    pub fn set_companions(&mut self, slot: u32, companions: [u32; 4]) {
        if let Some(c) = self.companions.get_mut(slot as usize) {
            c[..4].copy_from_slice(&companions);
            self.companions_dirty = true;
        }
    }

    /// Where ground picture `slot`'s height is kept, as [`height_in`]'s bits say.
    pub fn set_height_in(&mut self, slot: u32, bits: u32) {
        if let Some(c) = self.companions.get_mut(slot as usize) {
            c[4] = bits;
            self.companions_dirty = true;
        }
    }

    /// Ground picture `slot`'s detail grain: its picture, metres a repeat and strength.
    pub fn set_detail(&mut self, slot: u32, detail: u32, size: f32, strength: f32) {
        if let Some(c) = self.companions.get_mut(slot as usize) {
            c[5] = detail;
            c[6] = size.to_bits();
            c[7] = strength.to_bits();
            self.companions_dirty = true;
        }
    }

    /// Picture `slot`'s companions.
    #[must_use]
    pub fn companions(&self, slot: u32) -> Option<[u32; 8]> {
        self.companions.get(slot as usize).copied()
    }

    /// Adds a record; its index is what draws name.
    ///
    /// # Errors
    /// The material buffer is full.
    pub fn add_material(&mut self, record: MaterialRecord) -> Result<u32, EngineError> {
        if let Some(index) = self.free_records.pop() {
            self.records[index as usize] = record;
            self.live[index as usize] = true;
            if (index as usize) < self.uploaded {
                self.rewritten.push(index);
            }
            return Ok(index);
        }
        let index = u32::try_from(self.records.len()).unwrap_or(u32::MAX);
        if index >= MAX_MATERIALS {
            return Err(EngineError::PoolFull("material", 1));
        }
        self.records.push(record);
        self.live.push(true);
        Ok(index)
    }

    #[must_use]
    pub fn material(&self, index: u32) -> Option<&MaterialRecord> {
        self.records.get(index as usize)
    }

    /// Writes the records added since the last call, and rebuilds the bind group when a texture
    /// was added.
    pub fn upload(&mut self, gpu: &Gpu) {
        for index in std::mem::take(&mut self.rewritten) {
            let mut bytes = Vec::new();
            self.records[index as usize].write(&mut bytes);
            gpu.queue.write_buffer(
                &self.buffer,
                u64::from(index) * MATERIAL_RECORD_SIZE,
                &bytes,
            );
        }
        if self.uploaded < self.records.len() {
            let mut bytes = Vec::new();
            for r in &self.records[self.uploaded..] {
                r.write(&mut bytes);
            }
            gpu.queue.write_buffer(
                &self.buffer,
                self.uploaded as u64 * MATERIAL_RECORD_SIZE,
                &bytes,
            );
            self.uploaded = self.records.len();
        }
        if self.companions_dirty {
            let bytes: Vec<u8> = self
                .companions
                .iter()
                .flatten()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            gpu.queue.write_buffer(&self.companion_buffer, 0, &bytes);
            self.companions_dirty = false;
        }
        if self.group.is_none() {
            crate::frame_work::note(|w| w.bind_groups += 1);
            let views: Vec<&wgpu::TextureView> = self.views.iter().collect();
            self.group = Some(gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("material table"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureViewArray(&views),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.repeat),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.clamp),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.companion_buffer.as_entire_binding(),
                    },
                ],
            }));
        }
    }

    /// The bind group, once [`Self::upload`] has made it.
    #[must_use]
    pub fn group(&self) -> Option<&wgpu::BindGroup> {
        self.group.as_ref()
    }

    #[must_use]
    pub fn texture_count(&self) -> usize {
        self.views.len()
    }

    #[must_use]
    pub fn texture_bytes(&self) -> u64 {
        self.texture_bytes
    }

    #[must_use]
    pub fn material_count(&self) -> usize {
        self.records.len()
    }
}
