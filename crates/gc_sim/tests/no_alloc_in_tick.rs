//! The simulation must not touch the heap while ticking (docs/12-performance.md,
//! PRF-5). A counting allocator wraps the system allocator for this test binary.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use gc_sim::scenario::Drill;
use gc_sim::{Capacities, MatchSetup, World};

struct Counting;

static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn ticking_never_allocates() {
    let mut world = World::new(&MatchSetup {
        seed: 7,
        capacities: Capacities::default(),
    });
    let mut drill = Drill::new(7);
    let mut commands = Vec::with_capacity(512);
    for _ in 0..1_000 {
        drill.commands_for(&world, &mut commands);
        COUNTING.store(true, Ordering::SeqCst);
        world.tick(&commands);
        let _ = world.state_hash();
        COUNTING.store(false, Ordering::SeqCst);
    }
    assert_eq!(
        ALLOCS.load(Ordering::SeqCst),
        0,
        "World::tick or state_hash allocated"
    );
}
