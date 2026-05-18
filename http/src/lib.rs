pub mod bio;
pub mod buffer;
pub mod http11;
pub mod http2;
pub mod request;
pub mod response;
pub mod result;

pub use request::HeaderMap;
pub use request::Request;
pub use response::ResponseWriter;

/// Trait for determining if a value is sane
/// Not necessarily that it is valid for conversions,
/// just that it is a sane value
///
/// For pointers, it is checking nullity and alignment
/// For other values, it is just a preliminary sanity check
/// to ensure you aren't doing anything obviously wrong
pub(crate) trait IsSane {
    fn is_sane(&self) -> bool;
}

impl<T> IsSane for *const T {
    fn is_sane(&self) -> bool {
        !self.is_null() && self.is_aligned()
    }
}

impl<T> IsSane for *mut T {
    fn is_sane(&self) -> bool {
        <*const T as IsSane>::is_sane(&self.cast_const())
    }
}

pub(crate) fn is_nonoverlapping(
    dst: *const u8,
    dst_len: usize,
    src: *const u8,
    src_len: usize,
) -> bool {
    return dst.addr().saturating_add(dst_len) <= src.addr()
        || src.addr().saturating_add(src_len) <= dst.addr();
}

#[cfg(test)]
pub mod test_utils {
    use std::cell::Cell;

    use crate::buffer::{Allocator, internal_alloc, internal_free};

    pub(crate) fn parse_hex(data: &str) -> Option<Box<[u8]>> {
        if data.len() % 2 != 0 {
            println!("Len not multiple of two");
            return None;
        }

        let mut b = vec![0u8; data.len() / 2].into_boxed_slice();

        for i in 0..data.len() / 2 {
            b[i] = u8::from_str_radix(&data[i * 2..i * 2 + 2], 16)
                .inspect_err(|e| println!("{e}"))
                .ok()?;
        }

        Some(b)
    }

    pub(crate) fn to_hex(data: &[u8]) -> String {
        use std::fmt::Write;

        let mut s = String::with_capacity(data.len() * 2);

        for &d in data {
            _ = write!(s, "{d:02x}");
        }

        s
    }

    thread_local! {
        static ALLOCED: Cell<usize> = Cell::new(0);
        static FREED: Cell<usize> = Cell::new(0);
    }

    extern "C" fn test_alloc(len: usize) -> *mut u8 {
        ALLOCED.set(ALLOCED.get() + len);
        internal_alloc(len)
    }

    extern "C" fn test_free(ptr: *mut u8, len: usize) {
        FREED.set(FREED.get() + len);
        unsafe { internal_free(ptr, len) };
    }

    pub(crate) fn new_test_allocator() -> Allocator {
        Allocator {
            alloc: test_alloc,
            free: test_free,
        }
    }

    pub(crate) fn finish_test() {
        let a = ALLOCED.take();
        let f = FREED.take();

        assert_eq!(a, f);
    }
}

macro_rules! function {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let name = type_name_of(f);

        // Find and cut the rest of the path
        match &name[..name.len() - 3].rfind(':') {
            Some(pos) => &name[pos + 1..name.len() - 3],
            None => &name[..name.len() - 3],
        }
    }};
}

pub(crate) use function;
