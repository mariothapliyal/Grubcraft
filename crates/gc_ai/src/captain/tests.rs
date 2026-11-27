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
    let view = field(Vec2::new(128.0, 160.0), exit);
    let (_, fighting) = sides(&view);
    let mut mind = Mind::new();
    for pressing in [false, true] {
        let mut c = Captain::new();
        c.crossing = true;
        c.press(pressing);
        c.bridgehead = Some((exit - Vec2::Y * 5.0, exit));
        let mut ctx = mind.ctx(&view);
        // Nobody is in the water or across yet.
        c.hold_bridgehead(&mut ctx, &[], &fighting);
        assert_eq!(c.bridgehead, None);
        // Only one that chose to cross goes back to waiting; one sent to
        // attack must not halt at the bank for good.
        assert_eq!(c.crossing, pressing, "pressing {pressing}");
    }
}

#[test]
fn pressing_on_or_stopping_drops_the_wait_behind_the_water() {
    let mut c = Captain::new();
    c.baiting = true;
    c.press(true);
    assert!(!c.baiting, "no standing back to tempt them while attacking");
    c.bridgehead = Some((Vec2::ZERO, Vec2::ONE));
    c.press(false);
    assert!(!c.crossing);
    assert_eq!(c.bridgehead, None, "no ford to go for while waiting again");
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
