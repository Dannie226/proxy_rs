pub mod context;
pub mod error;
pub mod frame;
pub mod hpack;

use std::io::Read;

use hpack::tables::HeaderTable;

use crate::{
    IsSane,
    bio::{Reader, Writer},
    function,
    http2::{
        context::{SettingName, Settings, http_new_context},
        frame::{read_header, settings},
    },
    result::{HttpResult, set_err, set_ok},
};

static PREFACE: [u8; 24] = [
    0x50, 0x52, 0x49, 0x20, 0x2a, 0x20, 0x48, 0x54, 0x54, 0x50, 0x2f, 0x32, 0x2e, 0x30, 0x0d, 0x0a,
    0x0d, 0x0a, 0x53, 0x4d, 0x0d, 0x0a, 0x0d, 0x0a,
];

/// SAFETY:
///
/// 1) reader must be convertible to a reference
/// 2) writer must be convertible to a reference
/// 3) res must be convertible to a reference
/// 4) res must be a result to a pointer to a context
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_http2_start_connection(
    reader: *mut Reader,
    writer: *mut Writer,
    settings: Settings,
    res: *mut HttpResult,
) {
    assert!(
        reader.is_sane(),
        "{}: Reader is not convertible to a reference",
        function!()
    );
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

    let reader = unsafe { reader.as_mut_unchecked() };
    let writer = unsafe { writer.as_mut_unchecked() };
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a reference",
        function!()
    );

    let mut preface = [0u8; 24];

    match reader.read_exact(&mut preface) {
        Ok(_) if preface == PREFACE => {}
        Ok(_) => set_err!(res, (), "Client preface is incorrect"),
        Err(e) => set_err!(res, (), "Failed to read preface: {e}"),
    }

    let header = match read_header(&settings, reader) {
        Ok(h) => h,
        Err(e) => set_err!(res, (), "Failed to read client settings header: {e}"),
    };

    if header.frame_type != settings::TYPE_NUM {
        set_err!(res, (), "Client frame is not a settings frame")
    }

    let client_settings = match settings::read_frame(header, reader) {
        Ok(settings::SettingsFrame::Settings(s)) => s,
        Ok(settings::SettingsFrame::Ack) => {
            set_err!(res, (), "Failed to read client settings: Got ACK")
        }
        Err(e) => set_err!(res, (), "Failed to read client settings frame: {e}"),
    };

    match settings::write_frame(settings::SettingsFrame::Ack, writer) {
        Ok(_) => {}
        Err(e) => set_err!(res, (), "Failed to write ack: {e}"),
    }

    let table = HeaderTable::new(settings.get(SettingName::HeaderTableSize).value() as usize);

    match settings::write_frame(settings::SettingsFrame::Settings(settings), writer) {
        Ok(_) => {}
        Err(e) => set_err!(res, (), "Failed to write server settings: {e}"),
    };

    let ctx = http_new_context();

    // SAFETY:
    // ctx just came from a box, I can safely convert it to a reference

    let ctx = unsafe { ctx.as_mut_unchecked() };

    ctx.settings = client_settings;
    ctx.header_table = table;

    unsafe { set_ok(res, ctx as *mut _, function!()) };
}
