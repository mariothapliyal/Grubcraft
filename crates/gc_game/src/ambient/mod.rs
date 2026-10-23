//! The living world around the armies (docs/34-ambient-life.md): the work
//! of clearing land, and in time wildlife, villagers and busy bases. All of
//! it is a picture. It reads the simulation's world and events and draws,
//! and nothing here feeds back into the simulation, replays or the network,
//! so it may use floats, frame time and its own random numbers.

pub mod felling;

pub use felling::Felling;

/// Everything ambient the game keeps between frames.
#[derive(Default)]
pub struct Ambient {
    pub felling: Felling,
    /// Seconds until `--fell-demo` fells the trees round the camera.
    pub fell_demo: Option<f32>,
}

impl Ambient {
    pub fn new(fell_demo: bool) -> Ambient {
        Ambient {
            felling: Felling::default(),
            fell_demo: fell_demo.then_some(FELL_DEMO_AFTER),
        }
    }
}

/// How long `--fell-demo` waits before it fells, in seconds.
const FELL_DEMO_AFTER: f32 = 4.0;
