use std::{io::Read, marker::PhantomData};

use bstr::{BStr, BString};

use crate::{Buffer, ConstBuffer, result::Result};

pub mod ffi {
    use std::{
        ffi::c_int,
        marker::{PhantomData, PhantomPinned},
    };

    use crate::{Buffer, ConstBuffer, result::ffi::Result};

    #[repr(C)]
    pub struct Request {
        _data: (),
        _marker: PhantomData<(*mut u8, PhantomPinned)>,
    }

    unsafe extern "C" {
        // Request data
        pub fn http_read_body(request: *mut Request, data: *mut u8, len: usize, read: *mut Result);
        pub fn http_get_method(request: *const Request, string: *mut Buffer) -> c_int;
        pub fn http_get_uri(request: *const Request, string: *mut Buffer) -> c_int;
        pub fn http_get_version(request: *const Request, major: *mut u32, minor: *mut u32);
        pub fn http_get_header_count(request: *const Request, header: ConstBuffer) -> usize;
        pub fn http_get_header(
            request: *const Request,
            name: ConstBuffer,
            header_index: usize,
            value: *mut Buffer,
        ) -> c_int;
        pub fn http_destroy_request(request: *mut Request);
    }
}

pub struct Request<'a>(*mut ffi::Request, PhantomData<&'a ()>);

impl<'a> Request<'a> {
    // SAFETY:
    // req has to be a valid request
    pub(crate) unsafe fn new(req: *mut ffi::Request) -> Request<'a> {
        Request(req, PhantomData)
    }

    pub fn get_method(&self) -> BString {
        let mut b = Buffer::empty();

        // SAFETY: Buffer is valid, as is self.0
        unsafe { ffi::http_get_method(self.0, &raw mut b) };

        let mut v = vec![0u8; b.len];

        b.data = v.as_mut_ptr();

        // SAFETY: Buffer is valid, as is self.0
        unsafe { ffi::http_get_method(self.0, &mut b) };

        return BString::new(v);
    }

    pub fn get_uri(&self) -> BString {
        let mut b = Buffer::empty();

        // SAFETY: Buffer is valid, as is self.0
        unsafe { ffi::http_get_uri(self.0, &raw mut b) };

        let mut v = vec![0u8; b.len];

        b.data = v.as_mut_ptr();

        // SAFETY: Buffer is valid, as is self.0
        unsafe { ffi::http_get_uri(self.0, &mut b) };

        return BString::new(v);
    }

    pub fn get_version(&self) -> (u32, u32) {
        let mut major = 0;
        let mut minor = 0;

        // SAFETY: self.0 is valud, and major/minor are references
        unsafe { ffi::http_get_version(self.0, &mut major, &mut minor) };

        (major, minor)
    }

    pub fn get_headers(&self, header_name: &BStr) -> Vec<BString> {
        // SAFETY: self.0 is valid, and ConstBuffer is from a slice
        let header_name = ConstBuffer::from(header_name);
        let count = unsafe { ffi::http_get_header_count(self.0, header_name) };

        let mut headers = Vec::with_capacity(count);

        for i in 0..count {
            let mut b = Buffer::empty();

            // SAFETY: self.0 is valid, and ConstBuffer is from a slice, buffer is empty
            unsafe { ffi::http_get_header(self.0, header_name, i, &mut b) };

            let mut v = vec![0u8; b.len];
            b.data = v.as_mut_ptr();

            // SAFETY: self.0 is valid, and ConstBuffer is from a slice, buffer's data is valid for
            // len bytes
            unsafe { ffi::http_get_header(self.0, header_name, i, &mut b) };

            headers.push(BString::from(v));
        }

        headers
    }

    pub fn get_ffi(&self) -> *mut ffi::Request {
        self.0
    }
}

impl<'a> Read for Request<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut res = Result::ok(0).into_ffi();

        // SAFETY:
        // self.0 valid, buf is a slice, and res is a reference
        unsafe {
            ffi::http_read_body(self.0, buf.as_mut_ptr(), buf.len(), &mut res);
        }

        Result::from_ffi(res).into_io_result()
    }
}

impl<'a> Drop for Request<'a> {
    fn drop(&mut self) {
        unsafe {
            ffi::http_destroy_request(self.0);
        }
    }
}

// SAFETY:
// Request can be sent to another thread, as it holds the unique
// pointer to the underlying request
// Yes, the get_ffi method exists, but to use the pointer, you have to use
// unsafe code anyways, so it isn't really violating safety requirements
// All of the methods that take a &self also take a *const Request, and so
// are safe to use across threads as the underlying data cannot be modified
// therefore, Request is Sync too
unsafe impl<'a> Send for Request<'a> {}
unsafe impl<'a> Sync for Request<'a> {}
