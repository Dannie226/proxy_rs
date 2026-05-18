use crate::{
    IsSane,
    bio::{Reader, Writer},
    buffer::{Buffer, ConstBuffer},
    function,
    http2::{
        error::{ErrorCode, IOProtoResult},
        frame::{FrameHeader, write_header},
    },
    result::{HttpResult, set_err, set_ok},
};
use std::io::{Read, Write};

#[repr(C)]
pub struct DataFrame {
    pub is_end: bool,
    pub data: Buffer,
}

pub const TYPE_NUM: u8 = 0x0;

pub const FLAGS_END_STREAM: u8 = 0x1;
pub const FLAGS_PADDED: u8 = 0x8;

/// Reads a data frame from the given reader
///
/// On an error, the contents of the data buffer
/// are undefined. The buffer is still structurally
/// valid, so you can reset the length and move
/// on, but any contents of the buffer cannot be
/// trusted.
pub fn read_frame(
    header: FrameHeader,
    data: &mut DataFrame,
    reader: &mut impl Read,
) -> IOProtoResult<()> {
    let FrameHeader {
        frame_type,
        flags,
        frame_len,
        stream_id,
    } = header;

    assert_eq!(frame_type, TYPE_NUM);

    if stream_id == 0 {
        Err(ErrorCode::ProtocolError)?
    }

    let mut data_len = frame_len;

    let mut padding_len = [0];
    if flags & FLAGS_PADDED != 0 {
        if frame_len == 0 {
            Err(ErrorCode::ProtocolError)?
        }

        reader.read_exact(&mut padding_len)?;
        data_len -= 1;
    }

    if padding_len[0] as u32 > data_len {
        Err(ErrorCode::ProtocolError)?
    }

    data_len -= padding_len[0] as u32;

    data.data.reserve_len(data_len as usize);
    data.data.len = data_len as usize;

    reader.read_exact(&mut data.data)?;

    let mut pad_buf = [0u8; 256];
    reader.read_exact(&mut pad_buf[..padding_len[0] as usize])?;

    data.is_end = flags & FLAGS_END_STREAM != 0;

    Ok(())
}

