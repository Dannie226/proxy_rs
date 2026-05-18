use std::io::{Read, Write};

use crate::{
    IsSane,
    bio::{Reader, Writer},
    function,
    http2::{
        error::{ErrorCode, IOProtoResult},
        frame::{FrameHeader, read_u64, write_header, write_u64},
    },
    result::{HttpResult, set_err, set_ok},
};

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct PingFrame {
    data: u64,
    is_ack: bool,
}

pub const TYPE_NUM: u8 = 0x6;

pub const FLAGS_ACK: u8 = 0x1;

pub fn read_frame(header: FrameHeader, reader: &mut impl Read) -> IOProtoResult<PingFrame> {
    let FrameHeader {
        frame_type,
        frame_len,
        stream_id,
        flags,
    } = header;

    assert_eq!(frame_type, TYPE_NUM);

    if stream_id != 0 {
        Err(ErrorCode::ProtocolError)?
    }

    if frame_len != 8 {
        Err(ErrorCode::FrameSizeError)?
    }

    let data = read_u64(reader)?;
    let is_ack = flags & FLAGS_ACK != 0;

    Ok(PingFrame { data, is_ack })
}

/// SAFETY:
///
/// 1) reader must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a PingFrame
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_ping_read_frame(
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

pub fn write_frame(frame: PingFrame, writer: &mut impl Write) -> std::io::Result<usize> {
    let mut written = write_header(
        FrameHeader {
            stream_id: 0,
            frame_len: 8,
            frame_type: TYPE_NUM,
            flags: if frame.is_ack { FLAGS_ACK } else { 0 },
        },
        writer,
    )?;

    write_u64(frame.data, writer)?;
    written += 8;

    Ok(written)
}

/// SAFETY:
///
/// 1) writer must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_ping_write_frame(
    writer: *mut Writer,
    frame: PingFrame,
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

    match write_frame(frame, writer) {
        Ok(l) => unsafe { set_ok(res, l, function!()) },
        Err(e) => set_err!(res, (), "{e}"),
    };
}
