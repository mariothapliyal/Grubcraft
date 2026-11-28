//! Work kept between frames: done again once what it depends on has held
//! still for two frames running, and reused while it still holds. The kept
//! shadow map (`StillKey`) and the kept ground (`GroundKey`) both use it.

/// What a frame does with the kept work.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Use {
    /// Do the work as if nothing were kept.
    None,
    /// Do it and keep it.
    Keep,
    /// Reuse what was kept.
    Kept,
}

/// The key the kept work was done for, and last frame's key. Waiting for a
/// key to hold for a frame means that while it changes every frame (a
/// moving camera) no frame pays to keep work it will not reuse.
pub(crate) struct Settle<K> {
    seen: Option<K>,
    kept: Option<K>,
}

impl<K> Default for Settle<K> {
    fn default() -> Self {
        Settle {
            seen: None,
            kept: None,
        }
    }
}

impl<K: Copy + PartialEq> Settle<K> {
    /// What a frame whose work depends on `key` does. `ready` is false when
    /// it cannot keep anything this frame (a frame that draws none of it).
    pub fn plan(&mut self, key: K, ready: bool) -> Use {
        let settled = self.seen == Some(key);
        self.seen = Some(key);
        if !ready {
            Use::None
        } else if self.kept == Some(key) {
            Use::Kept
        } else if settled {
            self.kept = Some(key);
            Use::Keep
        } else {
            Use::None
        }
    }

    /// What was kept is gone.
    pub fn forget(&mut self) {
        *self = Settle::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_once_the_key_settles_and_reused_after() {
        let mut s = Settle::default();
        // Changing every frame: never kept.
        for x in 0..4 {
            assert_eq!(s.plan(x, true), Use::None);
        }
        // Holding: the second frame keeps, the rest reuse.
        assert_eq!(s.plan(3, true), Use::Keep);
        assert_eq!(s.plan(3, true), Use::Kept);
        assert_eq!(s.plan(3, true), Use::Kept);
        // A nudge and back: what was kept is still good.
        assert_eq!(s.plan(9, true), Use::None);
        assert_eq!(s.plan(3, true), Use::Kept);
        // A frame that cannot keep keeps nothing, and the next is not
        // settled by it.
        assert_eq!(s.plan(5, false), Use::None);
        assert_eq!(s.plan(6, true), Use::None);
        assert_eq!(s.plan(6, true), Use::Keep);
        s.forget();
        assert_eq!(s.plan(6, true), Use::None);
    }
}
