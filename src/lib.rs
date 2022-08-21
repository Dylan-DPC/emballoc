//! Simple allocator for embedded systems
//!
//! This crate provides a single type called [`Allocator`]. This type implements
//! the [`core::alloc::GlobalAlloc`]-trait, which is required to use the
//! [`alloc`-crate][alloc] on `#![no_std]`-targets. The allocator provided in
//! this crate is relatively simple, but reliable: its design is simple, so that
//! errors in the implementation are unlikely. Furthermore the crate is tested
//! by (unit) tests running under `miri`, so there shouldn't be any undefined
//! behavior.
//!
//! # Usage
//! The usage is simple: just copy and paste the following code snipped into
//! your binary crate and potentially adjust the number of bytes of the heap
//! (here 4K):
//! ```
//! #[global_allocator]
//! static ALLOCATOR: emballoc::Allocator<4096> = emballoc::Allocator::new();
//!
//! extern crate alloc;
//! ```
//! Afterwards you don't need to interact with the crate or the variable
//! `ALLOCATOR` anymore. Now you can just `use alloc::vec::Vec` or even
//! `use alloc::collections::BTreeMap`, i.e. every fancy collection which is
//! normally provided by the `std`.
//!
//! The minimal buffer size is `8`, which would allow exactly one allocation of
//! size up to 4 at a time. Adjust the size as necessary, e.g. by doing a worst
//! case calculation and potentially adding some backup space of 10% (for
//! example).
//!
//! Note to users with things like `MPU`s, `MMU`s, etc.: your device might
//! support things like memory remapping or memory protection with setting
//! read/write/execution rights. This crate _doesn't use_ those features at all!
//! If that is desired, you should take the address of the buffer and use that
//! along with the known size `N` to protect the heap memory. To users with a
//! fully-working MMU: it is recommended, that you use an allocator, that
//! actually supports paging, etc. This crate might still be helpful, e.g.
//! before setting up the MMU.
//!
//! # Implementation
//! This algorithm does a linear scan for free blocks. The basic algorithm is as
//! follows:
//! 1.  We start with an empty buffer.
//!     ```text
//!     xxxx 0000 0000 0000 0000 0000 0000 0000
//!     ^--- ^---------------------------------
//!     FREE size = 28
//!     ```
//!     There is a single entry, which spans all the remaining buffer bytes
//!     (after the entry itself, which is always 4 bytes).
//! 2.  A block of 8 is allocated.
//!     ```text
//!     xxxx 0000 0000 yyyy 0000 0000 0000 0000
//!     ^--- ^-------- ^--- ^------------------
//!     USED size = 8  FREE size = 16
//!     ```
//!     Now the only free block (the FREE block of step 1) is split into two.
//!     There is now a used block with a total size of 12 bytes, 4 bytes for the
//!     header and 8 bytes for the content. The remaining buffer space is
//!     occupied by the FREE-element. Note, that the total number of "usable"
//!     space (the memory without the headers) shrunk from 28 to 24 (16 + 8)
//!     bytes, since there is now an additional header.
//! 3.  Another block of 4 is allocated.
//!     ```text
//!     xxxx 0000 0000 yyyy 0000 zzzz 0000 0000
//!     ^--- ^-------- ^--- ^--- ^--- ^--------
//!     USED size = 8  USED size FREE size = 8
//!     ```
//!     The same thing as in step 2 happens. Now there are two used blocks and
//!     a single free block with a size of 8.
//! 4.  A request for a block of 16 comes in. There is not enough free memory
//!     for that request. Therefore the allocation fails.
//! 5.  A block of 5 is allocated.
//!     ```text
//!     xxxx 0000 0000 yyyy 0000 zzzz 0000 0000
//!     ^--- ^-------- ^--- ^--- ^--- ^-----!!!
//!     USED size = 8  USED size USED size = 8
//!     ```
//!     There is not enough space at the end of the memory buffer, therefore the
//!     current entry is enlarged to fill the remaining space. This "wastes" 3
//!     bytes, but those would not be usable anyway.
//!
//!     To prevent alignment issues, the blocks are always rounded up to a
//!     multiple of 4 as well, which has the same result (this implies, that the
//!     aforementioned special handling of the remaining bytes is not necessary,
//!     care has to be taken to handle 0-sized "free" blocks correctly).
//! 6.  A request for a block of 1 comes in. There is no free memory at all and
//!     hence not enough free memory for that request. Therefore the allocation
//!     fails.
//! 7.  The third allocation (block size 5) is freed.
//!     ```text
//!     xxxx 0000 0000 yyyy 0000 zzzz 0000 0000
//!     ^--- ^-------- ^--- ^--- ^--- ^--------
//!     USED size = 8  USED size FREE size = 8
//!     ```
//!     The picture of step 3 is restored.
//! 8.  The first allocation (block size 8) is freed.
//!     ```text
//!     xxxx 0000 0000 yyyy 0000 zzzz 0000 0000
//!     ^--- ^-------- ^--- ^--- ^--- ^--------
//!     FREE size = 8  USED size FREE size = 8
//!     ```
//!     Now there are two free blocks and a usable block. Note, that there is
//!     fragmentation, so a request for 12 bytes could not be fulfilled, since
//!     there is no contiguous memory of that size.
//! 9.  Another block of 8 is allocated.
//!     ```text
//!     xxxx 0000 0000 yyyy 0000 zzzz 0000 0000
//!     ^--- ^-------- ^--- ^--- ^--- ^--------
//!     USED size = 8  USED size FREE size = 8
//!     ```
//!     Nothing special here, except that the allocator could choose between the
//!     two blocks of 8. Here the first one was chosen (arbitrarily).
//! 10. The second allocation (block size 4) is freed.
//!     ```text
//!     xxxx 0000 0000 yyyy 0000 0000 0000 0000
//!     ^--- ^-------- ^--- ^------------------
//!     USED size = 8  FREE size = 16
//!     ```
//!     The block is simply replaced by a FREE block, but there is a caveat: the
//!     two adjacent blocks have to be connected to a single big FREE-block in
//!     order to prevent more fragmentation. They are one continuous block with
//!     a single header.
//!
//!     This connection is easy, since the middle block of step 9 just has to
//!     look for the next header (the position of that block is known by its
//!     size) and check, whether it is free. If so, the new block gets adjusted
//!     to have a size of `self.size + 4 + other.size`. This effectively erases
//!     the right free block.
//! 11. A new block of 8 is allocated. Afterwards the first block is freed.
//!     ```text
//!     xxxx 0000 0000 yyyy 0000 0000 0000 0000
//!     ^--- ^-------- ^--- ^-------- ^--- ^---
//!     FREE size = 8  USED size = 8  FREE size
//!     ```
//!     This is just an intermediate step without any issues.
//! 12. The remaining used block is freed.
//!     ```text
//!     xxxx 0000 0000 yyyy 0000 0000 0000 0000
//!     ^--- ^-------- ^--- ^------------------
//!     FREE size = 8  FREE size = 16
//!     ```
//!     Now there are two(!) free blocks, since the concatenation described in
//!     step 10 does only happen to the right side of the freed block. Since the
//!     left block has an unknown size, it is not possible to find the header
//!     (except for linearly scanning the memory from the beginning). Therefore
//!     it is easier to just live with that fragmentation.
//!
//!     Something interesting here is, that one could check for such conditions
//!     from time to time and fix them during that scan. Doing it this way does
//!     not come with a constant time penalty when deallocating. Furthermore it
//!     lets the user decide, whether that feature is necessary or not.
//!
//! [alloc]: https://doc.rust-lang.org/alloc/index.html
//! [gist_hosted-test]: https://gist.github.com/jfrimmel/61943f9879adfbe760a78efa17a0ecaa
//! [`Cell<T>`]: core::cell::Cell
#![cfg_attr(not(test), no_std)]
#![warn(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]

