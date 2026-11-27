//! The Captain's own state, checked against small made-up fields.

use super::*;
use crate::army::{Placement, spawn_commands};
use crate::difficulty::Difficulty;
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{Capacities, MatchSetup, PlayerCommand, World};

const MAP: u32 = 256;

/// A flat field with one regiment of ours (party 0) and one of theirs
/// (party 1) standing where given, as party 0 sees it.
fn field(ours: Vec2, theirs: Vec2) -> Perception {
    let nav = vec![gc_sim::nav::OPEN; (MAP as usize * gc_sim::nav::CELLS_PER_TILE as usize).pow(2)];
    let mut world = World::new(&MatchSetup {
        seed: 1,
        capacities: Capacities { regiments: 8 },
        units: &DRILL_UNITS,
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: false,
    });
    let at = |position: Vec2, facing: Vec2| Placement {
        unit: 0,
        position,
        facing,
    };
    let mut out: Vec<PlayerCommand> = Vec::new();
    spawn_commands(0, &[at(ours, Vec2::Y)], &mut out);
    spawn_commands(1, &[at(theirs, -Vec2::Y)], &mut out);
    world.tick(&out);
    let mut view = Perception::new();
    view.capture(&world);
    view
}

/// What a Captain thinks with, on `view`.
struct Mind {
    terrain: Terrain,
    knobs: Knobs,
    rng: Pcg32,
    journal: Journal,
    next_slip: u32,
}

impl Mind {
    fn new() -> Mind {
        Mind {
            terrain: Terrain::open(MAP),
            knobs: Difficulty::General.knobs(),
            rng: Pcg32::new(1, 1),
            journal: Journal::new(),
            next_slip: 0,
        }
    }

    fn ctx<'a>(&'a mut self, view: &'a Perception) -> Ctx<'a> {
        Ctx {
            view,
            terrain: &self.terrain,
            party: 0,
            knobs: &self.knobs,
            persona: &crate::personality::all()[0],
            rng: &mut self.rng,
            journal: &mut self.journal,
            next_slip: &mut self.next_slip,
        }
    }
}

fn sides(view: &Perception) -> (Vec<&Seen>, Vec<&Seen>) {
    let ours = view.regiments.iter().filter(|r| r.owner == 0).collect();
    let theirs = view.regiments.iter().filter(|r| r.owner == 1).collect();
    (ours, theirs)
}

#[test]
fn an_army_sent_to_attack_still_crosses_when_its_ford_is_called_off() {
    // The enemy stands right where the ford comes out: it is watched.
    let exit = Vec2::new(128.0, 100.0);
    let mut view = field(Vec2::new(128.0, 160.0), exit);
    view.tick = 60 * TICKS_PER_SECOND;
    let mut mind = Mind::new();
    for pressing in [false, true] {
        // It chose that ford itself, then (or not) was sent to attack.
        let mut c = Captain::new();
        c.started = true;
        c.posture = Posture::Advance;
        c.forward = -Vec2::Y;
        c.anchor = Vec2::new(128.0, 160.0);
        c.river = Some(ground::Crossing {
            near: 10.0,
            far: 20.0,
        });
        c.crossing = true;
        c.bridgehead = Some((exit - Vec2::Y * 5.0, exit));
        c.press(pressing);
        // Nobody is in the water or across yet.
        let (_, fighting) = sides(&view);
        c.hold_bridgehead(&mut mind.ctx(&view), &[], &fighting);
        assert_eq!(c.bridgehead, None);
        // Only one that chose to cross goes back to waiting; one sent to
        // attack must not halt at the bank for good.
        assert_eq!(c.crossing, pressing, "pressing {pressing}");
        let note = &mind.journal.latest().unwrap().text;
        assert_eq!(note.ends_with("pressing on"), pressing, "{note}");
        // After its moment's hold, one sent to attack goes on.
        let mut later = view.clone();
        later.tick += POSTURE_TICKS + 1;
        let (ours, fighting) = sides(&later);
        c.choose_posture(&mut mind.ctx(&later), &ours, &fighting);
        let advancing = c.posture == Posture::Advance;
        assert_eq!(advancing, pressing, "pressing {pressing}: {:?}", c.posture);
    }
}

#[test]
fn pressing_on_or_stopping_drops_the_wait_behind_the_water() {
    let mut c = Captain::new();
    c.baiting = true;
    c.press(true);
    assert!(!c.baiting, "no standing back to tempt them while attacking");
    c.press(false);
    assert!(!c.crossing, "still crossing with no ford to go for");
}

/// Open ground two tiles high with a river across the whole field, from
/// 110 to 130 tiles down it, wadeable everywhere.
fn river_across() -> Terrain {
    let n = MAP as usize;
    let heights = (0..n * n)
        .map(|t| {
            if (110..130).contains(&(t / n)) {
                0.0
            } else {
                2.0
            }
        })
        .collect();
    Terrain::open(MAP).with_heights(heights, 1.0)
}

