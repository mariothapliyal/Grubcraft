//! Shared skeletons and their tagged moves (see
//! `docs/33-animation-engine.md`). Optional tables in a model pack: a pack
//! without them (the building packs, and model packs baked before them)
//! loads as before, its models posed from their own bone rows
//! ([`crate::model`]).
//!
//! - `anim/skeletons`: one [`SkeletonRecord`] per skeleton.
//! - `anim/joints`: every skeleton's [`JointRecord`]s, back to back, in the
//!   order of the bones its models' vertices name.
//! - `anim/moves`: one [`MoveRecord`] per move, each on one skeleton.
//! - `anim/frames`: the joint pose texture, [`FRAME_TEXELS`] texels per row,
//!   one row per frame of every move. Each joint takes two texels of four
//!   floats: its rotation (a unit quaternion, x y z w), then its translation
//!   and uniform scale, both relative to its parent.
//! - `anim/fits`: one [`FitRecord`] per model posed from a skeleton, sorted
//!   by model.
//! - `anim/fixes`: per fitted model, the matrix that turns the skeleton's
//!   space into the model's (its size and turn), then one per bone that turns
//!   the model's stored vertices into that bone's rest (its bind pose
//!   difference), each the three rows of a 3 × 4 matrix.

use alloc::vec::Vec;
use core::fmt;

use bytemuck::{Pod, Zeroable};

use crate::model::{MAX_BONES, Models, NAME_BYTES};
use crate::{Pack, TableError, read_table};

/// Bumped whenever a record's layout or meaning changes.
pub const ANIM_SCHEMA: u32 = 1;
pub const SKELETONS_PATH: &str = "anim/skeletons";
pub const JOINTS_PATH: &str = "anim/joints";
pub const MOVES_PATH: &str = "anim/moves";
pub const FRAMES_PATH: &str = "anim/frames";
pub const FITS_PATH: &str = "anim/fits";
pub const FIXES_PATH: &str = "anim/fixes";

/// Most joints a skeleton may have: a model's bones, less the still one.
pub const MAX_JOINTS: u32 = MAX_BONES;
/// Texels in a frame row: two per joint.
pub const FRAME_TEXELS: u32 = 2 * (MAX_JOINTS + 1);
/// Floats in one fix (a 3 × 4 matrix, row by row).
pub const FIX_FLOATS: usize = 12;
/// Joint sets a move may be limited to (see [`Mask`]).
pub const MASKS: usize = 8;
/// No parent: a root joint.
pub const ROOT: u8 = u8::MAX;

/// The parts of a body a layered move can play on, by joint set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Mask {
    /// Every joint.
    All = 0,
    /// The hips and legs: footwork under a swing.
    Legs = 1,
    /// From the waist up.
    Upper = 2,
    /// The neck and head.
    Head = 3,
    /// The left arm, from the shoulder.
    LeftArm = 4,
    /// The right arm, from the shoulder.
    RightArm = 5,
}

impl Mask {
    pub const ALL: [Mask; 6] = [
        Mask::All,
        Mask::Legs,
        Mask::Upper,
        Mask::Head,
        Mask::LeftArm,
        Mask::RightArm,
    ];

    pub fn from_u8(v: u8) -> Option<Mask> {
        Mask::ALL.get(v as usize).copied()
    }
}

/// What a joint is to the engine: the joints look-at turns and foot IK
/// bends. Found by the baker from the joint names (Quaternius and Mixamo
/// names), or set in the model's config.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Role {
    None = 0,
    Hips = 1,
    Spine = 2,
    Chest = 3,
    Neck = 4,
    Head = 5,
    UpperLegL = 6,
    LowerLegL = 7,
    FootL = 8,
    UpperLegR = 9,
    LowerLegR = 10,
    FootR = 11,
    ShoulderL = 12,
    ShoulderR = 13,
}

impl Role {
    pub const ALL: [Role; 14] = [
        Role::None,
        Role::Hips,
        Role::Spine,
        Role::Chest,
        Role::Neck,
        Role::Head,
        Role::UpperLegL,
        Role::LowerLegL,
        Role::FootL,
        Role::UpperLegR,
        Role::LowerLegR,
        Role::FootR,
        Role::ShoulderL,
        Role::ShoulderR,
    ];

    pub fn from_u8(v: u8) -> Option<Role> {
        Role::ALL.get(v as usize).copied()
    }
}

/// One skeleton. 80 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct SkeletonRecord {
    /// Zero-padded UTF-8: the rig's name, for tools.
    pub name: [u8; NAME_BYTES],
    pub first_joint: u32,
    pub joints: u32,
    /// Per [`Mask`], the joints it covers, one bit each (low word first).
    pub masks: [[u32; 2]; MASKS],
}

