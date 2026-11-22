//! Real footprints (docs/42, stage 2): every soldier's block holds its
//! model. A soldier stands in a file spacing across and a rank spacing
//! deep (`gc_sim::UnitStats::spacing`), and the simulation keeps blocks
//! apart by those; a model wider or deeper than that is drawn through its
//! neighbours and through the blocks beside it. Cheap: it reads the baked
//! data and models, so it runs with the ordinary tests on every pull
//! request.

#![cfg(test)]

use gc_assets::model::{Models, POSITION_SCALE};
use gc_data::UnitRecord;
use gc_data::conquest::{Class, UNIT_FLYING};

/// What a model stands on: the share of its height, from the ground up,
/// that a body touching its neighbours fills. Above it are arms, weapons
/// and wings, which in close order reach over the next man's room.
const GROUND_BAND: f32 = 0.4;

/// A model's footprint across (x) and deep (z), in tiles: twice its
/// furthest point from the middle each way in [`GROUND_BAND`], as it
/// stands at rest.
fn extent(models: &Models<'_>, m: usize) -> (f32, f32) {
    let r = &models.models[m];
    let first = r.first_vertex as usize;
    let vertices = &models.vertices[first..first + r.vertex_count as usize];
    let height = |v: &gc_assets::model::ModelVertex| f32::from(v.pos[1]) / POSITION_SCALE;
    let top = vertices.iter().map(height).fold(0.0, f32::max);
    (vertices.iter())
        .filter(|v| height(v) <= top * GROUND_BAND)
        .fold((0.0, 0.0), |(x, z), v| {
            (
                f32::max(x, f32::from(v.pos[0]).abs() * 2.0 / POSITION_SCALE),
                f32::max(z, f32::from(v.pos[2]).abs() * 2.0 / POSITION_SCALE),
            )
        })
}

/// A model may stand this much wider or deeper than its room: legs and
/// feet splay, and neighbours' interleave.
const SLACK: f32 = 1.1;

/// Which measure of a unit fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum What {
    /// Its model is wider than its file spacing.
    Across,
    /// Its model is deeper than its rank spacing.
    Deep,
}

/// Units whose model does not fit its room, and why that stands. A fixed
/// entry fails the check too, so the list only shrinks.
const KNOWN: &[(What, &str, &str)] = &[
    // A pike levelled low reaches into the rank ahead, as pikes do.
    (What::Deep, "Yari Ashigaru", "pike levelled"),
    (What::Deep, "Bone Pikes", "pike levelled"),
    (What::Deep, "Grubfolk Militia", "pike levelled"),
    // The Brood's line insects are drawn bigger than their blocks. Giving
    // them that room is a Brood balance change of its own (docs/42, stage
    // 2), not part of the movement work.
    (What::Across, "Chitin Guard", "Brood line, to decide"),
    (What::Deep, "Chitin Guard", "Brood line, to decide"),
    (What::Across, "Mantid Reapers", "Brood line, to decide"),
    (What::Deep, "Mantid Reapers", "Brood line, to decide"),
    (What::Across, "Shellbacks", "Brood line, to decide"),
    (What::Deep, "Shellbacks", "Brood line, to decide"),
    (What::Across, "Spitters", "Brood line, to decide"),
    (What::Deep, "Spitters", "Brood line, to decide"),
    (What::Across, "Skitterlings", "Brood line, to decide"),
    (What::Deep, "Skitterlings", "Brood line, to decide"),
    (What::Deep, "Burrowers", "Brood line, to decide"),
];

#[test]
fn every_body_fits_the_room_it_takes() {
    let data = crate::data::load();
    let models = crate::models::load().expect("model pack");
    // Flyers pass over each other.
    let flying = |u: &UnitRecord| {
        (data.conquest_units.iter()).any(|c| {
            c.name() == u.name() && (c.flags & UNIT_FLYING != 0 || c.class() == Class::Air)
        })
    };
    let all = (data.units.iter()).chain(data.conquest_units.iter().map(|u| &u.stats));
    let mut found = Vec::new();
    let mut checked = 0;
    for u in all.filter(|u| !flying(u)) {
        let Some(m) = models.by_name(u.name()) else {
            continue;
        };
        checked += 1;
        let (across, deep) = extent(&models, m);
        let (files, ranks) = (
            crate::fx_to_f32(u.file_spacing()),
            crate::fx_to_f32(u.rank_spacing()),
        );
        for (what, model, room) in [(What::Across, across, files), (What::Deep, deep, ranks)] {
            if model > room * SLACK && !found.contains(&(what, u.name().to_string())) {
                println!("{} {what:?}: model {model:.2}, room {room:.2}", u.name());
                found.push((what, u.name().to_string()));
            }
        }
    }
    assert!(checked > 30, "only {checked} units with models");
    let known = |what: What, name: &str| KNOWN.iter().any(|&(w, n, _)| w == what && n == name);
    let new: Vec<_> = found.iter().filter(|(w, n)| !known(*w, n)).collect();
    assert!(
        new.is_empty(),
        "models bigger than the room their unit takes: {new:?}"
    );
    let fixed: Vec<_> = (KNOWN.iter())
        .filter(|&&(w, n, _)| !found.iter().any(|(fw, f)| *fw == w && f == n))
        .collect();
    assert!(fixed.is_empty(), "fixed, take them off KNOWN: {fixed:?}");
}
