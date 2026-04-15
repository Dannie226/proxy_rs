use std::{
    collections::HashMap,
    io::{self, Read, Write},
};

use anyhow::{Context, bail};

use crate::{
    http::{
        http2::frame::{Frame, FrameType, HTTP2Settings},
        request::{HeaderMap, Request},
        response,
    },
    tls::listener::TlsStream,
};

pub mod frame;
mod hpack;

static PREFACE: [u8; 24] = [
    0x50, 0x52, 0x49, 0x20, 0x2a, 0x20, 0x48, 0x54, 0x54, 0x50, 0x2f, 0x32, 0x2e, 0x30, 0x0d, 0x0a,
    0x0d, 0x0a, 0x53, 0x4d, 0x0d, 0x0a, 0x0d, 0x0a,
];

pub fn parse_request(mut stream: &TlsStream) -> anyhow::Result<Request<'static>> {
    let mut preface = [0u8; 24];

    stream.read_exact(&mut preface)?;

    println!("{}", preface == PREFACE);

    let frame = Frame::parse_frame(&mut stream).context("Failed to parse frame")?;

    println!("{frame:?}");

    let ack_frame = Frame {
        stream_id: frame.stream_id,
        data: frame::FrameType::Settings(frame::SettingsFrame::Ack),
    };

    ack_frame
        .write_frame(&mut stream)
        .context("Failed to write ack frame")?;

    let settings_frame = Frame {
        stream_id: frame.stream_id,
        data: FrameType::Settings(frame::SettingsFrame::Settings(HTTP2Settings::default())),
    };

    settings_frame
        .write_frame(&mut stream)
        .context("Failed to write settings frame")?;

    loop {
        let frame = Frame::parse_frame(&mut stream)?;
        println!("{frame:?}");
    }

    bail!("Not Implemented");
}

pub struct ResponseWriter {
    headers: HeaderMap,
}

impl ResponseWriter {
    pub fn new(stream: &TlsStream) -> ResponseWriter {
        ResponseWriter {
            headers: HashMap::new(),
        }
    }
}

impl Write for ResponseWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Err(io::Error::other("Unimplemented"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("Unimplemented"))
    }
}

impl response::ResponseWriter for ResponseWriter {
    fn get_headers(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }

    fn write_status(&mut self, status: response::StatusCode) -> io::Result<()> {
        Err(io::Error::other("Unimplemented"))
    }
}
