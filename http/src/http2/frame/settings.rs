use std::io::{self, Read, Write};

use crate::{
    IsSane,
    bio::{Reader, Writer},
    function,
    http2::{
        context::{self, SettingName},
        error::{ErrorCode, IOProtoResult},
        frame::{FrameHeader, read_u16, read_u32, write_header, write_u16, write_u32},
    },
    result::{HttpResult, set_err, set_ok},
};

pub const TYPE_NUM: u8 = 0x4;

pub const FLAGS_ACK: u8 = 0x1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C, u32)]
pub enum SettingsFrame {
    Settings(context::Settings) = 1,
    Ack = 2,
}

pub fn read_frame(header: FrameHeader, reader: &mut impl Read) -> IOProtoResult<SettingsFrame> {
    let FrameHeader {
        frame_type,
        flags,
        frame_len,
        stream_id,
    } = header;

    assert_eq!(frame_type, TYPE_NUM);

    if stream_id != 0 {
        Err(ErrorCode::ProtocolError)?
    }

    if flags & FLAGS_ACK != 0 {
        if frame_len != 0 {
            Err(ErrorCode::FrameSizeError)?
        }

        return Ok(SettingsFrame::Ack);
    }

    if frame_len % 6 != 0 {
        Err(ErrorCode::FrameSizeError)?
    }

    let mut settings = context::Settings::default();
    let mut read = 0;

    while read < frame_len {
        let setting = read_u16(reader)?;
        let value = read_u32(reader)?;
        read += 6;

        let Ok(name) = SettingName::try_from(setting) else {
            continue;
        };

        settings.get_mut(name).set_value(value);
    }

    {
        let enable_push = settings.get(SettingName::EnablePush).value();

        if enable_push != 0 && enable_push != 1 {
            Err(ErrorCode::ProtocolError)?
        }
    }

    {
        let window_size = settings.get(SettingName::InitialWindowSize).value();

        if window_size >= 0x80000000 {
            Err(ErrorCode::FlowControlError)?
        }
    }

    {
        let frame_size = settings.get(SettingName::MaxFrameSize).value();

        if frame_size < (1 << 14) || frame_size >= (1 << 24) {
            Err(ErrorCode::ProtocolError)?
        }
    }

    Ok(SettingsFrame::Settings(settings))
}

pub fn write_frame(settings: SettingsFrame, writer: &mut impl Write) -> io::Result<usize> {
    let s = match settings {
        SettingsFrame::Settings(s) => s,
        SettingsFrame::Ack => {
            return write_header(
                FrameHeader {
                    stream_id: 0,
                    frame_len: 0,
                    frame_type: TYPE_NUM,
                    flags: FLAGS_ACK,
                },
                writer,
            );
        }
    };

    let mut buf = [0u8; 36];
    let mut buf_writer = buf.as_mut_slice();
    let mut len = 0;

    let default = context::Settings::default();

    for setting in SettingName::ALL {
        let v = s.get(setting);

        if v != default.get(setting) {
            write_u16(v.num(), &mut buf_writer)?;
            write_u32(v.value(), &mut buf_writer)?;
            len += 6;
        }
    }

    let mut written = write_header(
        FrameHeader {
            stream_id: 0,
            frame_len: len,
            frame_type: TYPE_NUM,
            flags: 0,
        },
        writer,
    )?;

    writer.write_all(&buf[..len as usize])?;
    written += len as usize;

    Ok(written)
}

/// SAFETY:
///
/// 1) context must be convertible to a reference
/// 2) reader must be convertible to a reference
/// 3) res must be convertible to a reference
/// 4) res must be a result to a settings enum
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_settings_read_frame(
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
        Ok(s) => {
            unsafe { set_ok(res, s, function!()) };
            ErrorCode::NoError
        }
        Err(e) => set_err!(res, e.get_error_code(), "{e}"),
    }
}

