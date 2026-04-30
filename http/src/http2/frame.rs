use std::{
    borrow::Cow,
    io::{self, Read, Write},
};

use bstr::{BStr, BString};

use super::hpack::{self, *};
use crate::request::HeaderMap;

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

fn write_header(header: FrameHeader, output: &mut impl Write) -> io::Result<()> {
    write_u24(output, header.frame_len)?;
    write_u8(output, header.frame_type)?;
    write_u8(output, header.flags)?;
    write_u32(output, header.stream_id)?;

    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub struct FrameHeader {
    pub frame_len: u32,
    pub frame_type: u8,
    pub flags: u8,
    pub stream_id: u32,
}

impl FrameHeader {
    pub fn read_header(reader: &mut impl Read) -> io::Result<FrameHeader> {
        let frame_len = read_u24(reader)?;
        let frame_type = read_u8(reader)?;
        let flags = read_u8(reader)?;
        let stream_id = read_u32(reader)? & 0x7FFFFFFF;

        Ok(FrameHeader {
            frame_len,
            frame_type,
            flags,
            stream_id,
        })
    }
}

pub mod data {
    use super::*;
    use std::io::{self, Read};

    pub const END_BIT: u8 = 0x1;
    pub const PAD_BIT: u8 = 0x8;

    pub const TYPE_NUM: u8 = 0x0;

    pub fn read_frame(
        header: FrameHeader,
        frame_data: &mut impl Read,
    ) -> io::Result<(Vec<u8>, bool)> {
        assert_eq!(header.frame_type, TYPE_NUM);
        let FrameHeader {
            frame_len,
            frame_type: _,
            flags,
            stream_id: _,
        } = header;

        if frame_len == 0 {
            return Ok((Vec::new(), flags & END_BIT == END_BIT));
        }

        let pad_bytes = if flags & PAD_BIT == PAD_BIT {
            read_u8(frame_data)?
        } else {
            0
        } as u32;

        let data_len = frame_len - pad_bytes;
        let mut pad = [0u8; 256];

        // Safety: I am creating the vec with the capacity, and then setting it's length
        // These are also raw bytes, so nothing wrong there
        let mut data = unsafe {
            let mut v = Vec::with_capacity(data_len as usize);
            v.set_len(data_len as usize);
            v
        };

        frame_data.read_exact(&mut data)?;
        frame_data.read_exact(&mut pad[0..pad_bytes as usize])?;

        Ok((data, flags & END_BIT == END_BIT))
    }

    pub fn write_frame(
        stream_id: u32,
        data: &[u8],
        last: bool,
        output: &mut impl Write,
    ) -> io::Result<()> {
        write_header(
            FrameHeader {
                frame_len: data.len() as u32,
                frame_type: TYPE_NUM,
                flags: if last { END_BIT } else { 0x0 },
                stream_id,
            },
            output,
        )?;

        output.write_all(data)
    }
}

pub mod header {
    use super::hpack::tables::HeaderTable;

    use super::*;
    use std::io::{self, Read};

    pub const END_STREAM_BIT: u8 = 0x01;
    pub const END_HEAD_BIT: u8 = 0x04;
    pub const PADDED_BIT: u8 = 0x08;
    pub const PRIORITY_BIT: u8 = 0x20;

    pub const TYPE_NUM: u8 = 0x1;

    pub fn read_frame(
        header: FrameHeader,
        decode_table: &mut HeaderTable,
        frame_data: &mut impl Read,
    ) -> io::Result<HeaderMap> {
        assert_eq!(header.frame_type, TYPE_NUM);
        let FrameHeader {
            frame_len,
            flags,
            frame_type: _,
            stream_id: _,
        } = header;

        fn insert_header<'a>(headers: &mut HeaderMap, name: Cow<'a, BStr>, value: BString) {
            match headers.get_mut(&*name) {
                Some(v) => {
                    v.push(value);
                }
                None => {
                    let values = vec![value];

                    headers.insert(name.into_owned().into(), values);
                }
            }
        }

        let mut v = vec![0u8; frame_len as usize];

        frame_data.read_exact(&mut v)?;

        let mut offset = if flags & PRIORITY_BIT == PRIORITY_BIT {
            5
        } else {
            0
        };

        println!("[");
        for chunk in v[offset..].chunks(32) {
            print!("  ");
            for &v in chunk {
                print!("0x{v:02X}, ");
            }
            println!()
        }
        println!("]");
        let mut headers = HeaderMap::new();

        while offset < v.len() {
            let first_byte = v[offset];
            if first_byte & 0x80 == 0x80 {
                println!("Literal");
                let (index, count) = literal::parse_integer(7, &v[offset..])?;
                offset += count;

                let Some((name, Some(value))) = decode_table.get(index as usize, true) else {
                    println!("Index: {index}, Size: {}", decode_table.dyn_size() + 61);
                    return Err(io::Error::other(
                        "Raw indexed header must have name and value",
                    ));
                };

                if name.eq_ignore_ascii_case(b"accept-language") {
                    println!("Got accept language\n--------------------------------------")
                }

                insert_header(&mut headers, name, value);
            } else if first_byte & 0x40 == 0x40 {
                println!("Incremental");
                let (index, count) = literal::parse_integer(6, &v[offset..])?;
                offset += count;

                let n: Cow<'static, BStr> = if index == 0 {
                    let (mut name, count) = literal::parse_string(&v[offset..])?;
                    offset += count;
                    name.make_ascii_lowercase();

                    Cow::Owned(name)
                } else {
                    let Some((name, _)) = decode_table.get(index as usize, false) else {
                        println!("Index: {index}");
                        return Err(io::Error::other(
                            "Incremental indexed header index must exist",
                        ));
                    };

                    name
                };

                let (value, count) = literal::parse_string(&v[offset..])?;
                offset += count;

                decode_table.insert(&*n, &value);
                insert_header(&mut headers, n, value.into());
            } else if first_byte & 0x20 == 0x20 {
                let (_new_size, count) = literal::parse_integer(5, &v[offset..])?;
                offset += count;
            } else if first_byte & 0x10 == 0x10 {
                println!("Without");
                let (index, count) = literal::parse_integer(4, &v[offset..])?;
                offset += count;

                let n: Cow<'static, _> = if index == 0 {
                    let (mut name, count) = literal::parse_string(&v[offset..])?;
                    offset += count;
                    name.make_ascii_lowercase();

                    Cow::Owned(name)
                } else {
                    let Some((name, _)) = decode_table.get(index as usize, false) else {
                        println!("Index: {index}");
                        return Err(io::Error::other("Without indexing header index must exist"));
                    };

                    name
                };

                let (value, count) = literal::parse_string(&v[offset..])?;
                offset += count;

                insert_header(&mut headers, n, value.into());
            } else {
                println!("Never");
                let (index, count) = literal::parse_integer(4, &v[offset..])?;
                offset += count;

                let n: Cow<'static, _> = if index == 0 {
                    let (mut name, count) = literal::parse_string(&v[offset..])?;
                    offset += count;
                    name.make_ascii_lowercase();

                    Cow::Owned(name)
                } else {
                    let Some((name, _)) = decode_table.get(index as usize, false) else {
                        println!("Index: {index}");
                        return Err(io::Error::other("Never indexing header index must exist"));
                    };

                    name
                };

                let (value, count) = literal::parse_string(&v[offset..])?;
                offset += count;

                insert_header(&mut headers, n, value.into());
            }
        }

        Ok(headers)
    }

    pub fn write_ok(stream_id: u32, output: &mut impl Write) -> io::Result<()> {
        write_header(
            FrameHeader {
                frame_len: 1,
                frame_type: TYPE_NUM,
                flags: END_HEAD_BIT,
                stream_id,
            },
            output,
        )?;
        output.write_all(&[0x88])
    }
}

