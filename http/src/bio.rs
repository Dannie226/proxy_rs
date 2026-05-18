use std::{
    ffi::*,
    io::{self, Read, Write},
    ptr, slice,
};

use bstr::BStr;

use crate::{
    IsSane,
    buffer::{http_clear_buffer, http_new_buffer},
    function,
    result::{HttpResult, set_err, set_ok},
};

pub type ClearFn = unsafe extern "C" fn(arg: *mut c_void);

/// no-op clear function
/// Good for null pointers
#[unsafe(no_mangle)]
pub extern "C" fn http_null_clear(_: *mut c_void) {}

/// SAFETY:
/// 1) data must be convertible to a slice of len bytes
/// 2) res must be convertible to a reference
/// 3) res must be a result to a usize
///
/// Return:
/// Writes out the result to res
pub type ReadFn =
    unsafe extern "C" fn(arg: *mut c_void, data: *mut u8, len: usize, res: *mut HttpResult);

#[repr(C)]
pub struct Reader {
    pub(super) data: *mut c_void,
    pub(super) read: ReadFn,
    pub(super) clear: ClearFn,
}

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut written = 0usize;
        let mut res = HttpResult {
            is_ok: true,
            ok: (&raw mut written).cast(),
            err: http_new_buffer(0),
        };

        // SAFETY:
        // From the new reader functions, reader data must be the first argument
        // to the read property
        // buf is a slice itself
        // result is a reference
        // result holds a pointer to a usize
        unsafe {
            (self.read)(self.data, buf.as_mut_ptr(), buf.len(), &mut res);
        }

        let ret = if res.is_ok {
            Ok(written)
        } else {
            assert!(
                res.err.is_sane(),
                "{}: Result error is not convertible to a slice",
                function!()
            );

            let s = BStr::new(&res.err).to_string();
            Err(io::Error::other(s))
        };

        // SAFETY: res.err is a reference
        unsafe { http_clear_buffer(&mut res.err) };

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
    _: *mut u8,
    _: usize,
    res: *mut HttpResult,
) {
    // SAFETY: This is a read function, so the caller
    // ensures that read is convertible to a reference
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    set_err!(res, (), "Reading from empty reader");
}

/// SAFETY:
/// 1) read must take the data pointer as the first argument
/// 2) ReadFn's other safety requirements
/// 3) clear must take the data pointer and free associated memory
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
/// 1) read must take a null pointer as it's first argument
/// 2) ReadFn's other safety requirements
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_new_empty_data_reader(read: ReadFn) -> *mut Reader {
    // SAFETY:
    // 1) data is null ptr, and caller requirements say read handles a null ptr
    // 2) Caller requirement is the same
    // 3) null_clear explicitly is supposed to take a null ptr
    unsafe { http_new_reader(ptr::null_mut(), read, http_null_clear) }
}

#[unsafe(no_mangle)]
pub extern "C" fn http_new_empty_reader() -> *mut Reader {
    // SAFETY:
    // 1) data is null ptr, and null read/delete handle a null ptr
    // 2) null read has same requirements as read function
    // 3) see point 1
    unsafe { http_new_reader(ptr::null_mut(), http_null_read, http_null_clear) }
}

pub(crate) fn reader_from_read<T: Read>(reader: T) -> *mut Reader {
    let reader = Box::new(reader);

    // SAFETY:
    // arg must be convertible to a reference of T
    // data must be convertible to a slice of len bytes
    // read must be convertible to a reference
    // res must be a result to a usize
    unsafe extern "C" fn read<T: Read>(
        arg: *mut c_void,
        data: *mut u8,
        len: usize,
        res: *mut HttpResult,
    ) {
        // SAFETY: arg is a pointer to a T from a box, so is valid and convertible
        // to a reference
        let reader = unsafe { arg.cast::<T>().as_mut_unchecked() };

        // SAFETY: Data is convertible to a slice of len bytes (guaranteed by caller)
        let data = unsafe { slice::from_raw_parts_mut(data, len) };

        // SAFETY: Read is convertible to a reference (enforced by caller)
        let res = unsafe { res.as_mut_unchecked() };

        match reader.read(data) {
            Ok(v) => unsafe { set_ok(res, v, function!()) },
            Err(e) => set_err!(res, (), "{e}"),
        }
    }

    // SAFETY:
    // arg must be convertible to a reference of T
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
/// 1) reader must be convertible to a reference
/// 2) data must be convertible to a slice of len bytes
/// 3) read must be convertible to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_bio_read(
    reader: *mut Reader,
    data: *mut u8,
    len: usize,
    res: *mut HttpResult,
) {
    assert!(
        reader.is_sane(),
        "{}: Reader is not convertible to a reference",
        function!()
    );
    assert!(
        data.is_sane(),
        "{}: Data is not convertible to a slice",
        function!()
    );

    assert!(
        res.is_sane(),
        "{}: Result is not convertible to a reference",
        function!()
    );

    // SAFETY: Reader is convertible to a reference
    let reader = unsafe { reader.as_mut_unchecked() };

    // SAFETY:
    // reader internal data is passable as the first argument
    // data is convertible to a slice of len bytes,
    // read is convertible to a reference
    unsafe { (reader.read)(reader.data, data, len, res) }
}

