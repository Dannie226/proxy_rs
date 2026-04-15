use std::{
    borrow::Cow,
    io::{self, Read, Write},
};

use crate::{
    ffi::rand::fill_bytes,
    http::{
        http2::hpack::{self, huffman, literal, tables::STATIC_TABLE},
        request::HeaderMap,
    },
};

fn read_u8(reader: &mut impl Read) -> io::Result<u8> {
    let mut v = [0u8; 1];

    reader.read_exact(&mut v)?;

    Ok(u8::from_be_bytes(v))
}

fn read_u16(reader: &mut impl Read) -> io::Result<u16> {
    let mut v = [0u8; 2];

    reader.read_exact(&mut v)?;

    Ok(u16::from_be_bytes(v))
}

fn read_u24(reader: &mut impl Read) -> io::Result<u32> {
    let mut v = [0u8; 4];

    reader.read_exact(&mut v[1..])?;

    Ok(u32::from_be_bytes(v))
}

fn read_u32(reader: &mut impl Read) -> io::Result<u32> {
    let mut v = [0u8; 4];

    reader.read_exact(&mut v)?;

    Ok(u32::from_be_bytes(v))
}

fn read_u64(reader: &mut impl Read) -> io::Result<u64> {
    let mut v = [0u8; 8];

    reader.read_exact(&mut v)?;

    Ok(u64::from_be_bytes(v))
}

fn write_u8(writer: &mut impl Write, value: u8) -> io::Result<()> {
    let v = value.to_be_bytes();

    writer.write_all(&v)
}

fn write_u16(writer: &mut impl Write, value: u16) -> io::Result<()> {
    let v = value.to_be_bytes();

    writer.write_all(&v)
}

fn write_u24(writer: &mut impl Write, value: u32) -> io::Result<()> {
    let v = (value & 0x00FFFFFF).to_be_bytes();

    writer.write_all(&v[1..])
}

fn write_u32(writer: &mut impl Write, value: u32) -> io::Result<()> {
    let v = value.to_be_bytes();

    writer.write_all(&v)
}

fn write_u64(writer: &mut impl Write, value: u64) -> io::Result<()> {
    let v = value.to_be_bytes();

    writer.write_all(&v)
}

trait FrameParse: Sized {
    fn read_frame(flags: u8, len: u32, reader: &mut impl Read) -> io::Result<Self>;
}

trait FrameWrite {
    const TYPE_NUM: u8;
    fn write_frame(&self, stream_id: u32, output: &mut impl Write) -> io::Result<()>;
}

fn write_header<T: FrameWrite>(
    stream_id: u32,
    data_len: u32,
    flags: u8,
    output: &mut impl Write,
) -> io::Result<()> {
    const fn mask(value: u32, byte: u8) -> u8 {
        let m = 0xFFu32 << ((byte * 0x8) as u32);

        ((value & m) >> ((byte * 0x8) as u32)) as u8
    }

    write_u24(output, data_len)?;
    write_u8(output, T::TYPE_NUM)?;
    write_u8(output, flags)?;
    write_u32(output, stream_id)?;

    Ok(())
}

#[derive(Debug)]
pub struct DataFrame {
    pub data: Vec<u8>,
    pub last: bool,
}

impl DataFrame {
    const END_FLAG: u8 = 0x1;
    const PAD_FLAG: u8 = 0x8;
}

impl FrameParse for DataFrame {
    fn read_frame(flags: u8, len: u32, frame_data: &mut impl Read) -> io::Result<Self> {
        if len == 0 {
            return Ok(DataFrame {
                data: Vec::new(),
                last: flags & Self::END_FLAG == Self::END_FLAG,
            });
        }

        println!("{flags:b}");
        let pad_bytes = if flags & Self::PAD_FLAG == Self::PAD_FLAG {
            read_u8(frame_data)?
        } else {
            0
        } as u32;

        println!("Data Len: {len}, padding bytes: {pad_bytes}");

        let data_len = len - pad_bytes;

        // Safety: I am creating the vec with the capacity, and then setting it's length
        // These are also raw bytes, so nothing wrong there
        let mut data = unsafe {
            let mut v = Vec::with_capacity(data_len as usize);
            v.set_len(data_len as usize);
            v
        };

        frame_data.read_exact(&mut data)?;

        Ok(DataFrame {
            data,
            last: flags & Self::END_FLAG == Self::END_FLAG,
        })
    }
}

