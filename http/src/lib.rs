pub mod bio;
pub mod http11;
pub mod http2;
pub mod request;
pub mod response;
pub mod result;

pub use request::HeaderMap;
pub use request::Request;
pub use response::ResponseWriter;

use core::slice;
use std::ffi::c_int;
use std::io;
use std::marker::PhantomData;
use std::ptr;
use thiserror::Error;

/// Repr C struct for holding a mutable slice
/// Just nicer to pass this around as opposed to
/// pointer/len pairs
/// Invariants:
/// Data is either null, or convertable to a slice of len bytes
#[repr(C)]
pub struct Buffer<'a> {
    pub len: usize,
    pub data: *mut u8,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> Buffer<'a> {
    pub fn from(buf: &'a mut [u8]) -> Buffer<'a> {
        Buffer {
            len: buf.len(),
            data: buf.as_mut_ptr(),
            _marker: PhantomData,
        }
    }

    // SAFETY:
    // Data is either null or convertable to a slice of len bytes
    pub unsafe fn from_raw(data: *mut u8, len: usize) -> Buffer<'a> {
        Buffer {
            len,
            data,
            _marker: PhantomData,
        }
    }

    pub fn empty() -> Buffer<'a> {
        Buffer {
            len: 0,
            data: ptr::null_mut(),
            _marker: PhantomData,
        }
    }

    pub fn as_slice(&self) -> Option<&'a mut [u8]> {
        if !self.data.is_null() {
            // SAFETY:
            // Data is convertable to a slice of len bytes because
            // data isn't null
            Some(unsafe { slice::from_raw_parts_mut(self.data, self.len) })
        } else {
            None
        }
    }

    pub fn copy_slice(&mut self, src: &[u8]) -> c_int {
        let Some(buf) = self.as_slice() else {
            self.len = src.len();
            return 1;
        };

        if buf.len() < src.len() {
            return 2;
        }

        buf[..src.len()].copy_from_slice(src);
        self.len = src.len();
        0
    }
}

/// Repr C struct for holding an immuatable slice
/// Just nicer to pass this around as opposed to
/// pointer/len pairs
/// Invariants:
/// Data is convertable to a slice of len bytes
#[derive(Clone, Copy)]
#[repr(C)]
pub struct ConstBuffer<'a> {
    pub len: usize,
    pub data: *const u8,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> ConstBuffer<'a> {
    pub fn from(buf: &'a [u8]) -> ConstBuffer<'a> {
        ConstBuffer {
            len: buf.len(),
            data: buf.as_ptr(),
            _marker: PhantomData,
        }
    }

    // SAFETY:
    // Data must be convertable to a slice of len bytes
    pub unsafe fn from_raw(data: *const u8, len: usize) -> ConstBuffer<'a> {
        ConstBuffer {
            len,
            data,
            _marker: PhantomData,
        }
    }

    pub fn as_slice(&self) -> &'a [u8] {
        // SAFETY:
        // Data is convertable to a slice of len bytes
        // data isn't null
        unsafe { slice::from_raw_parts(self.data, self.len) }
    }
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Failed to read data: {0}")]
    IO(#[from] io::Error),

    #[error("Protocol error occurred: {0}")]
    Protocol(u32),
}

pub type Result<T> = std::result::Result<T, ProtocolError>;
