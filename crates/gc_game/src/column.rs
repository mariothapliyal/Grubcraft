//! Column of march (docs/42-armies-that-march.md, stage 4, item 19): the
//! player holds Alt and right-clicks the ground (or holds Ctrl and Alt and
//! draws a road), and the army goes there in road column: every regiment
//! in Column formation ([`gc_sim::COLUMN_FILES`] wide, a quarter faster),
//! one behind the other as a brigade on one track. The column deploys
//! into the army's shape when its head comes to the end, or, at once,
//! facing the enemy, when an enemy regiment its side can see comes within
//! [`NEAR`] of it.
//!
//! All of it is orders the game sends for the player (the formation
//! changes, the march, the deploying order): nothing new reaches the
//! simulation. The game watches each column it sent ([`watch`]) and sends
//! the deploying order itself ([`deploy`]).

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::arrange::{Arrangement, Preset, Role, Shape};
use gc_sim::formation::{GROUP_LINE_GAP, GroupMember, GroupOrder, block_size, centre_of, middle_of};
use gc_sim::orders::Step;
use gc_sim::{Command, Formation, PlayerId, RegimentHandle, World};

/// A seen enemy regiment this near a regiment of the column, in tiles,
/// makes it deploy: as far as a regiment sees ([`gc_sim::vision::SIGHT`]).
pub const NEAR: Fx = Fx::from_int(32);
/// The column's head this near its end, in tiles, makes it deploy.
const ARRIVE: Fx = Fx::from_int(12);

/// The shape a column deploys into.
#[derive(Clone, Debug)]
pub enum Form {
    Preset(Preset),
    Saved(Shape),
    /// Standing army `n` (see `gc_sim::arrange::Army`): its own shape.
    Army(usize),
}

impl Form {
    pub fn of(how: Arrangement<'_>) -> Form {
        match how {
            Arrangement::Preset(p) => Form::Preset(p),
            Arrangement::Saved(s) => Form::Saved(s.clone()),
        }
    }
}

/// A regiment of a column.
#[derive(Clone, Debug)]
struct Member {
    handle: RegimentHandle,
    /// The formation it deploys into: the one it marched off in.
    was: Formation,
    /// Its frontage before the march, which it deploys at.
    files: u16,
    /// Where each leg of the march sends it, first first.
    ends: Vec<FxVec2>,
}

/// A column on the march, watched until it deploys.
#[derive(Clone, Debug)]
pub struct March {
    members: Vec<Member>,
    /// The regiment at the column's front.
    head: Option<RegimentHandle>,
    /// Where the march ends, and the way the army faces there.
    end: FxVec2,
    facing: Angle,
    pub form: Form,
    /// Come to the end: filing on to its places in the army deployed.
    filing: bool,
}

/// Why a column deploys.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Why {
    /// Its head came to the end.
    Arrived,
    /// An enemy regiment, standing here, came near.
    Enemy(FxVec2),
}

