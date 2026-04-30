use std::{
    ffi::{c_char, c_int, c_void},
    marker::{PhantomData, PhantomPinned},
};

pub type StatusFn =
    unsafe extern "C" fn(writer: *mut ResponseWriter, code: StatusCode, res: *mut Result);

#[repr(C)]
pub struct Result {
    _data: [usize; 2],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct HeaderTable {
    _data: (),
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

unsafe extern "C" {
    // HTTP 2.0
    pub fn http_parse_http2_request(reader: *mut Reader, writer: *mut Writer) -> *mut Request;
    pub fn http_http2_response_writer(stream_id: u32, writer: *mut Writer) -> *mut ResponseWriter;
}