mod raw_allocator;
use raw_allocator::RawAllocator;

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;

/// The memory allocator for embedded systems.
///
/// This is the core type of this crate: it is an allocator with a predefined
/// heap size. Therefore the heap memory usage is statically limited to an upper
/// value, which also helps to prevent issues with heap/stack-smashes, as the
/// heap is counted to the static memory (e.g. `.data`/`.bss`-sections). Such a
/// smash might still happen though, if the stack pointer grows into the heap,
/// but the heap cannot grow into the stack pointer.
///
/// Its usage is simple: just copy and paste the following in the binary crate
/// you're developing. The memory size of the heap is `4096` or 4K in this
/// example. Adjust that value to your needs.
/// ```no_run
/// #[global_allocator]
/// static ALLOCATOR: emballoc::Allocator<4096> = emballoc::Allocator::new();
/// ```
/// Also please refer to the [crate-level](crate)-documentation for
/// recommendations on the buffer size and general usage.
pub struct Allocator<const N: usize> {
    /// The internal raw allocator.
    ///
    /// The raw allocator handles allocations of contiguous byte slices without
    /// needing to worry about alignment. The raw allocator is protected by a
    /// `spin::Mutex` to make it usable with shared references (requirement of
    /// [`GlobalAlloc`]).
    raw: spin::Mutex<RawAllocator<N>>,
}
impl<const N: usize> Allocator<N> {
    /// Create a new [`Allocator`].
    ///
    /// This function is a `const fn`, therefore you can call it directly when
    /// creating the allocator.
    ///
    /// Please see the [crate-level](crate)-documentation for recommendations on
    /// the buffer size and general usage.
    ///
    /// # Panics
    /// This function will panic, if the supplied buffer size, i.e. `N` is less
    /// than `8` or not divisible by `4`.
    #[must_use = "assign the allocator to a static variable and apply the `#[global_allocator]`-attribute to make it the global allocator"]
    pub const fn new() -> Self {
        let raw = spin::Mutex::new(RawAllocator::new());
        Self { raw }
    }

