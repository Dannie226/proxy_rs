use std::{
    ffi::c_int,
    io::{self, Read, Write},
    marker::PhantomData,
    mem::{self, ManuallyDrop},
    net::{Shutdown, TcpStream},
    os::fd::{AsRawFd, FromRawFd, IntoRawFd},
    ptr::{self, NonNull},
    slice,
};

use openssl_sys::*;

unsafe extern "C" {
    fn SSL_set_fd(ssl: *mut SSL, fd: c_int) -> c_int;
    fn SSL_dup(ssl: *mut SSL) -> *mut SSL;
}

use crate::tls;

pub struct TlsStream {
    ssl: NonNull<SSL>,
    stream: c_int,
    _marker: PhantomData<SSL>,
}

impl TlsStream {
    // SAFETY: Caller must ensure that ctx is a valid pointer to an SSL_CTX object
    pub unsafe fn new(ctx: *mut SSL_CTX, stream: TcpStream) -> tls::Result<TlsStream> {
        let ssl = NonNull::new(unsafe { SSL_new(ctx) }).ok_or_else(tls::Error::next_error)?;

        let stream_fd = stream.as_raw_fd();

        unsafe {
            if SSL_set_fd(ssl.as_ptr(), stream_fd.as_raw_fd()) == 0 {
                SSL_free(ssl.as_ptr());
                return Err(tls::Error::next_error());
            }
        }

        if unsafe { SSL_accept(ssl.as_ptr()) } <= 0 {
            return Err(tls::Error::next_error());
        }

        mem::forget(stream);

        Ok(TlsStream {
            ssl,
            stream: stream_fd,
            _marker: PhantomData {},
        })
    }

    pub fn get_selected_alpn(&self) -> &'_ [u8] {
        unsafe {
            let mut data = ptr::null();
            let mut len = 0;
            SSL_get0_alpn_selected(self.ssl.as_ptr(), &raw mut data, &raw mut len);

            if data.is_null() {
                "No negotiated protocol".as_bytes()
            } else {
                slice::from_raw_parts(data, len as usize)
            }
        }
    }

    pub fn split(self) -> io::Result<(TlsReadStream, TlsWriteStream)> {
        // SAFETY: ssl is a valid pointer, as validated earlier
        let write_ssl = NonNull::new(unsafe { SSL_dup(self.ssl.as_ptr()) })
            .ok_or_else(tls::Error::next_error)
            .map_err(io::Error::other)?;

        let local_stream = ManuallyDrop::new(unsafe { TcpStream::from_raw_fd(self.stream) });
        let self_stream = ManuallyDrop::new(self);

        let write_fd = match local_stream.try_clone() {
            Ok(fd) => fd.into_raw_fd(),
            Err(e) => {
                unsafe { SSL_free(write_ssl.as_ptr()) };
                return Err(e);
            }
        };

        unsafe {
            if SSL_set_fd(write_ssl.as_ptr(), write_fd.as_raw_fd()) == 0 {
                SSL_free(write_ssl.as_ptr());
                return Err(tls::Error::next_error()).map_err(io::Error::other);
            }
        }

        let write_stream = TlsWriteStream {
            ssl: write_ssl,
            stream: write_fd,
            _marker: PhantomData,
        };

        let read_stream = TlsReadStream {
            ssl: self_stream.ssl,
            stream: self_stream.stream,
            _marker: PhantomData,
        };

        Ok((read_stream, write_stream))
    }
}

impl Read for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read_bytes = 0;

        // SAFETY: SSL_read_ex is not thread safe, however the mutable
        // reference ensures we are the only one with access to the stream
        // at this time

        unsafe {
            let success = SSL_read_ex(
                self.ssl.as_ptr(),
                buf.as_mut_ptr() as _,
                buf.len(),
                &raw mut read_bytes,
            );

            if success == 0 {
                return Err(io::Error::other(tls::Error::next_error()));
            }
        };

        Ok(read_bytes)
    }
}

impl Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written_bytes = 0;

        // SAFETY: SSL_write_ex is not thread safe, however the mutable
        // reference ensures we are the only one with access to the stream
        // at this time

        unsafe {
            let success = SSL_write_ex(
                self.ssl.as_ptr(),
                buf.as_ptr() as _,
                buf.len(),
                &raw mut written_bytes,
            );

            if success == 0 {
                return Err(io::Error::other(tls::Error::next_error()));
            };
        };

        Ok(written_bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        // No buffering involved
        Ok(())
    }
}

// SAFETY:
// The TlsStream holds exclusive access to the file descriptor and the ssl
// stream. The borrow checker ensures exclusive access for reading and writing
// (because they take &mut) and for splitting (because it takes ownership)
// So, it is safe to be sent to another thread as is &TlsStream, though there
// isn't anything you can do with just &TlsStream
unsafe impl Send for TlsStream {}
unsafe impl Sync for TlsStream {}

impl Drop for TlsStream {
    fn drop(&mut self) {
        // SAFETY: SSL_free exists and self.ssl is a reference to the SSL object
        unsafe {
            SSL_free(self.ssl.as_ptr());

            let stream = TcpStream::from_raw_fd(self.stream);

            _ = stream.shutdown(Shutdown::Both);
        }
    }
}

pub struct TlsReadStream {
    ssl: NonNull<SSL>,
    stream: c_int,
    _marker: PhantomData<SSL>,
}

impl Read for TlsReadStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read_bytes = 0;

        // SAFETY: SSL_read_ex is not thread safe, however the mutable
        // reference ensures we are the only one with access to the stream
        // at this time

        unsafe {
            let success = SSL_read_ex(
                self.ssl.as_ptr(),
                buf.as_mut_ptr() as _,
                buf.len(),
                &raw mut read_bytes,
            );

            if success == 0 {
                return Err(io::Error::other(tls::Error::next_error()));
            }
        };

        Ok(read_bytes)
    }
}

// SAFETY:
// See rational for TlsStream
unsafe impl Send for TlsReadStream {}
unsafe impl Sync for TlsReadStream {}

impl Drop for TlsReadStream {
    fn drop(&mut self) {
        // SAFETY: SSL_free exists and self.ssl is a reference to the SSL object
        unsafe {
            SSL_free(self.ssl.as_ptr());

            let stream = TcpStream::from_raw_fd(self.stream);

            _ = stream.shutdown(Shutdown::Read);
        }
    }
}

pub struct TlsWriteStream {
    ssl: NonNull<SSL>,
    stream: c_int,
    _marker: PhantomData<SSL>,
}

impl Write for TlsWriteStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written_bytes = 0;

        // SAFETY: SSL_write_ex is not thread safe, however the mutable
        // reference ensures we are the only one with access to the stream
        // at this time

        unsafe {
            let success = SSL_write_ex(
                self.ssl.as_ptr(),
                buf.as_ptr() as _,
                buf.len(),
                &raw mut written_bytes,
            );

            if success == 0 {
                return Err(io::Error::other(tls::Error::next_error()));
            };
        };

        Ok(written_bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        // No buffering involved
        Ok(())
    }
}

// SAFETY:
// See rational for TlsStream
unsafe impl Send for TlsWriteStream {}
unsafe impl Sync for TlsWriteStream {}

impl Drop for TlsWriteStream {
    fn drop(&mut self) {
        // SAFETY: SSL_free exists and self.ssl is a reference to the SSL object
        unsafe {
            SSL_free(self.ssl.as_ptr());

            let stream = TcpStream::from_raw_fd(self.stream);

            _ = stream.shutdown(Shutdown::Write);
        }
    }
}
