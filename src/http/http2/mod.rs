use std::io::{self, Read, Write};

use anyhow::{Context, ensure};
use bstr::BString;

use crate::{
    http::{
        http2::{
            frame::{
                FrameHeader, data, go_away, header, ping, priority, rst_stream,
                settings::{self, HTTP2Settings, Settings},
                window_update,
            },
            hpack::tables::HeaderTable,
        },
        request::{HeaderMap, Request},
        response::{self, StatusCode},
    },
    tls::listener::TlsStream,
};

pub mod frame;
pub mod hpack;

static PREFACE: [u8; 24] = [
    0x50, 0x52, 0x49, 0x20, 0x2a, 0x20, 0x48, 0x54, 0x54, 0x50, 0x2f, 0x32, 0x2e, 0x30, 0x0d, 0x0a,
    0x0d, 0x0a, 0x53, 0x4d, 0x0d, 0x0a, 0x0d, 0x0a,
];

pub fn parse_request(
    mut stream: &TlsStream,
    decode_table: &mut HeaderTable,
) -> anyhow::Result<(u32, Request<'static>)> {
    let mut preface = [0u8; 24];

    stream.read_exact(&mut preface)?;

    println!("{}", preface == PREFACE);

    let settings_header =
        FrameHeader::read_header(&mut stream).context("Failed to parse settings header")?;

    println!("Settings header: {settings_header:?}");
    ensure!(
        settings_header.frame_type == settings::TYPE_NUM,
        "Settings frame was not a frame"
    );

    let settings = settings::read_frame(settings_header, &mut stream);

    println!("Settings: {settings:?}");

    settings::write_frame(Settings::Ack, &mut stream).context("Failed to write ack frame")?;
    settings::write_frame(Settings::Settings(HTTP2Settings::default()), &mut stream)
        .context("Failed to write settings frame")?;

    let mut last_id = 0;
    for _ in 0..4 {
        let header = FrameHeader::read_header(&mut stream).context("Failed to read next header")?;
        println!("Got Header: {header:?}");

        match header.frame_type {
            data::TYPE_NUM => {
                let mut data = vec![0u8; header.frame_len as usize];
                stream
                    .read_exact(&mut data)
                    .context("Failed to read data frame")?;

                println!("Got data frame: {}", BString::from(data));
            }
            header::TYPE_NUM => {
                let headers = header::read_frame(header, decode_table, &mut stream)
                    .context("Failed to read headers")?;
                println!("Got Header Frame: {headers:#?}");
            }
            priority::TYPE_NUM => {
                let prio = priority::read_frame(header, &mut stream)
                    .context("Failed to read priority frame")?;
                println!("Got Priority Frame: {prio:?}");
            }
            rst_stream::TYPE_NUM => {
                let err = rst_stream::read_frame(header, &mut stream)
                    .context("Failed to read reset stream frame")?;

                println!("Got reset stream: {err}");
            }
            settings::TYPE_NUM => {
                let settings = settings::read_frame(header, &mut stream)
                    .context("Failed to read settings frame")?;

                println!("Got settings frame: {settings:?}");
            }
            ping::TYPE_NUM => {
                let (payload, ack) =
                    ping::read_frame(header, &mut stream).context("Failed to read ping data")?;

                println!("Got ping data: {:X}", payload);

                if !ack {
                    ping::write_frame(payload, true, &mut stream)?;
                }
            }
            go_away::TYPE_NUM => {
                panic!("Got a go away frame");
            }
            window_update::TYPE_NUM => {
                let increment = window_update::read_frame(header, &mut stream)
                    .context("Failed to read window frame")?;

                println!("Got window increment frame: {increment}");
            }
            t => {
                println!("Unknown frame type: {t:X}")
            }
        }
    }

    return Ok((
        last_id,
        Request {
            method: "hi".into(),
            uri: "/".into(),
            version: (2, 0),
            headers: HeaderMap::new(),
            body: Box::new([].as_slice()),
        },
    ));
}

pub struct ResponseWriter<'a> {
    written: bool,
    stream: &'a TlsStream,
    headers: HeaderMap,
    stream_id: u32,
}

impl<'a> ResponseWriter<'a> {
    pub fn new(stream: &'a TlsStream, stream_id: u32) -> ResponseWriter<'a> {
        ResponseWriter {
            headers: HeaderMap::new(),
            written: false,
            stream: stream,
            stream_id,
        }
    }
}

impl<'a> Write for ResponseWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if !self.written {
            response::ResponseWriter::write_status(self, StatusCode::OK)?;
        }

        data::write_frame(self.stream_id, buf, true, &mut self.stream)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

impl<'a> response::ResponseWriter for ResponseWriter<'a> {
    fn get_headers(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }

    fn write_status(&mut self, status: response::StatusCode) -> io::Result<()> {
        if self.written {
            return Ok(());
        }

        header::write_ok(self.stream_id, &mut self.stream)?;

        self.written = true;
        Ok(())
    }
}
