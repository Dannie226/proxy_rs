use std::{
    ffi::*,
    io::{self, Read, Write},
    ptr, slice,
};

use crate::result::{
    http_destroy_res, http_res_err_as_bstr, http_res_get_count, http_res_is_ok, http_res_new_err,
    http_res_new_ok, result_from_string,
};

pub type ClearFn = unsafe extern "C" fn(arg: *mut c_void);

/// Empty clear function
/// Good for null pointers
#[unsafe(no_mangle)]
pub extern "C" fn http_null_clear(_: *mut c_void) {}

/// SAFETY:
/// 1) Data must be convertable to a slice of len bytes
/// 2) res must be convertable to a reference
/// 3) The result written to the pointer must be created
/// using http_res_new_err or http_res_new_ok
///
/// Return:
/// Write out the result to the given pointer
pub type ReadFn = unsafe extern "C" fn(
    arg: *mut c_void,
    data: *mut c_void,
    len: usize,
    res: *mut crate::result::Result,
);

#[repr(C)]
pub struct Reader {
    pub(super) data: *mut c_void,
    pub(super) read: ReadFn,
    pub(super) clear: ClearFn,
}

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut res = http_res_new_ok(0);

        // SAFETY:
        // From the new reader functions, reader data must be the first argument to the read property
        // buf is a slice itself
        // result is a reference
        unsafe {
            (self.read)(self.data, buf.as_mut_ptr().cast(), buf.len(), &raw mut res);
        }

        // SAFETY:
        // res is a reference
        let ret = if unsafe { http_res_is_ok(&res) } {
            // SAFETY: Res is the ok variant
            Ok(unsafe { http_res_get_count(&res) })
        } else {
            // SAFETY: Res is the err variant
            let s = unsafe { http_res_err_as_bstr(&res) }.to_string();

            Err(io::Error::other(s))
        };

        // SAFETY:
        // res is a reference
        // res isn't used after this call
        unsafe { http_destroy_res(&mut res) };

        ret
    }
}

/// Empty read function
/// Good for null pointers
/// This is a read function, and has the safety requirements
/// of the ReadFn type
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_null_read(
    _: *mut c_void,
    _: *mut c_void,
    _: usize,
    read: *mut crate::result::Result,
) {
    // SAFETY: This is a read function, so the caller
    // ensures that read is convertable to a reference
    let read = unsafe { read.as_mut_unchecked() };

    // SAFETY: The literal is null terminated
    // And, I am writing to read with an error from the
    // correct functions
    *read = unsafe { http_res_new_err(c"Reading from empty reader".as_ptr()) };
}

/// SAFETY:
/// 1) Read fn must take the data pointer as the first argument
/// 2) ReadFn's other safety requirements
/// 3) Clear fn must take the data pointer and free associated memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_new_reader(
    data: *mut c_void,
    read: ReadFn,
    clear: ClearFn,
) -> *mut Reader {
    let r = Reader { data, read, clear };

    let r = Box::new(r);

    Box::into_raw(r)
}

/// SAFETY:
/// 1) Read must take a null pointer as it's first argument
/// 2) ReadFn's other safety requirements
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_new_empty_data_reader(read: ReadFn) -> *mut Reader {
    // SAFETY:
    // 1) Data is null ptr, and caller requirements say read handles a null ptr
    // 2) Caller requirement is the same
    // 3) null_clear explicitly is supposed to take a null ptr
    unsafe { http_new_reader(ptr::null_mut(), read, http_null_clear) }
}

#[unsafe(no_mangle)]
pub extern "C" fn http_new_empty_reader() -> *mut Reader {
    // SAFETY:
    // 1) Data is null ptr, and null read/delete handle a null ptr
    // 2) Null read has same requirements as read function
    // 3) see point 1
    unsafe { http_new_reader(ptr::null_mut(), http_null_read, http_null_clear) }
}

