use std::{
    ffi::CStr,
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    num::NonZeroUsize,
};

use crate::{
    HeaderMap, IsSane, Request, ResponseWriter,
    bio::{Reader, Writer, http_destroy_reader, reader_from_read},
    function,
    response::{get_reason_phrase, http_new_response_writer},
    result::{HttpResult, set_err, set_ok},
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

        if let &Ok(read) = &res {
            self.content_length = NonZeroUsize::new(len - read);
        }

        res
    }
}

fn get_body_reader<'a, R: BufRead + 'a>(
    mut reader: R,
    headers: &HeaderMap,
) -> Result<Body<R>, String> {
    if headers
        .get(b"transfer-encoding".as_slice())
        .filter(|v| v.iter().any(|s| s == b"chunked"))
        .is_some()
    {
        let mut line = String::new();

        match reader.read_line(&mut line) {
            Ok(_) => {}
            Err(e) => {
                return Err(format!("Failed to read chunked encoding line: {e}"));
            }
        }

        let len = match usize::from_str_radix(line.trim_end(), 16) {
            Ok(u) => u,
            Err(e) => {
                return Err(format!("Failed to get chunked encoding first length: {e}"));
            }
        };

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

    let len = match headers
        .get(b"content-length".as_slice())
        .map(|v| &*v[0])
        .and_then(|v| str::from_utf8(v).ok())
        .unwrap_or("0")
        .parse::<usize>()
    {
        Ok(l) => l,
        Err(e) => {
            return Err(format!("Failed to parse content length: {e}"));
        }
    };

    Ok(Body {
        reader,
        content_length: NonZeroUsize::new(len),
        chunked_encoding: false,
    })
}

// Invariants:
// Reader (self.0) must be convertible to a reference
struct RAIIReader(*mut Reader);

impl Read for RAIIReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        unsafe { self.0.as_mut_unchecked() }.read(buf)
    }
}

impl Drop for RAIIReader {
    fn drop(&mut self) {
        unsafe {
            http_destroy_reader(self.0);
        }
    }
}

// SAFETY:
// reader must be from http_new_reader
// res must be convertible to a reference
// res must be a result to a pointer to a request
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_parse_http11_request(reader: *mut Reader, res: *mut HttpResult) {
    assert!(
        reader.is_sane(),
        "{}: Reader is not convertible to a reference",
        function!()
    );

    assert!(
        res.is_sane(),
        "{}: Result is not convertible to a reference",
        function!()
    );

    let reader = RAIIReader(reader);
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    match reader.read_line(&mut line) {
        Ok(_) => {}
        Err(e) => {
            set_err!(res, (), "Failed to read status line: {e}");
        }
    }

    let mut spl = line.trim_end().splitn(3, " ");

    let Some(method) = spl.next().filter(|&v| !v.is_empty()) else {
        set_err!(res, (), "No method found");
    };
    let method = method.to_string();

    let Some(uri) = spl.next() else {
        set_err!(res, (), "No URI found");
    };
    let uri = uri.to_string();

    let Some(version_string) = spl.next() else {
        set_err!(res, (), "No version found");
    };

    let Some(version_string) = version_string.strip_prefix("HTTP/") else {
        set_err!(res, (), "No http prefix");
    };

    let Some((major, minor)) = version_string.split_once(".") else {
        set_err!(res, (), "Version not split with \".\"");
    };

    if major != "1" || minor != "1" {
        set_err!(res, (), "HTTP not version 1.1");
    }

    let mut headers = HeaderMap::new();

    let mut line = Vec::new();
    loop {
        line.clear();

        match reader.read_until(b'\n', &mut line) {
            Ok(_) => {}
            Err(e) => {
                set_err!(res, (), "Failed to read header line: {e}");
            }
        };

        let line = line.strip_suffix(b"\n").unwrap_or(&*line);
        let line = line.strip_suffix(b"\r").unwrap_or(&*line);

        if line.is_empty() {
            break;
        }

        let Some(colon_index) = line.iter().position(|&v| v == b':') else {
            set_err!(res, (), "Failed to find \":\" in header");
        };

        let (header_name, header_value) = line.split_at(colon_index);

        let header_name = header_name.to_ascii_lowercase().into();
        let header_value = header_value[1..].trim_ascii_start().to_vec().into();

        headers
            .entry(header_name)
            .or_insert(Vec::with_capacity(5))
            .push(header_value);
    }

    let body = match get_body_reader(reader, &headers) {
        Ok(b) => b,
        Err(e) => {
            set_err!(res, (), "Failed to create body: {e}");
        }
    };

    let (path, host) = if let Some(host) = headers.get(b"host".as_slice()) {
        (uri, host[0].to_string())
    } else {
        let Some(uri) = uri.strip_prefix("https://") else {
            set_err!(res, (), "Invalid URI scheme");
        };

        let Some((host, path)) = uri.split_once('/') else {
            set_err!(res, (), "Invalid URI path");
        };

        (format!("/{}", path), host.to_string())
    };

    let req = unsafe { Request::new(method, path, host, (1, 1), headers, reader_from_read(body)) };

    // The result ok must point to a pointer to a request,
    // from safety requirements
    unsafe { set_ok(res, req, function!()) }
}