/// One joint. 4 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct JointRecord {
    /// The parent joint, or [`ROOT`].
    pub parent: u8,
    /// Joints between this one and its root (a root is 0).
    pub depth: u8,
    /// The joint that mirrors this one across the body (itself on the
    /// middle line).
    pub mirror: u8,
    /// See [`Role`].
    pub role: u8,
}

/// What a move is for: the fight picks moves by these, falling back to the
/// model's plain clip kinds when its skeleton has none tagged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Act {
    Idle = 0,
    Walk = 1,
    Run = 2,
    Guard = 3,
    Attack = 4,
    Parry = 5,
    Block = 6,
    Hit = 7,
    Stagger = 8,
    Knockdown = 9,
    GetUp = 10,
    Death = 11,
    Cheer = 12,
    Shoot = 13,
    /// A step in a fight: forward, back or to a side (by [`Side`]).
    Step = 14,
    /// A flinch played on top of whatever else (an additive layer).
    Flinch = 15,
}

impl Act {
    pub const ALL: [Act; 16] = [
        Act::Idle,
        Act::Walk,
        Act::Run,
        Act::Guard,
        Act::Attack,
        Act::Parry,
        Act::Block,
        Act::Hit,
        Act::Stagger,
        Act::Knockdown,
        Act::GetUp,
        Act::Death,
        Act::Cheer,
        Act::Shoot,
        Act::Step,
        Act::Flinch,
    ];

    pub fn from_u8(v: u8) -> Option<Act> {
        Act::ALL.get(v as usize).copied()
    }
}

/// Which way a move goes or a blow came from, seen by the man playing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Side {
    Any = 0,
    Front = 1,
    Back = 2,
    Left = 3,
    Right = 4,
    High = 5,
    Low = 6,
}

impl Side {
    pub const ALL: [Side; 7] = [
        Side::Any,
        Side::Front,
        Side::Back,
        Side::Left,
        Side::Right,
        Side::High,
        Side::Low,
    ];

    pub fn from_u8(v: u8) -> Option<Side> {
        Side::ALL.get(v as usize).copied()
    }

    /// The side seen in a mirror.
    pub fn mirrored(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
            s => s,
        }
    }
}

/// The weapon a move is made for (`Any` for the body's own moves: walking,
/// dying, a Brood claw).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Weapon {
    Any = 0,
    SwordShield = 1,
    Spear = 2,
    TwoHanded = 3,
    Polearm = 4,
    AxeMace = 5,
    Bow = 6,
    Claws = 7,
}

impl Weapon {
    pub const ALL: [Weapon; 8] = [
        Weapon::Any,
        Weapon::SwordShield,
        Weapon::Spear,
        Weapon::TwoHanded,
        Weapon::Polearm,
        Weapon::AxeMace,
        Weapon::Bow,
        Weapon::Claws,
    ];

    pub fn from_u8(v: u8) -> Option<Weapon> {
        Weapon::ALL.get(v as usize).copied()
    }
}

/// [`MoveRecord::flags`]: the move repeats (walking) rather than playing
/// once and holding (dying).
pub const LOOPS: u8 = 1;
/// [`MoveRecord::flags`]: the move is stored as a change from its first
/// frame and played on top of another.
pub const ADDITIVE: u8 = 2;
/// [`MoveRecord::flags`]: the move may be played mirrored (its skeleton is
/// symmetric).
pub const MIRRORS: u8 = 4;

/// No plain clip kind: [`MoveRecord::kind`] of a move that stands in for
/// none.
pub const NO_KIND: u8 = u8::MAX;

/// One move on one skeleton. 48 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct MoveRecord {
    /// Zero-padded UTF-8: the source animation's name.
    pub name: [u8; NAME_BYTES],
    pub skeleton: u16,
    /// The plain [`ClipKind`](crate::model::ClipKind) this move stands in
    /// for, or [`NO_KIND`].
    pub kind: u8,
    /// See [`Act`], [`Weapon`], [`Side`] and [`Mask`].
    pub act: u8,
    pub weapon: u8,
    pub side: u8,
    pub mask: u8,
    /// [`LOOPS`], [`ADDITIVE`], [`MIRRORS`].
    pub flags: u8,
    /// Frame texture row of the first frame.
    pub first_row: u32,
    pub frames: u32,
    /// Frames per second the move was sampled at.
    pub fps: f32,
}

impl MoveRecord {
    pub fn name(&self) -> &str {
        let len = self.name.iter().position(|&b| b == 0).unwrap_or(NAME_BYTES);
        core::str::from_utf8(&self.name[..len]).unwrap_or("?")
    }

