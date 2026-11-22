//! Order lag online: from the click to the tick that carries the order
//! out, on the machine that gave it, for the host and for a joiner, over
//! networks from one machine's loopback to a slow Tailscale relay.
//!
//! Each machine runs the game's frame loop (60 frames a second, as
//! `online_turns` in gc_game does it: pump, submit, then every due turn)
//! on the in-memory network's clock, so every run plays out the same way.
//! `cargo test -p gc_net --test order_lag -- --ignored --nocapture`
//! prints the table in docs/11-networking.md ("Order lag").

mod common;

use std::time::Duration;

use common::{STEP, join, lan, pass, profile};
use gc_fixed::{Fx, FxVec2, Pcg32};
use gc_net::net::{MemoryNet, Net};
use gc_net::{Lan, TURN};
use gc_sim::{Command, PlayerCommand, RegimentHandle};
use gc_ui::lobby::{LobbySettings, NetEvent, Network, SlotEdit};

const SEED: u64 = 0x6c61_6721;
/// A 60 Hz display's frame, rounded to the network clock's millisecond.
const FRAME: Duration = Duration::from_millis(17);

/// A host and one joiner on `net`, their match started and loaded.
fn two_machines(net: &MemoryNet) -> (Lan, Lan) {
    let mut host = lan(net);
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    let mut joiner = lan(net);
    join(net, &mut joiner, "Alex", &mut host).unwrap();
    let me = joiner.lobby().unwrap().local_slot;
    joiner.edit_slot(me, SlotEdit::Ready(true));
    pass(
        net,
        &mut [&mut host, &mut joiner],
        Duration::from_millis(1500),
    );
    host.start_match().unwrap();
    pass(
        net,
        &mut [&mut host, &mut joiner],
        Duration::from_millis(1500),
    );
    assert!(matches!(joiner.poll().as_slice(), [NetEvent::Started(_)]));
    host.loaded(Ok(()));
    joiner.loaded(Ok(()));
    (host, joiner)
}

/// A move order the test can find again by its brigade number.
fn order(player: u8, tag: u16) -> PlayerCommand {
    PlayerCommand {
        player,
        command: Command::MoveTo {
            regiment: RegimentHandle {
                index: 0,
                generation: 0,
            },
            target: FxVec2::from_ints(10, 10),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
            brigade: tag,
        },
    }
}

fn tag_of(c: &PlayerCommand) -> Option<u16> {
    match c.command {
        Command::MoveTo { brigade, .. } => Some(brigade),
        _ => None,
    }
}

/// One kind of link to measure on.
#[derive(Clone, Copy)]
struct Route {
    name: &'static str,
    /// One way, each packet: from this to that.
    one_way: (u64, u64),
    loss: u32,
    /// Halfway through, each packet takes this long instead (a slower
    /// route, or a busier one).
    later: Option<(u64, u64)>,
}

/// What a run found, in milliseconds.
#[derive(Debug)]
struct Lag {
    host: Vec<u64>,
    joiner: Vec<u64>,
    /// The input delay the match settled on, in turns.
    delay: u32,
    /// After the first five seconds of measuring, turn slots the joiner
    /// missed (a turn due and none come) and frames where it played two
    /// turns at once: its world hitched or jumped.
    hitches: u32,
}

struct Machine {
    lan: Lan,
    player: u8,
    /// Phase of this machine's frames against the other's.
    next_frame: Duration,
    /// Clicks waiting for the next frame, and orders on their way: tag
    /// and the click's time.
    clicked: Vec<(u16, Duration)>,
    sent: Vec<(u16, Duration)>,
    lags: Vec<u64>,
    last_turn: Option<Duration>,
    hitches: u32,
}

impl Machine {
    fn new(lan: Lan, player: u8, phase: Duration) -> Machine {
        Machine {
            lan,
            player,
            next_frame: phase,
            clicked: Vec::new(),
            sent: Vec::new(),
            lags: Vec::new(),
            last_turn: None,
            hitches: 0,
        }
    }

    fn frame(&mut self, now_net: std::time::Instant, now: Duration, measuring: bool) {
        self.lan.pump(now_net);
        let orders: Vec<PlayerCommand> = self
            .clicked
            .iter()
            .map(|&(t, _)| order(self.player, t))
            .collect();
        self.sent.append(&mut self.clicked);
        self.lan.submit(orders);
        while let Some(turn) = self.lan.next_turn(now_net) {
            if measuring {
                // Frames come every 17 ms and turns every 50, so a gap of
                // four frames or more missed a turn slot for each 50 ms
                // over 17; two turns in one frame jump.
                let gap = self.last_turn.map_or(Duration::ZERO, |last| now - last);
                if gap >= FRAME * 4 {
                    self.hitches += ((gap - FRAME) / TURN.as_millis() as u32).as_millis() as u32;
                } else if gap.is_zero() && self.last_turn.is_some() {
                    self.hitches += 1;
                }
            }
            self.last_turn = Some(now);
            for c in &turn {
                let Some(tag) = tag_of(c) else { continue };
                if let Some(i) = self.sent.iter().position(|&(t, _)| t == tag) {
                    let (_, at) = self.sent.swap_remove(i);
                    self.lags.push((now - at).as_millis() as u64);
                }
            }
        }
    }
}

