use std::{
    io::{self, BufRead, BufReader, Read, Write},
    num::NonZeroUsize,
    ptr,
};

use anyhow::Context;
use bstr::BStr;

use crate::{
    Buffer, ResponseWriter,
    bio::{Reader, Writer, reader_from_read},
    request::{HeaderMap, Request},
    response::{StatusCode, http_new_response_writer},
    result::result_from_string,
};

struct Body<R: BufRead> {
    reader: R,
    content_length: Option<NonZeroUsize>,
    chunked_encoding: bool,
}

impl<R: BufRead> Read for Body<R> {
    fn read(&mut self, mut buf: &mut [u8]) -> io::Result<usize> {
        let len = match (self.content_length, self.chunked_encoding) {
            (Some(len), _) => len.get(),
            (None, false) => return Ok(0),
            (None, true) => {
                let mut line = String::new();

                self.reader.read_line(&mut line)?;
                line.clear();
                self.reader.read_line(&mut line)?;

                let len = usize::from_str_radix(line.trim_end(), 16).map_err(io::Error::other)?;

                if len == 0 {
                    return Ok(0);
                } else {
                    len
                }
            }
        };

        if buf.len() > len {
            buf = &mut buf[0..len];
        }

        let res = self.reader.read(buf);
        self.content_length = NonZeroUsize::new(len - buf.len());

        res
    }
}

fn get_body_reader<'a, R: BufRead + 'a>(
    mut reader: R,
    headers: &HeaderMap,
) -> anyhow::Result<Body<R>> {
    if headers
        .get(b"transfer-encoding".as_slice())
        .filter(|v| v.iter().any(|s| s == b"chunked"))
        .is_some()
    {
        let mut line = String::new();

        reader.read_line(&mut line)?;
        let len = usize::from_str_radix(line.trim_end(), 16)
            .context("Failed to get chunked encoding first length")?;

        if len == 0 {
            return Ok(Body {
                reader,
                content_length: None,
                chunked_encoding: false,
            });
        } else {
            return Ok(Body {
                reader,
                content_length: NonZeroUsize::new(len),
                chunked_encoding: true,
            });
        }
    }

    let len = headers
        .get(b"content-length".as_slice())
        .map(|v| &*v[0])
        .and_then(|v| str::from_utf8(v).ok())
        .unwrap_or("0")
        .parse()
        .context("Failed to parse content length")?;

    Ok(Body {
        reader,
        content_length: NonZeroUsize::new(len),
        chunked_encoding: false,
    })
}

// SAFETY:
// reader must be from http_new_reader
// err must be convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_parse_http11_request(
    reader: *mut Reader,
    err: *mut Buffer,
) -> *mut Request {
    let reader = unsafe { reader.as_mut_unchecked() };
    let err = unsafe { err.as_mut_unchecked() };
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    if reader.read_line(&mut line).is_err() {
        err.copy_slice(b"Failed to read status line");
        return ptr::null_mut();
    }

    let mut spl = line.trim_end().splitn(3, " ");

    let Some(method) = spl.next() else {
        err.copy_slice(b"No method found");
        return ptr::null_mut();
    };
    let method = method.to_string();

    let Some(path) = spl.next() else {
        err.copy_slice(b"No path found");
        return ptr::null_mut();
    };
    let path = path.to_string();

    let Some(version_string) = spl.next() else {
        err.copy_slice(b"No version found");
        return ptr::null_mut();
    };

    let Some(version_string) = version_string.strip_prefix("HTTP/") else {
        err.copy_slice(b"No http prefix");
        return ptr::null_mut();
    };

    let Some((major, minor)) = version_string.split_once(".") else {
        err.copy_slice(b"Version not split with \".\"");
        return ptr::null_mut();
    };

    let Ok(major) = major.parse() else {
        err.copy_slice(b"Failed to parse major version");
        return ptr::null_mut();
    };

    let Ok(minor) = minor.parse() else {
        err.copy_slice(b"Failed to parse minor version");
        return ptr::null_mut();
    };

    let mut headers = HeaderMap::new();

    let mut line = Vec::new();
    loop {
        line.clear();

        if reader.read_until(b'\n', &mut line).is_err() {
            err.copy_slice(b"Failed to read header line");
            return ptr::null_mut();
        };

        let line = line
            .strip_suffix(b"\n")
            .unwrap_or(&*line)
            .strip_suffix(b"\r")
            .unwrap_or(&*line);

        if line.is_empty() {
            break;
        }

        let Some(colon_index) = line.iter().position(|&v| v == b':') else {
            err.copy_slice(b"Failed to find \":\" in header");
            return ptr::null_mut();
        };

        let (header_name, header_value) = line.split_at(colon_index);

        let header_name = header_name.to_ascii_lowercase().into();
        let header_value = header_value[1..].trim_ascii_start().to_vec().into();

        headers
            .entry(header_name)
            .or_insert(Vec::with_capacity(5))
            .push(header_value);
    }

    let Ok(body) = get_body_reader(reader, &headers) else {
        err.copy_slice(b"Failed to create body");
        return ptr::null_mut();
    };

    let Some(host) = headers.get(BStr::new(b"host")) else {
        err.copy_slice(b"No Host header");
        return ptr::null_mut();
    };

    let uri = format!("https://{}{}", host[0], path);

    // SAFETY:
    // reader_from_read calls one of the http_new_reader functions
    unsafe { Request::new(method, uri, (major, minor), headers, reader_from_read(body)) }
}

/// SAFETY:
/// 1) Writer must be convertable to a reference
/// 2) Result must be convertable to a reference
extern "C" fn http11_write_status(
    writer: *mut ResponseWriter,
    code: StatusCode,
    result: *mut crate::result::Result,
) {
    // SAFETY: writer is convertable to a reference
    let writer = unsafe { writer.as_mut_unchecked() };
    // SAFETY: result is convertable to a reference
    let result = unsafe { result.as_mut_unchecked() };

    // SAFETY: writer inner is convertable to a reference
    let inner = unsafe { writer.writer.as_mut_unchecked() };

    let Ok(_) = write!(
        inner,
        "HTTP/1.1 {} {}\r\n",
        code as u16,
        code.get_reason_phrase()
    )
    .inspect_err(|e| {
        *result = result_from_string(format!("{e}"));
    }) else {
        return;
    };

    for (k, v) in &writer.headers {
        for val in v {
            let Ok(_) = inner.write_all(k).inspect_err(|e| {
                *result = result_from_string(format!("{e}"));
            }) else {
                return;
            };

            let Ok(_) = inner.write_all(b": ").inspect_err(|e| {
                *result = result_from_string(format!("{e}"));
            }) else {
                return;
            };

            let Ok(_) = inner.write_all(val).inspect_err(|e| {
                *result = result_from_string(format!("{e}"));
            }) else {
                return;
            };

            let Ok(_) = inner.write_all(b"\r\n").inspect_err(|e| {
                *result = result_from_string(format!("{e}"));
            }) else {
                return;
            };
        }
    }

    let Ok(_) = inner
        .write_all(b"\r\n")
        .inspect_err(|e| *result = result_from_string(format!("{e}")))
    else {
        return;
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn http_http11_response_writer(writer: *mut Writer) -> *mut ResponseWriter {
    unsafe { http_new_response_writer(writer, http11_write_status) }
}