static PAD_BUF: [u8; 256] = [0u8; 256];

impl FrameWrite for DataFrame {
    const TYPE_NUM: u8 = 0x0;

    fn write_frame(&self, stream_id: u32, output: &mut impl Write) -> io::Result<()> {
        let mut pad_len = [0u8; 1];
        fill_bytes(&mut pad_len).map_err(io::Error::other)?;

        let pad_size = pad_len[0] as usize;
        let frame_len = self.data.len() as u32 + pad_size as u32;
        let flags = {
            let mut f = 0;

            if pad_size != 0 {
                f |= Self::PAD_FLAG;
            }

            if self.last {
                f |= Self::END_FLAG;
            }

            f
        };

        write_header::<Self>(stream_id, frame_len, flags, output)?;

        output.write_all(&pad_len)?;
        output.write_all(&self.data)?;

        if pad_size != 0 {
            output.write_all(&PAD_BUF[0..pad_size])?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct HeaderFrame(pub HeaderMap);

impl FrameParse for HeaderFrame {
    fn read_frame(flags: u8, len: u32, frame_data: &mut impl Read) -> io::Result<Self> {
        fn insert_header<'a>(headers: &mut HeaderMap, name: Cow<'a, [u8]>, value: Vec<u8>) {
            match headers.get_mut(&*name) {
                Some(v) => {
                    v.push(value);
                }
                None => {
                    let values = vec![value];

                    headers.insert(name.into_owned(), values);
                }
            }
        }

        let mut v = vec![0u8; len as usize];

        frame_data.read_exact(&mut v)?;

        let mut offset = 4;
        let mut headers = HeaderMap::new();

        while offset < v.len() {
            let first_byte = v[offset];
            if first_byte & 0x80 == 0x80 {
                let (index, count) = literal::parse_integer(7, &v[offset..])?;
                offset += count;

                if index > 0 && index <= 61 {
                    let index = (index - 1) as usize;
                    let name = STATIC_TABLE[index].name;
                    let Some(value) = STATIC_TABLE[index].value else {
                        continue;
                    };

                    insert_header(&mut headers, Cow::Borrowed(name), value.to_vec());
                }
            } else if first_byte & 0x40 == 0x40 {
                let (index, count) = literal::parse_integer(6, &v[offset..])?;
                offset += count;

                let n: Cow<'static, _> = if index == 0 {
                    let (name, count) = literal::parse_string(&v[offset..])?;
                    offset += count;

                    Cow::Owned(name)
                } else if index <= 61 {
                    Cow::Borrowed(STATIC_TABLE[index as usize - 1].name)
                } else {
                    continue;
                };

                let (value, count) = literal::parse_string(&v[offset..])?;
                offset += count;

                insert_header(&mut headers, n, value);
            } else if first_byte & 0x20 == 0x20 {
                let (new_size, count) = literal::parse_integer(5, &v[offset..])?;
                offset += count;
            } else if first_byte & 0x10 == 0x10 {
                let (index, count) = literal::parse_integer(4, &v[offset..])?;
                offset += count;

                if index == 0 {
                    let (name, count) = literal::parse_string(&v[offset..])?;
                    offset += count;
                }

                let (value, count) = literal::parse_string(&v[offset..])?;
                offset += count;
            } else {
                let (index, count) = literal::parse_integer(4, &v[offset..])?;
                offset += count;

                if index == 0 {
                    let (name, count) = literal::parse_string(&v[offset..])?;
                    offset += count;
                }

                let (value, count) = literal::parse_string(&v[offset..])?;
                offset += count;
            }
        }

        println!("{headers:?}");

        Ok(HeaderFrame(headers))
    }
}

impl FrameWrite for HeaderFrame {
    const TYPE_NUM: u8 = 0x1;

