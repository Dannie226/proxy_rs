pub mod request;

pub use request::Request;

pub(crate) fn read_string_to_buf(string: &str, buf: &mut [std::ffi::c_char]) -> std::ffi::c_int {
    if buf.len() < string.len() + 1 {
        return -2;
    }

    for (i, v) in string.bytes().enumerate() {
        buf[i] = v as std::ffi::c_char;
    }

    buf[string.len()] = 0;

    return 0;
}
