//! Room round the starts (docs/27, "Room round each start"): every start
//! gets the same flattened clearing, but past it the land differs. One
//! start can be ringed by old woods, another by open meadow, so one
//! player must clear for an hour what another builds on at once. This
//! measures the open ground near each start in play and evens it out:
//! starts with too little get the woods and scrub nearest them cleared
//! (felled woods become glades, cut scrub grassland), starts with too
//! much get scrub grown on the far edge of their open ground. Thicket is
//! never touched (it is a wall the walks count), and nor are cliffs and
//! water, which no clearing moves.

use super::field::{Grid, ONE, fbm};
use super::{Habitat, Raster, scaled};
use crate::def::Pos;

/// How far round a start its room is counted, in tiles on a 512-tile map:
/// a Conquest base and its natural expansion.
const REACH: i32 = 48;

/// How far apart the starts' open room may be, in thousandths of their
/// middle, once evened.
pub(crate) const ROOM_LIMIT: u32 = 100;

/// How far the starts are evened toward their middle: within this many
/// thousandths of it, so a little is left to the land.
const TOWARD: u32 = 40;

/// What a tile near a start holds, for its room.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Room {
    /// Open, standable ground: room to build at once.
    Open,
    /// Scrub or woods a worker can clear.
    Cuttable,
    /// Thicket, cliffs, steep ground or water.
    Shut,
}

/// What tile `i` holds, on `standable` ground (as `super::standable`).
pub(crate) fn room_of(r: &Raster, standable: &[u8], i: usize) -> Room {
    if standable[i] == 0 || r.thicket[i] {
        Room::Shut
    } else if r.cover[i] == gc_sim::Cover::Wood as u8 || r.habitat[i] == Habitat::ScrubEdge as u8 {
        Room::Cuttable
    } else {
        Room::Open
    }
}

/// The tiles within [`REACH`] (scaled) of `at`, nearest first by distance
/// squared.
fn near(n: usize, at: Pos) -> impl Iterator<Item = (usize, i32)> {
    let reach = scaled(n, REACH);
    let n = n as i32;
    ((at.1 - reach).max(0)..(at.1 + reach + 1).min(n)).flat_map(move |y| {
        ((at.0 - reach).max(0)..(at.0 + reach + 1).min(n)).filter_map(move |x| {
            let d2 = (x - at.0).pow(2) + (y - at.1).pow(2);
            (d2 <= reach * reach).then_some(((y * n + x) as usize, d2))
        })
    })
}

/// Each start's open tiles and cuttable tiles within [`REACH`].
pub(crate) fn count(r: &Raster, standable: &[u8], starts: &[Pos]) -> Vec<(u32, u32)> {
    let n = r.height.n;
    starts
        .iter()
        .map(|&s| {
            near(n, s).fold((0, 0), |(open, cut), (i, _)| {
                match room_of(r, standable, i) {
                    Room::Open => (open + 1, cut),
                    Room::Cuttable => (open, cut + 1),
                    Room::Shut => (open, cut),
                }
            })
        })
        .collect()
}

/// How unequal the starts' open room is: (most - least) / middle, in
/// thousandths.
pub(crate) fn spread(open: &[u32]) -> u32 {
    let mut v = open.to_vec();
    v.sort_unstable();
    let (lo, hi, mid) = (v[0], v[v.len() - 1], v[v.len() / 2].max(1));
    (u64::from(hi - lo) * 1000 / u64::from(mid)) as u32
}

