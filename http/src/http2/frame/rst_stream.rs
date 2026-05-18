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

const TYPE_NUM: u8 = 0x3;

pub fn read_frame(header: FrameHeader, reader: &mut impl Read) -> IOProtoResult<u32> {
    let FrameHeader {
        frame_type,
        frame_len,
        stream_id,
        ..
    } = header;

    assert_eq!(frame_type, TYPE_NUM);

    if stream_id == 0 {
        Err(ErrorCode::ProtocolError)?
    }

    if frame_len != 4 {
        Err(ErrorCode::FrameSizeError)?
    }

    read_u32(reader).map_err(Into::into)
}

/// SAFETY:
///
/// 1) reader must be convertible to a reference
/// 2) error_code must be convertible to a reference
/// 3) res must be convertible to a reference
/// 4) res must be a result to a u32
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_rst_stream_read_frame(
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
        Ok(frame) => {
            unsafe { set_ok(res, frame, function!()) };
            ErrorCode::NoError
        }
        Err(e) => set_err!(res, e.get_error_code(), "{e}"),
    }
}

pub fn write_frame(
    stream_id: u32,
    error_code: u32,
    writer: &mut impl Write,
) -> std::io::Result<usize> {
    let mut written = write_header(
        FrameHeader {
            stream_id,
            frame_len: 4,
            frame_type: TYPE_NUM,
            flags: 0,
        },
        writer,
    )?;

    write_u32(error_code, writer)?;
    written += 4;

    Ok(written)
}

/// SAFETY:
///
/// 1) writer must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_rst_stream_write_frame(
    writer: *mut Writer,
    stream_id: u32,
    error_code: u32,
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

    match write_frame(stream_id, error_code, writer) {
        Ok(l) => unsafe { set_ok(res, l, function!()) },
        Err(e) => set_err!(res, (), "{e}"),
    };
}