/// SAFETY:
/// 1) reader must be convertible to a reference
/// 2) reader must have been created from an http_new_reader function
/// 3) reader must not be used after this call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_destroy_reader(reader: *mut Reader) {
    assert!(
        reader.is_sane(),
        "{}: Reader is not convertible to a reference",
        function!()
    );

    // SAFETY: Reader is convertible to a reference
    let reader = unsafe { reader.as_mut_unchecked() };

    // SAFETY: Reader clear takes the data pointer and free's the memory
    unsafe { (reader.clear)(reader.data) }

    // SAFETY: Reader is created through a box in the http new
    // reader functions
    drop(unsafe { Box::from_raw(reader) });
}

/// SAFETY:
/// 1) data must be convertible to a slice of len bytes
/// 2) res must be convertible to a reference
/// 3) res must be a result to a usize
///
/// Return:
/// Writes out the result to res
pub type WriteFn =
    unsafe extern "C" fn(arg: *mut c_void, data: *const u8, len: usize, res: *mut HttpResult);

#[repr(C)]
pub struct Writer {
    pub(crate) data: *mut c_void,
    pub(crate) write: WriteFn,
    pub(crate) clear: ClearFn,
}

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written = 0usize;
        let mut res = HttpResult {
            is_ok: true,
            ok: (&raw mut written).cast(),
            err: http_new_buffer(0),
        };

        // SAFETY:
        // From the new writer functions, writer data must be the first argument
        // to the read property
        // buf is a slice itself
        // result is a reference
        // result holds a pointer to a usize
        unsafe {
            (self.write)(self.data, buf.as_ptr(), buf.len(), &mut res);
        }

        let ret = if res.is_ok {
            Ok(written)
        } else {
            assert!(
                res.err.is_sane(),
                "{}: Result error is not convertible to a slice",
                function!()
            );

            let s = BStr::new(&res.err).to_string();
            Err(io::Error::other(s))
        };

        // SAFETY: res.err is a reference
        unsafe { http_clear_buffer(&mut res.err) };

        ret
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Empty write function
/// Good for null pointers
/// This is a write function, and has the safety requirements
/// of the WriteFn type
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_null_write(
    _: *mut c_void,
    _: *const u8,
    _: usize,
    res: *mut HttpResult,
) {
    // SAFETY: This is a write function, so the caller
    // ensures that it is convertible to a reference
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    set_err!(res, (), "Writing to empty writer");
}

/// SAFETY:
/// 1) write must take the data pointer as the first argument
/// 2) WriteFn's other safety requirements
/// 3) clear must take the data pointer and free associated memory
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
/// 1) write must take a null pointer as it's first argument
/// 2) WriteFn's other safety requirements
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_new_empty_data_writer(write: WriteFn) -> *mut Writer {
    // SAFETY:
    // 1) data is null ptr, and caller requirements say read handles a null ptr
    // 2) Caller requirement is the same
    // 3) null_clear explicitly is supposed to take a null ptr
    unsafe { http_new_writer(ptr::null_mut(), write, http_null_clear) }
}

#[allow(dead_code)]
pub(crate) fn writer_from_write<T: Write>(writer: T) -> *mut Writer {
    let writer = Box::new(writer);

    // SAFETY:
    // arg must be convertible to a reference of T
    // data must be convertible to a slice of len bytes
    // res must be convertible to a reference
    unsafe extern "C" fn write<T: Write>(
        arg: *mut c_void,
        data: *const u8,
        len: usize,
        res: *mut HttpResult,
    ) {
        // SAFETY: arg is a pointer to a T from a box, so is valid and convertible
        // to a reference
        let writer = unsafe { arg.cast::<T>().as_mut_unchecked() };

        // SAFETY: data is convertible to a slice of len bytes (guaranteed by caller)
        let data = unsafe { slice::from_raw_parts(data.cast::<u8>(), len) };

        // SAFETY: res is convertible to a reference (enforced by caller)
        let res = unsafe { res.as_mut_unchecked() };

        match writer.write(data) {
            Ok(v) => unsafe { set_ok(res, v, function!()) },
            Err(e) => set_err!(res, (), "{e}"),
        }
    }

    // SAFETY:
    // arg must be convertible to a reference of T
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
/// 1) writer must be convertible to a reference
/// 2) data must be convertible to a slice of len bytes
/// 3) res must be convertible to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_bio_write(
    writer: *mut Writer,
    data: *const u8,
    len: usize,
    res: *mut HttpResult,
) {
    assert!(
        writer.is_sane(),
        "{}: Writer is not convertible to a reference",
        function!()
    );
    assert!(
        data.is_sane(),
        "{}: Data is not convertible to a slice",
        function!()
    );
    assert!(
        res.is_sane(),
        "{}: Result is not convertible to a reference",
        function!()
    );

    // SAFETY: Writer is convertible to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    // SAFETY:
    // 1) Writer write function takes writer data as first argument
    // 2) data is convertible to a slice of len bytes
    // 3) res is convertible to a reference
    unsafe { (writer.write)(writer.data, data, len, res) }
}

/// SAFETY:
/// 1) writer must be convertible to a reference
/// 2) writer must have been created with an http_new_writer method
/// 3) writer must not be used after a call to this function
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_destroy_writer(writer: *mut Writer) {
    assert!(
        writer.is_sane(),
        "{}: Writer is not convertible to a reference",
        function!()
    );

    // SAFETY: Writer is convertible to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    // SAFETY: Writer clear takes the data pointer and free's the memory
    unsafe { (writer.clear)(writer.data) }

    // SAFETY: Writer is created through a box in the http new
    // writer functions
    drop(unsafe { Box::from_raw(writer) });
}
