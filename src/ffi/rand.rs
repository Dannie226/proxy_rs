use std::ffi::{c_int, c_uchar};

unsafe extern "C" {
    fn RAND_bytes(buf: *mut c_uchar, num: c_int) -> c_int;
    fn RAND_priv_bytes(buf: *mut c_uchar, num: c_int) -> c_int;
}

pub fn fill_bytes(bytes: &mut [u8]) -> super::error::Result<()> {
    let ret = unsafe { RAND_bytes(bytes.as_mut_ptr(), bytes.len() as c_int) };

    if ret == 0 {
        return Err(super::error::Error::next_error());
    } else {
        Ok(())
    }
}

pub fn fill_priv_bytes(bytes: &mut [u8]) -> super::error::Result<()> {
    let ret = unsafe { RAND_priv_bytes(bytes.as_mut_ptr(), bytes.len() as c_int) };

    if ret == 0 {
        return Err(super::error::Error::next_error());
    } else {
        Ok(())
    }
}
