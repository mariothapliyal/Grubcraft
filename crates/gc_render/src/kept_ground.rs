//! The kept ground: what the ground is at each pixel, stored while the
//! camera, the land and the weather hold still and read back on the frames
//! after, so that only its lighting is worked out again. See
//! `fs_terrain_keep` and `fs_terrain_kept` in world.wgsl, and
//! docs/10-architecture.md ("the kept ground").

use crate::settle::{Settle, Use};

/// Both images hold the distance of the ground stored at the pixel; then
/// `a` its colour (three floats) and `b` its normal, roughness and occlusion
/// packed, and its drawn height (see `fs_terrain_keep`).
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Uint;

/// Everything the stored ground depends on that can change while the
/// camera holds still, besides the size of the world's image (a new size
/// forgets what was stored). A new input to `ground_material` in world.wgsl
/// goes here too.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct GroundKey {
    /// The camera's view and projection, and its position, bit for bit.
    pub view: [u32; 16],
    pub camera: [u32; 3],
    /// The rows of the world's image drawn.
    pub rows: u32,
    /// `Renderer::land_changes`.
    pub land: u64,
    /// The water line.
    pub water: u32,
    /// How dry the season is, how wet the ground and how much snow lies,
    /// each in `weather_step`s.
    pub weather: [u32; 3],
}

/// A weather amount (0 to 1) in `steps`. Rain soaks the ground and snow
/// settles by easing towards an amount, which moves a little every frame
/// for minutes: keyed bit for bit, the ground would never be kept in rain
/// or snow. Kept ground waits for a whole step, so it lags by up to one:
/// a 256th of the wetting or the straw darkens or tints the ground by about
/// 0.1%, under half a step of 255; a snow patch's edge fades over 0.12 of
/// the noise it is cut from and moves 0.9 of it as the snow goes from none
/// to a blanket, so a 1,024th moves it by under 1% of its fade.
pub(crate) fn weather_step(amount: f32, steps: f32) -> u32 {
    (amount.clamp(0.0, 1.0) * steps).round() as u32
}

/// The pipelines and their images, made where the GPU can write images
/// from a fragment shader (every desktop GPU).
pub(crate) struct KeptGround {
    keep_layout: wgpu::BindGroupLayout,
    kept_layout: wgpu::BindGroupLayout,
    /// The terrain drawn as `fs_terrain` does, storing what it works out.
    keep_pipeline: wgpu::RenderPipeline,
    /// The terrain drawn from what was stored, where it matches.
    kept_pipeline: wgpu::RenderPipeline,
    images: Images,
    pub marks: Marks,
    settle: Settle<(GroundKey, u64)>,
}

struct Images {
    /// Cleared to a distance no ground has (0) before each fill.
    a: wgpu::TextureView,
    b: wgpu::TextureView,
    keep: wgpu::BindGroup,
    kept: wgpu::BindGroup,
    bytes: u64,
}