/// Plan a column's march by `order`: `members` narrowed to a column's
/// frontage and stood one behind the other, so a brigade walks them in
/// file down one track. They file off in the order they stand: the one
/// furthest ahead the way they go first, and of those abreast, the left
/// first (a line marches off by its left). The column lies back from the
/// order's target the way it comes or, where the ground there is cut
/// off (`clear` says whether the straight line between two points can be
/// walked), swung about the target as little as keeps all of it in clear
/// line of its middle: a brigade marches as one body only those whose
/// places can be seen from the middle of them all.
pub fn arrange(
    members: &mut [GroupMember],
    order: GroupOrder,
    clear: &dyn Fn(FxVec2, FxVec2) -> bool,
    out: &mut Vec<Command>,
) {
    if members.is_empty() {
        return;
    }
    for m in members.iter_mut() {
        m.files = Formation::Column.files(m.files, m.models);
    }
    let centre = centre_of(members);
    let travel = order.target - centre;
    let way = if travel == FxVec2::ZERO {
        order.facing.unwrap_or(members[0].facing)
    } else {
        Angle::of_vec(travel)
    };
    // Who goes first: by how far ahead each stands (to the nearest
    // [`ABREAST`]), then from the left.
    let ahead = way.to_vec();
    let right = FxVec2::new(-ahead.y, ahead.x);
    let mut file: Vec<usize> = (0..members.len()).collect();
    file.sort_by_key(|&k| {
        let at = members[k].position - centre;
        (-(at.dot(ahead) / ABREAST).floor_int(), at.dot(right), k)
    });
    // How far behind the target each stands.
    let mut backs = Vec::with_capacity(file.len());
    let mut back = Fx::ZERO;
    for (n, &k) in file.iter().enumerate() {
        let m = &members[k];
        let depth = block_size(m.files, m.models, m.file_spacing, m.rank_spacing).1;
        if n > 0 {
            back += depth / 2 + GROUP_LINE_GAP;
        }
        backs.push(back);
        back += depth / 2;
    }
    let pace = if members.len() < 2 {
        Fx::ZERO
    } else {
        members.iter().map(|m| m.speed).min().unwrap_or(Fx::ZERO)
    };
    let mut best: Option<(usize, Angle)> = None;
    for turn in TURNS {
        let facing = way.wrapping_add(Angle(turn));
        let behind = facing.to_vec() * -Fx::ONE;
        let places: Vec<FxVec2> = backs.iter().map(|&b| order.target + behind * b).collect();
        let mid = middle_of(places.iter().copied());
        let cut = places.iter().filter(|&&p| !clear(mid, p)).count();
        if best.is_none_or(|(c, _)| cut < c) {
            best = Some((cut, facing));
        }
        if cut == 0 {
            break;
        }
    }
    let facing = best.map_or(way, |(_, f)| f);
    let behind = facing.to_vec() * -Fx::ONE;
    for (&k, &b) in file.iter().zip(&backs) {
        let m = &members[k];
        out.push(Command::MoveTo {
            regiment: m.handle,
            target: order.target + behind * b,
            facing: Some(facing),
            files: m.files,
            pace,
            brigade: order.brigade,
        });
    }
}

/// Regiments within this of each other the way they go, in tiles, stand
/// abreast: they file off from the left.
const ABREAST: Fx = Fx::from_int(4);

/// The turns of a column from the way it comes that [`arrange`] tries,
/// least first, in [`Angle`] units (every 20 degrees, either way).
const TURNS: [u16; 18] = {
    let mut t = [0u16; 18];
    let step = 0x10000 / 18;
    let mut k = 1;
    while k < 18 {
        let n = (k as u32).div_ceil(2) * step;
        t[k] = if k % 2 == 1 { n as u16 } else { (0x10000 - n) as u16 };
        k += 1;
    }
    t
};

/// Start a column on the march `legs` (each a list of moves, as
/// [`arrange`] plans them: the first sent as an order, the rest queued
/// after it), ending at `end`, where it deploys facing `facing`: the
/// formation changes to send before the march, and the column to watch.
/// Each move keeps the regiment's own frontage (a column stands narrower
/// anyway, and keeps that width to form up at again).
pub fn begin(
    world: &World,
    legs: &mut [Vec<Command>],
    end: FxVec2,
    facing: Angle,
    form: Form,
) -> (Vec<Command>, March) {
    let mut members: Vec<Member> = Vec::new();
    let mut changes = Vec::new();
    for leg in legs.iter_mut() {
        for c in leg.iter_mut() {
            let Command::MoveTo {
                regiment,
                target,
                files,
                ..
            } = c
            else {
                continue;
            };
            if let Some(m) = members.iter_mut().find(|m| m.handle == *regiment) {
                m.ends.push(*target);
                *files = m.files;
                continue;
            }
            let Some(r) = world.regiments.get(*regiment) else {
                continue;
            };
            let raised = world.units()[usize::from(r.unit)].files;
            // One in a column already deploys in line, as wide as raised.
            let (was, before) = match r.formation {
                Formation::Column => (Formation::Line, raised),
                f if f.own_frontage() => (f, raised),
                f => (f, r.frontage),
            };
            *files = before;
            if r.formation != Formation::Column {
                changes.push(Command::SetFormation {
                    regiment: *regiment,
                    formation: Formation::Column,
                });
            }
            members.push(Member {
                handle: *regiment,
                was,
                files: before,
                ends: vec![*target],
            });
        }
    }
    let mut march = March {
        members,
        head: None,
        end,
        facing,
        form,
        filing: false,
    };
    march.find_head();
    (changes, march)
}

impl March {
    /// Whether no regiment is left in it.
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Take `regiments` out of the column (they were given other orders).
    pub fn leave(&mut self, regiments: &[RegimentHandle]) {
        self.members.retain(|m| !regiments.contains(&m.handle));
        self.find_head();
    }

