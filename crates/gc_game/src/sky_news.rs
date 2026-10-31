//! The news line's word on the weather and on fires near the viewer's side
//! (docs/41, step 4): a storm, fog or snow coming in and clearing, and woods
//! catching fire near its troops or base. Read from the world after each
//! tick; it changes nothing in play.

use gc_sim::sky::{Weather, WeatherKind};
use gc_sim::{Burn, TICKS_PER_SECOND};

use crate::conquest::Foot;

/// The weather as troops feel it, with some give either way, so weather
/// fading in and out a step at a time is announced once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Felt {
    #[default]
    Clear,
    Rain,
    Storm,
    Snow,
    Fog,
}

/// From this strength weather has come in (the sim's spells are at least
/// 90 strong); under [`OUT`] it has cleared.
const IN: u8 = 80;
const OUT: u8 = 40;
/// Rain from this strength is a storm (where lightning starts); it is
/// rain again under [`CALMER`].
const STORM: u8 = gc_sim::sky::STORM;
const CALMER: u8 = STORM - 40;

impl Felt {
    /// How `weather` feels to troops who felt `self` until now.
    fn next(self, weather: Weather) -> Felt {
        let s = weather.strength;
        let kind = match weather.kind {
            WeatherKind::Clear => return Felt::Clear,
            WeatherKind::Rain => Felt::Rain,
            WeatherKind::Snow => Felt::Snow,
            WeatherKind::Fog => Felt::Fog,
        };
        let had = match self {
            Felt::Storm => Felt::Rain,
            other => other,
        };
        if had != kind {
            // A new kind first fades the old out, so this one starts weak.
            return if s >= IN { kind.stormy(s) } else { Felt::Clear };
        }
        match self {
            _ if s < OUT => Felt::Clear,
            Felt::Storm if s < CALMER => Felt::Rain,
            Felt::Storm => Felt::Storm,
            _ => kind.stormy(s),
        }
    }

    fn stormy(self, strength: u8) -> Felt {
        if self == Felt::Rain && strength >= STORM {
            Felt::Storm
        } else {
            self
        }
    }
}

/// What the news says when the weather turns from `was` to `now`.
fn weather_news(was: Felt, now: Felt) -> Option<&'static str> {
    Some(match (was, now) {
        (a, b) if a == b => return None,
        (Felt::Rain, Felt::Storm) => {
            "The rain is turning to a storm: the low ground by the rivers will go soft"
        }
        (Felt::Storm, Felt::Rain) => "The storm is easing to rain",
        (_, Felt::Storm) => {
            "A storm is coming in: troops see less far, and the low ground by the rivers will go soft"
        }
        (_, Felt::Rain) => {
            "Rain is setting in: troops see a little less far, and the low ground by the rivers will go soft"
        }
        (_, Felt::Snow) => "Snow is coming in: troops see less far and march slower",
        (_, Felt::Fog) => "Fog is rolling in: troops see much less far. Watch for ambushes",
        (Felt::Fog, Felt::Clear) => "The fog has lifted",
        (Felt::Snow, Felt::Clear) => "The snow has stopped",
        (Felt::Storm, Felt::Clear) => "The storm has passed",
        (_, Felt::Clear) => "The rain has stopped",
    })
}

/// Fire within this many tiles of a regiment or building is near it.
const FIRE_NEAR: i32 = 10;
/// Fires are looked for once a second (the sim's fires spread slower).
const FIRE_EVERY: u32 = TICKS_PER_SECOND;
/// A fire is announced when it first comes near, and not again for this
/// long, so a blaze flickering at the edge isn't announced over and over.
const FIRE_QUIET: u32 = 45 * TICKS_PER_SECOND;

/// Watches the weather and the fires for the news line.
#[derive(Debug, Default)]
pub(crate) struct SkyNews {
    /// The weather felt so far; `None` until the first look, so the
    /// weather a match starts in isn't announced.
    felt: Option<Felt>,
    /// For the base and the troops: whether a fire was near last time, and
    /// the tick of the last warning.
    near: [bool; 2],
    warned: [Option<u32>; 2],
    /// Tiles lightning set alight lately, with the tick (see
    /// [`SkyNews::struck`]).
    lit: Vec<(i32, i32, u32)>,
}

/// A fire newly near within this long of a bolt lighting a tile near the
/// same place is told as the bolt's doing.
const LIT_FOR: u32 = 2 * FIRE_EVERY;

/// What a fire can be near, in [`SkyNews::near`]'s order, and its news.
const BASE: usize = 0;
const TROOPS: usize = 1;
const FIRE_NEWS: [&str; 2] = [
    "Fire! It is spreading near your base",
    "Fire! It is spreading near your troops",
];
const LIGHTNING_NEWS: [&str; 2] = [
    "Lightning! It set the woods alight near your base",
    "Lightning! It set the woods alight near your troops",
];

