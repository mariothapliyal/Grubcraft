//! The news line's word on the weather and on fires near the viewer's side
//! (docs/41, step 4): a storm, fog or snow coming in and clearing, and woods
//! catching fire near its troops or base. Read from the world after each
//! tick; it changes nothing in play.

use gc_sim::sky::{Weather, WeatherKind};
use gc_sim::{Burn, TICKS_PER_SECOND};

/// The weather as troops feel it, with some give either way, so weather
/// fading in and out a step at a time is announced once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Felt {
    #[default]
    Clear,
    Rain,
    Storm,
    Snow,
    Fog,
}

/// From this strength weather has come in; under [`OUT`] it has cleared.
const IN: u8 = 100;
const OUT: u8 = 40;
/// Rain from this strength is a storm; it is rain again under [`CALMER`].
const STORM: u8 = 200;
const CALMER: u8 = 160;

impl Felt {
    /// How `weather` feels to troops who felt `self` until now.
    pub(crate) fn next(self, weather: Weather) -> Felt {
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
pub(crate) fn weather_news(was: Felt, now: Felt) -> Option<&'static str> {
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
pub(crate) const FIRE_NEAR: i32 = 10;
/// Fires are looked for once a second (the sim's fires spread slower).
const FIRE_EVERY: u32 = TICKS_PER_SECOND;
/// A fire is announced when it first comes near, and not again for this
/// long, so a blaze flickering at the edge isn't announced over and over.
const FIRE_QUIET: u32 = 45 * TICKS_PER_SECOND;

/// What a fire is near.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Near {
    Troops,
    Base,
}

/// Watches the weather and the fires for the news line.
#[derive(Debug, Default)]
pub(crate) struct SkyNews {
    /// The weather felt so far; `None` until the first look, so the
    /// weather a match starts in isn't announced.
    felt: Option<Felt>,
    /// Whether a fire was near last time, and the tick of the last warning.
    fire_near: bool,
    warned: Option<u32>,
}

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

    /// The fires' news at `tick`: woods newly burning near `side`'s
    /// regiments (`troops`, in tiles) or its buildings (`base`: each
    /// footprint's corner tile and size).
    pub(crate) fn fires(
        &mut self,
        tick: u32,
        fires: &[Burn],
        troops: impl Iterator<Item = (f32, f32)>,
        base: impl Iterator<Item = ((i32, i32), (i32, i32))>,
    ) -> Option<&'static str> {
        if !tick.is_multiple_of(FIRE_EVERY) {
            return None;
        }
        let near = fire_near(fires, troops, base);
        let was = std::mem::replace(&mut self.fire_near, near.is_some());
        let near = near?;
        if was || self.warned.is_some_and(|t| tick < t + FIRE_QUIET) {
            return None;
        }
        self.warned = Some(tick);
        Some(match near {
            Near::Base => "Fire! The woods are burning near your base",
            Near::Troops => "Fire! The woods are burning near your troops",
        })
    }
}

/// What, if anything, a burning tile is near: the base before the troops.
pub(crate) fn fire_near(
    fires: &[Burn],
    mut troops: impl Iterator<Item = (f32, f32)>,
    mut base: impl Iterator<Item = ((i32, i32), (i32, i32))>,
) -> Option<Near> {
    if fires.is_empty() {
        return None;
    }
    let by_base = base.any(|((x, y), (w, d))| {
        fires.iter().any(|b| {
            b.x >= x - FIRE_NEAR
                && b.x < x + w + FIRE_NEAR
                && b.y >= y - FIRE_NEAR
                && b.y < y + d + FIRE_NEAR
        })
    });
    if by_base {
        return Some(Near::Base);
    }
    let r = FIRE_NEAR as f32;
    troops
        .any(|(x, y)| {
            fires.iter().any(|b| {
                let (dx, dy) = (b.x as f32 + 0.5 - x, b.y as f32 + 0.5 - y);
                dx * dx + dy * dy <= r * r
            })
        })
        .then_some(Near::Troops)
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
        let base = [((10, 10), (3, 3))];
        let mut at = |tick, fires: &[Burn]| {
            news.fires(tick, fires, troops.iter().copied(), base.iter().copied())
        };
        let s = TICKS_PER_SECOND;
        // Far from both: nothing.
        assert_eq!(at(0, &[burn(100, 100)]), None);
        // Only once a second.
        assert_eq!(at(s + 1, &[burn(55, 52)]), None);
        assert_eq!(
            at(2 * s, &[burn(55, 52)]),
            Some("Fire! The woods are burning near your troops")
        );
        // Still burning, or spreading: said once.
        assert_eq!(at(3 * s, &[burn(55, 52), burn(56, 52)]), None);
        // Out, then alight again soon after: still quiet.
        assert_eq!(at(4 * s, &[]), None);
        assert_eq!(at(5 * s, &[burn(55, 52)]), None);
        // Long after, near the base (which comes first).
        assert_eq!(at(60 * s, &[]), None);
        assert_eq!(
            at(61 * s, &[burn(22, 12), burn(55, 52)]),
            Some("Fire! The woods are burning near your base")
        );
    }
}