pub mod priority {
    use super::*;
    use std::io::{self, Read};

    #[derive(Clone, Copy, Debug)]
    pub struct PriorityFrame {
        exclusive: bool,
        dependency: u32,
        weight: u8,
    }

    pub const TYPE_NUM: u8 = 0x2;

    pub fn read_frame(
        header: FrameHeader,
        frame_data: &mut impl Read,
    ) -> io::Result<PriorityFrame> {
        assert_eq!(header.frame_type, TYPE_NUM);
        let FrameHeader { frame_len, .. } = header;

        if frame_len != 5 {
            return Err(io::Error::other("Priority frame must be length 5"));
        }
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

    pub fn write_frame(
        stream_id: u32,
        data: PriorityFrame,
        output: &mut impl Write,
    ) -> io::Result<()> {
        let ex = if data.exclusive { 0x80000000u32 } else { 0x0 };
        let id = data.dependency & 0x7FFFFFFF;

        let id_ex = id | ex;

        write_header(
            FrameHeader {
                frame_len: 5,
                frame_type: TYPE_NUM,
                flags: 0x0,
                stream_id,
            },
            output,
        )?;
        write_u32(output, id_ex)?;
        write_u8(output, data.weight)?;

        Ok(())
    }
}

pub mod rst_stream {
    use super::*;

