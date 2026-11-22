//! What the network tests share: machines on a [`MemoryNet`], joining
//! and running them.
#![allow(dead_code)] // each test binary uses some of these

use std::time::Duration;

use gc_net::net::{MemoryNet, Net};
use gc_net::{Identity, Lan};
use gc_ui::lobby::{Faction, NetError, Profile};

/// How far the clock moves between looks at the network: finer than any
/// game's frame.
pub const STEP: Duration = Duration::from_millis(1);

pub fn id(version: &str) -> Identity {
    Identity {
        version: version.into(),
        data_hash: 42,
        map_hash: |_, _| 7,
    }
}

pub fn profile(name: &str) -> Profile {
    Profile {
        name: name.into(),
        color: 0,
        faction: Faction::EmberClans,
        version: "test".into(),
    }
}

pub fn lan_as(net: &MemoryNet, id: Identity) -> Lan {
    Lan::on(id, Box::new(net.clone()))
}

pub fn lan(net: &MemoryNet) -> Lan {
    lan_as(net, id("test"))
}

pub fn address(host: &Lan) -> String {
    format!("127.0.0.1:{}", host.port().unwrap())
}

/// `joiner` joins `host`, which answers meanwhile.
pub fn join(net: &MemoryNet, joiner: &mut Lan, name: &str, host: &mut Lan) -> Result<(), NetError> {
    let target = address(host);
    join_with(net, joiner, name, &target, "", host)
}

pub fn join_with(
    net: &MemoryNet,
    joiner: &mut Lan,
    name: &str,
    target: &str,
    password: &str,
    host: &mut Lan,
) -> Result<(), NetError> {
    joiner.start_join(&profile(name), target, password)?;
    loop {
        net.advance(STEP);
        host.pump(net.now());
        if let Some(done) = joiner.joining() {
            return done;
        }
    }
}

/// Run every machine for `time`.
pub fn pass(net: &MemoryNet, lans: &mut [&mut Lan], time: Duration) {
    let end = net.now() + time;
    while net.now() < end {
        net.advance(STEP);
        for l in lans.iter_mut() {
            l.pump(net.now());
        }
    }
}

/// Run every machine for a moment.
pub fn settle(net: &MemoryNet, lans: &mut [&mut Lan]) {
    pass(net, lans, Duration::from_millis(300));
}