    pub fn seconds(&self) -> f32 {
        self.frames as f32 / self.fps
    }

    pub fn loops(&self) -> bool {
        self.flags & LOOPS != 0
    }

    pub fn additive(&self) -> bool {
        self.flags & ADDITIVE != 0
    }
}

/// A model posed from a skeleton. 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct FitRecord {
    pub model: u32,
    pub skeleton: u32,
    /// The first of the model's fixes: its skeleton-to-model matrix, then one
    /// per bone (the model's bones, the still bone last).
    pub first_fix: u32,
    pub _pad: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimError {
    Table(&'static str, TableError),
    /// Some tables are there and some are not.
    Partial,
    /// A skeleton, joint, move or fit points outside its tables or breaks
    /// their rules.
    Bad(&'static str, usize),
}

impl fmt::Display for AnimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnimError::Table(p, e) => write!(f, "{p}: {e}"),
            AnimError::Partial => write!(f, "model pack has only some of the anim tables"),
            AnimError::Bad(what, k) => write!(f, "anim {what} {k} is malformed"),
        }
    }
}

impl core::error::Error for AnimError {}

/// The skeletons and moves in a pack, borrowed in place.
#[derive(Clone, Copy, Debug, Default)]
pub struct Anim<'a> {
    pub skeletons: &'a [SkeletonRecord],
    pub joints: &'a [JointRecord],
    pub moves: &'a [MoveRecord],
    /// Joint pose texels, four floats each, row by row.
    pub frames: &'a [[f32; 4]],
    pub fits: &'a [FitRecord],
    pub fixes: &'a [[f32; FIX_FLOATS]],
}

impl<'a> Anim<'a> {
    /// Read and check every table, against the pack's `models`. A pack with
    /// none of them has no skeletons (an empty `Anim`).
    pub fn from_pack(pack: &Pack<'a>, models: &Models<'_>) -> Result<Anim<'a>, AnimError> {
        const PATHS: [&str; 6] = [
            SKELETONS_PATH,
            JOINTS_PATH,
            MOVES_PATH,
            FRAMES_PATH,
            FITS_PATH,
            FIXES_PATH,
        ];
        let present = PATHS.iter().filter(|p| pack.get_path(p).is_some()).count();
        if present == 0 {
            return Ok(Anim::default());
        }
        if present < PATHS.len() {
            return Err(AnimError::Partial);
        }
        fn table<'a, T: Pod>(pack: &Pack<'a>, path: &'static str) -> Result<&'a [T], AnimError> {
            let entry = pack.get_path(path).ok_or(AnimError::Partial)?;
            read_table(ANIM_SCHEMA, entry.bytes).map_err(|e| AnimError::Table(path, e))
        }
        let a = Anim {
            skeletons: table(pack, SKELETONS_PATH)?,
            joints: table(pack, JOINTS_PATH)?,
            moves: table(pack, MOVES_PATH)?,
            frames: table(pack, FRAMES_PATH)?,
            fits: table(pack, FITS_PATH)?,
            fixes: table(pack, FIXES_PATH)?,
        };
        a.check(models)?;
        Ok(a)
    }

    fn check(&self, models: &Models<'_>) -> Result<(), AnimError> {
        let rows = self.frame_rows() as usize;
        if self.frames.len() != rows * FRAME_TEXELS as usize {
            return Err(AnimError::Bad("frames", 0));
        }
        for (k, s) in self.skeletons.iter().enumerate() {
            let bad = AnimError::Bad("skeleton", k);
            let end = s.first_joint as usize + s.joints as usize;
            let Some(joints) = self.joints.get(s.first_joint as usize..end) else {
                return Err(bad);
            };
            if s.joints == 0 || s.joints > MAX_JOINTS {
                return Err(bad);
            }
            // A joint is one deeper than its parent, so parents never loop
            // and posing depth by depth reaches every parent first.
            for joint in joints {
                let depth_ok = if joint.parent == ROOT {
                    joint.depth == 0
                } else {
                    joints
                        .get(joint.parent as usize)
                        .is_some_and(|p| p.depth.checked_add(1) == Some(joint.depth))
                };
                if !depth_ok
                    || joint.mirror as usize >= joints.len()
                    || Role::from_u8(joint.role).is_none()
                {
                    return Err(bad);
                }
            }
        }
        for (k, m) in self.moves.iter().enumerate() {
            if m.skeleton as usize >= self.skeletons.len()
                || Act::from_u8(m.act).is_none()
                || Weapon::from_u8(m.weapon).is_none()
                || Side::from_u8(m.side).is_none()
                || Mask::from_u8(m.mask).is_none()
                || (m.kind != NO_KIND && crate::model::ClipKind::from_u8(m.kind).is_none())
                || m.frames == 0
                || m.fps.is_nan()
                || m.fps <= 0.0
                || m.first_row as usize + m.frames as usize > rows
            {
                return Err(AnimError::Bad("move", k));
            }
        }
        for (k, f) in self.fits.iter().enumerate() {
            let bad = AnimError::Bad("fit", k);
            if k > 0 && self.fits[k - 1].model >= f.model {
                return Err(bad);
            }
            let (Some(model), Some(skeleton)) = (
                models.models.get(f.model as usize),
                self.skeletons.get(f.skeleton as usize),
            ) else {
                return Err(bad);
            };
            // The model's bones are the skeleton's joints plus the still one.
            if model.bones != skeleton.joints + 1
                || f.first_fix as usize + 1 + model.bones as usize > self.fixes.len()
            {
                return Err(bad);
            }
        }
        Ok(())
    }

    /// Rows in the frame texture.
    pub fn frame_rows(&self) -> u32 {
        self.frames.len() as u32 / FRAME_TEXELS
    }

    /// The fit of `model`, if it is posed from a skeleton.
    pub fn fit(&self, model: usize) -> Option<&'a FitRecord> {
        let k = self
            .fits
            .binary_search_by_key(&(model as u32), |f| f.model)
            .ok()?;
        Some(&self.fits[k])
    }

    /// The joints of skeleton `s`.
    pub fn joints_of(&self, s: usize) -> &'a [JointRecord] {
        let r = &self.skeletons[s];
        &self.joints[r.first_joint as usize..(r.first_joint + r.joints) as usize]
    }

    /// The moves on skeleton `s`.
    pub fn moves_of(&self, s: usize) -> impl Iterator<Item = (usize, &'a MoveRecord)> + 'a {
        let moves = self.moves;
        moves
            .iter()
            .enumerate()
            .filter(move |(_, m)| m.skeleton as usize == s)
    }
}