    pub const TYPE_NUM: u8 = 0x3;

    pub fn read_frame(header: FrameHeader, reader: &mut impl Read) -> io::Result<u32> {
        assert_eq!(header.frame_type, TYPE_NUM);
        if header.frame_len != 4 {
            return Err(io::Error::other("Reset stream frame must have length 4"));
        }

        read_u32(reader)
    }

    pub fn write_frame(stream_id: u32, error_code: u32, output: &mut impl Write) -> io::Result<()> {
        write_header(
            FrameHeader {
                frame_len: 4,
                frame_type: TYPE_NUM,
                flags: 0x0,
                stream_id,
            },
            output,
        )?;

        write_u32(output, error_code)
    }
    pub const fn create_header(stream_id: u32) -> FrameHeader {
        FrameHeader {
            frame_len: 4,
            frame_type: TYPE_NUM,
            flags: 0x0,
            stream_id,
        }
    }
}

pub mod settings {
    use super::*;
    use std::io::{self, Read};

    #[derive(Clone, Copy, Debug)]
    pub struct HTTP2Settings {
        pub header_table_size: u32,
        pub enable_push: bool,
        pub max_concurrent_streams: Option<u32>,
        pub initial_window_size: u32,
        pub max_frame_size: u32,
        pub max_header_list_size: Option<u32>,
    }

    impl HTTP2Settings {
        fn iter(&self) -> SettingsIter<'_> {
            SettingsIter {
                settings: self,
                setting_num: HEADER_TABLE_SIZE_NUM,
            }
        }
    }

    impl Default for HTTP2Settings {
        fn default() -> Self {
            HTTP2Settings {
                header_table_size: 4096,
                enable_push: true,
                max_concurrent_streams: None,
                initial_window_size: u16::MAX as u32,
                max_frame_size: 1 << 14,
                max_header_list_size: None,
            }
        }
    }