    /// Align a given pointer to the specified alignment.
    ///
    /// # Safety
    /// This function requires `align` to be a power of two and requires the
    /// `ptr` to point to a memory region, that is large enough, so that the
    /// aligned pointer is still in that memory region.
    unsafe fn align_to(ptr: *mut u8, align: usize) -> *mut u8 {
        let addr = ptr as usize;
        let mismatch = addr & (align - 1);
        let offset = if mismatch != 0 { align - mismatch } else { 0 };
        // SAFETY: "in-bound"-requirement is part of the safety-contract of this
        // function, therefore the caller is responsible for it
        unsafe { ptr.add(offset) }
    }
}
// SAFETY: the safety contracts of global allocator is a bit lengthy, but in
// short: the implementation does not panic (at least on purpose, if it would,
// there is a bug) and it actually adheres to the layout requirements (ensured
// by tests).
unsafe impl<const N: usize> GlobalAlloc for Allocator<N> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        // the raw allocator always returns 4-byte-aligned slices, therefore
        // smaller alignments are always fulfilled. Larger alignments are a bit
        // more tricky, since this requires over-allocation and adjusting the
        // pointer accordingly. The over-allocation is rather conservative and
        // uses a worst case estimation, therefore it allocates `align` bytes
        // more, ensuring there is enough memory.
        let size = if align > 4 {
            layout.size() + align
        } else {
            layout.size()
        };

        // allocate a memory block and return the sufficiently aligned pointer
        // into that memory block.
        match self.raw.lock().alloc(size) {
            // SAFETY: `align` is a power of two as by the contract of `Layout`.
            // Furthermore the memory slice is enlarged (see above), so that the
            // aligned pointer will still be in the same allocation.
            Some(memory) => unsafe { Self::align_to(ptr::addr_of_mut!(*memory).cast(), align) },
            None => ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        // alignment is irrelevant here, as `RawAllocator::free` can handle any
        // pointer in an entry's memory, so simply forward the pointer. The
        // `free()`-method might detect errors, but those cannot lead to panics
        // (by contract of `GlobalAlloc`). Therefore there are two choices:
        // 1. abort the process
        // 2. ignore the error
        // Since there is no process and there is no stable way to abort the
        // program on `core` the only viable option is option #1: do nothing.
        let _maybe_error = self.raw.lock().free(ptr.cast()).ok();
        // errors are ignored
    }
}

#[cfg(test)]
mod tests {
    use crate::Allocator;
    use core::alloc::{GlobalAlloc, Layout};
    use core::ptr;

