use std::{
    ffi::c_void,
    io::{self, Read, Write},
    marker::PhantomData,
    mem::ManuallyDrop,
    slice,
};

use crate::result::Result;

pub mod ffi {
    use crate::result::ffi::Result;
    use std::{
        ffi::*,
        marker::{PhantomData, PhantomPinned},
    };

    pub type ClearFn = unsafe extern "C" fn(arg: *mut c_void);
    pub type WriteFn = unsafe extern "C" fn(
        arg: *mut c_void,
        data: *const c_void,
        len: usize,
        written: *mut Result,
    );
    pub type ReadFn =
        unsafe extern "C" fn(arg: *mut c_void, data: *mut c_void, len: usize, res: *mut Result);

    #[repr(C)]
    pub struct Reader {
        _data: (),
        _marker: PhantomData<(*mut u8, PhantomPinned)>,
    }

    #[repr(C)]
    pub struct Writer {
        _data: (),
        _marker: PhantomData<(*mut u8, PhantomPinned)>,
    }

    unsafe extern "C" {
        // Null BIO
        pub fn http_null_clear(_: *mut c_void);
        pub fn http_null_read(arg: *mut c_void, data: *mut c_void, len: usize, res: *mut Result);
        pub fn http_null_write(arg: *mut c_void, data: *mut c_void, len: usize, res: *mut Result);

        // BIO read
        pub fn http_new_reader(data: *mut c_void, read: ReadFn, clear: ClearFn) -> *mut Reader;
        pub fn http_new_empty_data_reader(read: ReadFn) -> *mut Reader;
        pub fn http_bio_read(reader: *mut Reader, data: *mut c_void, len: usize, res: *mut Result);
        pub fn http_destroy_reader(reader: *mut Reader);

        // BIO write
        pub fn http_new_writer(data: *mut c_void, write: WriteFn, clear: ClearFn) -> *mut Writer;
        pub fn http_new_empty_data_writer(write: WriteFn) -> *mut Writer;
        pub fn http_bio_write(
            writer: *mut Writer,
            data: *const c_void,
            len: usize,
            res: *mut Result,
        );
        pub fn http_destroy_writer(writer: *mut Writer);
    }
}

pub struct Reader<'a>(*mut ffi::Reader, PhantomData<&'a ()>);

/// SAFETY:
/// arg must be convertable to a reference of T
/// data must be convertable to a slice of len bytes
/// res must be convertable to a reference
unsafe extern "C" fn read<T: Read>(
    arg: *mut c_void,
    data: *mut c_void,
    len: usize,
    res: *mut crate::result::ffi::Result,
) {
    // SAFETY: arg is convertable to a &mut T
    let reader = unsafe { arg.cast::<T>().as_mut_unchecked() };

    // SAFETY: data is convertable to a slice of len bytes
    let buf = unsafe { slice::from_raw_parts_mut(data.cast(), len) };

    // SAFETY: res is convertable to a reference
    let res = unsafe { res.as_mut_unchecked() };

    let r = crate::result::Result::from_io_result(reader.read(buf));

    *res = r.into_ffi();
}

/// SAFETY:
/// arg must be convertable to a reference of T
/// data must be convertable to a slice of len bytes
/// res must be convertable to a reference
unsafe extern "C" fn write<T: Write>(
    arg: *mut c_void,
    data: *const c_void,
    len: usize,
    res: *mut crate::result::ffi::Result,
) {
    // SAFETY: arg is convertable to a &mut T
    let writer = unsafe { arg.cast::<T>().as_mut_unchecked() };

    // SAFETY: data is convertable to a slice of len bytes
    let buf = unsafe { slice::from_raw_parts(data.cast(), len) };

    // SAFETY: res is convertable to a reference
    let res = unsafe { res.as_mut_unchecked() };

    let r = crate::result::Result::from_io_result(writer.write(buf));

    *res = r.into_ffi();
}

