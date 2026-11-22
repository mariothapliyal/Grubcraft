//! Order lag online: from the click to the tick that carries the order
//! out, on the machine that gave it, for the host and for a joiner, over
//! networks from one machine's loopback to a slow Tailscale relay.
//!
//! Each machine runs the game's frame loop (60 frames a second, as
//! `online_turns` in gc_game does it: pump, submit, then every due turn)
//! on the in-memory network's clock, so every run plays out the same way.
//! `cargo test -p gc_net --test order_lag -- --ignored --nocapture`
//! prints the table in docs/11-networking.md ("Order lag").

use std::time::Duration;

use gc_fixed::{Fx, FxVec2};
use gc_net::net::{MemoryNet, Net};
use gc_net::{Identity, Lan};
use gc_sim::{Command, PlayerCommand, RegimentHandle};
use gc_ui::lobby::{Faction, LobbySettings, NetEvent, Network, Profile, SlotEdit};

const SEED: u64 = 0x6c61_6721;
const STEP: Duration = Duration::from_millis(1);
/// A 60 Hz display's frame, rounded to the network clock's millisecond.
const FRAME: Duration = Duration::from_millis(17);

fn profile(name: &str) -> Profile {
    Profile {
        name: name.into(),
        color: 0,
        faction: Faction::EmberClans,
        version: "test".into(),
    }
}

fn lan(net: &MemoryNet) -> Lan {
    let id = Identity {
        version: "test".into(),
        data_hash: 42,
        map_hash: |_, _| 7,
    };
    Lan::on(id, Box::new(net.clone()))
}

fn pass(net: &MemoryNet, lans: &mut [&mut Lan], time: Duration) {
    let end = net.now() + time;
    while net.now() < end {
        net.advance(STEP);
        for l in lans.iter_mut() {
            l.pump(net.now());
        }
    }
}

/// A host and one joiner on `net`, their match started and loaded.
fn two_machines(net: &MemoryNet) -> (Lan, Lan) {
    let mut host = lan(net);
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    let mut joiner = lan(net);
    let target = format!("127.0.0.1:{}", host.port().unwrap());
    joiner.start_join(&profile("Alex"), &target, "").unwrap();
    while joiner.joining().is_none() {
        net.advance(STEP);
        host.pump(net.now());
    }
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

/// One network to measure on.
#[derive(Clone, Copy)]
pub struct Link {
    pub name: &'static str,
    /// One way, each packet: from this to that.
    pub one_way: (u64, u64),
    pub loss: u32,
    /// Halfway through, each packet takes this long instead (a slower
    /// route, or a busier one).
    pub later: Option<(u64, u64)>,
}

/// What a run found, in milliseconds.
#[derive(Debug)]
pub struct Lag {
    pub host: Vec<u64>,
    pub joiner: Vec<u64>,
    /// The input delay the match settled on, in turns.
    pub delay: u32,
    /// Frames, after the first five seconds of measuring, where the
    /// joiner had a turn due but none had come: its world hitched.
    pub hitches: u32,
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
            // Frames come every 17 ms and turns every 50: a gap of four
            // frames means one went by with a turn due and none to play.
            if let Some(last) = self.last_turn
                && measuring
                && now - last >= FRAME * 4
            {
                self.hitches += 1;
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
pub fn measure(network: Link, seconds: u64) -> Lag {
    let net = MemoryNet::new(SEED);
    let latency = |(lo, hi)| net.set_latency(Duration::from_millis(lo), Duration::from_millis(hi));
    latency(network.one_way);
    let (host, joiner) = two_machines(&net);
    net.set_loss(network.loss);
    let start = net.now();
    let mut machines = [
        Machine {
            lan: host,
            player: 0,
            next_frame: Duration::ZERO,
            clicked: Vec::new(),
            sent: Vec::new(),
            lags: Vec::new(),
            last_turn: None,
            hitches: 0,
        },
        Machine {
            lan: joiner,
            player: 1,
            next_frame: Duration::from_millis(7),
            clicked: Vec::new(),
            sent: Vec::new(),
            lags: Vec::new(),
            last_turn: None,
            hitches: 0,
        },
    ];
    let warm_up = Duration::from_secs(8);
    let end = warm_up + Duration::from_secs(seconds);
    let mut rng = SEED;
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
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                next_click[i] = now + Duration::from_millis(400 + rng % 600);
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

pub const NETWORKS: [Link; 7] = [
    Link {
        name: "Same computer",
        one_way: (0, 1),
        loss: 0,
        later: None,
    },
    Link {
        name: "Home Wi-Fi",
        one_way: (2, 8),
        loss: 1,
        later: None,
    },
    Link {
        name: "Tailscale, same city",
        one_way: (8, 16),
        loss: 0,
        later: None,
    },
    Link {
        name: "Tailscale, across NZ",
        one_way: (15, 30),
        loss: 1,
        later: None,
    },
    Link {
        name: "Tailscale relay (Sydney)",
        one_way: (40, 70),
        loss: 1,
        later: None,
    },
    Link {
        name: "Poor link",
        one_way: (80, 160),
        loss: 3,
        later: None,
    },
    Link {
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
        let n = NETWORKS.iter().find(|n| n.name == name).unwrap();
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
    println!("| Network | One way | Delay | Host mean / p95 | Joiner mean / p95 | Hitches |");
    println!("|---|---|---|---|---|---|");
    for n in NETWORKS {
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
