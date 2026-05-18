use std::io::{Read, Write};

use crate::{
    IsSane,
    bio::{Reader, Writer},
    function,
    http2::{
        error::{ErrorCode, IOProtoResult},
        frame::{FrameHeader, read_u8, read_u32, write_header, write_u8, write_u32},
    },
    result::{HttpResult, set_err, set_ok},
};

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct PriorityFrame {
    stream_dep: u32,
    exclusive: bool,
    weight: u8,
}

pub const TYPE_NUM: u8 = 0x2;

pub fn read_frame(header: FrameHeader, reader: &mut impl Read) -> IOProtoResult<PriorityFrame> {
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

    if frame_len != 5 {
        Err(ErrorCode::FrameSizeError)?
    }

    let stream_dep = read_u32(reader)?;
    let exclusive = stream_dep & 0x80000000 != 0;
    let stream_dep = stream_dep & 0x7FFFFFFF;

    if stream_dep == stream_id {
        Err(ErrorCode::ProtocolError)?
    }

    let weight = read_u8(reader)?;

    Ok(PriorityFrame {
        stream_dep: stream_dep,
        weight,
        exclusive,
    })
}

/// SAFETY:
///
/// 1) reader must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a PriorityFrame
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_priority_read_frame(
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
    frame: PriorityFrame,
    writer: &mut impl Write,
) -> std::io::Result<usize> {
    let mut buf = [0u8; 5];
    let mut buf_writer = buf.as_mut_slice();

    write_u32(
        frame.stream_dep | if frame.exclusive { 0x80000000 } else { 0x0 },
        &mut buf_writer,
    )?;
    write_u8(frame.weight, &mut buf_writer)?;

    let mut written = write_header(
        FrameHeader {
            stream_id,
            frame_len: 5,
            frame_type: TYPE_NUM,
            flags: 0,
        },
        writer,
    )?;

    writer.write_all(&buf[..5])?;
    written += 5;

    Ok(written)
}

/// SAFETY:
///
/// 1) writer must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a reference to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_priority_write_frame(
    writer: *mut Writer,
    stream_id: u32,
    frame: PriorityFrame,
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

    match write_frame(stream_id, frame, writer) {
        Ok(l) => unsafe { set_ok(res, l, function!()) },
        Err(e) => set_err!(res, (), "{e}"),
    };
}