    struct SettingsIter<'a> {
        settings: &'a HTTP2Settings,
        setting_num: u16,
    }

    impl<'a> Iterator for SettingsIter<'a> {
        type Item = Option<u32>;

        fn next(&mut self) -> Option<Self::Item> {
            let out = Some(match self.setting_num {
                HEADER_TABLE_SIZE_NUM => Some(self.settings.header_table_size),
                ENABLE_PUSH_NUM => Some(if self.settings.enable_push { 0x1 } else { 0x0 }),
                MAX_CONCURRENT_STREAMS_NUM => self.settings.max_concurrent_streams,
                INITIAL_WINDOW_SIZE_NUM => Some(self.settings.initial_window_size),
                MAX_FRAME_SIZE_NUM => Some(self.settings.max_frame_size),
                MAX_HEADER_LIST_SIZE_NUM => self.settings.max_header_list_size,
                _ => return None,
            });

            self.setting_num += 1;
            out
        }
    }

    pub const HEADER_TABLE_SIZE_NUM: u16 = 0x1;
    pub const ENABLE_PUSH_NUM: u16 = 0x2;
    pub const MAX_CONCURRENT_STREAMS_NUM: u16 = 0x3;
    pub const INITIAL_WINDOW_SIZE_NUM: u16 = 0x4;
    pub const MAX_FRAME_SIZE_NUM: u16 = 0x5;
    pub const MAX_HEADER_LIST_SIZE_NUM: u16 = 0x6;

    pub const ACK_BIT: u8 = 0x1;

    pub const TYPE_NUM: u8 = 0x4;

    #[derive(Clone, Copy, Debug)]
    pub enum Settings {
        Ack,
        Settings(HTTP2Settings),
    }

    pub fn read_frame(header: FrameHeader, reader: &mut impl Read) -> io::Result<Settings> {
        assert_eq!(header.frame_type, TYPE_NUM);
        let FrameHeader {
            frame_len,
            frame_type: _,
            flags,
            stream_id,
        } = header;

        if stream_id != 0 {
            return Err(io::Error::other("Settings frame stream id must be 0"));
        }

        if frame_len % 6 != 0 {
            return Err(io::Error::other("Settings frame len was not multiple of 6"));
        }

        if flags & ACK_BIT == ACK_BIT {
            if frame_len != 0 {
                return Err(io::Error::other("Ack settings frame must be 0 length"));
            }

            return Ok(Settings::Ack);
        }

        let mut settings = HTTP2Settings::default();
        let mut buf = [0u8; 6];
        let mut read = 0;

        while read < frame_len {
            reader.read_exact(&mut buf)?;
            read += 6;

            let setting_type = read_u16(&mut &buf[0..2]).expect("Never fails");
            let value = read_u32(&mut &buf[2..]).expect("Never fails");

            match setting_type {
                HEADER_TABLE_SIZE_NUM => settings.header_table_size = value,
                ENABLE_PUSH_NUM => {
                    if value == 0 {
                        settings.enable_push = false;
                    } else if value == 1 {
                        settings.enable_push = true;
                    } else {
                        return Err(io::Error::other("Enable push setting must be 0 or 1"));
                    }
                }
                MAX_CONCURRENT_STREAMS_NUM => settings.max_concurrent_streams = Some(value),
                INITIAL_WINDOW_SIZE_NUM => settings.initial_window_size = value,
                MAX_FRAME_SIZE_NUM => {
                    if value < 1 << 14 || value >= 1 << 24 {
                        return Err(io::Error::other(
                            "Max frame size must be between 2^14 (inclusive) and 2^24 (exclusive)",
                        ));
                    } else {
                        settings.max_frame_size = value;
                    }
                }
                MAX_HEADER_LIST_SIZE_NUM => settings.max_header_list_size = Some(value),
                _ => {}
            }
        }

        Ok(Settings::Settings(settings))
    }

    pub fn write_frame(settings: Settings, output: &mut impl Write) -> io::Result<()> {
        match settings {
            Settings::Ack => write_header(
                FrameHeader {
                    frame_len: 0,
                    frame_type: TYPE_NUM,
                    flags: ACK_BIT,
                    stream_id: 0x0,
                },
                output,
            ),
            Settings::Settings(s) => {
                let def = HTTP2Settings::default();

                let mut buf = [0u8; 36];
                let mut written = 0;
                for (n, value) in s
                    .iter()
                    .zip(def.iter())
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .filter(|(_, (a, _))| a.is_some())
                    .map(|(n, (a, _))| (n, a.unwrap()))
                {
                    let mut c = io::Cursor::new(&mut buf[written..written + 6]);
                    write_u16(&mut c, (n + 1) as u16)?;
                    write_u32(&mut c, value)?;

                    written += 6;
                }

                write_header(
                    FrameHeader {
                        frame_len: written as u32,
                        frame_type: TYPE_NUM,
                        flags: 0x0,
                        stream_id: 0x0,
                    },
                    output,
                )?;
                output.write_all(&buf[0..written])
            }
        }
    }
}

