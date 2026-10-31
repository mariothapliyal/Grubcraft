//! The clearance check (docs/40-real-play-checks.md): every passage troops
//! walk through or over, a gate's doorway or a bridge's deck, is measured
//! twice, as the simulation walks it (its footprint) and as the player sees
//! it (its model), and held against the widest unit that walks and a
//! column of line troops two abreast. Cheap: it reads the baked data and
//! models, so it runs with the ordinary tests on every pull request.
//!
//! A passage that is too narrow fails the check unless it is listed in
//! [`KNOWN`], the problems already found and owned by a thread; an entry
//! there that has been fixed fails it too, so the list only shrinks.
//! A new kind of passage (a dam's walkway) adds itself in [`passages`].

#![cfg(test)]

use crate::buildings::{Looks, Table};
use gc_assets::model::{Models, POSITION_SCALE};
use gc_data::conquest::{Class, UNIT_FLYING};

/// Room kept free each side of the widest body, in tiles.
const MARGIN: f32 = 0.1;
/// Soldiers abreast in the narrowest column line troops should keep.
const COLUMN_FILES: f32 = 2.0;
/// Line troops: foot regiments of at least this many soldiers (the
/// few-and-big, such as the Choir's Harmonic Walkers, go in single file).
const LINE_MODELS: u16 = 9;
/// The narrowest bridge a player can draw, in tiles across
/// (`gc_sim::drawn::BRIDGE_WIDTHS`).
const BRIDGE_WIDTH: f32 = gc_sim::drawn::BRIDGE_WIDTHS[0] as f32;
/// How many one-tile bridge models lie side by side in the measure of a
/// model's deck (two lanes, each with its rail on the outside). Drawn
/// bridges are built from the timber kit's pieces instead (`bridge_kit`).
const BRIDGE_LANES: f32 = 2.0;
/// The height band a model's passage must be open in, above its floor or
/// deck, in tiles: from the ankles to the chest of a man.
const BAND: (f32, f32) = (0.1, 1.2);
/// A model's floor (a threshold, a deck) lies below this height.
const FLOOR_BELOW: f32 = 0.6;

/// Problems found and owned: what fails, what fixes it, and the passages.
const KNOWN: &[(What, &str, &[&str])] = &[];

/// Which measure of a passage fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum What {
    /// The footprint is narrower than the widest walking body.
    Body,
    /// The footprint is narrower than a column of line troops.
    Column,
    /// The model's opening is narrower than the footprint needs.
    Model,
}

/// A passage: a building troops walk through (a gate) or over (a deck).
struct Passage {
    name: String,
    /// Its width across the way through, as the simulation walks it.
    width: f32,
    /// What its finished model leaves open, if it has a model.
    model: Option<f32>,
}

/// Every passage in the game data, measured: each building's finished
/// model is the one the game draws for it (a faction's bridge without a
/// model of its own shows the shared one).
fn passages(table: &Table, models: &Models<'_>) -> Vec<Passage> {
    let looks = Looks::new(table, models);
    (0..table.stats.len() as u16)
        .filter_map(|b| {
            let s = table.stats[usize::from(b)];
            let tris = || looks.finished(b).map(|m| triangles(models, m));
            let (width, model) = if table.is_gate(b) {
                let width = f32::from(s.width.max(s.depth));
                (width, tris().map(|t| doorway(&t, width / 2.0)))
            } else if table.is_bridge(b) {
                (BRIDGE_WIDTH, tris().map(|t| deck(&t)))
            } else {
                return None;
            };
            Some(Passage {
                name: table.name(b).to_string(),
                width,
                model,
            })
        })
        .collect()
}

