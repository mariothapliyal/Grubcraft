//! Reading the rivers (docs/39-water-engine.md, "What the player sees"):
//! every river's name, the gauge of the worst river the viewer can see
//! for the top bar, and the river watch, which tells the viewer when
//! water they can see is rising and when its crest will reach a ford.
//!
//! All of it is read from the simulation's rivers
//! ([`gc_sim::rivers::Rivers::reading`]); none of it changes the match.

use std::cmp::Ordering;
use std::time::{Duration, Instant};

use gc_render::Renderer;
use gc_sim::flood::FLOOD_RISE;
use gc_sim::rivers::{Band, Gauge, Reading, Rivers};

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

/// The name of river `river` of rivers laid out as `seed` says (see
/// `layout_seed`): "the Brackwater".
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
    let (h0, t0) = (
        (mix % HEADS.len() as u64) as usize,
        ((mix >> 8) % TAILS.len() as u64) as usize,
    );
    ((h0 + river) % HEADS.len(), (t0 + river * 5) % TAILS.len())
}

/// The gauge, in a few words: "in flood".
pub fn gauge_word(gauge: Gauge) -> &'static str {
    match gauge {
        Gauge::Low | Gauge::Normal => "",
        Gauge::Up => "up",
        Gauge::InFlood => "in flood",
        Gauge::Bursting => "bursting its banks",
    }
}

/// A number for the shape of a map's rivers, the same in every match on
/// that map: what the rivers' names are drawn from.
fn layout_seed(rivers: &Rivers) -> u64 {
    (0..rivers.stretches()).fold(rivers.stretches() as u64, |h, s| {
        let (x, y) = rivers.reading(s, 0).at;
        (h ^ ((x as u64) << 20) ^ (y as u64)).wrapping_mul(0x9E37_79B9_7F4A_7C15)
    })
}

/// What the viewer has been told of the rivers, and how they stood a
/// second ago.
#[derive(Default)]
pub struct RiverWatch {
    /// The rivers' layout the names below were made for.
    made_for: Option<u64>,
    names: Vec<String>,
    /// Each stretch's reading when last looked at, and whether its water
    /// was rising (`Greater`), falling (`Less`) or holding then.
    last: Vec<(Reading, Ordering)>,
    /// Per river: whether the crest now coming down it has been told, and
    /// whether one is still coming (room for each look).
    told: Vec<bool>,
    coming: Vec<bool>,
    /// The top bar's words, as of the last look.
    chip: Option<String>,
}

impl RiverWatch {
    /// The name of river `river` (empty before the first look).
    pub fn name(&self, river: u16) -> &str {
        self.names
            .get(usize::from(river))
            .map_or("", String::as_str)
    }

    /// Forget what was seen and told, for a new match (the names stay
    /// with the map).
    pub fn reset(&mut self) {
        self.last.clear();
        self.told.clear();
        self.chip = None;
    }

