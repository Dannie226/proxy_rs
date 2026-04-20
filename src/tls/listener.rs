use core::slice;
use std::{
    ffi::{CStr, c_int, c_uchar, c_uint, c_void},
    io::{self, Read, Write},
    marker::PhantomData,
    net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    os::fd::{FromRawFd, IntoRawFd},
    ptr::{self, NonNull},
};

use anyhow::Context;

use crate::ffi::{self, error::Error, ssl::*};

static PROTOS: &[u8] = b"\x02h2\x08http/1.1";

extern "C" fn select_alpn(
    _ssl: *mut SSL,
    out: *mut *const c_uchar,
    out_len: *mut c_uchar,
    client: *const c_uchar,
    client_len: c_uint,
    _arg: *mut c_void,
) -> c_int {
    let mut output = ptr::null_mut();
    let mut output_len = 0;

    let ret = unsafe {
        SSL_select_next_proto(
            &mut output,
            &mut output_len,
            PROTOS.as_ptr(),
            PROTOS.len() as c_uint,
            client,
            client_len,
        )
    };

    if ret == OPENSSL_NPN_NO_OVERLAP {
        return SSL_TLSEXT_ERR_ALERT_FATAL;
    }

    unsafe {
        *out = output;
        *out_len = output_len;
    };

    return SSL_TLSEXT_ERR_OK;
}

struct SSLContext {
    ctx: NonNull<SSL_CTX>,
    _marker: PhantomData<SSL_CTX>,
}

impl SSLContext {
    fn new(method: *const SSL_METHOD) -> ffi::error::Result<SSLContext> {
        let ctx = NonNull::new(unsafe { SSL_CTX_new(method) }).ok_or_else(Error::next_error)?;

        let ctx = SSLContext {
            ctx,
            _marker: PhantomData {},
        };

        unsafe {
            SSL_CTX_set_alpn_select_cb(ctx.ctx.as_ptr(), select_alpn, ptr::null_mut());
        }

        Ok(ctx)
    }

    fn set_key_pair(&self, cert: &CStr, key: &CStr) -> anyhow::Result<()> {
        if unsafe {
            SSL_CTX_use_certificate_file(self.ctx.as_ptr(), cert.as_ptr(), SSL_FILETYPE_PEM)
        } <= 0
        {
            return Err(Error::next_error()).context("Failed to set certificate");
        }

        if unsafe { SSL_CTX_use_PrivateKey_file(self.ctx.as_ptr(), key.as_ptr(), SSL_FILETYPE_PEM) }
            <= 0
        {
            return Err(Error::next_error()).context("Failed to set key");
        }

        Ok(())
    }
}

// I want to be able to send this across threads, but it cannot be synced
unsafe impl Send for SSLContext {}

impl Drop for SSLContext {
    fn drop(&mut self) {
        unsafe {
            SSL_CTX_free(self.ctx.as_ptr());
        }
    }
}

pub struct TlsStream {
    ssl: NonNull<SSL>,
    stream: c_int,
    _marker: PhantomData<SSL>,
}

impl TlsStream {
    pub fn new(listener: &TlsListener, stream: TcpStream) -> anyhow::Result<TlsStream> {
        let ssl = NonNull::new(unsafe { SSL_new(listener.ctx.ctx.as_ptr()) })
            .ok_or_else(Error::next_error)
            .context("Failed to create new TLS stream")?;

        let fd = stream.into_raw_fd();

        unsafe { SSL_set_fd(ssl.as_ptr(), fd) };

        if unsafe { SSL_accept(ssl.as_ptr()) } <= 0 {
            Err(Error::next_error()).context("Failed TLS handshake")?;
        }

        Ok(TlsStream {
            ssl,
            stream: fd,
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
}

impl Drop for TlsStream {
    fn drop(&mut self) {
        unsafe {
            SSL_shutdown(self.ssl.as_ptr());
            SSL_free(self.ssl.as_ptr());
            // Safety: TlsStream is the sole owner of the stream file descriptor
            TcpStream::from_raw_fd(self.stream);
        }
    }
}

impl Read for &TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read_len = 0;
        let ok = unsafe {
            SSL_read_ex(
                self.ssl.as_ptr(),
                buf.as_mut_ptr() as *mut c_void,
                buf.len(),
                &raw mut read_len,
            )
        };

        if ok == 0 {
            return Err(io::Error::other(Error::next_error()));
        }

        Ok(read_len)
    }
}

impl Write for &TlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written = 0;

        let ok = unsafe {
            SSL_write_ex(
                self.ssl.as_ptr(),
                buf.as_ptr() as *const c_void,
                buf.len(),
                &raw mut written,
            )
        };

        if ok == 0 {
            return Err(io::Error::other(Error::next_error()));
        }

        return Ok(written);
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self).read(buf)
    }
}

impl Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self).flush()
    }
}

// I would like to be able to send this around to different threads
unsafe impl Send for TlsStream {}

pub struct TlsListener {
    listener: TcpListener,
    ctx: SSLContext,
}

impl TlsListener {
    pub fn bind(addr: impl ToSocketAddrs) -> anyhow::Result<TlsListener> {
        let l = TcpListener::bind(addr)?;

        let ctx = {
            let method = unsafe { TLS_server_method() };

            SSLContext::new(method)?
        };

        Ok(TlsListener { listener: l, ctx })
    }

    pub fn set_key_pair(&self, cert: &CStr, key: &CStr) -> anyhow::Result<()> {
        self.ctx.set_key_pair(cert, key)
    }

    pub fn accept(&self) -> anyhow::Result<(TlsStream, SocketAddr)> {
        let (stream, addr) = self
            .listener
            .accept()
            .context("Failed to accept new connection")?;

        let stream = TlsStream::new(self, stream).map_err(io::Error::other)?;

        Ok((stream, addr))
    }
}
