//! Reading the rivers (docs/39-water-engine.md, "What the player sees"):
//! every river's name, the gauge of the worst river the viewer can see
//! for the top bar, and the river watch, which tells the viewer when
//! water they can see is rising and when its crest will reach a ford.
//!
//! All of it is read from the simulation's rivers
//! ([`gc_sim::rivers::Rivers::reading`]); none of it changes the match.

use std::time::{Duration, Instant};

use gc_render::Renderer;
use gc_sim::rivers::{Band, Gauge, Rivers};

/// The first halves and second halves river names are made of.
const HEADS: [&str; 24] = [
    "Brack", "Mir", "Cold", "Ash", "Grey", "Wil", "Thorn", "Fen", "Black", "Sedge", "Rush", "Stag",
    "Marrow", "Crow", "Hollow", "Bitter", "Long", "Pike", "Elder", "Rook", "Sorrow", "Iron",
    "Bram", "Wend",
];
const TAILS: [&str; 12] = [
    "water", "row", "burn", "brook", "wash", "run", "mere", "rill", "beck", "flow", "wend",
    "stream",
];

/// The name of river `river` of a map seeded `seed`: "the Brackwater".
/// The rivers of one map never share a name.
pub fn name(seed: u64, river: usize) -> String {
    let (h, t) = name_parts(seed, river);
    format!("the {}{}", HEADS[h], TAILS[t])
}

/// Which head and tail river `river` is named from: a pick from the seed
/// for the first, and each one after takes the next head and a tail
/// stepped by a number with no factor in common with the tails, so no two
/// rivers of a map (there are never 24) share either half.
fn name_parts(seed: u64, river: usize) -> (usize, usize) {
    let mix = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 16;
    let (h0, t0) = ((mix % 24) as usize, ((mix >> 8) % 12) as usize);
    ((h0 + river) % HEADS.len(), (t0 + river * 5) % TAILS.len())
}

/// The gauge, in a few words: "in flood".
pub fn gauge_word(gauge: Gauge) -> &'static str {
    match gauge {
        Gauge::Low => "low",
        Gauge::Normal => "normal",
        Gauge::Up => "up",
        Gauge::InFlood => "in flood",
        Gauge::Bursting => "bursting its banks",
    }
}

/// What the viewer has been told of the rivers, and how they stood a
/// second ago.
#[derive(Default)]
pub struct RiverWatch {
    /// The map's seed and stretches the names below were made for.
    made_for: Option<(u64, usize)>,
    names: Vec<String>,
    /// Each stretch's rise when last looked at, and whether it was rising
    /// (+1), falling (-1) or holding (0) then.
    last: Vec<u16>,
    trend: Vec<i8>,
    /// Per river: whether the crest now coming down it has been told.
    told: Vec<bool>,
}

impl RiverWatch {
    /// The name of river `river` (empty before the first look).
    pub fn name(&self, river: u16) -> &str {
        self.names
            .get(usize::from(river))
            .map_or("", String::as_str)
    }

    /// Look at the rivers once a second: `sees` says whether the viewer
    /// sees a tile, and `lift` is the map-wide flood's rise. Returns what the river watch tells the viewer,
    /// if anything: "The Brackwater is rising upriver: its crest reaches a
    /// ford in about 40 s".
    pub fn look(
        &mut self,
        rivers: &Rivers,
        seed: u64,
        lift: u16,
        sees: impl Fn((i32, i32)) -> bool,
    ) -> Option<String> {
        let count = rivers.stretches();
        if self.made_for != Some((seed, count)) {
            self.made_for = Some((seed, count));
            self.names = (0..rivers.river_count()).map(|r| name(seed, r)).collect();
            self.last = vec![0; count];
            self.trend = vec![0; count];
            self.told = vec![false; rivers.river_count()];
        }
        for s in 0..count {
            let rise = rivers.reading(s, lift).rise;
            self.trend[s] = match rise.cmp(&self.last[s]) {
                core::cmp::Ordering::Greater => 1,
                core::cmp::Ordering::Less => -1,
                core::cmp::Ordering::Equal => self.trend[s],
            };
            self.last[s] = rise;
        }
        // A crest no stretch still waits for has gone by: the next one is
        // news again.
        let mut coming = vec![false; self.told.len()];
        for s in 0..count {
            let r = rivers.reading(s, lift);
            if r.coming
                .is_some_and(|c| c.crest >= gc_sim::flood::FLOOD_RISE)
            {
                coming[usize::from(r.river)] = true;
            }
        }
        for (told, &c) in self.told.iter_mut().zip(&coming) {
            *told &= c;
        }
        // Water the viewer sees rising in flood, with a crest still to come
        // down on a ford below it.
        let mut news = None;
        for s in 0..count {
            let here = rivers.reading(s, 0);
            let river = usize::from(here.river);
            if self.told[river]
                || Gauge::of(here.rise, 0) < Gauge::InFlood
                || self.trend[s] <= 0
                || !sees(here.at)
            {
                continue;
            }
            let mut below = rivers.below(s);
            let mut soonest = None;
            while let Some(b) = below {
                if rivers.has_ford(b) {
                    let c = rivers.reading(b, 0).coming;
                    if let Some(c) = c.filter(|c| c.crest >= gc_sim::flood::FLOOD_RISE) {
                        soonest = Some(c);
                        break;
                    }
                }
                below = rivers.below(b);
            }
            let Some(c) = soonest else {
                continue;
            };
            self.told[river] = true;
            let big = match Band::of(c.crest) {
                Band::Catastrophic => "a catastrophic flood, ",
                Band::Major => "a major flood, ",
                _ => "",
            };
            news = Some(format!(
                "{} is rising upriver: {big}its crest reaches a ford in about {} s",
                capitalised(&self.names[river]),
                c.seconds
            ));
            break;
        }
        news
    }