/// Even out the open room round the starts numbered `in_play`, toward the
/// middle of them (see the module notes). On land that is its own turned
/// copy, each change is made to a tile and its turned copy alike, and
/// only one start of each twin pair is worked on (its twin follows).
pub(crate) fn even(r: &mut Raster, standable: &[u8], in_play: &[usize], seed: u32) {
    let n = r.height.n;
    let last = n * n - 1;
    let starts: Vec<Pos> = in_play.iter().map(|&k| r.starts[k]).collect();
    // A ragged edge: the order tiles are cleared or grown in wanders by
    // up to a quarter of the reach.
    let wobble: Grid = fbm(n, seed, scaled(n, 12).max(4) * ONE, 3);
    let ragged = |i: usize, d2: i32| {
        let d = gc_fixed::isqrt_u64(d2 as u64) as i32;
        d * 4 + (wobble.v[i] * scaled(n, REACH) / ONE)
    };
    // What each start could clear (woods and scrub, nearest first) and
    // where scrub could grow (meadow past its clearing, furthest first).
    let clearing = scaled(n, CLEARING);
    let (mut cut, mut grow): (Vec<Vec<usize>>, Vec<Vec<usize>>) = starts
        .iter()
        .map(|&s| {
            let mut cut: Vec<(i32, usize)> = Vec::new();
            let mut grow: Vec<(i32, usize)> = Vec::new();
            for (i, d2) in near(n, s) {
                match room_of(r, standable, i) {
                    Room::Cuttable => cut.push((ragged(i, d2), i)),
                    Room::Open if d2 > clearing * clearing && MEADOW.contains(&r.habitat[i]) => {
                        grow.push((-ragged(i, d2), i));
                    }
                    _ => {}
                }
            }
            cut.sort_unstable();
            grow.sort_unstable();
            (
                cut.into_iter().map(|(_, i)| i).collect(),
                grow.into_iter().map(|(_, i)| i).collect(),
            )
        })
        .unzip();
    let open: Vec<u32> = count(r, standable, &starts).iter().map(|c| c.0).collect();
    // The middle of the starts' room, held to what every start can reach:
    // no higher than the least a start could clear up to, no lower than
    // the most a start could grow down to (the middle of the two if they
    // cross).
    let mut sorted = open.clone();
    sorted.sort_unstable();
    let reach_up = (0..starts.len())
        .map(|k| open[k] + cut[k].len() as u32)
        .min()
        .unwrap_or(0);
    let reach_down = (0..starts.len())
        .map(|k| open[k].saturating_sub(grow[k].len() as u32))
        .max()
        .unwrap_or(0);
    let middle = if reach_down <= reach_up {
        sorted[sorted.len() / 2].clamp(reach_down, reach_up)
    } else {
        (reach_down + reach_up) / 2
    };
    let (low, high) = (
        (u64::from(middle) * u64::from(1000 - TOWARD) / 1000) as u32,
        (u64::from(middle) * u64::from(1000 + TOWARD) / 1000) as u32,
    );
    let twin = |s: Pos| (n as i32 - 1 - s.0, n as i32 - 1 - s.1);
    let set = |r: &mut Raster, i: usize, cover: gc_sim::Cover, habitat: Habitat| {
        for t in [i, last - i]
            .into_iter()
            .take(if r.mirrored { 2 } else { 1 })
        {
            r.cover[t] = cover as u8;
            r.habitat[t] = habitat as u8;
            r.land[t] = habitat.land() as u8;
        }
    };
    for (k, &s) in starts.iter().enumerate() {
        // On mirrored land, the twin of a start worked on earlier follows it.
        if r.mirrored && starts[..k].contains(&twin(s)) {
            continue;
        }
        if open[k] < low {
            for i in std::mem::take(&mut cut[k])
                .into_iter()
                .take((low - open[k]) as usize)
            {
                // (Another start's change may have reached this tile.)
                if room_of(r, standable, i) != Room::Cuttable {
                    continue;
                }
                let wooded = r.cover[i] == gc_sim::Cover::Wood as u8
                    && r.habitat[i] != Habitat::ScrubEdge as u8;
                let cleared = if wooded {
                    Habitat::Glade
                } else {
                    Habitat::Grassland
                };
                set(r, i, gc_sim::Cover::Open, cleared);
            }
        } else if open[k] > high {
            for i in std::mem::take(&mut grow[k])
                .into_iter()
                .take((open[k] - high) as usize)
            {
                if room_of(r, standable, i) == Room::Open {
                    set(r, i, gc_sim::Cover::Grass, Habitat::ScrubEdge);
                }
            }
        }
    }
}

/// How far round a start scrub never grows, in tiles on a 512-tile map:
/// its clearing.
const CLEARING: i32 = 24;

/// The meadow scrub can grow on.
const MEADOW: [u8; 2] = [Habitat::Grassland as u8, Habitat::FlowerMeadow as u8];

#[cfg(test)]
mod tests {
    use super::super::{Fairness, Grown, grow, standable};
    use super::*;
    use crate::wilds::Wildness;

    /// The open room of the starts in play on grown land.
    fn rooms(tiles: u32, what: Grown) -> Vec<(u32, u32)> {
        let r = grow(tiles, what);
        let step = standable(&r);
        let four = what.heart && tiles < crate::def::BIG_MAP_TILES as u32;
        let starts = &r.starts[..if four { 4 } else { r.starts.len() }];
        count(&r, &step, starts)
    }

    /// On Measured land, mirrored land and a Conquest map, of every
    /// wildness, the open ground round the starts in play comes out
    /// within [`ROOM_LIMIT`] of each other.
    #[test]
    fn every_start_has_the_same_room_to_build() {
        for w in Wildness::ALL {
            for seed in 1..=2 {
                for (tiles, fairness, heart) in [
                    (256, Fairness::Measured, false),
                    (320, Fairness::Mirrored, false),
                    (320, Fairness::Mirrored, true),
                ] {
                    let what = Grown {
                        seed,
                        wildness: w,
                        fairness,
                        heart,
                        ring: 0,
                    };
                    let room = rooms(tiles, what);
                    let open: Vec<u32> = room.iter().map(|c| c.0).collect();
                    assert!(
                        spread(&open) <= ROOM_LIMIT,
                        "{w:?} {tiles} {fairness:?} heart {heart} seed {seed}: {room:?}"
                    );
                }
            }
        }
    }

    /// The room before and after, by hand:
    /// `cargo test --release -p gc_map room::tests::probe -- --ignored --nocapture`.
    #[test]
    #[ignore = "a probe"]
    fn probe() {
        for w in Wildness::ALL {
            for seed in 1..=3 {
                for (tiles, fairness, heart) in [
                    (512, Fairness::Measured, false),
                    (448, Fairness::Mirrored, true),
                    (640, Fairness::Mirrored, true),
                ] {
                    let what = Grown {
                        seed,
                        wildness: w,
                        fairness,
                        heart,
                        ring: 0,
                    };
                    let room = rooms(tiles, what);
                    let open: Vec<u32> = room.iter().map(|c| c.0).collect();
                    println!(
                        "{tiles} {w:?} {fairness:?} heart {heart} seed {seed}: spread {} {room:?}",
                        spread(&open)
                    );
                }
            }
        }
    }
}
