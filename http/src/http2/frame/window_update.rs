use std::io::{Read, Write};

use crate::{
    IsSane,
    bio::{Reader, Writer},
    function,
    http2::{
        error::{ErrorCode, IOProtoResult},
        frame::{FrameHeader, read_u32, write_header, write_u32},
    },
    result::{HttpResult, set_err, set_ok},
};

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct WindowUpdateFrame {
    stream_id: u32,
    increment: u32,
}

pub const TYPE_NUM: u8 = 0x8;

pub fn read_frame(header: FrameHeader, reader: &mut impl Read) -> IOProtoResult<WindowUpdateFrame> {
    let FrameHeader {
        frame_type,
        frame_len,
        stream_id,
        ..
    } = header;

    assert_eq!(frame_type, TYPE_NUM);

    if frame_len != 4 {
        Err(ErrorCode::FrameSizeError)?
    }

    let increment = read_u32(reader)? & 0x7FFFFFFF;

    if increment == 0 {
        Err(ErrorCode::ProtocolError)?
    }

    Ok(WindowUpdateFrame {
        stream_id,
        increment,
    })
}

/// SAFETY:
///
/// 1) reader must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a WindowUpdateFrame
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_window_update_read_frame(
    reader: *mut Reader,
    header: FrameHeader,
    res: *mut HttpResult,
) -> ErrorCode {
    assert!(
        reader.is_sane(),
        "{}: Reader is not convertible to a reference",
        function!()
    );
    assert!(
        res.is_sane(),
        "{}: Result is not convertible to a reference",
        function!()
    );
    let reader = unsafe { reader.as_mut_unchecked() };
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    match read_frame(header, reader) {
        Ok(w) => {
            unsafe { set_ok(res, w, function!()) };

            ErrorCode::NoError
        }
        Err(e) => set_err!(res, e.get_error_code(), "{e}"),
    }
}

pub fn write_frame(
    stream_id: u32,
    increment: u32,
    writer: &mut impl Write,
) -> IOProtoResult<usize> {
    let mut written = write_header(
        FrameHeader {
            stream_id,
            flags: 0,
            frame_len: 4,
            frame_type: TYPE_NUM,
        },
        writer,
    )?;

    write_u32(increment, writer)?;
    written += 4;

    Ok(written)
}

/// SAFETY:
///
/// 1) writer must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a reference to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_window_update_write_frame(
    writer: *mut Writer,
    stream_id: u32,
    increment: u32,
    res: *mut HttpResult,
) {
    assert!(
        writer.is_sane(),
        "{}: Writer is not convertible to a reference",
        function!()
    );
    assert!(
        res.is_sane(),
        "{}: Result is not convertible to a reference",
        function!()
    );

    let writer = unsafe { writer.as_mut_unchecked() };
    let res = unsafe { res.as_mut_unchecked() };

    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    match write_frame(stream_id, increment, writer) {
        Ok(w) => unsafe { set_ok(res, w, function!()) },
        Err(e) => set_err!(res, (), "{e}"),
    }
}