impl SkyNews {
    /// The weather's news after a tick, if it turned.
    pub(crate) fn weather(&mut self, weather: Weather) -> Option<&'static str> {
        let Some(was) = self.felt else {
            self.felt = Some(Felt::Clear.next(weather));
            return None;
        };
        let now = was.next(weather);
        self.felt = Some(now);
        weather_news(was, now)
    }

    /// Lightning set the tile `(x, y)` alight at `tick`.
    pub(crate) fn struck(&mut self, x: i32, y: i32, tick: u32) {
        self.lit.push((x, y, tick));
    }

    /// The fires' news at `tick`: woods or scrub newly burning near the
    /// viewer's buildings (`base`, footprints) or regiments (`troops`, in
    /// tiles); the base first. Told as lightning's doing when a tile it
    /// lit lately (see [`SkyNews::struck`]) is near the same place.
    pub(crate) fn fires(
        &mut self,
        tick: u32,
        fires: &[Burn],
        troops: impl Iterator<Item = (f32, f32)>,
        base: impl Iterator<Item = Foot>,
    ) -> Option<&'static str> {
        if !tick.is_multiple_of(FIRE_EVERY) {
            return None;
        }
        self.lit.retain(|&(.., at)| tick < at + LIT_FOR);
        let r = FIRE_NEAR as f32;
        let centre = |x: i32, y: i32| (x as f32 + 0.5, y as f32 + 0.5);
        let mut now = [false; 2];
        let mut by_lightning = [false; 2];
        if !fires.is_empty() {
            for f in base {
                now[BASE] |= burning_in(fires, f, r).next().is_some();
                by_lightning[BASE] |=
                    (self.lit.iter()).any(|&(x, y, _)| crate::conquest::near(f, centre(x, y), r));
                if now[BASE] && (by_lightning[BASE] || self.lit.is_empty()) {
                    break;
                }
            }
            let close = |(x, y): (f32, f32), (bx, by): (f32, f32)| {
                let (dx, dy) = (bx - x, by - y);
                dx * dx + dy * dy <= r * r
            };
            for (x, y) in troops {
                let square = [x.floor() as i32, y.floor() as i32, 1, 1];
                now[TROOPS] |=
                    burning_in(fires, square, r).any(|b| close((x, y), centre(b.x, b.y)));
                by_lightning[TROOPS] |=
                    (self.lit.iter()).any(|&(lx, ly, _)| close((x, y), centre(lx, ly)));
                if now[TROOPS] && (by_lightning[TROOPS] || self.lit.is_empty()) {
                    break;
                }
            }
        }
        let was = std::mem::replace(&mut self.near, now);
        let place = (0..2).find(|&i| {
            now[i] && !was[i] && !self.warned[i].is_some_and(|t| tick < t + FIRE_QUIET)
        })?;
        self.warned[place] = Some(tick);
        Some(
            if by_lightning[place] {
                LIGHTNING_NEWS
            } else {
                FIRE_NEWS
            }[place],
        )
    }
}

