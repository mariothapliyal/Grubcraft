//! GPU timings: how long each pass of a frame takes on the GPU, from
//! timestamp queries, for the game's automatic resolution, the dev overlay
//! (F11) and `--gpu-times`. Taken only while one of those wants them, and
//! only where the GPU can write timestamps (`Features::TIMESTAMP_QUERY`).
//! Where it can also write them inside a pass (Vulkan and Direct 3D 12, not
//! Metal), and the splits are asked for, the world pass is split into what
//! it draws: terrain, scenery, soldiers and the rest.
//!
//! Results come back a few frames late: each frame's timestamps are copied
//! to one of a ring of readable buffers, and read once the GPU is done with
//! them, so the game never waits for the GPU to measure it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// Timestamps a frame may write: two a pass, and the world pass's splits.
const MAX_QUERIES: u32 = 32;
/// Frames in flight whose timestamps can wait to be read.
const RING: usize = 4;

/// One measured part of a frame: a pass, or part of the world pass.
#[derive(Clone, Copy, Debug)]
struct Section {
    name: &'static str,
    from: u32,
    to: u32,
}

/// A readable buffer and what the frame copied into it measures.
struct Slot {
    buffer: wgpu::Buffer,
    sections: Vec<Section>,
    /// The `reset` count when the frame was copied in: a frame from before
    /// the last reset is not added.
    epoch: u32,
    /// Where the slot is: `FREE` to `COPIED` to `MAPPING` to `READY` (or
    /// straight back to `FREE` if the read failed) and back.
    state: Arc<AtomicU8>,
}

const FREE: u8 = 0;
/// This frame's timestamps are copied in; it is mapped after the submit.
const COPIED: u8 = 1;
const MAPPING: u8 = 2;
const READY: u8 = 3;

pub(crate) struct GpuTimer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: Vec<Slot>,
    /// Nanoseconds a timestamp tick.
    period: f64,
    /// Whether timestamps can be written inside a pass.
    inside: bool,
    /// Whether to split the world pass (`split`), where `inside`.
    pub splits: bool,
    /// This frame's sections and the next free query.
    sections: Vec<Section>,
    next: u32,
    /// In a pass that splits are taken in: the query of its start or of
    /// its last split.
    open: Option<u32>,
    /// Whether this frame writes timestamps (none are left for it in the
    /// ring, or timing is off).
    on: bool,
    totals: Totals,
    /// How many times the averages have been reset.
    epoch: u32,
}