impl KeptGround {
    /// The bind group layouts for `fs_terrain_keep` (the two images to
    /// write, bindings 0 and 1) and `fs_terrain_kept` (to read, 2 and 3).
    pub fn layouts(device: &wgpu::Device) -> (wgpu::BindGroupLayout, wgpu::BindGroupLayout) {
        let write = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: FORMAT,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        };
        let read = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Uint,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let keep = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("keep ground"),
            entries: &[write(0), write(1)],
        });
        let kept = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kept ground"),
            entries: &[read(2), read(3)],
        });
        (keep, kept)
    }

    /// With images for a world drawn `size`.
    pub fn new(
        device: &wgpu::Device,
        (keep_layout, kept_layout): (wgpu::BindGroupLayout, wgpu::BindGroupLayout),
        keep_pipeline: wgpu::RenderPipeline,
        kept_pipeline: wgpu::RenderPipeline,
        size: (u32, u32),
    ) -> KeptGround {
        let images = Images::new(device, &keep_layout, &kept_layout, size);
        KeptGround {
            keep_layout,
            kept_layout,
            keep_pipeline,
            kept_pipeline,
            images,
            marks: Marks::default(),
            settle: Settle::default(),
        }
    }

    /// Make the images for a world drawn `size`. What they held is gone.
    pub fn resize(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        self.images = Images::new(device, &self.keep_layout, &self.kept_layout, size);
        self.settle.forget();
    }

    pub fn bytes(&self) -> u64 {
        self.images.bytes
    }

    /// What a frame whose ground depends on `key` does: stores it once the
    /// key (and the marks and lands in view) held since last frame, reads it
    /// back while they still hold. `drawn` is false for a frame that draws
    /// no ground.
    pub fn plan(&mut self, key: GroundKey, drawn: bool) -> Use {
        self.settle.plan((key, self.marks.version), drawn)
    }

    /// The terrain pipeline and its images for `using`, unless the ground
    /// is worked out as without them.
    pub fn pass(&self, using: Use) -> Option<(&wgpu::RenderPipeline, &wgpu::BindGroup)> {
        match using {
            Use::None => None,
            Use::Keep => Some((&self.keep_pipeline, &self.images.keep)),
            Use::Kept => Some((&self.kept_pipeline, &self.images.kept)),
        }
    }

    /// Before a frame that stores the ground: mark every pixel as holding
    /// none, so a pixel no ground reaches this time keeps nothing from an
    /// older fill.
    pub fn clear(&self, encoder: &mut wgpu::CommandEncoder) {
        let clear = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear kept ground"),
            color_attachments: &[clear(&self.images.a), clear(&self.images.b)],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
}

impl Images {
    fn new(
        device: &wgpu::Device,
        keep_layout: &wgpu::BindGroupLayout,
        kept_layout: &wgpu::BindGroupLayout,
        (width, height): (u32, u32),
    ) -> Images {
        let image = |label| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: FORMAT,
                    // A render target only to be cleared.
                    usage: wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let a = image("kept ground a");
        let b = image("kept ground b");
        let group = |label, layout, first: u32| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: first,
                        resource: wgpu::BindingResource::TextureView(&a),
                    },
                    wgpu::BindGroupEntry {
                        binding: first + 1,
                        resource: wgpu::BindingResource::TextureView(&b),
                    },
                ],
            })
        };
        let keep = group("keep ground", keep_layout, 0);
        let kept = group("kept ground", kept_layout, 2);
        let texel = u64::from(FORMAT.block_copy_size(None).unwrap_or(16));
        Images {
            a,
            b,
            keep,
            kept,
            bytes: 2 * u64::from(width) * u64::from(height) * texel,
        }
    }
}

/// The maps the game marks the ground with (`Renderer::set_scars`,
/// `Renderer::set_land`).
#[derive(Clone, Copy)]
pub(crate) enum MarkMap {
    Scars = 0,
    Lands = 1,
}

/// Which chunks the marks on the ground and the factions' lands changed in,
/// so that a change the camera does not see keeps the kept ground: the game
/// sends the whole map a few times a second while any regiment marches or
/// a land grows, most of it out of view.
#[derive(Default)]
pub(crate) struct Marks {
    /// The last of each map sent, four bytes a tile.
    sent: [Vec<u8>; 2],
    /// Chunks changed since the last frame, row by row, and whether any is.
    dirty: Vec<bool>,
    any: bool,
    /// Counts the frames a change was seen on, which the kept ground is
    /// stored again after.
    pub version: u64,
}

/// How far a change to a tile shows, in tiles: the maps are blurred over a
/// tile and their edges ragged by a quarter.
const MARK_REACH: u32 = 2;

