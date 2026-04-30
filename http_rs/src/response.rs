use std::{
    io::{self, Write},
    marker::PhantomData,
};

use crate::{ConstBuffer, result::Result};

pub mod ffi {
    use std::marker::{PhantomData, PhantomPinned};

    use crate::{ConstBuffer, result::ffi::Result};

    #[repr(C)]
    pub struct ResponseWriter {
        _data: (),
        _marker: PhantomData<(*mut u8, PhantomPinned)>,
    }

    type StatusCode = u16;

    unsafe extern "C" {
        // Response data
        pub fn http_write(
            writer: *mut ResponseWriter,
            data: *const u8,
            len: usize,
            written: *mut Result,
        );
        pub fn http_add_header(writer: *mut ResponseWriter, name: ConstBuffer, value: ConstBuffer);
        pub fn http_remove_headers(writer: *mut ResponseWriter, header: ConstBuffer);
        pub fn http_write_status(writer: *mut ResponseWriter, code: StatusCode, res: *mut Result);
        pub fn http_destroy_response_writer(writer: *mut ResponseWriter);
    }
}

pub enum StatusCode {
    Continue = 100,
    SwitchingProtocols = 101,
    Processing = 102,
    EarlyHints = 103,

    OK = 200,
    Created = 201,
    Accepted = 202,
    NonAuthoritativeInformation = 203,
    NoContent = 204,
    ResetContent = 205,
    PartialContent = 206,

    MultipleChoices = 300,
    MovedPermanently = 301,
    Found = 302,
    SeeOther = 303,
    NotModified = 304,
    TemporaryRedirect = 307,
    PermanentRedirect = 308,

    BadRequest = 400,
    Unauthorized = 401,
    PaymentRequired = 402,
    Forbidden = 403,
    NotFound = 404,
    MethodNotAllowed = 405,
    NotAcceptable = 406,
    ProxyAuthenticationRequired = 407,
    RequestTimeout = 408,
    Conflict = 409,
    Gone = 410,
    LengthRequired = 411,
    PreconditionFailed = 412,
    ContentTooLarge = 413,
    URITooLong = 414,
    UnsupportedMediaType = 415,
    RangeNotSatisfiable = 416,
    ExpectationFailed = 417,
    ImATeapot = 418,
    MisdirectedRequest = 421,
    UpgradeRequired = 426,
    PreconditionRequired = 428,
    TooManyRequests = 429,
    RequestHeaderFieldsTooLarge = 431,
    UnavailableForLegalReasons = 451,

    InternalServerError = 500,
    NotImplemented = 501,
    BadGateway = 502,
    ServiceUnavailable = 503,
    GatewayTimeout = 504,
    HTTPVersionNotSupported = 505,
    VariantAlsoNegotiates = 506,
    NotExtended = 510,
    NetworkAuthenticationRequired = 511,
}

pub struct ResponseWriter<'a>(*mut ffi::ResponseWriter, PhantomData<&'a ()>);

impl<'a> ResponseWriter<'a> {
    pub(crate) unsafe fn new(writer: *mut ffi::ResponseWriter) -> ResponseWriter<'a> {
        ResponseWriter(writer, PhantomData)
    }

    pub fn add_header(&mut self, name: &[u8], value: &[u8]) {
        let name = ConstBuffer::from(name);
        let value = ConstBuffer::from(value);

        unsafe {
            ffi::http_add_header(self.0, name, value);
        }
    }

    pub fn remove_headers(&mut self, name: &[u8]) {
        let name = ConstBuffer::from(name);

        unsafe {
            ffi::http_remove_headers(self.0, name);
        }
    }

    pub fn write_status(&mut self, code: StatusCode) -> io::Result<usize> {
        let mut res = Result::ok(0).into_ffi();
        unsafe { ffi::http_write_status(self.0, code as u16, &mut res) };

        Result::from_ffi(res).into_io_result()
    }

    pub fn get_ffi(&self) -> *mut ffi::ResponseWriter {
        self.0
    }
}

impl<'a> Write for ResponseWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut res = Result::ok(0).into_ffi();

        unsafe { ffi::http_write(self.0, buf.as_ptr(), buf.len(), &mut res) };

        Result::from_ffi(res).into_io_result()
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> Drop for ResponseWriter<'a> {
    fn drop(&mut self) {
        unsafe {
            ffi::http_destroy_response_writer(self.0);
        }
    }
}

// SAFETY:
// ResponseWriter can be sent to another thread, as it holds the unique
// pointer to the underlying writer
// Yes, the get_ffi method exists, but to use the pointer, you have to use
// unsafe code anyways, so it isn't really violating safety requirements
// This also means it is ok to Sync the writer across threads
unsafe impl<'a> Send for ResponseWriter<'a> {}
unsafe impl<'a> Sync for ResponseWriter<'a> {}