/// SAFETY:
/// 1) Writer must be convertible to a reference
/// 2) Result must be convertible to a reference
/// 3) Result must be a result to a usize
extern "C" fn http11_write_status(writer: *mut ResponseWriter, code: u16, result: *mut HttpResult) {
    assert!(
        writer.is_sane(),
        "{}: Writer is not convertible to a reference",
        function!()
    );
    assert!(
        result.is_sane(),
        "{}: Result is not convertible to a reference",
        function!()
    );

    let writer = unsafe { writer.as_mut_unchecked() };
    let res = unsafe { result.as_mut_unchecked() };

    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    let inner = unsafe { writer.writer.as_mut_unchecked() };

    // SAFETY: get_reason_phrase returns a valid pointer
    let reason_phrase = unsafe { CStr::from_ptr(get_reason_phrase(code)) };

    // SAFETY: get_reason_phrase returns only ASCII text
    let reason_phrase = unsafe { str::from_utf8_unchecked(reason_phrase.to_bytes()) };

    let mut buf_writer = BufWriter::new(inner);
    let mut written = 0;

    match write!(buf_writer, "HTTP/1.1 {} {}\r\n", code, reason_phrase) {
        Ok(_) => written += 8 + 1 + 3 + 1 + reason_phrase.len(),
        Err(e) => set_err!(res, (), "{e}"),
    }

    for (k, v) in &writer.headers {
        for val in v {
            match write!(buf_writer, "{k}: {val}\r\n") {
                Ok(_) => written += k.len() + 2 + val.len() + 2,
                Err(e) => set_err!(res, (), "{e}"),
            };
        }
    }

    match buf_writer.write_all(b"\r\n") {
        Ok(_) => written += 2,
        Err(e) => set_err!(res, (), "{e}"),
    };

    unsafe {
        set_ok(res, written, function!());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn http_http11_response_writer(writer: *mut Writer) -> *mut ResponseWriter {
    assert!(writer.is_sane(), "Writer is not convertible to a reference");

    unsafe { http_new_response_writer(writer, http11_write_status) }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use bstr::{BStr, ByteSlice};

    use crate::{
        Request,
        bio::reader_from_read,
        buffer::{http_clear_buffer, http_new_buffer},
        http11::http_parse_http11_request,
        request::*,
        result::*,
    };

    #[test]
    fn test_parse() {
        let data = "GET / HTTP/1.1
Host: example.com
Content-Length: 12

Hello World!";

        let reader = reader_from_read(data.as_bytes());

        let mut request = ptr::null_mut::<Request>();
        let mut res = HttpResult {
            is_ok: true,
            ok: (&raw mut request).cast(),
            err: http_new_buffer(32),
        };

        unsafe { http_parse_http11_request(reader, &mut res) };

        let mut buf = http_new_buffer(512);

        assert!(res.is_ok, "{}", BStr::new(&res.err));
        assert_eq!(res.err.len, 0);

        unsafe { http_get_method(request, &mut buf) };
        assert_eq!(&*buf, b"GET");

        unsafe { http_get_uri(request, &mut buf) };
        assert_eq!(&*buf, b"https://example.com/");

        unsafe { http_get_header(request, "content-length".into(), 0, &mut buf) };
        assert_eq!(&*buf, b"12");

        unsafe {
            http_clear_buffer(&mut buf);
        };

        let mut read = 0usize;
        let mut buf = vec![0u8; 12];

        res.ok = (&raw mut read).cast();
        res.err.clear();

        unsafe { http_read_body(request, buf.as_mut_ptr(), buf.len(), &mut res) };

        assert!(res.is_ok);
        assert_eq!(res.err.len, 0);
        unsafe { http_clear_buffer(&mut res.err) };

        assert_eq!(read, 12);

        assert_eq!(buf, b"Hello World!");
    }

    #[test]
    fn test_real_world() {
        // I did remove the cookies from this request because
        // I don't want to leak any information... just in case...
        // The request was also changed to be HTTP/1.1 instead of HTTP/2
        // so I also had to add the content-length header

        let req = "GET /rfc/rfc7541.html HTTP/1.1
Host: www.rfc-editor.org
User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:149.0) Gecko/20100101 Firefox/149.0
Accept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8
Accept-Language: en-US,en;q=0.9
Accept-Encoding: gzip, deflate, br, zstd
Sec-GPC: 1
Connection: keep-alive
Upgrade-Insecure-Requests: 1
Sec-Fetch-Dest: document
Sec-Fetch-Mode: navigate
Sec-Fetch-Site: none
Priority: u=0, i
Pragma: no-cache
Cache-Control: no-cache
Content-Length: 0";

        let headers = [
            ("host", "www.rfc-editor.org"),
            (
                "user-agent",
                "Mozilla/5.0 (X11; Linux x86_64; rv:149.0) Gecko/20100101 Firefox/149.0",
            ),
            (
                "accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
            ("accept-language", "en-US,en;q=0.9"),
            ("accept-encoding", "gzip, deflate, br, zstd"),
            ("sec-gpc", "1"),
            ("connection", "keep-alive"),
            ("upgrade-insecure-requests", "1"),
            ("sec-fetch-dest", "document"),
            ("sec-fetch-mode", "navigate"),
            ("sec-fetch-site", "none"),
            ("priority", "u=0, i"),
            ("pragma", "no-cache"),
            ("cache-control", "no-cache"),
            ("content-length", "0"),
        ];

        let reader = reader_from_read(req.as_bytes());
        let mut req = ptr::null_mut::<Request>();
        let mut res = HttpResult {
            is_ok: true,
            ok: (&raw mut req).cast(),
            err: http_new_buffer(32),
        };
        let mut buf = http_new_buffer(512);

        unsafe { http_parse_http11_request(reader, &mut res) };

        assert!(res.is_ok, "{}", BStr::new(&res.err));
        assert!(res.err.len == 0);
        assert!(!req.is_null());

        unsafe { http_get_method(req, &mut buf) };
        assert_eq!(&*buf, b"GET");

        unsafe { http_get_uri(req, &mut buf) };
        assert_eq!(&*buf, b"https://www.rfc-editor.org/rfc/rfc7541.html");

        for (k, v) in headers {
            unsafe { http_get_header(req, k.into(), 0, &mut buf) };
            assert_eq!(&*buf, v.as_bytes());
        }

        unsafe { http_clear_buffer(&mut buf) };

        let mut buf = vec![0u8; 4096];
        let mut read = 0usize;

        res.ok = (&raw mut read).cast();
        res.is_ok = true;

        unsafe { http_read_body(req, buf.as_mut_ptr(), buf.len(), &mut res) };

        assert!(res.is_ok, "{}", BStr::new(&res.err));
        assert_eq!(read, 0);
    }

    #[test]
    fn test_chunked() {
        let data = "GET / HTTP/1.1
Host: example.com
Transfer-Encoding: chunked

d
Hello World!\n
8
how are 
11
you doing today?\n
1D
I'm doing great. How are you?
0";

        let reader = reader_from_read(data.as_bytes());
        let mut request = ptr::null_mut::<Request>();

        let mut res = HttpResult {
            is_ok: true,
            ok: (&raw mut request).cast(),
            err: http_new_buffer(32),
        };
        let mut buf = http_new_buffer(512);

        unsafe { http_parse_http11_request(reader, &mut res) };

        assert!(res.is_ok, "{}", BStr::new(&res.err));
        assert_eq!(res.err.len, 0);
        assert!(!request.is_null());

        unsafe { http_get_method(request, &mut buf) };
        assert_eq!(&*buf, b"GET");

        unsafe { http_get_uri(request, &mut buf) };
        assert_eq!(&*buf, b"https://example.com/");

        unsafe { http_get_header(request, "transfer-encoding".into(), 0, &mut buf) };
        assert_eq!(&*buf, b"chunked");

        unsafe {
            http_clear_buffer(&mut buf);
        }

        let mut buf = vec![0u8; 4096];
        let mut offset = 0;
        let mut read = 0usize;

        res.is_ok = true;
        res.ok = (&raw mut read).cast();

        loop {
            let b = &mut buf[offset..];
            unsafe { http_read_body(request, b.as_mut_ptr(), b.len(), &mut res) };

            assert!(res.is_ok, "{}", BStr::new(&res.err));

            if read == 0 {
                break;
            }

            offset += read;
        }

        assert_eq!(
            &buf[0..offset],
            b"Hello World!
how are you doing today?
I'm doing great. How are you?"
        );

        unsafe { http_clear_buffer(&mut res.err) };
    }

    #[test]
    fn test_parse_errors() {
        let cases = [
            ("", "No method found"),
            ("GET", "No URI found"),
            ("GET /", "No version found"),
            ("GET / 1.1", "No http prefix"),
            ("GET / HTTP/11", "Version not split with \".\""),
            (
                "GET / HTTP/1.1
Host example.com",
                "Failed to find \":\" in header",
            ),
            ("GET / HTTP/1.0", "HTTP not version 1.1"),
            ("GET htps://example.com/ HTTP/1.1", "Invalid URI scheme"),
            ("GET https://example.com HTTP/1.1", "Invalid URI path"),
        ];

        let mut res = HttpResult {
            is_ok: true,
            ok: ptr::null_mut(),
            err: http_new_buffer(32),
        };

        for (data, error) in cases {
            let reader = reader_from_read(data.as_bytes());
            let mut req = ptr::null_mut::<Request>();
            res.ok = (&raw mut req).cast();

            unsafe { http_parse_http11_request(reader, &mut res) };

            assert!(!res.is_ok);
            assert!(res.err.len != 0);
            assert!(req.is_null());

            assert!(
                BStr::new(&res.err).starts_with_str(error),
                "{} vs {}",
                BStr::new(&res.err),
                error
            );
        }

        unsafe {
            http_clear_buffer(&mut res.err);
        }
    }
}