pub(crate) fn reader_from_read<T: Read>(reader: T) -> *mut Reader {
    let reader = Box::new(reader);

    // SAFETY:
    // arg must be convertable to a reference of T
    // data must be convertable to a slice of len bytes
    // read must be convertable to a reference
    unsafe extern "C" fn read<T: Read>(
        arg: *mut c_void,
        data: *mut c_void,
        len: usize,
        res: *mut crate::result::Result,
    ) {
        // SAFETY: arg is a pointer to a T from a box, so is valid and convertable
        // to a reference
        let reader = unsafe { arg.cast::<T>().as_mut_unchecked() };

        // SAFETY: Data is convertable to a slice of len bytes (guaranteed by caller)
        let data = unsafe { slice::from_raw_parts_mut(data.cast::<u8>(), len) };

        // SAFETY: Read is convertable to a reference (enforced by caller)
        let res = unsafe { res.as_mut_unchecked() };

        match reader.read(data) {
            Ok(v) => {
                // SAFETY:
                // Res is being written with the correct function call
                *res = http_res_new_ok(v);
            }
            Err(e) => {
                *res = result_from_string(format!("{e}"));
            }
        }
    }

    // SAFETY:
    // arg must be convertable to a reference of T
    // arg must have been created by Box::into_raw on a Box<T>
    unsafe extern "C" fn clear<T: Read>(arg: *mut c_void) {
        // Data was created by a pointer to a T, so cast is safe
        let arg = arg.cast::<T>();

        // SAFETY: arg was created from Box::into_raw
        let data = unsafe { Box::from_raw(arg) };

        drop(data)
    }

    // SAFETY:
    // reader is a T, and read/clear are both safe with that T
    unsafe { http_new_reader(Box::into_raw(reader).cast(), read::<T>, clear::<T>) }
}

/// SAFETY:
/// 1) Reader must be convertable to a reference
/// 2) Data must be convertable to a slice of len bytes
/// 3) Read must be convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_bio_read(
    reader: *mut Reader,
    data: *mut c_void,
    len: usize,
    res: *mut crate::result::Result,
) {
    // SAFETY: Reader is convertable to a reference
    let reader = unsafe { reader.as_mut_unchecked() };

    // SAFETY:
    // Reader internal data is passable as the first argument
    // Data is convertable to a slice of len bytes,
    // read is convertable to a reference
    unsafe { (reader.read)(reader.data, data, len, res) }
}

/// SAFETY:
/// 1) Reader must be convertable to a reference
/// 2) Reader must have been created from an http_new_reader function
/// 3) Reader must not be used after this call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_destroy_reader(reader: *mut Reader) {
    // SAFETY: Reader is convertable to a reference
    let reader = unsafe { reader.as_mut_unchecked() };

    // SAFETY: Reader clear takes the data pointer and free's the memory
    unsafe { (reader.clear)(reader.data) }

    // SAFETY: Reader is created through a box in the http new
    // reader functions
    drop(unsafe { Box::from_raw(reader) });
}

/// SAFETY:
/// 1) Data must be convertable to a slice of len bytes
/// 2) Res must be convertable to a reference
///
/// Return:
/// 0 is no error
/// Anything else is an error code
pub type WriteFn = unsafe extern "C" fn(
    arg: *mut c_void,
    data: *const c_void,
    len: usize,
    res: *mut crate::result::Result,
);

#[repr(C)]
pub struct Writer {
    pub(crate) data: *mut c_void,
    pub(crate) write: WriteFn,
    pub(crate) clear: ClearFn,
}

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut res = http_res_new_ok(0);

        // SAFETY:
        // From the new reader functions, reader data must be the first argument to the read property
        // buf is a slice itself
        // result is a reference
        unsafe {
            (self.write)(self.data, buf.as_ptr().cast(), buf.len(), &raw mut res);
        }

        // SAFETY:
        // res is a reference
        let ret = if unsafe { http_res_is_ok(&res) } {
            // SAFETY: Res is the ok variant
            Ok(unsafe { http_res_get_count(&res) })
        } else {
            // SAFETY: Res is the err variant
            let s = unsafe { http_res_err_as_bstr(&res) }.to_string();

            Err(io::Error::other(s))
        };

        ret
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn http_null_write(
    _: *mut c_void,
    _: *const c_void,
    _: usize,
    res: *mut crate::result::Result,
) {
    // SAFETY: This is a read function, so the caller
    // ensures that read is convertable to a reference
    let res = unsafe { res.as_mut_unchecked() };

    // SAFETY: The literal is null terminated
    // And, I am writing to read with an error from the
    // correct functions
    *res = unsafe { http_res_new_err(c"Writing to empty writer".as_ptr()) };
}

