//! Big uploads (the models and their textures, about 350 MB) handed to the
//! GPU a slice at a time. wgpu copies every queued write into a staging
//! buffer it keeps until the GPU has run the next submit, so writing all of
//! them at once held a second copy of every byte when a match loaded (the
//! process peaked about 300 MB above where it settled).

/// Bytes queued before the uploader hands them to the GPU and waits for it.
const SLICE: usize = 32 << 20;

/// Writes to the GPU that keep at most about [`SLICE`] bytes queued.
pub(crate) struct Uploader<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    queued: usize,
}

impl<'a> Uploader<'a> {
    pub fn new(device: &'a wgpu::Device, queue: &'a wgpu::Queue) -> Self {
        Uploader {
            device,
            queue,
            queued: 0,
        }
    }

    /// Count `bytes` just queued; once a slice's worth are, submit them and
    /// wait for the GPU, which frees their staging.
    fn queued(&mut self, bytes: usize) {
        self.queued += bytes;
        if self.queued >= SLICE {
            self.queue.submit([]);
            // A lost device shows in its own error report.
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            self.queued = 0;
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
        for (k, slice) in bytes[..whole].chunks(SLICE).enumerate() {
            self.queue.write_buffer(&buffer, (k * SLICE) as u64, slice);
            self.queued(slice.len());
        }
        let tail = &bytes[whole..];
        if !tail.is_empty() {
            let mut padded = [0; 4];
            padded[..tail.len()].copy_from_slice(tail);
            self.queue.write_buffer(&buffer, whole as u64, &padded);
        }
        buffer
    }

    /// [`wgpu::Queue::write_texture`], counted.
    pub fn texture(
        &mut self,
        to: wgpu::TexelCopyTextureInfo<'_>,
        data: &[u8],
        layout: wgpu::TexelCopyBufferLayout,
        size: wgpu::Extent3d,
    ) {
        self.queue.write_texture(to, data, layout, size);
        self.queued(data.len());
    }
}