/// Each section's time summed over the frames read, for averages.
#[derive(Clone, Debug, Default)]
struct Totals {
    frames: u32,
    ms: Vec<(&'static str, f64)>,
}

/// The average milliseconds each part of a frame took on the GPU.
#[derive(Clone, Debug, Default)]
pub struct GpuTimes {
    /// Frames averaged.
    pub frames: u32,
    /// Each part's name ("world/terrain" for part of a pass) and average
    /// milliseconds, in the order the frame draws them.
    pub parts: Vec<(&'static str, f32)>,
    /// The GPU's time for the whole frame: its passes added up.
    pub total: f32,
}

impl GpuTimer {
    /// The features to ask the device for, so timings can be taken: what
    /// the adapter has of them.
    pub fn features(adapter: wgpu::Features) -> wgpu::Features {
        adapter & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES)
    }

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<GpuTimer> {
        let features = device.features();
        if !features.contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let bytes = u64::from(MAX_QUERIES) * 8;
        let set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("gpu timings"),
            ty: wgpu::QueryType::Timestamp,
            count: MAX_QUERIES,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu timings"),
            size: bytes,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let slots = (0..RING)
            .map(|_| Slot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("gpu timings read"),
                    size: bytes,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                sections: Vec::new(),
                epoch: 0,
                state: Arc::new(AtomicU8::new(FREE)),
            })
            .collect();
        Some(GpuTimer {
            set,
            resolve,
            slots,
            period: f64::from(queue.get_timestamp_period()),
            inside: features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES),
            splits: false,
            sections: Vec::new(),
            next: 0,
            open: None,
            on: false,
            totals: Totals::default(),
            epoch: 0,
        })
    }

    /// Start a frame's timings; nothing is written unless a slot is free.
    pub fn begin(&mut self) {
        self.sections.clear();
        self.next = 0;
        self.open = None;
        self.on = self
            .slots
            .iter()
            .any(|s| s.state.load(Ordering::Acquire) == FREE);
    }

    /// Two queries for a pass named `name`, or none when this frame is not
    /// timed or has used them all.
    fn take(&mut self, name: &'static str) -> Option<(u32, u32)> {
        if !self.on || self.next + 2 > MAX_QUERIES {
            return None;
        }
        let (from, to) = (self.next, self.next + 1);
        self.next += 2;
        self.sections.push(Section { name, from, to });
        Some((from, to))
    }

    /// Two queries for a run of passes named `name`, timed from the start
    /// of the first to the end of the last, which write them themselves.
    pub fn span(&mut self, name: &'static str) -> Option<(&wgpu::QuerySet, u32, u32)> {
        let (from, to) = self.take(name)?;
        Some((&self.set, from, to))
    }

    pub fn render_pass(
        &mut self,
        name: &'static str,
    ) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let (from, to) = self.take(name)?;
        self.open = Some(from);
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &self.set,
            beginning_of_pass_write_index: Some(from),
            end_of_pass_write_index: Some(to),
        })
    }

    pub fn compute_pass(
        &mut self,
        name: &'static str,
    ) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        let (from, to) = self.take(name)?;
        Some(wgpu::ComputePassTimestampWrites {
            query_set: &self.set,
            beginning_of_pass_write_index: Some(from),
            end_of_pass_write_index: Some(to),
        })
    }

    /// Mark the end of part `name` of the pass last begun (where timestamps
    /// can be written inside a pass): it runs from the pass's start or the
    /// last split to here. The pass's own section stays its whole time.
    pub fn split(&mut self, pass: &mut wgpu::RenderPass<'_>, name: &'static str) {
        let Some(last) = self.open.filter(|_| self.inside && self.splits && self.on) else {
            return;
        };
        if self.next + 1 > MAX_QUERIES {
            return;
        }
        let at = self.next;
        self.next += 1;
        pass.write_timestamp(&self.set, at);
        self.sections.push(Section {
            name,
            from: last,
            to: at,
        });
        self.open = Some(at);
    }

    /// Copy this frame's timestamps out for reading, after its last pass.
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if !self.on || self.next == 0 {
            self.on = false;
            return;
        }
        let Some(slot) = self
            .slots
            .iter_mut()
            .find(|s| s.state.load(Ordering::Acquire) == FREE)
        else {
            self.on = false;
            return;
        };
        encoder.resolve_query_set(&self.set, 0..self.next, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &slot.buffer, 0, u64::from(self.next) * 8);
        // Swapped, so both keep their room and no frame allocates.
        std::mem::swap(&mut slot.sections, &mut self.sections);
        self.sections.clear();
        slot.epoch = self.epoch;
        slot.state.store(COPIED, Ordering::Release);
    }

    /// After the frame is submitted: ask for its timestamps, and add up any
    /// that have arrived.
    pub fn after_submit(&mut self, device: &wgpu::Device) {
        self.on = false;
        for slot in &self.slots {
            if slot.state.load(Ordering::Acquire) == COPIED {
                let state = slot.state.clone();
                state.store(MAPPING, Ordering::Release);
                slot.buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |r| {
                        state.store(if r.is_ok() { READY } else { FREE }, Ordering::Release);
                    });
            }
        }
        // Run the callbacks of any reads the GPU has finished; never wait.
        let _ = device.poll(wgpu::PollType::Poll);
        for slot in &mut self.slots {
            if slot.state.load(Ordering::Acquire) == READY {
                if slot.epoch == self.epoch
                    && let Ok(data) = slot.buffer.slice(..).get_mapped_range()
                {
                    let ticks: &[u64] = bytemuck::cast_slice(&data);
                    self.totals.add(&slot.sections, ticks, self.period);
                }
                slot.buffer.unmap();
                slot.state.store(FREE, Ordering::Release);
            }
        }
    }

    /// Frames averaged since the last `reset`.
    pub fn frames(&self) -> u32 {
        self.totals.frames
    }

    /// The averages since the last `reset`.
    pub fn times(&self) -> GpuTimes {
        let n = f64::from(self.totals.frames.max(1));
        let parts: Vec<(&'static str, f32)> = self
            .totals
            .ms
            .iter()
            .map(|&(name, ms)| (name, (ms / n) as f32))
            .collect();
        let total = parts
            .iter()
            .filter(|(name, _)| !name.contains('/'))
            .map(|(_, ms)| ms)
            .sum();
        GpuTimes {
            frames: self.totals.frames,
            parts,
            total,
        }
    }

    pub fn reset(&mut self) {
        self.totals = Totals::default();
        self.epoch = self.epoch.wrapping_add(1);
    }
}

impl Totals {
    fn add(&mut self, sections: &[Section], ticks: &[u64], period: f64) {
        // A frame where a part was not drawn has no time for it: count it as
        // nothing rather than leaving the average to the frames that drew it.
        for s in sections {
            let (Some(&a), Some(&b)) = (ticks.get(s.from as usize), ticks.get(s.to as usize))
            else {
                continue;
            };
            let ms = b.saturating_sub(a) as f64 * period / 1e6;
            match self.ms.iter_mut().find(|(name, _)| *name == s.name) {
                Some(entry) => entry.1 += ms,
                None => self.ms.push((s.name, ms)),
            }
        }
        self.frames += 1;
    }
}

impl GpuTimes {
    /// One line a part, for the dev overlay and `--gpu-times`.
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![format!(
            "GPU {:.2} ms a frame ({} frames)",
            self.total, self.frames
        )];
        for (name, ms) in &self.parts {
            let share = if self.total > 0.0 {
                ms / self.total * 100.0
            } else {
                0.0
            };
            out.push(format!("  {name:<18} {ms:7.2} ms {share:5.1}%"));
        }
        out
    }
}