impl Marks {
    /// Note `map` as sent: `values` (`tiles × tiles` tiles of four bytes),
    /// or none (all zero). The renderer's chunks are `chunk_tiles` a side.
    pub fn sent(&mut self, map: MarkMap, values: Option<&[u8]>, tiles: u32, chunk_tiles: u32) {
        let side = tiles.div_ceil(chunk_tiles);
        let row = tiles as usize * 4;
        let old = &mut self.sent[map as usize];
        if old.len() != row * tiles as usize {
            // A new map: everything changed.
            old.clear();
            old.resize(row * tiles as usize, 0);
            self.dirty.clear();
            self.dirty.resize((side * side) as usize, true);
            self.any = true;
        }
        let zeros = vec![0; row];
        for (y, was) in old.chunks_exact_mut(row).enumerate() {
            let now = values.map_or(&zeros[..], |v| &v[y * row..(y + 1) * row]);
            if was == now {
                continue;
            }
            for (x, (a, b)) in was.chunks_exact(4).zip(now.chunks_exact(4)).enumerate() {
                if a == b {
                    continue;
                }
                let (x, y) = (x as u32, y as u32);
                let near = |v: u32| {
                    v.saturating_sub(MARK_REACH) / chunk_tiles
                        ..=((v + MARK_REACH) / chunk_tiles).min(side - 1)
                };
                for cy in near(y) {
                    for cx in near(x) {
                        self.dirty[(cy * side + cx) as usize] = true;
                    }
                }
                self.any = true;
            }
            was.copy_from_slice(now);
        }
    }

    /// Count a new `version` if a chunk the camera sees (`[x, z, levels]`
    /// instances by level) changed since the last frame.
    pub fn look(&mut self, levels: &[Vec<[u32; 3]>], chunks_per_side: u32) {
        if !self.any {
            return;
        }
        let dirty = &self.dirty;
        let seen = levels.iter().flatten().any(|&[cx, cz, _]| {
            dirty
                .get((cz * chunks_per_side + cx) as usize)
                .copied()
                .unwrap_or(false)
        });
        if seen {
            self.version += 1;
        }
        self.dirty.fill(false);
        self.any = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_out_of_view_keep_the_ground() {
        let tiles = 64;
        let mut marks = Marks::default();
        let mut map = vec![0u8; (tiles * tiles * 4) as usize];
        // Whether a frame seeing `chunk` (one of 2 × 2) stores the ground
        // again.
        let saw = |marks: &mut Marks, chunk: [[u32; 3]; 1]| {
            let before = marks.version;
            marks.look(&[chunk.to_vec()], 2);
            marks.version != before
        };
        let near = [[0, 0, 0]];
        // The first map sent covers everything.
        marks.sent(MarkMap::Scars, Some(&map), tiles, 32);
        assert!(saw(&mut marks, near));
        assert!(!saw(&mut marks, near), "nothing new");
        // A mark at tile (50, 50) is in chunk (1, 1) only.
        map[((50 * tiles + 50) * 4) as usize] = 9;
        marks.sent(MarkMap::Scars, Some(&map), tiles, 32);
        assert!(!saw(&mut marks, near));
        marks.sent(MarkMap::Scars, Some(&map), tiles, 32);
        assert!(!saw(&mut marks, [[1, 1, 0]]), "sent again unchanged");
        // One at (40, 33) shows two tiles into chunk (1, 0).
        map[((33 * tiles + 40) * 4) as usize] = 9;
        marks.sent(MarkMap::Scars, Some(&map), tiles, 32);
        assert!(saw(&mut marks, [[1, 0, 0]]), "a tile from the chunk's edge");
        // Lands are their own map; none sent clears it.
        marks.sent(MarkMap::Lands, Some(&map), tiles, 32);
        assert!(saw(&mut marks, [[1, 1, 0]]));
        marks.sent(MarkMap::Lands, None, tiles, 32);
        assert!(saw(&mut marks, [[1, 1, 0]]));
        assert!(!saw(&mut marks, [[1, 1, 0]]));
        // Changes in two corners do not reach the chunks between.
        let tiles = 96;
        let mut marks = Marks::default();
        let mut map = vec![0u8; (tiles * tiles * 4) as usize];
        marks.sent(MarkMap::Scars, Some(&map), tiles, 32);
        marks.look(&[], 3);
        map[((2 * tiles + 2) * 4) as usize] = 9;
        map[((93 * tiles + 93) * 4) as usize] = 9;
        marks.sent(MarkMap::Scars, Some(&map), tiles, 32);
        let before = marks.version;
        marks.look(&[vec![[1, 1, 0]]], 3);
        assert_eq!(marks.version, before, "the middle chunk saw nothing");
    }
}