    /// Take out of the column the regiments `commands` (orders about to
    /// be sent) send elsewhere, other than its own; filing, each forms up
    /// as it goes (its order to, into `out`).
    pub fn heed(&mut self, commands: &[Command], out: &mut Vec<Command>) {
        let before = self.members.len();
        for c in commands {
            let (regiment, to) = match *c {
                Command::MoveTo {
                    regiment, target, ..
                }
                | Command::Then {
                    regiment,
                    step: Step::Move { target, .. },
                } => (regiment, Some(target)),
                Command::Then { regiment, .. }
                | Command::Attack { regiment, .. }
                | Command::AttackMove { regiment, .. }
                | Command::AttackStructure { regiment, .. }
                | Command::FallBack { regiment }
                | Command::Hold { regiment, on: true } => (regiment, None),
                _ => continue,
            };
            let Some(k) = self.members.iter().position(|m| m.handle == regiment) else {
                continue;
            };
            if to.is_some_and(|t| self.members[k].ends.contains(&t)) {
                continue;
            }
            let m = self.members.remove(k);
            if self.filing {
                out.push(m.form());
            }
        }
        if self.members.len() != before {
            self.find_head();
        }
    }

    /// The head: the one whose march ends nearest the column's end.
    fn find_head(&mut self) {
        let end = self.end;
        self.head = (self.members.iter())
            .filter_map(|m| Some((m.handle, (*m.ends.last()? - end).length())))
            .min_by_key(|&(_, d)| d)
            .map(|(h, _)| h);
    }
}

/// Look at column `march` of `side`. On the march: drop the regiments no
/// longer of it (fallen, routed or out of column), and say whether it
/// deploys now. Filing into its places after it came to the end (see
/// [`deploy`]): put into `out` the formation changes of those come to
/// their places, and of all of them if an enemy comes near.
pub fn watch(
    world: &World,
    side: PlayerId,
    march: &mut March,
    out: &mut Vec<Command>,
) -> Option<Why> {
    let before = march.members.len();
    march.members.retain(|m| {
        world
            .regiments
            .get(m.handle)
            .is_some_and(|r| r.formation == Formation::Column && !r.morale.broken())
    });
    if march.members.len() != before || march.head.is_none() {
        march.find_head();
    }
    let positions: Vec<FxVec2> = (march.members.iter())
        .filter_map(|m| world.regiments.get(m.handle).map(|r| r.position))
        .collect();
    if positions.is_empty() {
        return None;
    }
    // The nearest seen enemy within reach of any of the column.
    let enemy = (world.regiments.iter())
        .filter(|r| world.hostile(side, r.owner) && !r.morale.broken())
        .filter(|r| world.sees(side, r.handle))
        .filter_map(|r| {
            let d = (positions.iter())
                .map(|&p| (r.position - p).length())
                .min()?;
            (d <= NEAR).then_some((r.position, d))
        })
        .min_by_key(|&(_, d)| d);
    if march.filing {
        // Each forms up at its place, or all of them at once with an
        // enemy near.
        march.members.retain(|m| {
            let formed = enemy.is_some()
                || world.regiments.get(m.handle).is_none_or(|r| {
                    let place = *m.ends.last().unwrap_or(&r.position);
                    (place - r.position).length() <= FORM_AT
                        || (!r.moving && world.goal(m.handle).is_none())
                });
            if formed {
                out.push(m.form());
            }
            !formed
        });
        return None;
    }
    if let Some((at, _)) = enemy {
        return Some(Why::Enemy(at));
    }
    let head = march.head?;
    let m = march.members.iter().find(|m| m.handle == head)?;
    let r = world.regiments.get(head)?;
    let last = *m.ends.last()?;
    (r.queued == 0 && (last - r.position).length() <= ARRIVE).then_some(Why::Arrived)
}

/// A regiment of a column this near its place in the army deployed, in
/// tiles, forms up there.
const FORM_AT: Fx = Fx::from_int(6);

impl Member {
    /// The order that takes it out of column.
    fn form(&self) -> Command {
        Command::SetFormation {
            regiment: self.handle,
            formation: self.was,
        }
    }
}

