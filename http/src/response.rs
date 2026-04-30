use std::ffi::c_void;

use bstr::BString;

use crate::{
    ConstBuffer,
    bio::{Writer, http_bio_write, http_destroy_writer},
    request::HeaderMap,
    result::{http_res_is_ok, http_res_new_ok},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum StatusCode {
    Continue = 100,
    SwitchingProtocols = 101,
    Processing = 102,
    EarlyHints = 103,

    OK = 200,
    Created = 201,
    Accepted = 202,
    NonAuthoritativeInformation = 203,
    NoContent = 204,
    ResetContent = 205,
    PartialContent = 206,

    MultipleChoices = 300,
    MovedPermanently = 301,
    Found = 302,
    SeeOther = 303,
    NotModified = 304,
    TemporaryRedirect = 307,
    PermanentRedirect = 308,

    BadRequest = 400,
    Unauthorized = 401,
    PaymentRequired = 402,
    Forbidden = 403,
    NotFound = 404,
    MethodNotAllowed = 405,
    NotAcceptable = 406,
    ProxyAuthenticationRequired = 407,
    RequestTimeout = 408,
    Conflict = 409,
    Gone = 410,
    LengthRequired = 411,
    PreconditionFailed = 412,
    ContentTooLarge = 413,
    URITooLong = 414,
    UnsupportedMediaType = 415,
    RangeNotSatisfiable = 416,
    ExpectationFailed = 417,
    ImATeapot = 418,
    MisdirectedRequest = 421,
    UpgradeRequired = 426,
    PreconditionRequired = 428,
    TooManyRequests = 429,
    RequestHeaderFieldsTooLarge = 431,
    UnavailableForLegalReasons = 451,

    InternalServerError = 500,
    NotImplemented = 501,
    BadGateway = 502,
    ServiceUnavailable = 503,
    GatewayTimeout = 504,
    HTTPVersionNotSupported = 505,
    VariantAlsoNegotiates = 506,
    NotExtended = 510,
    NetworkAuthenticationRequired = 511,
}

impl StatusCode {
    pub fn get_reason_phrase(self) -> &'static str {
        match self {
            Self::Continue => "Continue",
            Self::SwitchingProtocols => "Switching Protocols",
            Self::Processing => "Processing",
            Self::EarlyHints => "Early Hints",

            Self::OK => "OK",
            Self::Created => "Created",
            Self::Accepted => "Accepted",
            Self::NonAuthoritativeInformation => "Non-Authoritative Information",
            Self::NoContent => "No Content",
            Self::ResetContent => "Reset Content",
            Self::PartialContent => "Partial Content",

            Self::MultipleChoices => "Multiple Choices",
            Self::MovedPermanently => "Moved Permanently",
            Self::Found => "Found",
            Self::SeeOther => "See Other",
            Self::NotModified => "Not Modified",
            Self::TemporaryRedirect => "Temporary Redirect",
            Self::PermanentRedirect => "Permanent Redirect",

            Self::BadRequest => "Bad Request",
            Self::Unauthorized => "Unauthorized",
            Self::PaymentRequired => "Payment Required",
            Self::Forbidden => "Forbidden",
            Self::NotFound => "Not Found",
            Self::MethodNotAllowed => "Method Not Allowed",
            Self::NotAcceptable => "Not Acceptable",
            Self::ProxyAuthenticationRequired => "Proxy Authentication Required",
            Self::RequestTimeout => "Request Timeout",
            Self::Conflict => "Conflict",
            Self::Gone => "Gone",
            Self::LengthRequired => "Length Required",
            Self::PreconditionFailed => "Precondition Failed",
            Self::ContentTooLarge => "Content Too Large",
            Self::URITooLong => "URI Too Long",
            Self::UnsupportedMediaType => "Unsupported Media Type",
            Self::RangeNotSatisfiable => "Range Not Satisfiable",
            Self::ExpectationFailed => "Expectation Failed",
            Self::ImATeapot => "I'm A Teapot",
            Self::MisdirectedRequest => "Misdirected Request",
            Self::UpgradeRequired => "Upgrade Required",
            Self::PreconditionRequired => "Precondition Required",
            Self::TooManyRequests => "Too Many Requests",
            Self::RequestHeaderFieldsTooLarge => "Request Header Fields Too Large",
            Self::UnavailableForLegalReasons => "Unavailable For Legal Reasons",

            Self::InternalServerError => "Internal Server Error",
            Self::NotImplemented => "Not Implemented",
            Self::BadGateway => "Bad Gateway",
            Self::ServiceUnavailable => "Service Unavailable",
            Self::GatewayTimeout => "Gateway Timeout",
            Self::HTTPVersionNotSupported => "HTTP Version Not Supported",
            Self::VariantAlsoNegotiates => "Variant Also Negotiates",
            Self::NotExtended => "Not Extended",
            Self::NetworkAuthenticationRequired => "Network Authentication Required",
        }
    }
}

