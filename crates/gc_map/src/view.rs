//! The float views of a map, for drawing and the computer players, which
//! only the host runs. The map itself is made without floats, so every
//! machine makes the same one; nothing here feeds back into it.

#![allow(clippy::float_arithmetic)]

use glam::Vec2;

use crate::map::{MAX_HEIGHT, Map, SAMPLES_PER_TILE};

impl Map {
    /// The highest ground, in tiles, for drawing.
    pub fn max_height(&self) -> f32 {
        MAX_HEIGHT as f32
    }

    /// The water line, in tiles, for drawing and the computer players.
    pub fn water_level(&self) -> f32 {
        f32::from(self.sample_water) * self.max_height() / f32::from(u16::MAX)
    }

    /// The water's level on tile `i` (row by row), in tiles: its own on
    /// highland water and its shores (docs/27 stage 5), else the map's line.
    pub fn tile_water_level(&self, i: usize) -> f32 {
        match self.levels.get(i) {
            Some(&l) if l != 0 => f32::from(l) / gc_sim::terrain::HEIGHT_UNITS as f32,
            _ => self.water_level(),
        }
    }

    /// [`Map::tile_water_level`] for every tile, or empty when every tile's
    /// is the map's line.
    pub fn tile_water_levels(&self) -> Vec<f32> {
        if self.levels.is_empty() {
            return Vec::new();
        }
        (0..self.levels.len())
            .map(|i| self.tile_water_level(i))
            .collect()
    }

    /// The water's level at a map position in tiles: eased between tile
    /// centres as the renderer draws it, so it is flat over each body of
    /// water and steps down at a fall. The map's line on a map without
    /// highland water.
    pub fn water_at(&self, x: f32, z: f32) -> f32 {
        if self.levels.is_empty() {
            return self.water_level();
        }
        let n = self.tiles as i32;
        let (sx, sz) = (x - 0.5, z - 0.5);
        let (ix, iz) = (sx.floor() as i32, sz.floor() as i32);
        let (fx, fz) = (sx - ix as f32, sz - iz as f32);
        let at = |x: i32, z: i32| {
            self.tile_water_level((z.clamp(0, n - 1) * n + x.clamp(0, n - 1)) as usize)
        };
        let top = at(ix, iz) + (at(ix + 1, iz) - at(ix, iz)) * fx;
        let bottom = at(ix, iz + 1) + (at(ix + 1, iz + 1) - at(ix, iz + 1)) * fx;
        top + (bottom - top) * fz
    }

    /// The ground or the water over it, whichever is higher, at a map
    /// position in tiles: what things standing on the land or on the water
    /// stand on.
    pub fn surface_at(&self, x: f32, z: f32) -> f32 {
        self.height_at(x, z).max(self.water_at(x, z))
    }

    fn sample(&self, x: i32, z: i32) -> f32 {
        let side = self.side() as i32;
        let max = side - 1;
        let i = (z.clamp(0, max) * side + x.clamp(0, max)) as usize;
        f32::from(self.samples[i]) * (self.max_height() / f32::from(u16::MAX))
    }

    /// Bilinear height in tiles at a map position in tiles (matches the shader).
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let sx = x * SAMPLES_PER_TILE as f32;
        let sz = z * SAMPLES_PER_TILE as f32;
        let (ix, iz) = (sx.floor() as i32, sz.floor() as i32);
        let (fx, fz) = (sx - ix as f32, sz - iz as f32);
        let h00 = self.sample(ix, iz);
        let h10 = self.sample(ix + 1, iz);
        let h01 = self.sample(ix, iz + 1);
        let h11 = self.sample(ix + 1, iz + 1);
        let top = h00 + (h10 - h00) * fx;
        let bottom = h01 + (h11 - h01) * fx;
        top + (bottom - top) * fz
    }

    /// The height things stand on where they are drawn at a map position:
    /// the ground, or just under the surface of deeper water (matches the
    /// shaders).
    pub fn drawn_height(&self, x: f32, z: f32) -> f32 {
        self.height_at(x, z).max(self.water_at(x, z) - 0.05)
    }

    /// Each player's start as a map position in tiles (see [`Map::starts`]).
    pub fn start_positions(&self) -> Vec<Vec2> {
        self.starts()
            .into_iter()
            .map(|(x, y)| Vec2::new(x as f32, y as f32))
            .collect()
    }
}