/// The orders that deploy column `march` for `why`: a group order
/// (brigade `brigade`) laid out by `arrange` (the march's [`Form`]) at
/// the end, facing the way the march said, or about the regiment nearest
/// the enemy, facing it. With an enemy near, each regiment forms up at
/// once; come to the end, the column files on to its places at its quick
/// march, each forming up as it comes to its own (see [`watch`]), and the
/// march is watched until all have. `roles` is each unit's role in a
/// shape, by unit (none: a guess from its stats).
pub fn deploy(
    world: &World,
    march: &mut March,
    why: Why,
    brigade: u16,
    roles: &[Role],
    arrange: impl FnOnce(&mut [GroupMember], GroupOrder, &mut Vec<Command>),
) -> Vec<Command> {
    let mut out = Vec::new();
    let mut members = Vec::with_capacity(march.members.len());
    for m in &march.members {
        let Some(r) = world.regiments.get(m.handle) else {
            continue;
        };
        if matches!(why, Why::Enemy(_)) {
            out.push(m.form());
        }
        // As it stands formed up: its pace marched in column is quicker
        // still while it files on.
        let stats = &world.units()[usize::from(r.unit)];
        let mut g = GroupMember::of(&r, stats);
        (g.file_spacing, g.rank_spacing) = stats.spacing(m.was);
        g.files = m.files;
        g.speed = Fx::from_raw(
            (i64::from(stats.speed.raw()) * i64::from(m.was.pace_percent()) / 100) as i32,
        );
        if let Some(&role) = roles.get(usize::from(r.unit)) {
            g.role = role;
        }
        members.push(g);
    }
    if members.is_empty() {
        march.members.clear();
        return out;
    }
    let (target, facing) = match why {
        Why::Arrived => (march.end, march.facing),
        Why::Enemy(at) => {
            let near = (members.iter())
                .map(|m| m.position)
                .min_by_key(|&p| (at - p).length())
                .unwrap_or(at);
            let way = at - near;
            let facing = if way == FxVec2::ZERO {
                march.facing
            } else {
                Angle::of_vec(way)
            };
            (near, facing)
        }
    };
    let order = GroupOrder {
        target,
        facing: Some(facing),
        width: None,
        brigade: if members.len() > 1 { brigade } else { 0 },
    };
    let start = out.len();
    arrange(&mut members, order, &mut out);
    if why == Why::Arrived {
        march.filing = true;
        let placed: Vec<(RegimentHandle, FxVec2)> = (out[start..].iter())
            .filter_map(|c| match *c {
                Command::MoveTo {
                    regiment, target, ..
                } => Some((regiment, target)),
                _ => None,
            })
            .collect();
        march.members.retain_mut(|m| {
            let Some(&(_, at)) = placed.iter().find(|(h, _)| *h == m.handle) else {
                return false;
            };
            m.ends = vec![at];
            true
        });
    } else {
        march.members.clear();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_sim::COLUMN_FILES;
    use gc_sim::scenario::NORTH;

    fn member(index: u32, x: i32) -> GroupMember {
        GroupMember {
            handle: RegimentHandle {
                index,
                generation: 0,
            },
            position: FxVec2::from_ints(x, 100),
            facing: NORTH,
            files: 8,
            models: 32,
            file_spacing: Fx::ONE,
            rank_spacing: Fx::ONE,
            speed: Fx::from_int(3),
            role: Role::Melee,
            raised_files: 8,
        }
    }

    #[test]
    fn a_column_marches_one_behind_the_other() {
        let mut members = vec![member(0, 50), member(1, 60), member(2, 70)];
        let mut out = Vec::new();
        let order = GroupOrder {
            target: FxVec2::from_ints(60, 20),
            facing: None,
            width: None,
            brigade: 1,
        };
        arrange(&mut members, order, &|_, _| true, &mut out);
        let at: Vec<FxVec2> = (out.iter())
            .filter_map(|c| match *c {
                Command::MoveTo { target, files, .. } => {
                    assert!(files <= COLUMN_FILES);
                    Some(target)
                }
                _ => None,
            })
            .collect();
        assert_eq!(at.len(), 3);
        // One file, north to south, a block deep and more apart.
        let mut ys: Vec<Fx> = at.iter().map(|p| p.y).collect();
        ys.sort();
        for p in &at {
            assert!((p.x - Fx::from_int(60)).abs() <= Fx::ONE, "{at:?}");
        }
        assert_eq!(ys[0], Fx::from_int(20));
        assert!(ys.windows(2).all(|w| w[1] - w[0] >= Fx::from_int(8)), "{ys:?}");
    }
}
