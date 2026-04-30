use std::{
    ffi::c_void,
    io::{self, Read, Write},
    slice,
};

use bstr::BString;

pub mod frame;
pub mod hpack;

use crate::{
    HeaderMap, Request, ResponseWriter,
    bio::{Reader, Writer, http_new_empty_reader, writer_from_write},
    response::{StatusCode, http_custom_response_writer},
    result::{http_res_new_ok, result_from_string},
};

use frame::{
    settings::{HTTP2Settings, Settings},
    *,
};
use hpack::tables::HeaderTable;

static PREFACE: [u8; 24] = [
    0x50, 0x52, 0x49, 0x20, 0x2a, 0x20, 0x48, 0x54, 0x54, 0x50, 0x2f, 0x32, 0x2e, 0x30, 0x0d, 0x0a,
    0x0d, 0x0a, 0x53, 0x4d, 0x0d, 0x0a, 0x0d, 0x0a,
];

#[unsafe(no_mangle)]
pub extern "C" fn http_parse_http2_request(
    reader: *mut Reader,
    writer: *mut Writer,
) -> *mut Request {
    let reader = unsafe { reader.as_mut_unchecked() };
    let writer = unsafe { writer.as_mut_unchecked() };
    let mut decode_table = HeaderTable::new(65536);

    let mut preface = [0u8; 24];

    reader
        .read_exact(&mut preface)
        .expect("Failed to read preface");

    println!("{}", preface == PREFACE);

    let settings_header =
        FrameHeader::read_header(reader).expect("Failed to parse settings header");

    println!("Settings header: {settings_header:?}");
    let settings = settings::read_frame(settings_header, reader);

    println!("Settings: {settings:?}");

    settings::write_frame(Settings::Ack, writer).expect("Failed to write ack frame");
    settings::write_frame(Settings::Settings(HTTP2Settings::default()), writer)
        .expect("Failed to write settings frame");

    for _ in 0..4 {
        let header = FrameHeader::read_header(reader).expect("Failed to read next header");
        println!("Got Header: {header:?}");

        match header.frame_type {
            data::TYPE_NUM => {
                let mut data = vec![0u8; header.frame_len as usize];
                reader
                    .read_exact(&mut data)
                    .expect("Failed to read data frame");

                println!("Got data frame: {}", BString::from(data));
            }
            header::TYPE_NUM => {
                let headers = header::read_frame(header, &mut decode_table, reader)
                    .expect("Failed to read headers");
                println!("Got Header Frame: {headers:#?}");
            }
            priority::TYPE_NUM => {
                let prio =
                    priority::read_frame(header, reader).expect("Failed to read priority frame");
                println!("Got Priority Frame: {prio:?}");
            }
            rst_stream::TYPE_NUM => {
                let err = rst_stream::read_frame(header, reader)
                    .expect("Failed to read reset stream frame");

                println!("Got reset stream: {err}");
            }
            settings::TYPE_NUM => {
                let settings =
                    settings::read_frame(header, reader).expect("Failed to read settings frame");

                println!("Got settings frame: {settings:?}");
            }
            ping::TYPE_NUM => {
                let (payload, ack) =
                    ping::read_frame(header, reader).expect("Failed to read ping data");

                println!("Got ping data: {:X}", payload);

                if !ack {
                    ping::write_frame(payload, true, writer)
                        .expect("Failed to write ping frame ack");
                }
            }
            go_away::TYPE_NUM => {
                panic!("Got a go away frame");
            }
            window_update::TYPE_NUM => {
                let increment =
                    window_update::read_frame(header, reader).expect("Failed to read window frame");

                println!("Got window increment frame: {increment}");
            }
            t => {
                println!("Unknown frame type: {t:X}")
            }
        }
    }

    unsafe {
        Request::new(
            "hi".into(),
            "/".into(),
            (2, 0),
            HeaderMap::new(),
            http_new_empty_reader(),
        )
    }
}

struct FrameWriter(u32, *mut Writer);

impl Write for FrameWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        unsafe { self.1.as_mut_unchecked() }.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

extern "C" fn http2_write_ok(
    writer: *mut ResponseWriter,
    _: StatusCode,
    written: *mut crate::result::Result,
) {
    // SAFETY: writer is convertable to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    let inner = unsafe { writer.writer.as_mut_unchecked() };

    // This cast is OK because the writer passed in is a frame writer
    let frame_writer = inner.data.cast::<FrameWriter>();
    // SAFETY: writer data is convertable to a reference
    let frame_writer = unsafe { frame_writer.as_mut_unchecked() };

    // SAFETY: result is convertable to a reference
    let written = unsafe { written.as_mut_unchecked() };

    match header::write_ok(frame_writer.0, unsafe { frame_writer.1.as_mut_unchecked() }) {
        Ok(()) => {
            *written = http_res_new_ok(0);
        }
        Err(e) => *written = result_from_string(format!("{e}")),
    };
}

extern "C" fn http2_write_data(
    writer: *mut Writer,
    data: *const c_void,
    len: usize,
    written: *mut crate::result::Result,
) {
    // SAFETY: writer is convertable to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    let data = unsafe { slice::from_raw_parts(data.cast(), len) };

    let written = unsafe { written.as_mut_unchecked() };

    // This cast is OK because the writer passed in is a frame writer
    let frame_writer = writer.data.cast::<FrameWriter>();
    // SAFETY: writer data is convertable to a reference
    let frame_writer = unsafe { frame_writer.as_mut_unchecked() };

    match data::write_frame(frame_writer.0, data, true, frame_writer) {
        Ok(()) => {
            *written = http_res_new_ok(data.len());
        }
        Err(e) => *written = result_from_string(format!("{e}")),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn http_http2_response_writer(
    stream_id: u32,
    writer: *mut Writer,
) -> *mut ResponseWriter {
    let writer = FrameWriter(stream_id, writer);
    let writer = writer_from_write(writer);

    // SAFETY:
    // This specific writer is supposed to go into the
    // writer argument
    // The other safety arguments for the functions themselves apply
    unsafe { http_custom_response_writer(writer, http2_write_ok, http2_write_data) }
}
