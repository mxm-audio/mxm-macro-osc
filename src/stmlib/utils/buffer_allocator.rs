//! Ported from stmlib/utils/buffer_allocator.h: a bump allocator over one block of memory; and
//! [`Arena`], the block, which is plaits.cc's `char shared_buffer[16384]`.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ hands out pointers into the buffer; here `allocate` hands out the **byte offset** of
//! the region, and an engine keeps that offset, never a reference, reading and writing its
//! regions through [`Arena::view`] and [`Arena::view_mut`], typed views of the bytes. As on x64,
//! `allocate::<T>(n)` takes `size_of::<T>() * n` bytes with no padding, so what one engine leaves
//! in the arena the next one reads, byte for byte.

use bytemuck::Pod;

/// The bytes of plaits.cc's `shared_buffer`, which every engine of a voice shares.
pub const K_ARENA_SIZE: usize = 16384;

/// The engines' shared memory: one per voice, zeroed as the module's static buffer starts.
/// Aligned to 16, so a region allocated at an offset that is a multiple of its type's alignment
/// can be viewed as that type.
#[repr(C, align(16))]
pub struct Arena {
    pub bytes: [u8; K_ARENA_SIZE],
}

impl Default for Arena {
    fn default() -> Self {
        Self {
            bytes: [0; K_ARENA_SIZE],
        }
    }
}

impl Arena {
    /// `count` values of `T` from byte `offset`. The region must lie in the arena, and
    /// `offset` be a multiple of `T`'s alignment (every region Plaits allocates of a type it
    /// views is); both are fixed when the engine is initialised, never by an input.
    #[inline]
    pub fn view<T: Pod>(&self, offset: usize, count: usize) -> &[T] {
        bytemuck::cast_slice(&self.bytes[offset..offset + count * size_of::<T>()])
    }

    /// As [`Arena::view`], mutable.
    #[inline]
    pub fn view_mut<T: Pod>(&mut self, offset: usize, count: usize) -> &mut [T] {
        bytemuck::cast_slice_mut(&mut self.bytes[offset..offset + count * size_of::<T>()])
    }

    /// Two regions at once, mutable: `(offset_a, count_a)` of `A` and `(offset_b, count_b)` of
    /// `B`, the first ending at or before the second's start.
    #[inline]
    pub fn view_mut2<A: Pod, B: Pod>(
        &mut self,
        offset_a: usize,
        count_a: usize,
        offset_b: usize,
        count_b: usize,
    ) -> (&mut [A], &mut [B]) {
        let (low, high) = self.bytes.split_at_mut(offset_b);
        (
            bytemuck::cast_slice_mut(&mut low[offset_a..offset_a + count_a * size_of::<A>()]),
            bytemuck::cast_slice_mut(&mut high[..count_b * size_of::<B>()]),
        )
    }
}

/// `BufferAllocator`, over a voice's [`Arena`]: `next_` is the offset of the next free byte.
pub struct BufferAllocator<'a> {
    next_: usize,
    buffer_: &'a mut Arena,
    free_: usize,
    size_: usize,
}

impl<'a> BufferAllocator<'a> {
    /// `BufferAllocator(void* buffer, size_t size)`, the buffer being the whole arena.
    pub fn new(buffer: &'a mut Arena) -> Self {
        let mut allocator = Self {
            next_: 0,
            buffer_: buffer,
            free_: 0,
            size_: K_ARENA_SIZE,
        };
        allocator.free();
        allocator
    }

    /// `Allocate<T>()`.
    #[inline]
    pub fn allocate_one<T>(&mut self) -> Option<usize> {
        self.allocate::<T>(1)
    }

    /// `Allocate<T>(size)`: the offset of `size` values of `T`, or `None` (the C++'s `NULL`)
    /// when they do not fit.
    #[inline]
    pub fn allocate<T>(&mut self, size: usize) -> Option<usize> {
        let size_bytes = size_of::<T>() * size;
        if size_bytes <= self.free_ {
            let start = self.next_;
            self.next_ += size_bytes;
            self.free_ -= size_bytes;
            Some(start)
        } else {
            None
        }
    }

    #[inline]
    pub fn free(&mut self) {
        self.next_ = 0;
        self.free_ = self.size_;
    }

    /// `free()`: the bytes left.
    #[inline]
    pub fn free_bytes(&self) -> usize {
        self.free_
    }

    /// The arena itself, for an engine whose `Init` writes into what it allocated.
    #[inline]
    pub fn arena_mut(&mut self) -> &mut Arena {
        self.buffer_
    }
}
