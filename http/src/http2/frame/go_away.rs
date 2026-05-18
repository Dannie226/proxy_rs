use std::io::{Read, Write};

use crate::{
    IsSane,
    bio::{Reader, Writer},
    buffer::{Buffer, ConstBuffer},
    function,
    http2::{
        error::{ErrorCode, IOProtoResult},
        frame::{FrameHeader, read_u32, write_header, write_u32},
    },
    result::{HttpResult, set_err, set_ok},
};

#[repr(C)]
pub struct GoAwayFrame {
    last_id: u32,
    error_code: u32,
    debug_data: Buffer,
}

pub const TYPE_NUM: u8 = 0x7;

/// Reads a go away frame from the given reader
///
/// On an error, the contents of the debug data buffer
/// are undefined. The buffer is still structurally
/// valid, so you can reset the length and move
/// on, but any contents of the buffer cannot be
/// trusted.
pub fn read_frame(
    header: FrameHeader,
    data: &mut GoAwayFrame,
    reader: &mut impl Read,
) -> IOProtoResult<()> {
    let FrameHeader {
        stream_id,
        mut frame_len,
        frame_type,
        ..
    } = header;

    assert_eq!(frame_type, TYPE_NUM);

    if stream_id != 0 {
        Err(ErrorCode::ProtocolError)?
    }

    if frame_len < 8 {
        Err(ErrorCode::FrameSizeError)?
    }

    let stream_id = read_u32(reader)? & 0x7FFFFFFF;
    let error_code = read_u32(reader)?;

    frame_len -= 8;

    data.debug_data.reserve_len(frame_len as usize);
    data.debug_data.len = frame_len as usize;

    reader.read_exact(&mut data.debug_data)?;

    data.last_id = stream_id;
    data.error_code = error_code;

    Ok(())
}

/// SAFETY:
///
/// 1) reader must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a GoAwayFrame
///
/// On error, the contents of the debug data buffer are undefined
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_go_away_read_frame(
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

    let ok = res.ok.cast::<GoAwayFrame>();
    assert!(
        ok.is_sane(),
        "{}: Result ok is not convertible to a reference to a GoAwayFrame",
        function!()
    );

    let frame = unsafe { ok.as_mut_unchecked() };

    match read_frame(header, frame, reader) {
        Ok(()) => {
            res.is_ok = true;

            ErrorCode::NoError
        }
        Err(e) => set_err!(res, e.get_error_code(), "{e}"),
    }
}

pub fn write_frame(
    last_id: u32,
    error: ErrorCode,
    data: &[u8],
    writer: &mut impl Write,
) -> std::io::Result<usize> {
    let mut written = write_header(
        FrameHeader {
            stream_id: 0,
            frame_len: data.len() as u32 + 8,
            frame_type: TYPE_NUM,
            flags: 0,
        },
        writer,
    )?;

    write_u32(last_id & 0x7FFFFFFF, writer)?;
    written += 4;

    write_u32(error as u32, writer)?;
    written += 4;

    writer.write_all(data)?;
    written += data.len();

    Ok(written)
}

/// SAFETY:
///
/// 1) writer must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_go_away_write_frame(
    writer: *mut Writer,
    last_id: u32,
    error: ErrorCode,
    data: ConstBuffer,
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
        "Result error is not convertible to a slice"
    );

    match write_frame(last_id, error, &data, writer) {
        Ok(e) => unsafe { set_ok(res, e, function!()) },
        Err(e) => set_err!(res, (), "{e}"),
    }
}