    fn write_frame(&self, stream_id: u32, output: &mut impl Write) -> io::Result<()> {
        Err(io::Error::other("Not Implemented"))
    }
}

#[derive(Debug)]
pub struct PriorityFrame {
    exclusive: bool,
    dependency: u32,
    weight: u8,
}

impl FrameParse for PriorityFrame {
    fn read_frame(flags: u8, len: u32, frame_data: &mut impl Read) -> io::Result<Self> {
        let id_ex = read_u32(frame_data)?;
        let weight = read_u8(frame_data)?;
        let id = id_ex & 0x7FFFFFFF;
        let exclusive = id_ex & 0x80000000 == 0x80000000;

        Ok(PriorityFrame {
            exclusive,
            dependency: id,
            weight,
        })
    }
}

impl FrameWrite for PriorityFrame {
    const TYPE_NUM: u8 = 0x2;
    fn write_frame(&self, stream_id: u32, output: &mut impl Write) -> io::Result<()> {
        write_header::<Self>(stream_id, 5, 0, output)?;

        let ex = if self.exclusive { 0x80000000u32 } else { 0x0 };
        let id = self.dependency & 0x7FFFFFFF;

        let id_ex = id | ex;

        write_u32(output, id_ex)?;
        write_u8(output, self.weight)?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct RstStreamFrame {
    error_code: u32,
}

impl FrameParse for RstStreamFrame {
    fn read_frame(flags: u8, len: u32, reader: &mut impl Read) -> io::Result<Self> {
        let error_code = read_u32(reader)?;

        Ok(RstStreamFrame { error_code })
    }
}

impl FrameWrite for RstStreamFrame {
    const TYPE_NUM: u8 = 0x3;

    fn write_frame(&self, stream_id: u32, output: &mut impl Write) -> io::Result<()> {
        write_header::<Self>(stream_id, 4, 0, output);

        write_u32(output, self.error_code)
    }
}

#[derive(Debug)]
pub struct HTTP2Settings {
    pub header_table_size: u32,
    pub enable_push: bool,
    pub max_concurrent_streams: u32,
    pub initial_window_size: u32,
    pub max_frame_size: u32,
    pub max_header_list_size: u32,
}

impl Default for HTTP2Settings {
    fn default() -> Self {
        HTTP2Settings {
            header_table_size: 4096,
            enable_push: true,
            max_concurrent_streams: u32::MAX,
            initial_window_size: u16::MAX as u32,
            max_frame_size: 1 << 14,
            max_header_list_size: u32::MAX,
        }
    }
}

#[derive(Debug)]
pub enum SettingsFrame {
    Ack,
    Settings(HTTP2Settings),
}

impl SettingsFrame {
    const ACK_MASK: u8 = 0x1;
}

impl FrameParse for SettingsFrame {
    fn read_frame(flags: u8, len: u32, reader: &mut impl Read) -> io::Result<Self> {
        if flags & Self::ACK_MASK == Self::ACK_MASK {
            if len == 0 {
                return Ok(SettingsFrame::Ack);
            } else {
                return Err(io::Error::other("Len must be 0 for ack"));
            }
        }

        let mut settings = HTTP2Settings::default();

        let mut r = 0;

        while r < len {
            let setting = read_u16(reader)?;
            let value = read_u32(reader)?;

            match setting {
                0x1 => settings.header_table_size = value,
                0x2 if value == 0 || value == 1 => settings.enable_push = value == 1,
                0x2 => return Err(io::Error::other("Enable value must be 0 or 1")),
                0x3 => settings.max_concurrent_streams = value,
                0x4 => settings.initial_window_size = value,
                0x5 => settings.max_frame_size = value,
                0x6 => settings.max_header_list_size = value,
                _ => {}
            }

            r += 6;
        }

        Ok(SettingsFrame::Settings(settings))
    }
}

impl FrameWrite for SettingsFrame {
    const TYPE_NUM: u8 = 0x4;

    fn write_frame(&self, stream_id: u32, output: &mut impl Write) -> io::Result<()> {
        let data = match self {
            SettingsFrame::Ack => {
                write_header::<Self>(stream_id, 0, Self::ACK_MASK, output);
                return Ok(());
            }
            SettingsFrame::Settings(s) => s,
        };

        let defaults = HTTP2Settings::default();

        let mut buffer = [0u8; 36];
        let mut len = 0;

        if defaults.header_table_size != data.header_table_size {
            buffer[len..len + 2].copy_from_slice(&(0x1u16.to_be_bytes()));
            buffer[len + 2..len + 6].copy_from_slice(&(data.header_table_size.to_be_bytes()));
            len += 6;
        }

        if defaults.enable_push != data.enable_push {
            buffer[len..len + 2].copy_from_slice(&(0x2u16.to_be_bytes()));
            buffer[len + 2..len + 6].copy_from_slice(&((data.enable_push as u32).to_be_bytes()));
            len += 6;
        }

        if defaults.max_concurrent_streams != data.max_concurrent_streams {
            buffer[len..len + 2].copy_from_slice(&(0x3u16.to_be_bytes()));
            buffer[len + 2..len + 6].copy_from_slice(&(data.max_concurrent_streams.to_be_bytes()));
            len += 6;
        }

        if defaults.initial_window_size != data.initial_window_size {
            buffer[len..len + 2].copy_from_slice(&(0x4u16.to_be_bytes()));
            buffer[len + 2..len + 6].copy_from_slice(&(data.initial_window_size.to_be_bytes()));
            len += 6;
        }

        if defaults.max_frame_size != data.max_frame_size {
            buffer[len..len + 2].copy_from_slice(&(0x5u16.to_be_bytes()));
            buffer[len + 2..len + 6].copy_from_slice(&(data.max_frame_size.to_be_bytes()));
            len += 6;
        }

        if defaults.max_header_list_size != data.max_header_list_size {
            buffer[len..len + 2].copy_from_slice(&(0x6u16.to_be_bytes()));
            buffer[len + 2..len + 6].copy_from_slice(&(data.max_header_list_size.to_be_bytes()));
            len += 6;
        }

        write_header::<Self>(stream_id, len as u32, 0x0, output)?;
        output.write_all(&buffer[0..len])
    }
}

#[derive(Debug)]
pub struct PingFrame {
    pub is_ack: bool,
    pub payload: u64,
}

impl PingFrame {
    const ACK_MASK: u8 = 0x1;
}

impl FrameParse for PingFrame {
    fn read_frame(flags: u8, len: u32, reader: &mut impl Read) -> io::Result<Self> {
        let is_ack = flags & Self::ACK_MASK == Self::ACK_MASK;
        let payload = read_u64(reader)?;

        Ok(PingFrame { is_ack, payload })
    }
}

impl FrameWrite for PingFrame {
    const TYPE_NUM: u8 = 0x6;

    fn write_frame(&self, stream_id: u32, output: &mut impl Write) -> io::Result<()> {
        write_header::<Self>(stream_id, 8, if self.is_ack { 0x1 } else { 0x0 }, output)?;

        write_u64(output, self.payload)
    }
}

#[derive(Debug)]
pub struct GoAwayFrame {
    pub last_stream_id: u32,
    pub error_code: u32,
    pub additional_data: Vec<u8>,
}

impl FrameParse for GoAwayFrame {
    fn read_frame(flags: u8, len: u32, reader: &mut impl Read) -> io::Result<Self> {
        let stream_id = read_u32(reader)?;
        let error_code = read_u32(reader)?;

        let data_len = (len - 8) as usize;

        let mut data = unsafe {
            let mut v = Vec::with_capacity(data_len);
            v.set_len(data_len);

            v
        };

        reader.read_exact(&mut data)?;

        Ok(GoAwayFrame {
            last_stream_id: stream_id,
            error_code,
            additional_data: data,
        })
    }
}

impl FrameWrite for GoAwayFrame {
    const TYPE_NUM: u8 = 0x7;

    fn write_frame(&self, stream_id: u32, output: &mut impl Write) -> io::Result<()> {
        write_header::<Self>(
            stream_id,
            (self.additional_data.len() as u32) + 8,
            0x0,
            output,
        )?;

        write_u32(output, self.last_stream_id & 0x7FFFFFFF)?;
        write_u32(output, self.error_code)?;

        output.write_all(&self.additional_data)
    }
}

#[derive(Debug)]
pub struct WindowUpdateFrame {
    pub stream_id: u32,
}

impl FrameParse for WindowUpdateFrame {
    fn read_frame(flags: u8, len: u32, reader: &mut impl Read) -> io::Result<Self> {
        let stream_id = read_u32(reader)?;

        Ok(WindowUpdateFrame {
            stream_id: stream_id,
        })
    }
}

impl FrameWrite for WindowUpdateFrame {
    const TYPE_NUM: u8 = 0x8;

    fn write_frame(&self, stream_id: u32, output: &mut impl Write) -> io::Result<()> {
        write_header::<Self>(stream_id, 4, 0x0, output)?;

        write_u32(output, self.stream_id & 0x7FFFFFFF)
    }
}

#[repr(u8)]
#[derive(Debug)]
pub enum FrameType {
    Data(DataFrame),
    Header(HeaderFrame),
    Priority(PriorityFrame),
    RstStream(RstStreamFrame),
    Settings(SettingsFrame),
    Ping(PingFrame),
    GoAway(GoAwayFrame),
    WindowUpdate(WindowUpdateFrame),
}

#[derive(Debug)]
pub struct Frame {
    pub stream_id: u32,
    pub data: FrameType,
}

impl Frame {
    pub fn parse_frame(reader: &mut impl Read) -> io::Result<Frame> {
        let frame_len = read_u24(reader)?;
        let frame_type = read_u8(reader)?;
        let flags = read_u8(reader)?;
        let id = read_u32(reader)? & 0x7FFFFFFF;

        println!(
            "Flags: {flags:b}, Frame Len: {frame_len}, Frame Type: {frame_type}, Stream id: {id}"
        );

        let frame_data = match frame_type {
            0x0 => FrameType::Data(FrameParse::read_frame(flags, frame_len, reader)?),
            0x1 => FrameType::Header(FrameParse::read_frame(flags, frame_len, reader)?),
            0x2 => {
                if frame_len != 5 {
                    return Err(io::Error::other("Priority frame len must be 5 octets"));
                }

                FrameType::Priority(FrameParse::read_frame(flags, frame_len, reader)?)
            }
            0x3 => {
                if frame_len != 4 {
                    return Err(io::Error::other("RST Stream frame len must be 4 octets"));
                }

                FrameType::RstStream(FrameParse::read_frame(flags, frame_len, reader)?)
            }
            0x4 => FrameType::Settings(FrameParse::read_frame(flags, frame_len, reader)?),
            0x6 => {
                if frame_len != 8 {
                    return Err(io::Error::other("Ping frame len must be 8 octets"));
                }

                FrameType::Ping(FrameParse::read_frame(flags, frame_len, reader)?)
            }
            0x7 => FrameType::GoAway(FrameParse::read_frame(flags, frame_len, reader)?),
            0x8 => {
                if frame_len != 4 {
                    return Err(io::Error::other("Window update frame len must be 4 octets"));
                }

                FrameType::WindowUpdate(FrameParse::read_frame(flags, frame_len, reader)?)
            }
            t => return Err(io::Error::other(format!("Unsupported frame type {t}"))),
        };

        Ok(Frame {
            stream_id: id,
            data: frame_data,
        })
    }

    pub fn write_frame(&self, writer: &mut impl Write) -> io::Result<()> {
        match &self.data {
            FrameType::Data(d) => d.write_frame(self.stream_id, writer),
            FrameType::Header(h) => h.write_frame(self.stream_id, writer),
            FrameType::Priority(p) => p.write_frame(self.stream_id, writer),
            FrameType::RstStream(r) => r.write_frame(self.stream_id, writer),
            FrameType::Settings(s) => s.write_frame(self.stream_id, writer),
            FrameType::Ping(p) => p.write_frame(self.stream_id, writer),
            FrameType::GoAway(g) => g.write_frame(self.stream_id, writer),
            FrameType::WindowUpdate(w) => w.write_frame(self.stream_id, writer),
        }
    }
}
