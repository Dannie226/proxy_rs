use std::{
    collections::HashMap,
    fmt,
    io::{self, BufRead, BufReader, Cursor, Read, Write},
    num::NonZeroUsize,
};

use anyhow::{Context, anyhow};

use crate::{
    http::{
        self,
        request::{HeaderMap, Request},
        response::StatusCode,
    },
    tls::listener::TlsStream,
};

struct Body<'a> {
    reader: BufReader<&'a TlsStream>,
    content_length: Option<NonZeroUsize>,
    chunked_encoding: bool,
}

impl<'a> Read for Body<'a> {
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

fn get_body_reader<'a>(
    mut reader: BufReader<&'a TlsStream>,
    headers: &HeaderMap,
) -> anyhow::Result<Body<'a>> {
    if headers
        .get("transfer-encoding")
        .filter(|v| v.iter().any(|s| s == "chunked"))
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
        .get("content-length")
        .map(|v| &*v[0])
        .unwrap_or("0")
        .parse()
        .context("Failed to parse content length")?;

    Ok(Body {
        reader,
        content_length: NonZeroUsize::new(len),
        chunked_encoding: false,
    })
}

pub fn parse_request(stream: &TlsStream) -> anyhow::Result<Request<'_>> {
    let mut reader = BufReader::new(stream);

    let mut line = String::new();

    reader
        .read_line(&mut line)
        .context("Failed to read status line")?;

    let mut spl = line.trim_end().splitn(3, " ");

    let method = spl
        .next()
        .ok_or_else(|| anyhow!("No method found"))?
        .to_string();

    let path = spl
        .next()
        .ok_or_else(|| anyhow!("No path found"))?
        .to_string();

    let version_string = spl.next().ok_or_else(|| anyhow!("No version found"))?;

    let (major, minor) = version_string
        .strip_prefix("HTTP/")
        .ok_or_else(|| anyhow!("No http prefix"))?
        .split_once(".")
        .ok_or_else(|| anyhow!("Version not split with \".\""))?;

    let major = major.parse().context("Failed to parse major version")?;
    let minor = minor.parse().context("Failed to parse minor version")?;

    let mut headers = HashMap::new();

    loop {
        line.clear();

        reader
            .read_line(&mut line)
            .context("Failed to read header line")?;

        let line = line
            .strip_suffix("\n")
            .unwrap_or(&*line)
            .strip_suffix("\r")
            .unwrap_or(&*line);

        if line.is_empty() {
            break;
        }

        let (header_name, header_value) = line
            .split_once(":")
            .ok_or_else(|| anyhow!("Failed to find header name end delimiter"))?;

        let header_name = header_name.to_lowercase();
        let header_value = header_value.trim_start().to_string();

        headers
            .entry(header_name)
            .or_insert(Vec::with_capacity(5))
            .push(header_value);
    }

    println!("{:?}", headers);

    let body = get_body_reader(reader, &headers)?;

    Ok(Request {
        method: method,
        uri: path,
        version: (major, minor),
        headers,
        body: Box::new(body),
    })
}

pub struct ResponseWriter<'a> {
    stream: &'a TlsStream,
    headers: HeaderMap,
    written: bool,
}

impl<'a> ResponseWriter<'a> {
    pub fn new(stream: &'a TlsStream) -> ResponseWriter<'a> {
        ResponseWriter {
            stream,
            headers: HeaderMap::new(),
            written: false,
        }
    }
}

impl<'a> Write for ResponseWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !self.written {
            http::response::ResponseWriter::write_status(self, StatusCode::OK)?;
        }

        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

impl<'a> http::response::ResponseWriter for ResponseWriter<'a> {
    fn get_headers(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }

    fn write_status(&mut self, status: StatusCode) -> io::Result<()> {
        if self.written {
            return Ok(());
        }

        write!(
            self.stream,
            "HTTP/1.1 {} {}\r\n",
            status as u16,
            status.get_reason_phrase()
        )?;

        for (k, v) in &self.headers {
            for val in v {
                self.stream.write_all(k)?;
                self.stream.write_all(b": ")?;
                self.stream.write_all(val)?;
                self.stream.write_all(b"\r\n")?;
            }
        }

        self.stream.write_all(b"\r\n")?;

        self.written = true;

        Ok(())
    }
}