    /// Look at the rivers once a second (`sees` says whether the viewer
    /// sees a tile). Returns what the river watch tells the viewer, if
    /// anything: "The Brackwater is rising upriver: its crest reaches a
    /// ford in about 40 s". The map-wide flood, which shuts every ford at
    /// once, is told on its own, so it is not read here.
    pub fn look(&mut self, rivers: &Rivers, sees: impl Fn((i32, i32)) -> bool) -> Option<String> {
        let count = rivers.stretches();
        if self.last.len() != count {
            let seed = layout_seed(rivers);
            if self.made_for != Some(seed) {
                self.made_for = Some(seed);
                self.names = (0..rivers.river_count()).map(|r| name(seed, r)).collect();
            }
            self.last = (0..count)
                .map(|s| (rivers.reading(s, 0), Ordering::Equal))
                .collect();
            self.told = vec![false; rivers.river_count()];
        }
        self.coming.clear();
        self.coming.resize(self.told.len(), false);
        for (s, (last, trend)) in self.last.iter_mut().enumerate() {
            let now = rivers.reading(s, 0);
            *trend = now.rise.cmp(&last.rise);
            *last = now;
            if now.coming.is_some_and(|c| c.crest >= FLOOD_RISE) {
                self.coming[usize::from(now.river)] = true;
            }
        }
        self.chip = self.chip_now(&sees);
        // A crest no stretch still waits for has gone by: the next one is
        // news again.
        for (told, &c) in self.told.iter_mut().zip(&self.coming) {
            *told &= c;
        }
        // Water the viewer sees rising in flood, with a crest still to come
        // down on a ford below it.
        for (s, &(here, trend)) in self.last.iter().enumerate() {
            let river = usize::from(here.river);
            if self.told[river]
                || here.rise < FLOOD_RISE
                || trend != Ordering::Greater
                || !sees(here.at)
            {
                continue;
            }
            let mut below = rivers.below(s);
            let mut soonest = None;
            while let Some(b) = below {
                let c = self.last[b].0.coming;
                if let Some(c) = c.filter(|c| rivers.has_ford(b) && c.crest >= FLOOD_RISE) {
                    soonest = Some(c);
                    break;
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
            return Some(format!(
                "{} is rising upriver: {big}its crest reaches a ford in about {} s",
                capitalised(&self.names[river]),
                c.seconds
            ));
        }
        None
    }

    /// The worst river the viewer saw at the last look, for the top bar,
    /// if any is up or higher: "the Brackwater in flood, rising". (Low
    /// water is told map-wide, with what it opens.)
    pub fn chip(&self) -> Option<&str> {
        self.chip.as_deref()
    }

    fn chip_now(&self, sees: impl Fn((i32, i32)) -> bool) -> Option<String> {
        let (gauge, trend, river) = (self.last.iter().enumerate())
            .filter(|(_, (r, _))| sees(r.at))
            .map(|(s, &(r, trend))| {
                (
                    Gauge::of(r.rise, 0),
                    r.rise,
                    core::cmp::Reverse(s),
                    trend,
                    r.river,
                )
            })
            .max_by_key(|&(g, rise, s, _, _)| (g, rise, s))
            .map(|(g, _, _, trend, river)| (g, trend, river))?;
        if gauge <= Gauge::Normal {
            return None;
        }
        let way = match trend {
            Ordering::Greater => ", rising",
            Ordering::Less => ", falling",
            Ordering::Equal => "",
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
    /// What the renderer was last given, and room for the next.
    grid: Vec<u8>,
    next: Vec<u8>,
    /// When it was last brought up to date.
    last: Option<Instant>,
}

impl RiverPicture {
    /// Forget the map, for a new one (whose texture starts at the line).
    pub fn reset(&mut self) {
        *self = RiverPicture::default();
    }

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
            self.next = self.grid.clone();
            self.last = None;
            renderer.set_river_water(None);
        }
        if !terrain.river_up() {
            // (Nothing to do when the water is already at its line.)
            if self.grid.iter().any(|&b| b > 0) {
                self.grid.fill(0);
                renderer.set_river_water(None);
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
        self.next.fill(0);
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
                        let cell = &mut self.next[(by * n + bx) as usize];
                        *cell = (*cell).max(byte);
                    }
                }
            }
        }
        if self.next != self.grid {
            std::mem::swap(&mut self.next, &mut self.grid);
            renderer.set_river_water(Some(&self.grid));
        }
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

    /// A 128-tile map with one river running east off it along rows
    /// 60 to 67, a ford at columns 100 to 103, and a storm ten seconds in.
    fn river_world(seed: u64) -> gc_sim::World {
        use gc_sim::sky::{Spell, Weather, WeatherKind};
        const MAP: i32 = 128;
        const WATER: u16 = 256;
        static STORM: [Spell; 3] = [
            Spell {
                at: 0,
                weather: Weather::FAIR,
            },
            Spell {
                at: 10 * gc_sim::TICKS_PER_SECOND,
                weather: Weather {
                    kind: WeatherKind::Rain,
                    strength: 255,
                    cloud: 255,
                    wind: (0, 0),
                },
            },
            Spell {
                at: 20 * gc_sim::TICKS_PER_SECOND,
                weather: Weather::FAIR,
            },
        ];
        let river = |y: i32| (60..68).contains(&y);
        let ford = |x: i32| (100..104).contains(&x);
        let heights: Vec<u16> = (0..MAP * MAP)
            .map(|i| match (i % MAP, i / MAP) {
                (x, y) if river(y) && ford(x) => WATER - 40,
                (_, y) if river(y) => 20,
                _ => 600,
            })
            .collect();
        let side = MAP * gc_sim::nav::CELLS_PER_TILE;
        let nav: Vec<u8> = (0..side * side)
            .map(|c| {
                let (x, y) = (
                    c % side / gc_sim::nav::CELLS_PER_TILE,
                    c / side / gc_sim::nav::CELLS_PER_TILE,
                );
                match () {
                    _ if river(y) && ford(x) => 3,
                    _ if river(y) => gc_sim::nav::BLOCKED,
                    _ => 1,
                }
            })
            .collect();
        let flow: Vec<u8> = (0..MAP * MAP)
            .map(|i| {
                if river(i / MAP) {
                    gc_sim::Flow::byte(gc_fixed::Angle(0), 2)
                } else {
                    0
                }
            })
            .collect();
        gc_sim::World::new(&gc_sim::MatchSetup {
            seed,
            capacities: gc_sim::Capacities::default(),
            units: &gc_sim::scenario::DRILL_UNITS,
            map_tiles: MAP as u32,
            nav: &nav,
            teams: &[],
            ground: gc_sim::Ground {
                heights: &heights,
                water_level: WATER,
                cover: &[],
                growth: &[],
                footing: &[],
                flow: &flow,
            },
            champions: false,
            sky: gc_sim::Sky {
                spells: &STORM,
                ..gc_sim::Sky::STILL
            },
            generals: false,
        })
    }

    #[test]
    fn the_river_watch_tells_a_crest_coming_once_and_the_chip_reads_the_river() {
        // A seed whose storm sends a front that shuts the ford.
        let floods =
            |w: &gc_sim::World| w.flood().rivers().fronts().any(|f| f.band() >= Band::Flood);
        let seed = (0..4000)
            .find(|&seed| {
                let mut w = river_world(seed);
                (0..11 * gc_sim::TICKS_PER_SECOND).for_each(|_| {
                    w.tick(&[]);
                });
                floods(&w)
            })
            .expect("a flood");
        let mut w = river_world(seed);
        let mut watch = RiverWatch::default();
        let (mut told, mut chips) = (Vec::new(), Vec::new());
        while w.tick_count() < 90 * gc_sim::TICKS_PER_SECOND {
            w.tick(&[]);
            if !w.tick_count().is_multiple_of(gc_sim::TICKS_PER_SECOND) {
                continue;
            }
            let rivers = w.flood().rivers();
            told.extend(watch.look(rivers, |_| true));
            chips.extend(watch.chip().map(str::to_owned));
        }
        let river = watch.name(0).to_owned();
        let name = capitalised(&river);
        let [news] = told.as_slice() else {
            panic!("{told:?}");
        };
        assert!(
            news.starts_with(&format!("{name} is rising upriver: ")),
            "{news}"
        );
        assert!(
            news.contains("its crest reaches a ford in about "),
            "{news}"
        );
        assert!(
            chips.iter().any(|c| c.ends_with("in flood, rising")),
            "{chips:?}"
        );
        assert!(chips.iter().any(|c| c.ends_with(", falling")), "{chips:?}");
        assert!(chips.iter().all(|c| c.starts_with(&river)), "{chips:?}");
        // Nobody sees the river: nothing is told.
        let mut w = river_world(seed);
        let mut blind = RiverWatch::default();
        while w.tick_count() < 90 * gc_sim::TICKS_PER_SECOND {
            w.tick(&[]);
            if w.tick_count().is_multiple_of(gc_sim::TICKS_PER_SECOND) {
                let rivers = w.flood().rivers();
                assert_eq!(blind.look(rivers, |_| false), None);
                assert_eq!(blind.chip(), None);
            }
        }
    }

    #[test]
    fn a_name_starts_with_a_capital_at_the_start_of_a_line() {
        assert_eq!(capitalised("the Brackwater"), "The Brackwater");
        assert_eq!(capitalised(""), "");
    }
}
