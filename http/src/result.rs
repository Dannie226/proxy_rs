use std::{ffi::c_void, io::Write};

use bstr::BStr;

use crate::{IsSane, bio::Writer, buffer::Buffer, function};

/// Invariants:
/// This will always be received as a pointer to write information
/// out of a function
/// The function will always write either true or false out to
/// is_ok
///
/// if is_ok == true, then the result is written to the value at the ok pointer
///
/// The ok pointer is some type determined by the function
/// the pointer is being passed into. Look at the function
/// documentation to figure out what kind of pointer is needed
/// The ok pointer must always be convertible to a reference of
/// the documented type.
/// This will be documented as "x must be a result to a y" where
/// x is the result variable and y is the type
///
/// If is_ok == false, then an error message was written out to
/// err. The error is usually, but not guaranteed to be utf8 encoded
#[repr(C)]
pub struct HttpResult {
    pub is_ok: bool,
    pub ok: *mut c_void,
    pub err: Buffer,
}

/// Prints the result error message out to stdout
/// Does not append a new line to the end of the message
///
/// SAFETY:
///
/// 1) res must be convertible to a reference
/// 2) res must be an error
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_print_err(res: *const HttpResult) {
    let res = unsafe { res.as_ref_unchecked() };

    assert!(!res.is_ok, "{}: Result is not an error", function!());
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    print!("{}", BStr::new(&res.err));
}

/// Writes the error message out to the given writer.
/// If an error occurs during writing, then that is a
/// fatal condition... I don't know what else to do...
///
/// SAFETY:
///
/// 1) res must be convertible to a reference
/// 2) res must be an error
/// 3) writer must be convertible to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_write_err(res: *const HttpResult, writer: *mut Writer) {
    assert!(
        res.is_sane(),
        "{}: Result pointer is not convertible to a reference",
        function!(),
    );
    assert!(
        writer.is_sane(),
        "{}: Writer pointer is not convertible to a reference",
        function!(),
    );

    let res = unsafe { res.as_ref_unchecked() };
    let writer = unsafe { writer.as_mut_unchecked() };

    assert!(
        !res.is_ok,
        "{}: Result pointer is not an error",
        function!()
    );
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    let r = writer.write_all(&res.err);

    // Kill the process, because I can't really think of a much
    // better way to do this...
    if r.is_err() {
        eprintln!(
            "{}: Failed to write to buffer: {}",
            function!(),
            r.unwrap_err()
        );
        std::process::exit(-1);
    }
}

// SAFETY:
//
// res must be a result to a T
pub(crate) unsafe fn set_ok<T>(res: &mut HttpResult, val: T, func_name: &'static str) {
    let ok = res.ok.cast::<T>();
    assert!(
        ok.is_sane(),
        "{}: Result ok is not convertible to a reference to a {}",
        func_name,
        std::any::type_name::<T>(),
    );

    let v = unsafe { ok.as_mut_unchecked() };
    *v = val;

    res.is_ok = true;
    res.err.clear();
}

macro_rules! set_err {
    ($res:ident) => {
        compile_error!("Requires arguments for the error to write")
    };
    ($res:ident, $ret:expr, $($arg:tt)*) => {{
        $res.is_ok = false;
        $res.err.len = 0;
        _ = ::std::fmt::Write::write_fmt(&mut $res.err, format_args!($($arg)*));
        return $ret
    }};
}

pub(crate) use set_err;
