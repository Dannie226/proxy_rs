use std::io;

use bstr::BStr;

use crate::{
    Buffer,
    bio::{Reader, Writer},
    http11::ffi::http_http11_response_writer,
    request::Request,
    response::ResponseWriter,
};

pub mod ffi {
    use crate::{
        Buffer,
        bio::ffi::{Reader, Writer},
        request::ffi::Request,
        response::ffi::ResponseWriter,
    };

    unsafe extern "C" {
        // HTTP 1.1
        pub fn http_parse_http11_request(reader: *mut Reader, err: *mut Buffer) -> *mut Request;
        pub fn http_http11_response_writer(writer: *mut Writer) -> *mut ResponseWriter;
    }
}

pub fn parse_request<'a>(reader: Reader<'a>) -> io::Result<Request<'a>> {
    let reader = reader.into_ffi();

    let mut err_buf = [0u8; 512];
    let mut err = Buffer::from(&mut err_buf);
    let req = unsafe { ffi::http_parse_http11_request(reader, &mut err) };

    if req.is_null() {
        Err(io::Error::other(
            BStr::new(err.as_slice().unwrap()).to_string(),
        ))
    } else {
        Ok(unsafe { Request::new(req) })
    }
}

pub fn new_response_writer<'a>(writer: Writer<'a>) -> ResponseWriter<'a> {
    let writer = writer.into_ffi();

    unsafe { ResponseWriter::new(http_http11_response_writer(writer)) }
}
