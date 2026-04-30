use std::{
    ffi::{CStr, c_int, c_uchar, c_uint, c_void},
    io::{self},
    marker::PhantomData,
    net::{SocketAddr, TcpListener, ToSocketAddrs},
    ptr::{self, NonNull},
};

use anyhow::Context;
use openssl_sys::*;

use crate::tls::{self, Error, stream::*};

static PROTOS: &[u8] = b"\x08http/1.1";

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
    fn new(method: *const SSL_METHOD) -> tls::Result<SSLContext> {
        let ctx = NonNull::new(unsafe { SSL_CTX_new(method) }).ok_or_else(Error::next_error)?;

        let ctx = SSLContext {
            ctx,
            _marker: PhantomData {},
        };

        unsafe {
            SSL_CTX_set_alpn_select_cb__fixed_rust(
                ctx.ctx.as_ptr(),
                Some(select_alpn),
                ptr::null_mut(),
            );
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

        // SAFETY: The ssl context on self is valid
        let stream =
            unsafe { TlsStream::new(self.ctx.ctx.as_ptr(), stream).map_err(io::Error::other)? };

        Ok((stream, addr))
    }
}
