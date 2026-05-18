use std::io::{self, Read, Write};

use crate::{
    IsSane,
    bio::{Reader, Writer},
    function,
    http2::{
        context::{Context, SettingName, Settings},
        error::{ErrorCode, IOProtoResult},
    },
    result::{HttpResult, set_err, set_ok},
};

pub mod continuation;
pub mod data;
pub mod go_away;
pub mod header;
pub mod ping;
pub mod priority;
pub mod rst_stream;
pub mod settings;
pub mod window_update;

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct FrameHeader {
    pub stream_id: u32,
    pub frame_len: u32,
    pub frame_type: u8,
    pub flags: u8,
}

pub fn read_u8(reader: &mut impl Read) -> io::Result<u8> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    Ok(u8::from_be_bytes(buf))
}

pub fn read_u16(reader: &mut impl Read) -> io::Result<u16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(u16::from_be_bytes(buf))
}

pub fn read_u24(reader: &mut impl Read) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf[1..])?;
    Ok(u32::from_be_bytes(buf))
}

pub fn read_u32(reader: &mut impl Read) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_be_bytes(buf))
}

pub fn read_u64(reader: &mut impl Read) -> io::Result<u64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_be_bytes(buf))
}

pub fn write_u8(value: u8, writer: &mut impl Write) -> io::Result<()> {
    writer.write_all(&[value])
}

pub fn write_u16(value: u16, writer: &mut impl Write) -> io::Result<()> {
    let value = value.to_be_bytes();
    writer.write_all(&value)
}

pub fn write_u24(value: u32, writer: &mut impl Write) -> io::Result<()> {
    let value = value & 0x00FFFFFF;
    let value = value.to_be_bytes();
    writer.write_all(&value[1..])
}

pub fn write_u32(value: u32, writer: &mut impl Write) -> io::Result<()> {
    let value = value.to_be_bytes();
    writer.write_all(&value)
}

pub fn write_u64(value: u64, writer: &mut impl Write) -> io::Result<()> {
    let value = value.to_be_bytes();
    writer.write_all(&value)
}

pub fn read_header(settings: &Settings, reader: &mut impl Read) -> IOProtoResult<FrameHeader> {
    let mut header = [0u8; 9];

    reader.read_exact(&mut header)?;

    let mut buf = header.as_slice();

    let frame_len = read_u24(&mut buf)?;

    if frame_len > settings.get(SettingName::MaxFrameSize).value() {
        Err(ErrorCode::FrameSizeError)?
    }

    let frame_type = read_u8(&mut buf)?;
    let flags = read_u8(&mut buf)?;
    let stream_id = read_u32(&mut buf)? & 0x7FFFFFFF;

    Ok(FrameHeader {
        stream_id,
        frame_len,
        frame_type,
        flags,
    })
}

pub fn write_header(header: FrameHeader, writer: &mut impl Write) -> io::Result<usize> {
    let mut buf = [0u8; 9];
    let mut buf_writer = buf.as_mut_slice();

    write_u24(header.frame_len, &mut buf_writer)?;
    write_u8(header.frame_type, &mut buf_writer)?;
    write_u8(header.flags, &mut buf_writer)?;
    write_u32(header.stream_id & 0x7FFFFFFF, &mut buf_writer)?;

    writer.write_all(&buf).map(|_| 9)
}

/// SAFETY:
///
/// 1) context must be convertible to a reference
/// 2) reader must be convertible to a reference
/// 3) res must be convertible to a reference
/// 4) res must be a result to a FrameHeader
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_frame_read_header(
    context: *const Context,
    reader: *mut Reader,
    res: *mut HttpResult,
) -> ErrorCode {
    assert!(
        context.is_sane(),
        "{}: Context is not convertible to a reference",
        function!()
    );
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
    let context = unsafe { context.as_ref_unchecked() };
    let reader = unsafe { reader.as_mut_unchecked() };
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    match read_header(&context.settings, reader) {
        Ok(header) => {
            unsafe { set_ok(res, header, function!()) };
            ErrorCode::NoError
        }
        Err(e) => set_err!(res, e.get_error_code(), "{e}"),
    }
}

/// SAFETY:
///
/// 1) context must be convertible to a reference
/// 2) writer must be convertible to a reference
/// 3) res must be convertible to a reference
/// 4) res must be a result to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_frame_write_header(
    writer: *mut Writer,
    header: FrameHeader,
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

    match write_header(header, writer) {
        Ok(_) => unsafe { set_ok(res, 9usize, function!()) },
        Err(e) => set_err!(res, (), "{e}"),
    };
}

#[cfg(test)]
mod tests {
    use crate::http2::{context, error::IOProtoError};

    use super::*;

    #[test]
    fn test_read() {
        let header = [0x00, 0x00, 0x12, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01].as_slice();

        let mut reader = header;

        let header = read_header(&context::Settings::default(), &mut reader).unwrap();

        assert_eq!(header.frame_type, 0x0);
        assert_eq!(header.flags, 0x1);
        assert_eq!(header.frame_len, 0x12);
        assert_eq!(header.stream_id, 0x1);
    }

    #[test]
    fn fail_on_large() {
        let header = [0x00, 0xFF, 0xFF, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01].as_slice();

        let mut reader = header;

        let mut settings = context::Settings::default();
        settings
            .get_mut(SettingName::MaxFrameSize)
            .set_value(0x1000);

        let header = read_header(&settings, &mut reader);

        match header {
            Err(IOProtoError::Protocol(c)) => assert_eq!(c, ErrorCode::FrameSizeError),
            Err(IOProtoError::Io(e)) => panic!("Not protocol error: {e:?}"),
            Ok(_) => panic!("Didn't fail to parse"),
        }
    }
}
