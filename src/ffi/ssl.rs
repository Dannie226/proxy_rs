#![allow(dead_code)]

use std::ffi::*;

use crate::opaque_type;

opaque_type!(SSL_METHOD, SSL_CTX, SSL);

type ALPNSelectCallback = extern "C" fn(
    ssl: *mut SSL,
    out: *mut *const c_uchar,
    out_len: *mut c_uchar,
    r#in: *const c_uchar,
    in_len: c_uint,
    arg: *mut c_void,
) -> c_int;

pub const SSL_FILETYPE_PEM: c_int = 1;
pub const SSL_FILETYPE_ASN1: c_int = 2;

pub const OPENSSL_NPN_NEGOTIATED: c_int = 1;
pub const OPENSSL_NPN_NO_OVERLAP: c_int = 2;

pub const SSL_TLSEXT_ERR_OK: c_int = 0;
pub const SSL_TLSEXT_ERR_ALERT_WARNING: c_int = 1;
pub const SSL_TLSEXT_ERR_ALERT_FATAL: c_int = 2;
pub const SSL_TLSEXT_ERR_NOACK: c_int = 3;

unsafe extern "C" {
    pub fn TLS_server_method() -> *const SSL_METHOD;
    pub fn SSL_CTX_new(method: *const SSL_METHOD) -> *mut SSL_CTX;
    pub fn SSL_CTX_use_certificate_file(
        ctx: *mut SSL_CTX,
        file: *const c_char,
        r#type: c_int,
    ) -> c_int;
    pub fn SSL_CTX_use_PrivateKey_file(
        ctx: *mut SSL_CTX,
        file: *const c_char,
        r#type: c_int,
    ) -> c_int;
    pub fn SSL_CTX_set_alpn_select_cb(ctx: *mut SSL_CTX, cb: ALPNSelectCallback, arg: *mut c_void);
    pub fn SSL_select_next_proto(
        out: *mut *mut c_uchar,
        outlen: *mut c_uchar,
        server: *const c_uchar,
        server_len: c_uint,
        client: *const c_uchar,
        client_len: c_uint,
    ) -> c_int;

    pub fn SSL_get0_alpn_selected(
        ssl: *const SSL,
        data: *mut *const c_uchar,
        data_len: *mut c_uchar,
    );

    pub fn SSL_CTX_free(ctx: *mut SSL_CTX);

    pub fn SSL_new(ctx: *mut SSL_CTX) -> *mut SSL;
    pub fn SSL_set_fd(ssl: *mut SSL, fd: c_int);
    pub fn SSL_accept(ssl: *mut SSL) -> c_int;

    pub fn SSL_shutdown(ssl: *mut SSL);
    pub fn SSL_free(ssl: *mut SSL);

    pub fn SSL_read_ex(
        ssl: *mut SSL,
        buf: *mut c_void,
        buf_len: usize,
        read_bytes: *mut usize,
    ) -> c_int;
    pub fn SSL_write_ex(
        ssl: *mut SSL,
        buf: *const c_void,
        buf_len: usize,
        written: *mut usize,
    ) -> c_int;
}