/// Play `seconds` of match on `network`, each machine clicking an order
/// about every 700 ms at a moment that doesn't line up with its frames,
/// after a warm-up that lets the input delay settle.
fn measure(network: Route, seconds: u64) -> Lag {
    let net = MemoryNet::new(SEED);
    let latency = |(lo, hi)| net.set_latency(Duration::from_millis(lo), Duration::from_millis(hi));
    latency(network.one_way);
    let (host, joiner) = two_machines(&net);
    net.set_loss(network.loss);
    let start = net.now();
    let mut machines = [
        Machine::new(host, 0, Duration::ZERO),
        Machine::new(joiner, 1, Duration::from_millis(7)),
    ];
    let warm_up = Duration::from_secs(8);
    let end = warm_up + Duration::from_secs(seconds);
    let mut rng = Pcg32::new(SEED, 0);
    let mut next_click = [
        warm_up + Duration::from_millis(3),
        warm_up + Duration::from_millis(11),
    ];
    let mut tag: u16 = 1;
    let mut changed = false;
    loop {
        net.advance(STEP);
        let now = net.now() - start;
        if let Some(later) = network.later
            && !changed
            && now >= warm_up + Duration::from_secs(seconds / 2)
        {
            latency(later);
            changed = true;
        }
        if now > end + Duration::from_secs(2) {
            break;
        }
        for (i, m) in machines.iter_mut().enumerate() {
            if now >= next_click[i] && now < end {
                m.clicked.push((tag, now));
                tag = tag.wrapping_add(1).max(1);
                next_click[i] = now + Duration::from_millis(400 + u64::from(rng.below(600)));
            }
            if now >= m.next_frame {
                m.next_frame += FRAME;
                m.frame(net.now(), now, now >= warm_up + Duration::from_secs(5));
            }
        }
    }
    let [host, joiner] = machines;
    assert!(
        host.sent.is_empty() && joiner.sent.is_empty(),
        "orders lost"
    );
    Lag {
        delay: host.lan.input_delay().unwrap(),
        host: host.lags,
        joiner: joiner.lags,
        hitches: joiner.hitches,
    }
}

const ROUTES: [Route; 7] = [
    Route {
        name: "Same computer",
        one_way: (0, 1),
        loss: 0,
        later: None,
    },
    Route {
        name: "Home Wi-Fi",
        one_way: (2, 8),
        loss: 1,
        later: None,
    },
    Route {
        name: "Tailscale, same city",
        one_way: (8, 16),
        loss: 0,
        later: None,
    },
    Route {
        name: "Tailscale, across NZ",
        one_way: (15, 30),
        loss: 1,
        later: None,
    },
    Route {
        name: "Tailscale relay (Sydney)",
        one_way: (40, 70),
        loss: 1,
        later: None,
    },
    Route {
        name: "Poor link",
        one_way: (80, 160),
        loss: 3,
        later: None,
    },
    Route {
        name: "Across NZ, then relayed",
        one_way: (15, 30),
        loss: 1,
        later: Some((40, 70)),
    },
];

fn percentile(v: &[u64], p: usize) -> u64 {
    let mut s = v.to_vec();
    s.sort_unstable();
    s[(s.len() - 1) * p / 100]
}

fn mean(v: &[u64]) -> u64 {
    v.iter().sum::<u64>() / v.len() as u64
}

/// Orders keep coming through about as fast as measured when this was
/// built (docs/11-networking.md, "Order lag"), with the joiner's world
/// running smoothly.
#[test]
fn orders_come_through_quickly_over_tailscale() {
    for (name, host, joiner) in [
        ("Tailscale, across NZ", 100, 170),
        ("Tailscale relay (Sydney)", 110, 260),
    ] {
        let n = ROUTES.iter().find(|n| n.name == name).unwrap();
        let lag = measure(*n, 60);
        assert!(mean(&lag.host) <= host, "{name}: host {:?}", lag.host);
        assert!(
            mean(&lag.joiner) <= joiner,
            "{name}: joiner {:?}",
            lag.joiner
        );
        assert!(lag.hitches <= 3, "{name}: {} hitches", lag.hitches);
    }
}

#[test]
#[ignore = "prints the order lag table; run by hand"]
fn order_lag_table() {
    println!("| Link | One way | Delay | Host mean / p95 | Joiner mean / p95 | Hitches |");
    println!("|---|---|---|---|---|---|");
    for n in ROUTES {
        let lag = measure(n, 120);
        println!(
            "| {} | {}-{} ms | {} turns | {} / {} ms | {} / {} ms | {} |",
            n.name,
            n.one_way.0,
            n.one_way.1,
            lag.delay,
            mean(&lag.host),
            percentile(&lag.host, 95),
            mean(&lag.joiner),
            percentile(&lag.joiner, 95),
            lag.hitches,
        );
    }
}