/// SAFETY:
/// arg must be a pointer to a T,
/// arg must have been created via Box::into_raw on a T
unsafe extern "C" fn clear_read<T: Read>(arg: *mut c_void) {
    drop(unsafe { Box::from_raw(arg.cast::<T>()) });
}

/// SAFETY:
/// arg must be a pointer to a T,
/// arg must have been created via Box::into_raw on a T
unsafe extern "C" fn clear_write<T: Write>(arg: *mut c_void) {
    drop(unsafe { Box::from_raw(arg.cast::<T>()) });
}

impl<'a> Reader<'a> {
    pub fn new<T: Read + 'a>(reader: T) -> Reader<'a> {
        let reader = Box::new(reader);

        let data = Box::into_raw(reader);

        // SAFETY:
        // data is a pointer to a T, created from Box::into_raw, and the other
        // safety requirements will be dealt with later
        let r = unsafe { ffi::http_new_reader(data.cast(), read::<T>, clear_read::<T>) };

        Reader(r, PhantomData)
    }

    pub fn into_ffi(self) -> *mut ffi::Reader {
        ManuallyDrop::new(self).0
    }
}

impl<'a> Read for Reader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut r = Result::ok(0).into_ffi();
        // SAFETY:
        // self.0 came from a call to http_new_reader, and so is valid
        // buf is itself a slice to buf.len() bytes, so is convertable back
        // r is, quite obviously, a reference
        unsafe { ffi::http_bio_read(self.0, buf.as_mut_ptr().cast(), buf.len(), &mut r) };

        Result::from_ffi(r).into_io_result()
    }
}

// SAFETY:
// A reader is the sole owner of the underlying pointer, so as long as it's lifetime lives long
// enough, it is safe to send to another thread
// It is also safe to send &Reader to another thread, though you cannot actually do anything with it
unsafe impl<'a> Send for Reader<'a> {}
unsafe impl<'a> Sync for Reader<'a> {}

impl<'a> Drop for Reader<'a> {
    fn drop(&mut self) {
        // SAFETY:
        // This is the drop function, so self will not be used again
        // And, because self is in fact valid, the reader came from
        // a new_reader call, and therefore delete_reader is safe
        unsafe {
            ffi::http_destroy_reader(self.0);
        }
    }
}

pub struct Writer<'a>(*mut ffi::Writer, PhantomData<&'a ()>);

impl<'a> Writer<'a> {
    pub fn new<T: Write + 'a>(reader: T) -> Writer<'a> {
        let reader = Box::new(reader);

        let data = Box::into_raw(reader);

        // SAFETY:
        // data is a pointer to a T, created from Box::into_raw, and the other
        // safety requirements will be dealt with later
        let r = unsafe { ffi::http_new_writer(data.cast(), write::<T>, clear_write::<T>) };

        Writer(r, PhantomData)
    }

    pub fn into_ffi(self) -> *mut ffi::Writer {
        ManuallyDrop::new(self).0
    }
}

impl<'a> Write for Writer<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut r = Result::ok(0).into_ffi();
        // SAFETY:
        // self.0 came from a call to http_new_writer, and so is valid
        // buf is itself a slice to buf.len() bytes, so is convertable back
        // r is, quite obviously, a reference
        unsafe { ffi::http_bio_write(self.0, buf.as_ptr().cast(), buf.len(), &mut r) };

        Result::from_ffi(r).into_io_result()
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> Drop for Writer<'a> {
    fn drop(&mut self) {
        // SAFETY:
        // This is the drop function, so self will not be used again
        // And, because self is in fact valid, the writer came from
        // a new_writer call, and therefore delete_writer is safe
        unsafe {
            ffi::http_destroy_writer(self.0);
        }
    }
}

// SAFETY:
// A writer is the sole owner of the underlying pointer, so as long as it's lifetime lives long
// enough, it is safe to send to another thread
// It is also safe to send &Writer to another thread, though you cannot actually do anything with it
unsafe impl<'a> Send for Writer<'a> {}
unsafe impl<'a> Sync for Writer<'a> {}