/// The burning tiles within `margin` of footprint `f`, looking only at the
/// rows in reach (the sim keeps its fires sorted by row, then column).
fn burning_in(fires: &[Burn], f: Foot, margin: f32) -> impl Iterator<Item = &Burn> {
    let m = margin.ceil() as i32;
    let [x, y, _, d] = f;
    let from = fires.partition_point(|b| (b.y, b.x) < (y - m, x - m));
    fires[from..]
        .iter()
        .take_while(move |b| b.y < y + d + m)
        .filter(move |b| crate::conquest::near(f, (b.x as f32 + 0.5, b.y as f32 + 0.5), margin))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_sim::Growth;

    fn sky(kind: WeatherKind, strength: u8) -> Weather {
        Weather {
            kind,
            strength,
            ..Weather::default()
        }
    }

    /// Plays `steps` of weather through the watcher, returning what it said.
    fn said(steps: &[(WeatherKind, u8)]) -> Vec<&'static str> {
        let mut news = SkyNews::default();
        (steps.iter())
            .filter_map(|&(k, s)| news.weather(sky(k, s)))
            .collect()
    }

    #[test]
    fn weather_fading_in_and_out_is_announced_once_each_way() {
        use WeatherKind::*;
        let rain: Vec<_> = (0..=255).step_by(5).map(|s| (Rain, s as u8)).collect();
        let mut steps = vec![(Clear, 0)];
        steps.extend(rain.iter().copied());
        // Wobbling about the thresholds says nothing more.
        steps.extend([(Rain, 190), (Rain, 210), (Rain, 170), (Rain, 205)]);
        steps.extend(rain.iter().rev().copied());
        assert_eq!(
            said(&steps),
            [
                weather_news(Felt::Clear, Felt::Rain).unwrap(),
                weather_news(Felt::Rain, Felt::Storm).unwrap(),
                weather_news(Felt::Storm, Felt::Rain).unwrap(),
                weather_news(Felt::Rain, Felt::Clear).unwrap(),
            ]
        );
    }

    #[test]
    fn the_weather_a_match_starts_in_is_not_announced() {
        use WeatherKind::*;
        assert!(said(&[(Fog, 255), (Fog, 250)]).is_empty());
        assert_eq!(said(&[(Fog, 255), (Fog, 20)]), ["The fog has lifted"]);
    }

    #[test]
    fn a_new_kind_comes_in_after_the_old_fades() {
        use WeatherKind::*;
        // The sim fades snow out, then fades fog in.
        let steps = [(Snow, 200), (Snow, 30), (Fog, 30), (Fog, 120)];
        assert_eq!(
            said(&steps),
            [
                "The snow has stopped",
                weather_news(Felt::Clear, Felt::Fog).unwrap()
            ]
        );
    }

    fn burn(x: i32, y: i32) -> Burn {
        Burn {
            x,
            y,
            growth: Growth::Woods,
            left: 100,
            total: 100,
        }
    }

    #[test]
    fn fire_near_the_base_or_troops_is_announced_once() {
        let mut news = SkyNews::default();
        let troops = [(50.0, 50.0)];
        let base = [[10, 10, 3, 3]];
        let mut at = |tick, fires: &[Burn]| {
            news.fires(tick, fires, troops.iter().copied(), base.iter().copied())
        };
        let s = TICKS_PER_SECOND;
        // Far from both: nothing.
        assert_eq!(at(0, &[burn(100, 100)]), None);
        // Only once a second.
        assert_eq!(at(s + 1, &[burn(55, 52)]), None);
        assert_eq!(at(2 * s, &[burn(55, 52)]), Some(FIRE_NEWS[TROOPS]));
        // Still burning, or spreading: said once.
        assert_eq!(at(3 * s, &[burn(55, 52), burn(56, 52)]), None);
        // Out, then alight again soon after: still quiet.
        assert_eq!(at(4 * s, &[]), None);
        assert_eq!(at(5 * s, &[burn(55, 52)]), None);
        // Long after, near the base (which comes first).
        assert_eq!(at(60 * s, &[]), None);
        assert_eq!(
            at(61 * s, &[burn(22, 12), burn(55, 52)]),
            Some(FIRE_NEWS[BASE])
        );
    }

    #[test]
    fn a_fire_reaching_the_base_is_told_though_troops_were_warned() {
        let mut news = SkyNews::default();
        // Troops stand between the woods and the base.
        let troops = [(30.0, 12.0)];
        let base = [[10, 10, 3, 3]];
        let mut at = |tick, fires: &[Burn]| {
            news.fires(tick, fires, troops.iter().copied(), base.iter().copied())
        };
        let s = TICKS_PER_SECOND;
        assert_eq!(at(0, &[burn(38, 12)]), Some(FIRE_NEWS[TROOPS]));
        // It spreads towards the base, still near the troops.
        assert_eq!(at(s, &[burn(30, 12), burn(38, 12)]), None);
        assert_eq!(
            at(2 * s, &[burn(22, 12), burn(30, 12), burn(38, 12)]),
            Some(FIRE_NEWS[BASE])
        );
    }

    #[test]
    fn a_fire_lightning_lit_is_told_as_lightning() {
        let mut news = SkyNews::default();
        let troops = [(50.0, 50.0)];
        let base = [[10, 10, 3, 3]];
        let s = TICKS_PER_SECOND;
        // A bolt lights woods by the troops a tick after a whole second, as
        // the sim's do; the next look tells it as the bolt's.
        news.struck(52, 50, s + 1);
        let mut at = |tick, fires: &[Burn]| {
            news.fires(tick, fires, troops.iter().copied(), base.iter().copied())
        };
        assert_eq!(at(2 * s, &[burn(52, 50)]), Some(LIGHTNING_NEWS[TROOPS]));
        // A fire set by hand near the base meanwhile is no bolt's.
        assert_eq!(
            at(3 * s, &[burn(13, 12), burn(52, 50)]),
            Some(FIRE_NEWS[BASE])
        );
    }

    #[test]
    fn a_bolt_far_off_does_not_take_a_fire_near_the_troops() {
        let mut news = SkyNews::default();
        let troops = [(50.0, 50.0)];
        let s = TICKS_PER_SECOND;
        news.struck(200, 200, s + 1);
        let fires = [burn(52, 50), burn(200, 200)];
        let said = news.fires(2 * s, &fires, troops.iter().copied(), [].into_iter());
        assert_eq!(said, Some(FIRE_NEWS[TROOPS]));
    }
}