/// SAFETY:
///
/// 1) writer must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_settings_write_frame(
    writer: *mut Writer,
    settings: SettingsFrame,
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

    match write_frame(settings, writer) {
        Ok(l) => unsafe { set_ok(res, l, function!()) },
        Err(e) => set_err!(res, (), "{e}"),
    };
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use crate::{
        http2::{error::IOProtoError, frame::read_header},
        test_utils::{parse_hex, to_hex},
    };

    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
    struct ExpectedSettings {
        settings: [Option<u32>; 6],
        ack: bool,
        error: Option<u32>,
    }

    impl From<SettingsFrame> for ExpectedSettings {
        fn from(value: SettingsFrame) -> Self {
            let mut ack = false;
            let mut settings = [None; 6];

            match value {
                SettingsFrame::Ack => {
                    ack = true;
                }
                SettingsFrame::Settings(s) => {
                    for n in SettingName::ALL {
                        let v = s.get(n);

                        settings[n as usize - 1] = Some(v.value()).filter(|_| v.exists());
                    }
                }
            }

            Self {
                settings,
                ack,
                error: None,
            }
        }
    }

    impl From<ExpectedSettings> for SettingsFrame {
        fn from(value: ExpectedSettings) -> Self {
            let mut settings = context::Settings::default();

            if value.ack {
                return SettingsFrame::Ack;
            }

            for (i, v) in value.settings.into_iter().enumerate() {
                if let Some(v) = v {
                    settings
                        .get_mut(SettingName::from(
                            SettingName::try_from(i as u16 + 1).unwrap(),
                        ))
                        .set_value(v);
                }
            }

            SettingsFrame::Settings(settings)
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
    struct Test {
        name: String,
        wire: String,
        expected: ExpectedSettings,
    }

    fn run_read_test(test_name: &str, settings: context::Settings) {
        let path = format!(
            "{}/test_cases/frame/settings/{test_name}.json",
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
            "{test_name}: Didn't read settings frame"
        );

        if test.expected.error.is_none() {
            if test.expected.ack {
                assert_eq!(
                    header.flags, FLAGS_ACK,
                    "{test_name}: Expected ACK, didn't get it"
                );
            } else {
                assert_eq!(header.flags, 0, "{test_name}: Expected no ACK, got one");
            }
        }

        let settings = read_frame(header, &mut reader);

        if let Some(e) = test.expected.error {
            let err = ErrorCode::try_from(e);
            assert!(
                err.is_ok(),
                "{test_name}: Invalid test case error code: {e}"
            );
            let err = err.unwrap();

            match settings {
                Err(IOProtoError::Protocol(c)) => {
                    assert_eq!(
                        c, err,
                        "{test_name}: Incorrect Protocol Error: {c} vs {err}"
                    )
                }
                Err(IOProtoError::Io(e)) => panic!("{test_name}: Not protocol error: {e:?}"),
                Ok(_) => panic!("{test_name}: Didn't fail to parse"),
            }
        } else {
            let settings = settings;
            assert!(settings.is_ok(), "{test_name}: Failed to read settings");
            let settings = settings.unwrap();

            let s: ExpectedSettings = settings.into();

            assert_eq!(s, test.expected, "{test_name}: Settings mismatch");
        }
    }

    fn run_write_test(test_name: &str) {
        let path = format!(
            "{}/test_cases/frame/settings/{test_name}.json",
            env!("CARGO_MANIFEST_DIR")
        );

        let file = std::fs::read_to_string(&path);
        assert!(file.is_ok(), "{test_name}: Failed to read test file");

        let file = file.unwrap();

        let test: Test = serde_json::from_str(&file).expect("Failed to parse test file");

        let mut writer = Vec::new();

        let res = write_frame(test.expected.into(), &mut writer);

        assert!(
            res.is_ok(),
            "{test_name}: Failed to write frame: {}",
            res.unwrap_err()
        );

        let str = to_hex(&writer);

        assert_eq!(str, test.wire, "{test_name}: Wire format mismatch");
    }

    #[test]
    fn test_read() {
        let settings = context::Settings::default();

        run_read_test("empty", settings);
        run_read_test("ack", settings);
        run_read_test("settings", settings);
        run_read_test("unknown", settings);
        run_read_test("ack_len", settings);
        run_read_test("invalid_len", settings);
        run_read_test("invalid_push", settings);
        run_read_test("invalid_window", settings);
        run_read_test("invalid_frame_small", settings);
        run_read_test("invalid_frame_large", settings);
    }

    #[test]
    fn test_write() {
        run_write_test("ack_write");
        run_write_test("default_write");
        run_write_test("all_settings");
    }
}