/// The widest body that walks (flyers pass over), in tiles across, and
/// its unit: across its model if it has one, else its file spacing.
fn widest_body(data: &gc_data::GameData, models: &Models<'_>) -> (f32, String) {
    (data.conquest_units.iter())
        .filter(|u| u.flags & UNIT_FLYING == 0 && u.class() != Class::Air)
        .map(|u| {
            let across = models.by_name(u.name()).map_or_else(
                || crate::fx_to_f32(u.stats.file_spacing()),
                |m| {
                    let r = &models.models[m];
                    let first = r.first_vertex as usize;
                    (models.vertices[first..first + r.vertex_count as usize].iter())
                        .map(|v| f32::from(v.pos[0]).abs() * 2.0 / POSITION_SCALE)
                        .fold(0.0, f32::max)
                },
            );
            (across, u.name().to_string())
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("units")
}

/// The widest column of line troops [`COLUMN_FILES`] abreast, in tiles,
/// and its unit.
fn widest_column(data: &gc_data::GameData) -> (f32, String) {
    (data.conquest_units.iter())
        .filter(|u| {
            matches!(
                u.class(),
                Class::Light | Class::Spear | Class::Heavy | Class::Ranged
            ) && u.stats.models >= LINE_MODELS
        })
        .map(|u| {
            (
                COLUMN_FILES * crate::fx_to_f32(u.stats.file_spacing()),
                u.name().to_string(),
            )
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("line troops")
}

type Triangle = [[f32; 3]; 3];

/// Model `m`'s triangles at full detail, in tiles.
fn triangles(models: &Models<'_>, m: usize) -> Vec<Triangle> {
    let r = &models.models[m];
    let [first, count] = r.lods[0];
    let vertex = |i: u16| {
        let p = models.vertices[r.first_vertex as usize + usize::from(i)].pos;
        p.map(|c| f32::from(c) / POSITION_SCALE)
    };
    (models.indices[first as usize..(first + count) as usize].chunks_exact(3))
        .map(|t| [vertex(t[0]), vertex(t[1]), vertex(t[2])])
        .collect()
}

/// Where along `axis` triangles block between heights `lo` and `hi`, as
/// intervals: each triangle's part inside the band, seen along the way
/// through (so anything anywhere along it blocks).
fn blocked(tris: &[Triangle], axis: usize, lo: f32, hi: f32) -> Vec<(f32, f32)> {
    tris.iter()
        .filter_map(|t| {
            let mut span: Option<(f32, f32)> = None;
            let mut add = |v: f32| {
                span = Some(span.map_or((v, v), |(a, b)| (a.min(v), b.max(v))));
            };
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if (lo..=hi).contains(&a[1]) {
                    add(a[axis]);
                }
                for plane in [lo, hi] {
                    if (a[1] - plane) * (b[1] - plane) < 0.0 {
                        let s = (plane - a[1]) / (b[1] - a[1]);
                        add(a[axis] + s * (b[axis] - a[axis]));
                    }
                }
            }
            span
        })
        .collect()
}

/// The free stretches between `from` and `to` that `blocks` leave.
fn gaps(mut blocks: Vec<(f32, f32)>, from: f32, to: f32) -> Vec<(f32, f32)> {
    blocks.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut free = Vec::new();
    let mut reach = from;
    for (a, b) in blocks {
        if a > reach {
            free.push((reach, a.min(to)));
        }
        reach = reach.max(b);
    }
    if reach < to {
        free.push((reach, to));
    }
    free.retain(|(a, b)| b > a);
    free
}

/// The top of a model's floor (a gate's threshold, a bridge's deck): the
/// highest face that looks up from under the middle of the way through,
/// below [`FLOOR_BELOW`]; 0 if there is none.
fn floor(tris: &[Triangle]) -> f32 {
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    tris.iter()
        .filter(|t| {
            let (u, v) = (sub(t[1], t[0]), sub(t[2], t[0]));
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let flat = n[1].abs() >= 0.9 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            // Over the middle (x, z) = (0, 0), seen from above.
            let side = |a: [f32; 3], b: [f32; 3]| a[0] * b[2] - a[2] * b[0];
            let d = [side(t[0], t[1]), side(t[1], t[2]), side(t[2], t[0])];
            let over = d.iter().all(|&e| e >= 0.0) || d.iter().all(|&e| e <= 0.0);
            flat && over && n[1] != 0.0
        })
        .map(|t| (t[0][1] + t[1][1] + t[2][1]) / 3.0)
        .filter(|&y| y < FLOOR_BELOW)
        .fold(0.0, f32::max)
}

/// The widest free doorway of a gate model: across its longer side, at
/// [`BAND`] above its floor, within `half` of its middle (its footprint).
fn doorway(tris: &[Triangle], half: f32) -> f32 {
    let extent = |axis: usize| {
        (tris.iter().flatten())
            .map(|v| v[axis].abs())
            .fold(0.0, f32::max)
    };
    let across = if extent(0) >= extent(2) { 0 } else { 2 };
    let half = extent(across).min(half);
    let top = floor(tris);
    gaps(
        blocked(tris, across, top + BAND.0, top + BAND.1),
        -half,
        half,
    )
    .iter()
    .map(|(a, b)| b - a)
    .fold(0.0, f32::max)
}

/// The free width of a two-lane deck built from bridge tile model `tris`
/// (one lane, running along its z, its rail on one side): the free
/// stretch of one lane from its inner edge, twice, at [`BAND`] above the
/// deck.
fn deck(tris: &[Triangle]) -> f32 {
    let top = floor(tris);
    let free = gaps(blocked(tris, 0, top + BAND.0, top + BAND.1), -0.5, 0.5);
    // The two lanes meet at their inner edges: whichever edge is open.
    let edge = |e: f32| {
        (free.iter())
            .filter(|(a, b)| (a - e).abs() < 1e-3 || (b - e).abs() < 1e-3)
            .map(|(a, b)| b - a)
            .fold(0.0, f32::max)
    };
    BRIDGE_LANES * edge(-0.5).max(edge(0.5))
}

#[test]
fn passages_fit_the_widest_unit_and_a_column() {
    let data = crate::data::load();
    let table = Table::new(&data);
    let models = crate::models::load().expect("model pack");
    let (body, body_unit) = widest_body(&data, &models);
    let (column, column_unit) = widest_column(&data);
    let room = body + 2.0 * MARGIN;
    let needed = room.max(column);
    println!(
        "widest walking body: {body_unit} {body:.2} tiles; widest column of line troops \
         {COLUMN_FILES} abreast: {column_unit} {column:.2} tiles"
    );
    let mut found = Vec::new();
    for p in passages(&table, &models) {
        let model = p
            .model
            .map_or("no model".into(), |m| format!("model {m:.2}"));
        println!("{}: footprint {:.2}, {model}", p.name, p.width);
        if p.width < room {
            found.push((p.name.clone(), What::Body));
        }
        if p.width < column {
            found.push((p.name.clone(), What::Column));
        }
        // The model must leave open what the footprint lets through, up
        // to what troops need.
        if p.model.is_some_and(|m| m < p.width.min(needed) - MARGIN) {
            found.push((p.name.clone(), What::Model));
        }
    }
    let known: Vec<(&str, What)> = (KNOWN.iter())
        .flat_map(|&(w, _, names)| names.iter().map(move |&n| (n, w)))
        .collect();
    let new: Vec<_> = (found.iter())
        .filter(|&(n, w)| !known.contains(&(n.as_str(), *w)))
        .collect();
    let fixed: Vec<_> = (known.iter())
        .filter(|&&(n, w)| !found.iter().any(|(f, fw)| f == n && *fw == w))
        .collect();
    assert!(
        new.is_empty(),
        "passages too narrow (widest body {body:.2} + margins, column {column:.2}): {new:?}"
    );
    assert!(
        fixed.is_empty(),
        "fixed, so take them off clearance::KNOWN: {fixed:?}"
    );
}

#[test]
fn the_measures_find_a_doorway_and_a_deck() {
    // An upright wall at `x`, from `z0` to `z1` and `y0` to `y1`.
    let wall = |x: f32, z0: f32, z1: f32, y0: f32, y1: f32| -> [Triangle; 2] {
        [
            [[x, y0, z0], [x, y0, z1], [x, y1, z1]],
            [[x, y0, z0], [x, y1, z1], [x, y1, z0]],
        ]
    };
    // A level face at height `y` over x0..x1 and z0..z1.
    let level = |x0: f32, x1: f32, z0: f32, z1: f32, y: f32| -> [Triangle; 2] {
        [
            [[x0, y, z0], [x1, y, z0], [x1, y, z1]],
            [[x0, y, z0], [x1, y, z1], [x0, y, z1]],
        ]
    };
    // A gate's two posts, two tiles apart across its three-tile side, on
    // a threshold 0.2 high (which doesn't block the doorway).
    let gate = [
        wall(0.0, -1.5, -1.0, 0.0, 2.0),
        wall(0.0, 1.0, 1.5, 0.0, 2.0),
        level(-0.5, 0.5, -1.5, 1.5, 0.2),
        wall(0.5, -1.5, 1.5, 0.0, 0.2),
    ]
    .concat();
    assert!(
        (doorway(&gate, 1.5) - 2.0).abs() < 1e-3,
        "{}",
        doorway(&gate, 1.5)
    );
    // Shut, its leaves close the doorway.
    let shut = [gate.clone(), wall(0.0, -1.0, 1.0, 0.2, 1.8).to_vec()].concat();
    assert!(doorway(&shut, 1.5) < 1e-3, "{}", doorway(&shut, 1.5));
    // A lane: a deck 0.3 up, and a rail 0.05 in from its outer edge
    // with a level top at 0.55 (not the deck).
    let rail = [[[0.45, 0.3, -0.5], [0.45, 0.3, 0.5], [0.45, 1.5, 0.5]]];
    let lane = [
        level(-0.5, 0.5, -0.5, 0.5, 0.3).to_vec(),
        rail.to_vec(),
        level(0.44, 0.46, -0.5, 0.5, 0.55).to_vec(),
    ]
    .concat();
    assert!((floor(&lane) - 0.3).abs() < 1e-3, "{}", floor(&lane));
    assert!((deck(&lane) - 1.88).abs() < 1e-3, "{}", deck(&lane));
}
