//! The shared GPU pools: one buffer each, sized once at start-up and sub-allocated (measured by
//! `pyreal-gpu-probe`: every pool under 4 GiB, allocated once, because the first touch of new memory is slow).

use crate::EngineError;

/// First-fit sub-allocation of a fixed range, with freed ranges merged with their neighbours.
#[derive(Debug, Clone)]
pub struct RangeAllocator {
    size: u64,
    /// Free ranges `(start, len)`, sorted by start, never adjacent.
    free: Vec<(u64, u64)>,
}

impl RangeAllocator {
    #[must_use]
    pub fn new(size: u64) -> Self {
        Self {
            size,
            free: vec![(0, size)],
        }
    }

    /// `len` units aligned to `align` (a power of two), or `None` when no free range fits.
    pub fn alloc(&mut self, len: u64, align: u64) -> Option<u64> {
        if len == 0 {
            return Some(0);
        }
        let align = align.max(1);
        for i in 0..self.free.len() {
            let (start, flen) = self.free[i];
            let at = start.div_ceil(align) * align;
            let pad = at - start;
            if flen < pad + len {
                continue;
            }
            let rest = flen - pad - len;
            match (pad > 0, rest > 0) {
                (false, false) => {
                    self.free.remove(i);
                }
                (false, true) => self.free[i] = (at + len, rest),
                (true, false) => self.free[i] = (start, pad),
                (true, true) => {
                    self.free[i] = (start, pad);
                    self.free.insert(i + 1, (at + len, rest));
                }
            }
            return Some(at);
        }
        None
    }

    /// Returns `len` units at `start` to the free list.
    pub fn free(&mut self, start: u64, len: u64) {
        if len == 0 {
            return;
        }
        let i = self.free.partition_point(|&(s, _)| s < start);
        self.free.insert(i, (start, len));
        // Merge with the next, then the previous.
        if i + 1 < self.free.len() && self.free[i].0 + self.free[i].1 == self.free[i + 1].0 {
            self.free[i].1 += self.free[i + 1].1;
            self.free.remove(i + 1);
        }
        if i > 0 && self.free[i - 1].0 + self.free[i - 1].1 == self.free[i].0 {
            self.free[i - 1].1 += self.free[i].1;
            self.free.remove(i);
        }
    }

    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Units not handed out.
    #[must_use]
    pub fn free_units(&self) -> u64 {
        self.free.iter().map(|f| f.1).sum()
    }

    #[must_use]
    pub fn used_units(&self) -> u64 {
        self.size - self.free_units()
    }

    /// The longest free range: how large an allocation still fits.
    #[must_use]
    pub fn largest_free(&self) -> u64 {
        self.free.iter().map(|f| f.1).max().unwrap_or(0)
    }
}

/// One buffer and its allocator, in elements of `stride` bytes.
#[derive(Debug)]
pub struct Pool {
    pub name: &'static str,
    pub buffer: wgpu::Buffer,
    pub stride: u64,
    alloc: RangeAllocator,
}

impl Pool {
    /// A pool of `capacity` elements of `stride` bytes; refused at 4 GiB or more, where a storage
    /// binding silently wraps.
    ///
    /// # Errors
    /// A pool of 4 GiB or more, or larger than the device's buffer limit.
    pub fn new(
        device: &wgpu::Device,
        name: &'static str,
        stride: u64,
        capacity: u64,
        usage: wgpu::BufferUsages,
    ) -> Result<Self, EngineError> {
        let bytes = stride * capacity;
        if bytes >= 1 << 32 || bytes > device.limits().max_buffer_size {
            return Err(EngineError::Device(format!(
                "the {name} pool of {bytes} bytes is over the 4 GiB a binding holds"
            )));
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(name),
            size: bytes,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            name,
            buffer,
            stride,
            alloc: RangeAllocator::new(capacity),
        })
    }

    /// `count` elements; the first one's index.
    ///
    /// # Errors
    /// The pool is full.
    pub fn alloc(&mut self, count: u64) -> Result<u64, EngineError> {
        self.alloc
            .alloc(count, 1)
            .ok_or(EngineError::PoolFull(self.name, count))
    }

    pub fn free(&mut self, first: u64, count: u64) {
        self.alloc.free(first, count);
    }

    /// Frees every element at once.
    pub fn reset(&mut self) {
        self.alloc = RangeAllocator::new(self.alloc.size());
    }

    /// Writes `bytes` at element `first`.
    pub fn write(&self, queue: &wgpu::Queue, first: u64, bytes: &[u8]) {
        if !bytes.is_empty() {
            queue.write_buffer(&self.buffer, first * self.stride, bytes);
        }
    }

    #[must_use]
    pub fn capacity(&self) -> u64 {
        self.alloc.size()
    }

    #[must_use]
    pub fn used(&self) -> u64 {
        self.alloc.used_units()
    }

    /// The longest free run of elements.
    #[must_use]
    pub fn largest_free(&self) -> u64 {
        self.alloc.largest_free()
    }

    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.alloc.size() * self.stride
    }
}