/// Everything the anim tables hold, for writing them (or joining packs).
#[derive(Default)]
pub struct AnimTables {
    pub skeletons: Vec<SkeletonRecord>,
    pub joints: Vec<JointRecord>,
    pub moves: Vec<MoveRecord>,
    pub frames: Vec<[f32; 4]>,
    pub fits: Vec<FitRecord>,
    pub fixes: Vec<[f32; FIX_FLOATS]>,
}

impl AnimTables {
    /// Add `a`, whose fits name models counted from `first_model` here.
    pub fn append(&mut self, a: &Anim<'_>, first_model: u32) {
        let (skeletons, joints, rows, fixes) = (
            self.skeletons.len() as u32,
            self.joints.len() as u32,
            (self.frames.len() / FRAME_TEXELS as usize) as u32,
            self.fixes.len() as u32,
        );
        self.skeletons.extend(a.skeletons.iter().map(|s| {
            let mut s = *s;
            s.first_joint += joints;
            s
        }));
        self.joints.extend_from_slice(a.joints);
        self.moves.extend(a.moves.iter().map(|m| {
            let mut m = *m;
            m.skeleton += skeletons as u16;
            m.first_row += rows;
            m
        }));
        self.frames.extend_from_slice(a.frames);
        self.fits.extend(a.fits.iter().map(|f| FitRecord {
            model: f.model + first_model,
            skeleton: f.skeleton + skeletons,
            first_fix: f.first_fix + fixes,
            _pad: 0,
        }));
        self.fixes.extend_from_slice(a.fixes);
    }

    pub fn is_empty(&self) -> bool {
        self.skeletons.is_empty()
    }

    /// Write the tables, unless there are none (a pack without skeletons
    /// carries no anim tables at all).
    pub fn write(&self, pack: &mut crate::PackWriter) {
        use crate::{Kind, table_bytes};
        if self.is_empty() {
            return;
        }
        pack.add(
            SKELETONS_PATH,
            Kind::Table,
            &table_bytes(ANIM_SCHEMA, &self.skeletons),
        );
        pack.add(
            JOINTS_PATH,
            Kind::Table,
            &table_bytes(ANIM_SCHEMA, &self.joints),
        );
        pack.add(
            MOVES_PATH,
            Kind::Table,
            &table_bytes(ANIM_SCHEMA, &self.moves),
        );
        pack.add(
            FRAMES_PATH,
            Kind::Table,
            &table_bytes(ANIM_SCHEMA, &self.frames),
        );
        pack.add(
            FITS_PATH,
            Kind::Table,
            &table_bytes(ANIM_SCHEMA, &self.fits),
        );
        pack.add(
            FIXES_PATH,
            Kind::Table,
            &table_bytes(ANIM_SCHEMA, &self.fixes),
        );
    }
}