/// SAFETY:
/// 1) Write fn must take the data pointer as the first argument
/// 2) WriteFn's other safety requirements
/// 3) Clear fn must take the data pointer and free associated memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_new_writer(
    data: *mut c_void,
    write: WriteFn,
    clear: ClearFn,
) -> *mut Writer {
    let w = Writer { data, write, clear };

    let w = Box::new(w);

    Box::into_raw(w)
}

/// SAFETY:
/// 1) Write must take a null pointer as it's first argument
/// 2) WriteFn's other safety requirements
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_new_empty_data_writer(write: WriteFn) -> *mut Writer {
    // SAFETY:
    // 1) Data is null ptr, and caller requirements say read handles a null ptr
    // 2) Caller requirement is the same
    // 3) null_clear explicitly is supposed to take a null ptr
    unsafe { http_new_writer(ptr::null_mut(), write, http_null_clear) }
}

pub(crate) fn writer_from_write<T: Write>(writer: T) -> *mut Writer {
    let writer = Box::new(writer);

    // SAFETY:
    // arg must be convertable to a reference of T
    // data must be convertable to a slice of len bytes
    // res must be convertable to a reference
    unsafe extern "C" fn write<T: Write>(
        arg: *mut c_void,
        data: *const c_void,
        len: usize,
        res: *mut crate::result::Result,
    ) {
        // SAFETY: arg is a pointer to a T from a box, so is valid and convertable
        // to a reference
        let writer = unsafe { arg.cast::<T>().as_mut_unchecked() };

        // SAFETY: Data is convertable to a slice of len bytes (guaranteed by caller)
        let data = unsafe { slice::from_raw_parts(data.cast::<u8>(), len) };

        // SAFETY: res is convertable to a reference (enforced by caller)
        let res = unsafe { res.as_mut_unchecked() };

        match writer.write(data) {
            Ok(v) => {
                // SAFETY:
                // Res is being written with the correct function call
                *res = http_res_new_ok(v);
            }
            Err(e) => *res = result_from_string(format!("{e}")),
        }
    }

    // SAFETY:
    // arg must be convertable to a reference of T
    // arg must have been created by Box::into_raw on a Box<T>
    unsafe extern "C" fn clear<T: Write>(arg: *mut c_void) {
        // Data was created by a pointer to a T, so cast is safe
        let arg = arg.cast::<T>();

        // SAFETY: arg was created from Box::into_raw
        let data = unsafe { Box::from_raw(arg) };

        drop(data)
    }

    // SAFETY:
    // writer is a T, write and clear both take a T and are safe with that T
    unsafe { http_new_writer(Box::into_raw(writer).cast(), write::<T>, clear::<T>) }
}

/// SAFETY:
/// 1) Writer must be convertable to a reference
/// 2) Data must be convertable to a slice of len bytes
/// 3) Res must be convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_bio_write(
    writer: *mut Writer,
    data: *const c_void,
    len: usize,
    res: *mut crate::result::Result,
) {
    // SAFETY: Writer is convertable to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    // SAFETY:
    // 1) Writer write function takes writer data as first argument
    // 2) Data is convertable to a slice of len bytes
    // 3) res is convertable to a reference
    unsafe { (writer.write)(writer.data, data, len, res) }
}

/// SAFETY:
/// 1) Writer must be convertable to a reference
/// 2) Writer must have been created with an http_new_writer method
/// 3) Writer must not be used after a call to this function
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_destroy_writer(writer: *mut Writer) {
    // SAFETY: Writer is convertable to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    // SAFETY: Writer clear takes the data pointer and free's the memory
    unsafe { (writer.clear)(writer.data) }
}
