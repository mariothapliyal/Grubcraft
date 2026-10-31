//! Veterancy (docs/05-combat.md, "Veterancy"; docs/29, "Holding until
//! bloodied"). A regiment gains experience from the enemy soldiers it
//! kills, in melee or with its volleys, weighed by their hit points against
//! its own: killing its own worth makes it rank 1, twice its worth rank 2,
//! four times its worth rank 3. Kills by volleys count half, as shooting
//! from afar is safer work. Each rank adds [`RANK_DAMAGE_PERCENT`] to the
//! damage it deals and takes [`RANK_BREAK`] off its breaking point, so
//! veterans hold longer. (docs/05's "+10 morale per rank" is that lower
//! breaking point: a higher morale ceiling on top of it made veterans
//! nearly unbreakable and stalled Conquest games.) Ranks are
//! never lost. The screen shows them as chevrons on the banner and card.

use crate::regiments::Regiments;
use crate::{UnitId, UnitStats};

/// Experience for each rank, in thousandths of the regiment's own full
/// hit points killed.
pub(crate) const RANKS: [u16; 3] = [1000, 2000, 4000];
/// Percent of the experience a kill by a volley earns.
const SHOT_PERCENT: u64 = 50;
/// More damage, in percent, for each rank.
const RANK_DAMAGE_PERCENT: u64 = 10;
/// Percent off the breaking point for each rank.
pub const RANK_BREAK: u8 = 5;

/// The rank (0 to 3) that `xp` earns.
fn rank(xp: u16) -> u8 {
    RANKS.iter().filter(|&&t| xp >= t).count() as u8
}

/// The least experience that earns `rank` (0 for none).
pub(crate) fn xp_for(rank: u8) -> u16 {
    match rank {
        0 => 0,
        r => RANKS[usize::from(r.min(3)) - 1],
    }
}

/// Regiment `i`'s rank.
pub(crate) fn rank_of(regs: &Regiments, i: usize) -> u8 {
    rank(regs.xp[i])
}

/// Percent of its usual damage a regiment of `rank` deals.
pub fn rank_damage_percent(rank: u8) -> u64 {
    100 + RANK_DAMAGE_PERCENT * u64::from(rank)
}

/// Percent of its usual damage regiment `i` deals for its rank.
pub(crate) fn damage_percent(regs: &Regiments, i: usize) -> u64 {
    rank_damage_percent(rank_of(regs, i))
}

/// Regiment `killer` killed `men` soldiers of unit `victim`, with a volley
/// if `shot`: their hit points, in thousandths of its own full hit points
/// (half for a volley), go to its experience.
pub(crate) fn credit(
    regs: &mut Regiments,
    units: &[UnitStats],
    killer: usize,
    victim: UnitId,
    men: u16,
    shot: bool,
) {
    let own = u64::from(regs.full_hp(killer, &units[regs.unit[killer] as usize]));
    let theirs = u64::from(units[victim as usize].hp_per_model);
    // Rounded, so a band that kills its own worth man by man gets there.
    let share = if shot { SHOT_PERCENT } else { 100 };
    let gained = (u64::from(men) * theirs * 10 * share + own / 2) / own;
    regs.xp[killer] = (u64::from(regs.xp[killer]) + gained).min(u64::from(u16::MAX)) as u16;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_come_at_once_twice_and_four_times_its_worth() {
        assert_eq!(rank(0), 0);
        assert_eq!(rank(999), 0);
        assert_eq!(rank(1000), 1);
        assert_eq!(rank(2000), 2);
        assert_eq!(rank(3999), 2);
        assert_eq!(rank(4000), 3);
        assert_eq!(rank(u16::MAX), 3);
        assert_eq!(rank_damage_percent(3), 130);
        assert_eq!(xp_for(0), 0);
        assert_eq!(rank(xp_for(2)), 2);
        assert_eq!(rank(xp_for(3)), 3);
    }
}
