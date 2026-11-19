//! Big uploads (the models and their textures, about 350 MB) handed to the
//! GPU a slice at a time. wgpu copies every queued write into a staging
//! buffer it keeps until the GPU has run the next submit, so writing all of
//! them at once held a second copy of every byte when a match loaded (the
//! process peaked about 300 MB above where it settled).

use std::time::Duration;

use gc_platform::logln;

/// Bytes staged before the uploader hands them to the GPU and waits for it.
const SLICE: usize = 32 << 20;
/// How long a slice may take the GPU before loading goes on without
/// waiting (logged: the staging is then freed a frame or two later).
const SLICE_WAIT: Duration = Duration::from_secs(10);

/// Writes to the GPU that keep at most about [`SLICE`] bytes staged; what
/// is left is handed over when it is dropped.
pub(crate) struct Uploader<'a> {
    pub device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    staged: usize,
}

impl<'a> Uploader<'a> {
    pub fn new(device: &'a wgpu::Device, queue: &'a wgpu::Queue) -> Self {
        Uploader {
            device,
            queue,
            staged: 0,
        }
    }

    /// Count `bytes` just staged; once a slice's worth are, hand them over.
    fn count(&mut self, bytes: usize) {
        self.staged += bytes;
        if self.staged >= SLICE {
            self.hand_over();
        }
    }

    /// Submit what is staged and wait for the GPU, which frees the staging.
    fn hand_over(&mut self) {
        let index = self.queue.submit([]);
        let wait = wgpu::PollType::Wait {
            submission_index: Some(index),
            timeout: Some(SLICE_WAIT),
        };
        if let Err(e) = self.device.poll(wait) {
            logln!("grubcraft: GPU upload not done in {SLICE_WAIT:?} ({e}); loading on");
        }
        self.staged = 0;
    }

    /// Write `bytes` (a multiple of 4 long) into `buffer` from `offset`.
    pub fn write(&mut self, buffer: &wgpu::Buffer, offset: u64, bytes: &[u8]) {
        for (k, slice) in bytes.chunks(SLICE).enumerate() {
            self.queue
                .write_buffer(buffer, offset + (k * SLICE) as u64, slice);
            self.count(slice.len());
        }
    }

    /// A new buffer for `usage` (and copying to) holding `bytes`, its size
    /// rounded up to the 4 bytes buffers must be a multiple of.
    pub fn buffer(&mut self, label: &str, bytes: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (bytes.len() as u64).next_multiple_of(4).max(4),
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let whole = bytes.len() / 4 * 4;
        self.write(&buffer, 0, &bytes[..whole]);
        let tail = &bytes[whole..];
        if !tail.is_empty() {
            let mut padded = [0; 4];
            padded[..tail.len()].copy_from_slice(tail);
            self.write(&buffer, whole as u64, &padded);
        }
        buffer
    }

    /// [`wgpu::Queue::write_texture`], counted as wgpu stages it: each row
    /// padded to 256 bytes.
    pub fn texture(
        &mut self,
        to: wgpu::TexelCopyTextureInfo<'_>,
        data: &[u8],
        layout: wgpu::TexelCopyBufferLayout,
        size: wgpu::Extent3d,
    ) {
        self.queue.write_texture(to, data, layout, size);
        let staged = layout.bytes_per_row.map_or(data.len(), |row| {
            let row = row as usize;
            data.len() / row.max(1)
                * row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize)
        });
        self.count(staged);
    }
}

impl Drop for Uploader<'_> {
    fn drop(&mut self) {
        if self.staged > 0 {
            self.hand_over();
        }
    }
}
