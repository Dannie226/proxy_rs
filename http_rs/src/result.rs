use std::{io, mem};

use bstr::{BStr, BString};

use crate::{Buffer, ConstBuffer};

pub mod ffi {
    use std::ffi::{c_char, c_int};

    use crate::{Buffer, ConstBuffer};

    #[repr(C)]
    pub struct Result {
        _data: [usize; 2],
    }

    unsafe extern "C" {
        pub fn http_res_new_err(str: *const c_char) -> Result;
        pub fn http_res_new_err_buf(buf: ConstBuffer) -> Result;
        pub fn http_res_new_ok(count: usize) -> Result;
        pub fn http_destroy_res(res: *mut Result);
        pub fn http_res_is_ok(res: *const Result) -> bool;
        pub fn http_res_get_count(res: *const Result) -> usize;
        pub fn http_res_get_err(res: *const Result, buf: *mut Buffer) -> c_int;
    }
}

pub struct Result(ffi::Result);

impl Result {
    pub fn ok(count: usize) -> Result {
        // SAFETY: http_res_new_ok is safe, c ffi just kinda sucks...
        Result(unsafe { ffi::http_res_new_ok(count) })
    }

    pub fn err(str: &BStr) -> Result {
        // SAFETY: http_res_new_err_buf is safe as long as the invariants
        // of ConstBuffer are upheld, and because ConstBuffer::from is safe,
        // the call itself is safe
        Result(unsafe { ffi::http_res_new_err_buf(ConstBuffer::from(str)) })
    }

    pub fn is_ok(&self) -> bool {
        // SAFETY: self.0 is a valid result
        unsafe { ffi::http_res_is_ok(&self.0) }
    }

    pub fn is_err(&self) -> bool {
        !self.is_ok()
    }

    pub fn into_io_result(self) -> std::io::Result<usize> {
        let r = if self.is_ok() {
            // SAFETY: this call is safe
            Ok(unsafe { ffi::http_res_get_count(&self.0) })
        } else {
            let mut b = Buffer::empty();

            // SAFETY: This call is safe on any Buffer that satisfies
            // it's invariants. In this case, b has null data, and so
            // res_get_err returns in it's length field how large the
            // error is
            // And, self.0 is a valid result
            unsafe { ffi::http_res_get_err(&self.0, &mut b) };

            let mut v = vec![0u8; b.len];

            b.data = v.as_mut_ptr();

            // SAFETY: Buffer's data is a pointer to a slice of b.len bytes
            // and self.0 is a valid result
            unsafe { ffi::http_res_get_err(&self.0, &mut b) };

            Err(io::Error::other(BString::new(v).to_string()))
        };

        r
    }

    pub fn into_ffi(mut self) -> ffi::Result {
        // SAFETY: This call is safe
        let mut r = unsafe { ffi::http_res_new_ok(0) };

        mem::swap(&mut r, &mut self.0);

        // Nothing else needs to happen with self, as http_res_new_ok doesn't allocate
        mem::forget(self);

        r
    }

    pub fn from_io_result(res: std::io::Result<usize>) -> Self {
        match res {
            Ok(v) => Self::ok(v),
            Err(e) => {
                let msg = format!("{e}");

                Self::err(msg.as_bytes().into())
            }
        }
    }

    pub fn from_ffi(res: ffi::Result) -> Self {
        Result(res)
    }
}

impl Drop for Result {
    fn drop(&mut self) {
        // SAFETY:
        // self.0 is a valid result
        unsafe { ffi::http_destroy_res(&mut self.0) }
    }
}
