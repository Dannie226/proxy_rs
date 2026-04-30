use std::{ffi::*, ptr::NonNull, slice};

use bstr::BStr;

use crate::{Buffer, ConstBuffer};

#[repr(C)]
pub struct Result {
    size: usize,
    str: Option<NonNull<u8>>,
}

// SAFETY:
// str must point to a null-terminated str
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_res_new_err(str: *const c_char) -> Result {
    // SAFETY: str points to a null-terminated str
    let s = unsafe { CStr::from_ptr(str) };

    let b = s.to_bytes().to_owned().into_boxed_slice();

    let ptr = Box::into_raw(b);

    // SAFETY: ptr just came from a box, it is safe to convert
    // to a reference
    let slice = unsafe { ptr.as_mut_unchecked() };

    let ptr = slice.as_mut_ptr();
    let len = slice.len();

    Result {
        size: len,
        // SAFETY: ptr is from a slice, and therefore not null
        str: Some(unsafe { NonNull::new_unchecked(ptr) }),
    }
}

// SAFETY:
// str must point to a null-terminated str
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_res_new_err_buf(str: ConstBuffer) -> Result {
    let b = str.as_slice().to_owned().into_boxed_slice();
    let ptr = Box::into_raw(b);

    // SAFETY: ptr just came from a box, it is safe to convert
    // to a reference
    let slice = unsafe { ptr.as_mut_unchecked() };

    let ptr = slice.as_mut_ptr();
    let len = slice.len();

    Result {
        size: len,
        // SAFETY: ptr is from a slice, and therefore not null
        str: Some(unsafe { NonNull::new_unchecked(ptr) }),
    }
}

pub fn result_from_string(str: String) -> Result {
    let b = str.into_bytes().into_boxed_slice();

    let ptr = Box::into_raw(b);

    // SAFETY: ptr just came from a box, it is safe to convert
    // to a reference
    let slice = unsafe { ptr.as_mut_unchecked() };

    let ptr = slice.as_mut_ptr();
    let len = slice.len();

    Result {
        size: len,
        // SAFETY: ptr is from a slice, and therefore not null
        str: Some(unsafe { NonNull::new_unchecked(ptr) }),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn http_res_new_ok(count: usize) -> Result {
    Result {
        size: count,
        str: None,
    }
}

/// SAFETY:
/// res must be convertable to a reference
/// res must not be used after this call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_destroy_res(res: *mut Result) {
    // SAFETY: res is convertable to a reference
    let res = unsafe { res.as_mut_unchecked() };

    if let Some(p) = res.str {
        // SAFETY: If p exists, it was created from a slice of len bytes
        let slice = unsafe { slice::from_raw_parts_mut(p.as_ptr(), res.size) };

        // SAFETY: slice pointer was created via into raw in the first place
        drop(unsafe { Box::from_raw(slice) });
    }

    res.size = 0;
    res.str = None;
}

// SAFETY:
// Res must be convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_res_is_ok(res: *const Result) -> bool {
    // SAFETY: Res is convertable to a reference
    let res = unsafe { res.as_ref_unchecked() };
    res.str.is_none()
}

// SAFETY:
// Res must be convertable to a reference
// Res must be the ok variant
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_res_get_count(res: *const Result) -> usize {
    // SAFETY: Res is convertable to a reference
    let res = unsafe { res.as_ref_unchecked() };

    // If res is ok, then size is the number of bytes or written
    res.size
}

// SAFETY:
// Res must be convertable to a reference
// Res must be the err variant
// err must be convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_res_get_err(res: *const Result, err: *mut Buffer) -> c_int {
    // SAFETY: res is convertable to a reference
    let res = unsafe { res.as_ref_unchecked() };

    // SAFETY: res is an err, so str exists
    let ptr = unsafe { res.str.unwrap_unchecked() };

    // SAFETY: res is an err, so the pointer is to a slice of res.size bytes
    let data = unsafe { slice::from_raw_parts(ptr.as_ptr(), res.size) };

    // SAFETY: err is convertable to a reference
    let err = unsafe { err.as_mut_unchecked() };

    err.copy_slice(data)
}

// SAFETY:
// res must be the err variant
pub unsafe fn http_res_err_as_bstr(res: &Result) -> &BStr {
    // SAFETY: res is an err, so str exists
    let ptr = unsafe { res.str.unwrap_unchecked() };

    // SAFETY: res is an err, so the pointer is to a slice of res.size bytes
    let data = unsafe { slice::from_raw_parts(ptr.as_ptr(), res.size) };

    BStr::new(data)
}
