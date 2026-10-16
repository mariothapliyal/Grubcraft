//! The map as the checks see it: `gc_map`'s map, with each tile's ground
//! worked out once from the walkable-ground grid the simulation uses, so
//! the checker and the game agree on every cliff and ford.

pub use gc_map::{Ground, Placed};
use gc_map::def::{MapDef, Pos};

pub struct Map {
    pub map: gc_map::Map,
    pub tiles: i32,
    /// Each tile's ground ([`gc_map::Map::tile_ground`]); the outermost
    /// ring of tiles counts as blocked, so nothing is placed on the edge.
    pub ground: Vec<Ground>,
}

impl std::ops::Deref for Map {
    type Target = gc_map::Map;
    fn deref(&self) -> &gc_map::Map {
        &self.map
    }
}

impl Map {
    pub fn build(def: &MapDef) -> Map {
        let map = gc_map::Map::from_def(def);
        let tiles = map.tiles as i32;
        let mut ground = map.tile_ground(&map.nav_costs());
        for y in 0..tiles {
            for x in 0..tiles {
                if x == 0 || y == 0 || x == tiles - 1 || y == tiles - 1 {
                    ground[(y * tiles + x) as usize] = Ground::Blocked;
                }
            }
        }
        Map { map, tiles, ground }
    }

    pub fn idx(&self, p: Pos) -> usize {
        (p.1 * self.tiles + p.0) as usize
    }

    pub fn in_bounds(&self, p: Pos) -> bool {
        (0..self.tiles).contains(&p.0) && (0..self.tiles).contains(&p.1)
    }

    pub fn ground_at(&self, p: Pos) -> Ground {
        if self.in_bounds(p) {
            self.ground[self.idx(p)]
        } else {
            Ground::Blocked
        }
    }

    /// Tiles a regiment's centre can use: walkable, and not touching a
    /// blocked tile (the sim keeps blocks about a tile off cliffs).
    pub fn walkable(&self) -> Vec<bool> {
        let mut out = vec![false; self.ground.len()];
        for y in 0..self.tiles {
            for x in 0..self.tiles {
                let clear = self.ground_at((x, y)) != Ground::Blocked
                    && (-1..=1).all(|dy| {
                        (-1..=1).all(|dx| self.ground_at((x + dx, y + dy)) != Ground::Blocked)
                    });
                out[self.idx((x, y))] = clear;
            }
        }
        out
    }

    pub fn start_of(&self, player: u8) -> Option<Pos> {
        self.objects_of(gc_map::Kind::Start)
            .find(|o| o.owner == Some(player))
            .map(|o| o.at)
    }

    /// The fingerprint the game compares between machines, in hex.
    pub fn hash(&self) -> String {
        format!("{:016x}", self.map.hash())
    }
}