#[test]
fn stopping_on_the_way_to_a_ford_of_its_own_gives_it_up_unless_in_the_water() {
    // It chose a ford and was then sent to attack; the attack ends. On the
    // near bank it gives the ford up and waits again, rather than wading
    // on towards the enemy it was pulled away from; already in the water
    // it goes on to the far bank rather than halting in the river. Either
    // way it never waits with a ford still calling it forward.
    let exit = Vec2::new(128.0, 109.0);
    let to = exit - Vec2::Y * 5.0;
    for (at, in_water) in [(160.0, false), (120.0, true)] {
        let view = field(Vec2::new(128.0, at), Vec2::new(40.0, 40.0));
        let (ours, _) = sides(&view);
        let mut mind = Mind::new();
        mind.terrain = river_across();
        let mut c = Captain::new();
        c.started = true;
        c.posture = Posture::Advance;
        c.crossing = true;
        c.bridgehead = Some((to, exit));
        c.press(true);
        c.press(false);
        c.called_back(&mut mind.ctx(&view), &ours);
        assert_eq!(c.crossing, in_water, "at {at}");
        assert_eq!(c.bridgehead.is_some(), in_water, "at {at}");
        if !in_water {
            assert_eq!(c.posture, Posture::Hold);
            assert!(c.waiting_behind_water() || c.river.is_none());
        }
    }
}

#[test]
fn no_bridgehead_by_a_ford_the_water_is_shutting() {
    // Nobody is watching where the ford comes out, straight ahead.
    let view = field(Vec2::new(128.0, 160.0), Vec2::new(40.0, 40.0));
    let (ours, fighting) = sides(&view);
    let mut mind = Mind::new();
    for (shut_x, chosen) in [(None, true), (Some(175), true), (Some(148), false)] {
        let mut c = Captain::new();
        c.started = true;
        c.forward = -Vec2::Y;
        c.anchor = Vec2::new(128.0, 160.0);
        c.river = Some(ground::Crossing {
            near: 30.0,
            far: 50.0,
        });
        mind.terrain = river_across().with_shut(shut_x.map(|x| (x, 109)).into_iter().collect());
        let b = c.bridgehead_at(&mind.ctx(&view), &ours, &fighting);
        assert_eq!(b.is_some(), chosen, "shut at {shut_x:?}: {b:?}");
        // One it picks is not called off as soon as it is chosen.
        if let Some(b) = b {
            c.bridgehead = Some(b);
            c.crossing = true;
            c.hold_bridgehead(&mut mind.ctx(&view), &ours, &fighting);
            assert!(c.bridgehead.is_some(), "shut at {shut_x:?}");
        }
    }
}

#[test]
fn turning_to_face_a_new_way_forgets_the_covered_end() {
    // The enemy is off to the east; the line faced south.
    let view = field(Vec2::new(100.0, 128.0), Vec2::new(180.0, 128.0));
    let (ours, fighting) = sides(&view);
    let mut mind = Mind::new();
    let mut c = Captain::new();
    c.started = true;
    c.posture = Posture::Hold;
    c.forward = Vec2::Y;
    c.covered = 1.0;
    c.relayout = false;
    c.update_frame(&mut mind.ctx(&view), &ours, &fighting);
    assert!(c.forward.x > 0.9, "it turned: {}", c.forward);
    assert_eq!(c.covered, 0.0);
}

#[test]
fn going_over_to_a_bridgehead_forgets_the_covered_end() {
    // Nobody is watching where the ford comes out.
    let view = field(Vec2::new(128.0, 160.0), Vec2::new(40.0, 40.0));
    let mut mind = Mind::new();
    // A river across the field, from 110 to 130 tiles down it.
    let n = MAP as usize;
    let heights = (0..n * n)
        .map(|t| {
            if (110..130).contains(&(t / n)) {
                0.0
            } else {
                2.0
            }
        })
        .collect();
    mind.terrain = Terrain::open(MAP).with_heights(heights, 1.0);
    let mut c = Captain::new();
    c.started = true;
    c.posture = Posture::Advance;
    c.crossing = true;
    c.bridgehead = Some((Vec2::new(200.0, 100.0), Vec2::new(200.0, 105.0)));
    c.covered = 1.0;
    c.relayout = false;
    let (ours, fighting) = sides(&view);
    c.hold_bridgehead(&mut mind.ctx(&view), &ours, &fighting);
    assert_eq!(c.anchor, Vec2::new(200.0, 100.0), "not going over");
    assert_eq!(c.covered, 0.0);
    assert!(c.relayout);
}

#[test]
fn falling_back_forgets_the_covered_end() {
    let mut view = field(Vec2::new(128.0, 160.0), Vec2::new(128.0, 80.0));
    // Long enough into the battle that it may fall back at once.
    view.tick = 60 * TICKS_PER_SECOND;
    let (ours, fighting) = sides(&view);
    let mut mind = Mind::new();
    let mut c = Captain::new();
    c.started = true;
    c.posture = Posture::Hold;
    c.forward = -Vec2::Y;
    c.anchor = Vec2::new(128.0, 160.0);
    c.covered = -1.0;
    c.relayout = false;
    // Keeping out of others' way: it steps back now.
    c.shy = Some(Vec2::Y);
    c.choose_posture(&mut mind.ctx(&view), &ours, &fighting);
    assert_eq!(c.posture, Posture::Withdraw);
    assert_eq!(c.covered, 0.0);
    assert!(c.relayout);
}