    /// The worst river the viewer sees, for the top bar, if any is out of
    /// the ordinary: "the Brackwater in flood, rising". `lift` is the
    /// map-wide flood's rise and `low` low water's drop.
    pub fn chip(
        &self,
        rivers: &Rivers,
        lift: u16,
        low: u16,
        sees: impl Fn((i32, i32)) -> bool,
    ) -> Option<String> {
        let (gauge, s, river) = (0..rivers.stretches().min(self.last.len()))
            .map(|s| (s, rivers.reading(s, lift)))
            .filter(|(_, r)| sees(r.at))
            .map(|(s, r)| (Gauge::of(r.rise, low), s, r.river))
            .max_by_key(|&(g, s, _)| (g, self.last[s], core::cmp::Reverse(s)))?;
        // (Low water is told map-wide, with what it opens.)
        if gauge <= Gauge::Normal {
            return None;
        }
        let way = match self.trend[s] {
            1 => ", rising",
            -1 => ", falling",
            _ => "",
        };
        Some(format!("{} {}{way}", self.name(river), gauge_word(gauge)))
    }
}

/// How often the drawn water is brought up to date while a river is up.
const DRAW_EVERY: Duration = Duration::from_millis(100);

/// The rivers' water drawn as high as it stands: a byte a tile for the
/// renderer ([`Renderer::set_river_water`]), the rise over the map's water
/// line in half height units, on the river's tiles and the bank beside
/// them (so the water meets its banks at its own height).
#[derive(Default)]
pub struct RiverPicture {
    /// The map's side and stretches the tiles below were found for.
    made_for: Option<(u32, usize)>,
    /// The river tiles, as `(x, y)`.
    tiles: Vec<(i32, i32)>,
    grid: Vec<u8>,
    /// Whether the renderer was last given any rise, and when.
    drawn: bool,
    last: Option<Instant>,
}

impl RiverPicture {
    /// Draw the rivers' water as `world`'s rivers raise it, on a map
    /// `side` tiles across. Call it every frame; it redraws only while a
    /// river is up, a few times a second.
    pub fn update(
        &mut self,
        now: Instant,
        renderer: &mut Renderer,
        world: &gc_sim::World,
        side: u32,
    ) {
        let (rivers, terrain) = (world.flood().rivers(), world.terrain());
        if self.made_for != Some((side, rivers.stretches())) {
            self.made_for = Some((side, rivers.stretches()));
            let n = side as i32;
            self.tiles = (0..n * n)
                .map(|i| (i % n, i / n))
                .filter(|&(x, y)| rivers.stretch_at(x, y).is_some())
                .collect();
            self.grid = vec![0; (side * side) as usize];
            self.last = None;
            if self.drawn {
                renderer.set_river_water(None);
                self.drawn = false;
            }
        }
        if !terrain.river_up() {
            if self.drawn {
                renderer.set_river_water(None);
                self.drawn = false;
            }
            return;
        }
        if self
            .last
            .is_some_and(|t| now.duration_since(t) < DRAW_EVERY)
        {
            return;
        }
        self.last = Some(now);
        let n = side as i32;
        self.grid.fill(0);
        for &(x, y) in &self.tiles {
            let rise = terrain.river_rise(gc_fixed::FxVec2::from_ints(x, y));
            let byte = (rise / 2).min(255) as u8;
            if byte == 0 {
                continue;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (bx, by) = (x + dx, y + dy);
                    if (0..n).contains(&bx) && (0..n).contains(&by) {
                        let cell = &mut self.grid[(by * n + bx) as usize];
                        *cell = (*cell).max(byte);
                    }
                }
            }
        }
        renderer.set_river_water(Some(&self.grid));
        self.drawn = true;
    }
}

/// `s` with its first letter in capitals: "The Brackwater".
pub fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rivers_of_a_map_have_names_of_their_own() {
        for seed in [0, 1, 7, 0x570a, u64::MAX] {
            let names: Vec<String> = (0..12).map(|r| name(seed, r)).collect();
            for (i, a) in names.iter().enumerate() {
                assert!(a.starts_with("the "), "{a}");
                assert!(names[..i].iter().all(|b| b != a), "{names:?}");
            }
        }
        assert_eq!(name(5, 2), name(5, 2));
    }

    #[test]
    fn a_name_starts_with_a_capital_at_the_start_of_a_line() {
        assert_eq!(capitalised("the Brackwater"), "The Brackwater");
        assert_eq!(capitalised(""), "");
    }
}