pub mod ping {
    use super::*;
    use std::io::{self, Read, Write};

    pub const ACK_BIT: u8 = 0x1;

    pub const TYPE_NUM: u8 = 0x6;

    pub fn read_frame(header: FrameHeader, reader: &mut impl Read) -> io::Result<(u64, bool)> {
        assert_eq!(header.frame_type, TYPE_NUM);
        let FrameHeader {
            frame_len,
            frame_type: _,
            flags,
            stream_id,
        } = header;

        if stream_id != 0 {
            return Err(io::Error::other("Ping frame stream id must be 0"));
        }
        if frame_len != 8 {
            return Err(io::Error::other("Ping frame length was non-zero"));
        }

        let is_ack = flags & ACK_BIT == ACK_BIT;
        let payload = read_u64(reader)?;

        Ok((payload, is_ack))
    }

    pub fn write_frame(data: u64, is_ack: bool, output: &mut impl Write) -> io::Result<()> {
        write_header(
            FrameHeader {
                frame_len: 8,
                frame_type: TYPE_NUM,
                flags: if is_ack { ACK_BIT } else { 0x0 },
                stream_id: 0x0,
            },
            output,
        )?;

        write_u64(output, data)
    }
}

pub mod go_away {
    use super::*;
    use std::io::{self, Read, Write};

    #[derive(Debug)]
    pub struct GoAwayFrame {
        pub last_stream_id: u32,
        pub error_code: u32,
        pub additional_data: Vec<u8>,
    }

    pub const TYPE_NUM: u8 = 0x7;

    pub fn read_frame(header: FrameHeader, reader: &mut impl Read) -> io::Result<GoAwayFrame> {
        assert_eq!(header.frame_type, TYPE_NUM);
        let FrameHeader {
            frame_len,
            frame_type: _,
            flags: _,
            stream_id,
        } = header;

        if stream_id != 0 {
            return Err(io::Error::other("Go Away frame must have stream id 0"));
        }

        if frame_len < 8 {
            return Err(io::Error::other("Go Away frame must have at least 8 bytes"));
        }

        let last_stream_id = read_u32(reader)? & 0x7FFFFFFF;
        let error_code = read_u32(reader)?;
        let mut additional_data = vec![0u8; frame_len as usize - 8];

        reader.read_exact(&mut additional_data)?;

        Ok(GoAwayFrame {
            last_stream_id,
            error_code,
            additional_data,
        })
    }

    pub fn write_frame(
        last_id: u32,
        error_code: u32,
        additional_data: &[u8],
        output: &mut impl Write,
    ) -> io::Result<()> {
        write_header(
            FrameHeader {
                frame_len: 8 + additional_data.len() as u32,
                frame_type: TYPE_NUM,
                flags: 0x0,
                stream_id: 0x0,
            },
            output,
        )?;

        write_u32(output, last_id)?;
        write_u32(output, error_code)?;
        output.write_all(additional_data)
    }
}

pub mod window_update {
    use super::*;
    use std::io::{self, Read, Write};

    pub const TYPE_NUM: u8 = 0x8;

    pub fn read_frame(header: FrameHeader, reader: &mut impl Read) -> io::Result<u32> {
        assert_eq!(header.frame_type, TYPE_NUM);
        let FrameHeader { frame_len, .. } = header;

        if frame_len != 4 {
            return Err(io::Error::other("Window update frame must have length 4"));
        }

        read_u32(reader)
    }

    pub fn write_frame(stream_id: u32, increment: u32, output: &mut impl Write) -> io::Result<()> {
        write_header(
            FrameHeader {
                frame_len: 4,
                frame_type: TYPE_NUM,
                flags: 0x0,
                stream_id,
            },
            output,
        )?;
        write_u32(output, increment)
    }
}
