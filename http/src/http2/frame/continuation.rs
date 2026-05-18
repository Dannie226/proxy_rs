use std::io::{Read, Write};

use crate::{
    IsSane,
    bio::{Reader, Writer},
    buffer::{Buffer, ConstBuffer},
    function,
    http2::{
        error::{ErrorCode, IOProtoResult},
        frame::{FrameHeader, header::FLAGS_END_HEADERS, write_header},
    },
    result::{HttpResult, set_err, set_ok},
};

#[repr(C)]
pub struct ContinuationFrame {
    data: Buffer,
    is_end: bool,
}

pub const TYPE_NUM: u8 = 0x9;

/// Reads a continuation frame from the given reader
/// Appends to the buffer, unlike most other functions.
/// This can be used if you have streamed headers and got
/// a TooSmall error when decoding, but still have bytes left.
/// Preferably shift those bytes to the front, update the length,
/// and then call this, but there are no requirements for doing
/// that.
///
/// On an error, the contents of the data buffer
/// are undefined. The buffer is still structurally
/// valid, so you can reset the length and move
/// on, but any contents of the buffer cannot be
/// trusted.
pub fn read_frame(
    header: FrameHeader,
    out: &mut ContinuationFrame,
    reader: &mut impl Read,
) -> IOProtoResult<()> {
    let FrameHeader {
        frame_type,
        frame_len,
        stream_id,
        flags,
    } = header;

    assert_eq!(frame_type, TYPE_NUM);

    if stream_id == 0 {
        Err(ErrorCode::ProtocolError)?
    }

    out.data.reserve(frame_len as usize);
    let start = out.data.len;

    out.data.len += frame_len as usize;

    reader.read_exact(&mut out.data[start..])?;

    out.is_end = flags & FLAGS_END_HEADERS != 0;

    Ok(())
}

/// SAFETY:
///
/// 1) reader must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a ContinuationFrame
///
/// On error, the contents of the data buffer are undefined
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_continuation_read_frame(
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
        function!(),
    );
    let reader = unsafe { reader.as_mut_unchecked() };
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    let ok = res.ok.cast::<ContinuationFrame>();
    assert!(
        ok.is_sane(),
        "{}: Result ok is not convertible to a continuation frame reference",
        function!()
    );

    let frame = unsafe { ok.as_mut_unchecked() };
    assert!(
        frame.data.is_sane(),
        "{}: Result ok data is not convertible to a slice",
        function!()
    );

    match read_frame(header, frame, reader) {
        Ok(()) => {
            res.is_ok = true;

            ErrorCode::NoError
        }
        Err(e) => set_err!(res, e.get_error_code(), "{e}"),
    }
}

pub fn write_frame(
    stream_id: u32,
    is_end: bool,
    data: &[u8],
    writer: &mut impl Write,
) -> std::io::Result<usize> {
    let mut written = write_header(
        FrameHeader {
            stream_id,
            frame_len: data.len() as u32,
            frame_type: TYPE_NUM,
            flags: if is_end { FLAGS_END_HEADERS } else { 0 },
        },
        writer,
    )?;

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
pub unsafe extern "C" fn http_continuation_write_frame(
    writer: *mut Writer,
    stream_id: u32,
    is_end: bool,
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
    assert!(
        data.is_sane(),
        "{}: Header frame data is not convertible to a slice",
        function!()
    );

    let writer = unsafe { writer.as_mut_unchecked() };
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    match write_frame(stream_id, is_end, &data, writer) {
        Ok(l) => unsafe { set_ok(res, l, function!()) },
        Err(e) => set_err!(res, (), "{e}"),
    };
}
