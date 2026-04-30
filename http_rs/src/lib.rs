use core::slice;
use std::{marker::PhantomData, ptr};

pub mod bio;
pub mod http11;
pub mod request;
pub mod response;
pub mod result;

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

    pub fn empty() -> Buffer<'a> {
        Buffer {
            len: 0,
            data: ptr::null_mut(),
            _marker: PhantomData,
        }
    }

    pub fn as_slice(&self) -> Option<&'a mut [u8]> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { slice::from_raw_parts_mut(self.data, self.len) })
    }
}

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
}