impl TryFrom<u16> for StatusCode {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Ok(match value {
            100 => Self::Continue,
            101 => Self::SwitchingProtocols,
            102 => Self::Processing,
            103 => Self::EarlyHints,

            200 => Self::OK,
            201 => Self::Created,
            202 => Self::Accepted,
            203 => Self::NonAuthoritativeInformation,
            204 => Self::NoContent,
            205 => Self::ResetContent,
            206 => Self::PartialContent,

            300 => Self::MultipleChoices,
            301 => Self::MovedPermanently,
            302 => Self::Found,
            303 => Self::SeeOther,
            304 => Self::NotModified,
            307 => Self::TemporaryRedirect,
            308 => Self::PermanentRedirect,

            400 => Self::BadRequest,
            401 => Self::Unauthorized,
            402 => Self::PaymentRequired,
            403 => Self::Forbidden,
            404 => Self::NotFound,
            405 => Self::MethodNotAllowed,
            406 => Self::NotAcceptable,
            407 => Self::ProxyAuthenticationRequired,
            408 => Self::RequestTimeout,
            409 => Self::Conflict,
            410 => Self::Gone,
            411 => Self::LengthRequired,
            412 => Self::PreconditionFailed,
            413 => Self::ContentTooLarge,
            414 => Self::URITooLong,
            415 => Self::UnsupportedMediaType,
            416 => Self::RangeNotSatisfiable,
            417 => Self::ExpectationFailed,
            418 => Self::ImATeapot,
            421 => Self::MisdirectedRequest,
            426 => Self::UpgradeRequired,
            428 => Self::PreconditionRequired,
            429 => Self::TooManyRequests,
            431 => Self::RequestHeaderFieldsTooLarge,
            451 => Self::UnavailableForLegalReasons,

            500 => Self::InternalServerError,
            501 => Self::NotImplemented,
            502 => Self::BadGateway,
            503 => Self::ServiceUnavailable,
            504 => Self::GatewayTimeout,
            505 => Self::HTTPVersionNotSupported,
            506 => Self::VariantAlsoNegotiates,
            510 => Self::NotExtended,
            511 => Self::NetworkAuthenticationRequired,
            _ => return Err(()),
        })
    }
}

/// SAFETY:
/// 1) Writer must be convertable to a reference
/// 2) res must be convertable to a reference
pub type StatusFn = unsafe extern "C" fn(
    writer: *mut ResponseWriter,
    code: StatusCode,
    res: *mut crate::result::Result,
);

/// Caller must ensure the following:
/// 1) Writer is convertable to a reference
/// 2) Data is convertable to a slice of len bytes
/// 3) Written is convertable to a reference
pub type WriteFn = unsafe extern "C" fn(
    writer: *mut Writer,
    data: *const c_void,
    len: usize,
    written: *mut crate::result::Result,
);

pub struct ResponseWriter {
    pub(crate) writer: *mut Writer,
    pub(crate) headers: HeaderMap,
    write_fn: WriteFn,
    status_fn: StatusFn,
    written: bool,
}