/// SAFETY:
///
/// 1) reader must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a DataFrame
///
/// On error, the contents of the data buffer are undefined
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_data_read_frame(
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

    let ok = res.ok.cast::<DataFrame>();
    assert!(
        ok.is_sane(),
        "{}: Result ok is not convertible to a reference to a DataFrame",
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

/// All zeroes, always, so it doesn't have to be allocated
/// on the stack
pub(crate) static PAD_BUF: [u8; 256] = [0; 256];

pub fn write_frame(
    stream_id: u32,
    pad: Option<u8>,
    data: &[u8],
    is_end: bool,
    writer: &mut impl Write,
) -> std::io::Result<usize> {
    let mut pad_len = 0;
    let mut frame_len = data.len() as u32;
    let mut flags = 0;

    if is_end {
        flags |= FLAGS_END_STREAM;
    }

    if let Some(pad) = pad {
        pad_len = pad;
        frame_len += 1 + (pad as u32);
        flags |= FLAGS_PADDED;
    }

    let mut written = write_header(
        FrameHeader {
            stream_id,
            frame_len,
            frame_type: TYPE_NUM,
            flags,
        },
        writer,
    )?;

    if pad.is_some() {
        writer.write_all(&[pad.unwrap()])?;
        written += 1;
    }

    writer.write_all(data)?;
    written += data.len();

    if pad.is_some() {
        writer.write_all(&PAD_BUF[..pad_len as usize])?;
        written += pad_len as usize;
    }

    Ok(written)
}

/// SAFETY:
///
/// 1) writer must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_data_write_frame(
    writer: *mut Writer,
    stream_id: u32,
    enable_pad: bool,
    pad: u8,
    buffer: ConstBuffer,
    is_end: bool,
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
        buffer.is_sane(),
        "{}: Data buffer is not convertible to a slice",
        function!()
    );

    let writer = unsafe { writer.as_mut_unchecked() };
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    match write_frame(
        stream_id,
        Some(pad).filter(|_| enable_pad),
        &buffer,
        is_end,
        writer,
    ) {
        Ok(l) => unsafe { set_ok(res, l, function!()) },
        Err(e) => set_err!(res, (), "{e}"),
    };
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use crate::{
        buffer::{http_clear_buffer, http_new_buffer_with},
        http2::{
            context,
            error::{ErrorCode, IOProtoError},
            frame::read_header,
        },
        test_utils::{finish_test, new_test_allocator, parse_hex, to_hex},
    };

    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
    struct ExpectedFrame {
        is_end: bool,
        data: String,
        error: Option<u32>,
    }

    impl From<DataFrame> for ExpectedFrame {
        fn from(mut value: DataFrame) -> Self {
            let d = Self {
                is_end: value.is_end,
                data: to_hex(&value.data),
                error: None,
            };

            unsafe {
                http_clear_buffer(&mut value.data);
            }

            d
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
    struct Test {
        name: String,
        wire: String,
        pad: Option<u8>,
        expected: ExpectedFrame,
    }

    fn run_read_test(test_name: &str, settings: context::Settings) {
        let allocator = new_test_allocator();

        let path = format!(
            "{}/test_cases/frame/data/{test_name}.json",
            env!("CARGO_MANIFEST_DIR")
        );

        let file = std::fs::read_to_string(&path);
        assert!(file.is_ok(), "{test_name}: Failed to read test file");
        let file = file.unwrap();

        let test = serde_json::from_str::<Test>(&file);
        assert!(test.is_ok(), "{test_name}: Failed to parse test file");
        let test = test.unwrap();

        let code = parse_hex(&test.wire);
        assert!(code.is_some(), "{test_name}: Failed to parse hex");
        let code = code.unwrap();

        let mut reader = &*code;

        let header = read_header(&settings, &mut reader);
        assert!(
            header.is_ok(),
            "{test_name}: Failed to read header: {}",
            header.unwrap_err()
        );
        let header = header.unwrap();

        assert_eq!(
            header.frame_type, TYPE_NUM,
            "{test_name}: Expected data frame"
        );

        if test.expected.error.is_none() {
            if test.expected.is_end {
                assert!(
                    header.flags & FLAGS_END_STREAM != 0,
                    "{test_name}: Expected end stream"
                );
            } else {
                assert_eq!(
                    header.flags & FLAGS_END_STREAM,
                    0,
                    "{test_name}: Expected no end stream"
                );
            }
        }

        let mut frame = DataFrame {
            is_end: false,
            data: http_new_buffer_with(0, allocator),
        };

        let res = read_frame(header, &mut frame, &mut reader);

        if let Some(e) = test.expected.error {
            let err = ErrorCode::try_from(e);
            assert!(
                err.is_ok(),
                "{test_name}: Invalid test case error code: {e}"
            );
            let err = err.unwrap();
            match res {
                Err(IOProtoError::Protocol(e)) => assert_eq!(e, err),
                Err(IOProtoError::Io(e)) => panic!("{test_name}: Not protocol error: {e:?}"),
                Ok(_) => panic!("{test_name}: Didn't fail to parse"),
            }
        } else {
            assert!(
                res.is_ok(),
                "{test_name}: Failed to read data frame: {}",
                res.err().unwrap()
            );

            let f = ExpectedFrame::from(frame);

            assert_eq!(f, test.expected, "{test_name}");
        }

        finish_test();
    }

    fn run_write_test(test_name: &str) {
        let path = format!(
            "{}/test_cases/frame/data/{test_name}.json",
            env!("CARGO_MANIFEST_DIR")
        );

        let file = std::fs::read_to_string(&path);
        assert!(file.is_ok(), "{test_name}: Failed to read test file");
        let file = file.unwrap();

        let test = serde_json::from_str::<Test>(&file);
        assert!(test.is_ok(), "{test_name}: Failed to parse test file");
        let test = test.unwrap();

        let mut writer = Vec::new();

        let data = parse_hex(&test.expected.data);
        assert!(data.is_some(), "{test_name}: Failed to parse hex");
        let data = data.unwrap();

        let res = write_frame(1, test.pad, &data, test.expected.is_end, &mut writer);
        assert!(
            res.is_ok(),
            "{test_name}: Failed to write frame: {}",
            res.unwrap_err()
        );

        let str = to_hex(&writer);

        assert_eq!(str, test.wire);
    }

    #[test]
    fn test_read() {
        let settings = context::Settings::default();
        run_read_test("standard", settings);
        run_read_test("padded", settings);
        run_read_test("stream", settings);
        run_read_test("invalid_id", settings);
        run_read_test("invalid_padding", settings);
    }

    #[test]
    fn test_write() {
        run_write_test("basic_write");
        run_write_test("stream_write");
        run_write_test("padded_write");
        run_write_test("padded_stream_write");
    }
}