    #[test]
    fn alignment_of_align_to() {
        // create buffer memory for proper indexing. One could use random
        // integers and cast them to pointers, but this would violate the strict
        // provenance rules and `miri` would detect that. Therefore this uses a
        // valid and suitable aligned buffer and uses pointers into that buffer.
        #[repr(align(16))]
        struct Align([u8; 16]);
        let mut just_a_buffer_to_get_a_valid_address = Align([0_u8; 16]);
        let base: *mut u8 = ptr::addr_of_mut!(just_a_buffer_to_get_a_valid_address.0).cast();

        // create some pointers to the buffer with some offsets
        let ptr_0x10 = base;
        let ptr_0x11 = base.wrapping_add(1);
        let ptr_0x14 = base.wrapping_add(4);
        let ptr_0x1c = base.wrapping_add(0xc);
        let ptr_0x20 = base.wrapping_add(0x10);

        // the actual test for the alignment of `align_to()`
        assert_eq!(unsafe { Allocator::<8>::align_to(ptr_0x11, 4) }, ptr_0x14);
        assert_eq!(unsafe { Allocator::<8>::align_to(ptr_0x10, 4) }, ptr_0x10);

        assert_eq!(unsafe { Allocator::<8>::align_to(ptr_0x11, 1) }, ptr_0x11);

        assert_eq!(unsafe { Allocator::<8>::align_to(ptr_0x1c, 16) }, ptr_0x20);
    }

    // the following tests ensure, that a pointer with the requested alignment
    // is returned

    /// Assert the given alignment of pointers.
    macro_rules! assert_alignment {
        ($ptr:expr, $align:expr) => {{
            assert_eq!(($ptr as usize) % $align, 0, "Alignment not fulfilled");
        }};
    }

    #[test]
    fn small_alignments() {
        let allocator = Allocator::<32>::new();

        let ptr = unsafe { allocator.alloc(Layout::from_size_align(8, 2).unwrap()) };
        assert_alignment!(ptr, 1);

        let ptr = unsafe { allocator.alloc(Layout::from_size_align(4, 4).unwrap()) };
        assert_alignment!(ptr, 4);
    }

    #[test]
    fn medium_alignments() {
        let allocator = Allocator::<128>::new();

        let ptr = unsafe { allocator.alloc(Layout::from_size_align(4, 8).unwrap()) };
        assert_alignment!(ptr, 8);

        let ptr = unsafe { allocator.alloc(Layout::from_size_align(4, 32).unwrap()) };
        assert_alignment!(ptr, 32);
    }

    #[cfg(not(miri))] // too slow
    #[test]
    fn huge_alignment() {
        // in static memory to prevent stack overflow
        const FOUR_MEG: usize = 4 * 1024 * 1024;

        static ALLOCATOR: Allocator<{ 10 * 1024 * 1024 }> = Allocator::new();
        let ptr = unsafe { ALLOCATOR.alloc(Layout::from_size_align(4, FOUR_MEG).unwrap()) };

        assert_alignment!(ptr, FOUR_MEG);
    }

    #[test]
    fn example_usage() {
        // do some example allocations. There is an intermediate deallocation,
        // different allocation/deallocation-orders, different alignments and
        // different sizes.
        static ALLOCATOR: Allocator<4096> = Allocator::new();

        unsafe {
            let layout1 = Layout::new::<u32>();
            let ptr1 = ALLOCATOR.alloc(layout1);
            assert_ne!(ptr1, ptr::null_mut());

            let layout2 = Layout::new::<f64>();
            let ptr2 = ALLOCATOR.alloc(layout2);
            assert_ne!(ptr2, ptr::null_mut());

            let layout3 = Layout::new::<[u16; 12]>();
            let ptr3 = ALLOCATOR.alloc(layout3);
            assert_ne!(ptr3, ptr::null_mut());

            ALLOCATOR.dealloc(ptr2, layout2);

            let layout4 = Layout::new::<[u128; 3]>();
            let ptr4 = ALLOCATOR.alloc(layout4);
            assert_ne!(ptr4, ptr::null_mut());

            let layout5 = Layout::new::<f32>();
            let ptr5 = ALLOCATOR.alloc(layout5);
            assert_ne!(ptr5, ptr::null_mut());

            ALLOCATOR.dealloc(ptr3, layout3);
            ALLOCATOR.dealloc(ptr4, layout4);
            ALLOCATOR.dealloc(ptr5, layout5);
            ALLOCATOR.dealloc(ptr1, layout1);
        }
    }
}
