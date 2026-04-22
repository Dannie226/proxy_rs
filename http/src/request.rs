use std::{collections::HashMap, ffi::*, fmt::Debug, io::Read, slice};

use bstr::BString;

use crate::read_string_to_buf;

pub type HeaderMap = HashMap<BString, Vec<BString>>;

pub struct Request<'a> {
    pub(crate) method: String,
    pub(crate) uri: String,
    pub(crate) version: (u32, u32),
    pub(crate) headers: HeaderMap,
    pub(crate) body: Box<dyn Read + 'a>,
}

impl<'a> Debug for Request<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Request {{
\tmethod: {:?}
\turi: {:?}
\tversion: {:?}
\theaders: {:?}
        }}",
            self.method, self.uri, self.version, self.headers
        )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn http_read_body(request: *mut Request<'_>, data: *mut u8, len: usize) -> c_int {
    let data = unsafe { slice::from_raw_parts_mut(data, len) };
    let Some(req) = (unsafe { request.as_mut() }) else {
        return -1;
    };

    let err = req.body.read(data);
    match err {
        Ok(s) => s as c_int,
        Err(e) => -(e.kind() as c_int) - 2,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn http_get_method(
    request: *const Request<'_>,
    string: *mut c_char,
    str_len: usize,
) -> c_int {
    let Some(req) = (unsafe { request.as_ref() }) else {
        return -1;
    };
    let string = unsafe { slice::from_raw_parts_mut(string, str_len) };

    read_string_to_buf(&req.method, string)
}

#[unsafe(no_mangle)]
pub extern "C" fn http_get_uri(
    request: *const Request<'_>,
    string: *mut c_char,
    str_len: usize,
) -> c_int {
    let Some(req) = (unsafe { request.as_ref() }) else {
        return -1;
    };
    let string = unsafe { slice::from_raw_parts_mut(string, str_len) };

    read_string_to_buf(&req.uri, string)
}

#[unsafe(no_mangle)]
pub extern "C" fn get_version(
    request: *const Request<'_>,
    major: *mut u32,
    minor: *mut u32,
) -> c_int {
    let Some(req) = (unsafe { request.as_ref() }) else {
        return -1;
    };

    let Some(major) = (unsafe { major.as_mut() }) else {
        return -1;
    };

    let Some(minor) = (unsafe { minor.as_mut() }) else {
        return -1;
    };

    *major = req.version.0;
    *minor = req.version.1;

    0
}