/// SAFETY:
/// 1) Writer must have been created from http_new_writer function
/// 2) WriteFn's safety requirements
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_new_response_writer(
    writer: *mut Writer,
    status_fn: StatusFn,
) -> *mut ResponseWriter {
    unsafe { http_custom_response_writer(writer, status_fn, http_bio_write) }
}

/// SAFETY:
/// 1) Writer must have been created from http_new_writer function
/// 2) WriteFn's safety requirements
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_custom_response_writer(
    writer: *mut Writer,
    status_fn: StatusFn,
    write_fn: WriteFn,
) -> *mut ResponseWriter {
    let w = Box::new(ResponseWriter {
        writer,
        headers: HeaderMap::new(),
        status_fn,
        write_fn,
        written: false,
    });

    Box::into_raw(w)
}

/// Caller must ensure the following:
/// 1) Writer is convertable to a reference
/// 2) Data is convertable to a slice of len bytes
/// 3) res is convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_write(
    writer: *mut ResponseWriter,
    data: *const u8,
    len: usize,
    res: *mut crate::result::Result,
) {
    // SAFETY: Writer is convertable to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    if !writer.written {
        // SAFETY:
        // Writer is convertable to a reference, and status code is obviously a status code
        // Written is convertable to a reference
        unsafe { http_write_status(writer, StatusCode::OK, res) };

        // SAFETY:
        // res is convertable to a reference
        if !unsafe { http_res_is_ok(res) } {
            return;
        }
    }

    // SAFETY: written is convertable to a reference
    let written = unsafe { res.as_mut_unchecked() };

    unsafe { (writer.write_fn)(writer.writer, data.cast(), len, written) }
}

/// Caller must ensure the following:
/// 1) Writer is convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_add_header(
    writer: *mut ResponseWriter,
    name: ConstBuffer,
    value: ConstBuffer,
) {
    // SAFETY: Writer is convertable to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    // SAFETY: header_name is convertable to a slice of name_len bytes
    let header_name = name.as_slice();

    // SAFETY: header_value is convertable to a slice of value_len bytes
    let header_value = value.as_slice();

    let mut header_name = BString::from(header_name);
    header_name.make_ascii_lowercase();

    let headers = &mut writer.headers;

    headers
        .entry(header_name)
        .or_insert(Vec::new())
        .push(BString::from(header_value));
}

/// Caller must ensure the following:
/// 1) Writer is convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_remove_header(writer: *mut ResponseWriter, header: ConstBuffer) {
    // SAFETY: Writer is convertable to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    let header = header.as_slice();

    let headers = &mut writer.headers;

    headers.remove(header);
}

/// Caller must ensure the following:
/// 1) Writer is convertable to a reference
/// 2) status must be a valid status code (technically, you may put any u16 in here)
/// 3) res must be convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_write_status(
    writer: *mut ResponseWriter,
    status: StatusCode,
    res: *mut crate::result::Result,
) {
    // SAFETY: Writer is convertable to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    // SAFETY: res is convertable to a reference
    let res = unsafe { res.as_mut_unchecked() };

    if writer.written {
        *res = http_res_new_ok(0);
    }

    // SAFETY:
    // writer is a reference
    // res can be converted to a reference
    unsafe { (writer.status_fn)(writer, status, res) };

    // SAFETY: res is convertable to a reference
    if !unsafe { http_res_is_ok(res) } {
        return;
    }

    writer.written = true;
}

/// SAFETY:
/// Writer must be a writer created from http_new_response_writer
/// Writer must not be used after call to this function
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_destroy_response_writer(writer: *mut ResponseWriter) {
    // SAFETY: Writer was created from http_new_response_writer, so the pointer
    // is from Box::into_raw
    let w = unsafe { Box::from_raw(writer) };

    // SAFETY: Writer is a reference, and writer was created from the http_new_writer method
    unsafe {
        http_destroy_writer(w.writer);
    }

    drop(w);
}
