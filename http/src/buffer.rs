use std::{
    alloc::{Layout, alloc, dealloc, handle_alloc_error},
    fmt,
    ops::{Deref, DerefMut},
    ptr, slice,
};

use crate::{IsSane, function, is_nonoverlapping};

/// Invariants
/// alloc always returns a non-null pointer
/// free releases any memory returned from
/// alloc (can ignore len parameter if it
/// isn't necessary for your allocator)
/// You can assume alloc will never be called
/// with a length of zero
///
/// if free gets a null pointer, it must do
/// nothing.
/// Arg is some pointer to internal data used by
/// the allocator
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Allocator {
    pub alloc: extern "C" fn(len: usize) -> *mut u8,
    pub free: unsafe extern "C" fn(ptr: *mut u8, len: usize),
}

pub(crate) extern "C" fn internal_alloc(len: usize) -> *mut u8 {
    assert_ne!(len, 0, "Alloc shouldn't be called with 0 length");
    let l = Layout::array::<u8>(len).expect("Should be able to create layout of len bytes");

    // SAFETY: l is not zero sized
    let ptr = unsafe { alloc(l) };

    if ptr.is_null() {
        handle_alloc_error(l)
    }

    ptr
}

/// SAFETY:
/// the pointer passed into this function must be
/// from internal_free or null
/// The len passed in must be the size of the allocation
/// or zero is ptr is null
pub(crate) unsafe extern "C" fn internal_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }

    // SAFETY:
    // Because the pointer is from internal_free, and len is the size
    // of the allocation, that means that len is non-zero
    let l = unsafe { Layout::from_size_align_unchecked(len, 1) };

    unsafe {
        dealloc(ptr, l);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn http_internal_allocator() -> Allocator {
    // Invariants are intact, see safety comment from
    // the internal_free function
    // Internal alloc never returns null, but a null
    // can still happen from freeing empty buffer
    // arg is never used in either function, so it is
    // safe to be null

    Allocator {
        alloc: internal_alloc,
        free: internal_free,
    }
}

/// Buffer for holding a byte slice, essentially
///
/// Invariants:
/// Either
/// buf is null and len is zero
/// OR
/// buf is not null and valid for reads of len bytes
#[repr(C)]
pub struct ConstBuffer {
    pub len: usize,
    pub buf: *const u8,
}

impl ConstBuffer {
    pub fn as_slice(&self) -> &[u8] {
        let ptr = if self.buf.is_null() {
            ptr::dangling()
        } else {
            self.buf
        };

        &*unsafe { slice::from_raw_parts(ptr, self.len) }
    }
}

impl Deref for ConstBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl IsSane for ConstBuffer {
    fn is_sane(&self) -> bool {
        return !self.buf.is_null() || (self.buf.is_null() && self.len == 0);
    }
}

impl AsRef<[u8]> for ConstBuffer {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl<T: ?Sized + AsRef<[u8]>> From<&T> for ConstBuffer {
    fn from(value: &T) -> Self {
        let v = value.as_ref();

        ConstBuffer {
            len: v.len(),
            buf: v.as_ptr(),
        }
    }
}

/// Fully self contained byte buffer allocation, complete with
/// a custom allocator for nicer interop
///
/// Invariants:
/// Either:
///  - buf is null, len == 0, and cap == 0
///
/// OR:
///  - buf is not null,
///  - buf is convertible to a mutable slice of cap bytes
///  - cap > 0
///  - len <= cap
///  - buf is always allocated and freed with the given allocator
///
/// Only the first len bytes are meaningful
#[repr(C)]
pub struct Buffer {
    pub len: usize,
    pub cap: usize,
    pub buf: *mut u8,
    pub alloc: Allocator,
}

impl Buffer {
    pub fn as_slice(&self) -> &[u8] {
        let ptr = if self.buf.is_null() {
            ptr::dangling()
        } else {
            self.buf
        };

        &*unsafe { slice::from_raw_parts(ptr, self.len) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        let ptr = if self.buf.is_null() {
            ptr::dangling_mut()
        } else {
            self.buf
        };

        &mut *unsafe { slice::from_raw_parts_mut(ptr, self.len) }
    }

    pub fn reserve(&mut self, additional: usize) {
        self.reserve_len(self.len + additional);
    }

    pub fn reserve_len(&mut self, len: usize) {
        let mut new_cap = self.cap;

        while new_cap < len {
            new_cap += new_cap.max(1);
        }

        if new_cap != self.cap {
            let new_buf = (self.alloc.alloc)(new_cap);
            assert!(
                !new_buf.is_null(),
                "{}: pointer returned from alloc must be non-null",
                function!()
            );

            if self.len != 0 {
                unsafe { ptr::copy_nonoverlapping(self.buf, new_buf, self.len) };
            }

            unsafe {
                (self.alloc.free)(self.buf, self.cap);
            }

            self.buf = new_buf;
            self.cap = new_cap;
        }
    }

    pub fn push_slice(&mut self, slice: &[u8]) {
        if slice.len() == 0 {
            return;
        }

        self.reserve(slice.len());

        unsafe {
            ptr::copy_nonoverlapping(slice.as_ptr(), self.buf.add(self.len), slice.len());
        }

        self.len += slice.len();
    }

    pub fn push(&mut self, v: u8) {
        if self.len == self.cap {
            self.reserve(1);
        }

        unsafe { ptr::write(self.buf.add(self.len), v) }
        self.len += 1;
    }

    pub fn copy_slice(&mut self, slice: &[u8]) {
        self.len = 0;
        self.push_slice(slice);
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }
}

impl Extend<u8> for Buffer {
    fn extend<T: IntoIterator<Item = u8>>(&mut self, iter: T) {
        // Pulled from rust std for vec, and modified slightly

        let mut iter = iter.into_iter();
        while let Some(element) = iter.next() {
            if self.len == self.cap {
                let (lower, _) = iter.size_hint();
                self.reserve(lower.saturating_add(1));
            }

            self.push(element);
        }
    }
}

impl Deref for Buffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl AsRef<[u8]> for Buffer {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for Buffer {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

impl IsSane for Buffer {
    fn is_sane(&self) -> bool {
        return (!self.buf.is_null() && self.cap > 0 && self.len <= self.cap)
            || (self.buf.is_null() && self.len == 0 && self.cap == 0);
    }
}

impl fmt::Write for Buffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_slice(s.as_bytes());
        Ok(())
    }
}

/// Creates a new buffer using the internal
/// allocator to this library
/// If you wish to make a buffer with a
/// different allocator, use the new_buffer_with
/// function
#[unsafe(no_mangle)]
pub extern "C" fn http_new_buffer(len: usize) -> Buffer {
    http_new_buffer_with(len, http_internal_allocator())
}

/// Creates a new buffer using the provided
/// allocator.
/// If len is zero, no allocation actually
/// occurs, and the null pointer is instead
/// inserted into the buffer
#[unsafe(no_mangle)]
pub extern "C" fn http_new_buffer_with(len: usize, alloc: Allocator) -> Buffer {
    let ptr = if len > 0 {
        let p = (alloc.alloc)(len);
        assert!(
            p.is_sane(),
            "{}: Pointer returned from alloc must be non-null",
            function!()
        );
        p
    } else {
        ptr::null_mut()
    };

    Buffer {
        len: 0,
        cap: len,
        buf: ptr,
        alloc,
    }
}

/// Copies the contents of the const buffer into the
/// given buffer, reallocating if necessary.
/// Don't create a const buffer from the passed in
/// buffer and use this as a mem-move. It won't work,
/// allocations get involved, it is a mess.
///
/// SAFETY:
///
/// 1) dst must be convertible to a reference
/// 2) dst and src must not overlap at all
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_copy_buffer(dst: *mut Buffer, src: ConstBuffer) {
    assert!(
        dst.is_sane(),
        "{}: Dst is not convertible to a reference",
        function!()
    );
    assert!(
        src.is_sane(),
        "{}: Src is not convertible to a slice",
        function!()
    );

    let dst = unsafe { dst.as_mut_unchecked() };

    assert!(
        dst.is_sane(),
        "{}: Dst is not convertible to a slice",
        function!()
    );

    assert!(
        is_nonoverlapping(dst.buf, dst.cap, src.buf, src.len),
        "{}: Destination and source overlap",
        function!()
    );

    dst.copy_slice(&src);
}

/// Appends the contents the const buffer into the
/// given buffer, reallocating if necessary.
/// Don't create a const buffer from the passed in
/// buffer and use this as a mem-move. It won't work,
/// allocations get involved, it is a mess.
///
/// SAFETY:
///
/// 1) dst must be convertible to a reference
/// 2) dst and src must not overlap at all
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_buffer_push_slice(dst: *mut Buffer, src: ConstBuffer) {
    assert!(
        dst.is_sane(),
        "{}: Dst is not convertible to a reference",
        function!()
    );
    assert!(
        src.is_sane(),
        "{}: Src is not convertible to a slice",
        function!()
    );

    let dst = unsafe { dst.as_mut_unchecked() };

    assert!(
        dst.is_sane(),
        "{}: Dst is not convertible to a slice",
        function!()
    );

    assert!(
        is_nonoverlapping(dst.buf, dst.cap, src.buf, src.len),
        "{}: Destination and source overlap",
        function!()
    );

    dst.push_slice(&src);
}

/// Frees the underlying memory inside the buffer using
/// the contained allocator
/// This does nothing to free a given buffer itself,
/// just frees internal memory of the buffer
/// It also resets the buffer to an empty state, so
/// it can be reused after freeing internal memory
///
/// SAFETY:
///
/// 1) buffer must be convertible to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_clear_buffer(buffer: *mut Buffer) {
    assert!(
        buffer.is_sane(),
        "{}: Buffer is not convertible to a reference",
        function!()
    );

    let buf = unsafe { buffer.as_mut_unchecked() };
    assert!(
        buf.is_sane(),
        "{}: Buffer is not convertible to a slice",
        function!()
    );

    unsafe { (buf.alloc.free)(buf.buf, buf.cap) };

    buf.len = 0;
    buf.cap = 0;
    buf.buf = ptr::null_mut();
}
