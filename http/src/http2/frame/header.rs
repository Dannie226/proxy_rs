use std::io::{Read, Write};

use crate::{
    IsSane,
    bio::{Reader, Writer},
    buffer::{Buffer, ConstBuffer},
    function,
    http2::{
        context::Context,
        error::{ErrorCode, IOProtoResult},
        frame::{FrameHeader, data::PAD_BUF, read_u8, read_u32, write_header, write_u8, write_u32},
        hpack::{
            huffman::DecodeError,
            literal::{parse_integer, parse_string},
        },
    },
    result::{HttpResult, set_err, set_ok},
};

#[repr(C)]
pub struct HeaderFrame {
    data: Buffer,
    stream_dep: u32,
    exclusive: bool,
    weight: u8,
    end: u8,
}

pub const TYPE_NUM: u8 = 0x1;

pub const FLAGS_END_STREAM: u8 = 0x01;
pub const FLAGS_END_HEADERS: u8 = 0x04;
pub const FLAGS_PADDED: u8 = 0x08;
pub const FLAGS_PRIORITY: u8 = 0x20;

/// Reads a header frame from the given reader
///
/// On an error, the contents of the data buffer
/// are undefined. The buffer is still structurally
/// valid, so you can reset the length and move
/// on, but any contents of the buffer cannot be
/// trusted.
pub fn read_frame(
    header: FrameHeader,
    frame: &mut HeaderFrame,
    reader: &mut impl Read,
) -> IOProtoResult<()> {
    let FrameHeader {
        stream_id,
        mut frame_len,
        frame_type,
        flags,
    } = header;

    assert_eq!(frame_type, TYPE_NUM);

    if stream_id == 0 {
        Err(ErrorCode::ProtocolError)?
    }

    let pad_len = if flags & FLAGS_PADDED != 0 {
        if frame_len == 0 {
            Err(ErrorCode::ProtocolError)?
        }

        frame_len -= 1;
        read_u8(reader)?
    } else {
        0
    };

    let (dep, exclusive, weight) = if flags & FLAGS_PRIORITY != 0 {
        if frame_len < 5 {
            Err(ErrorCode::ProtocolError)?
        }

        let d = read_u32(reader)?;
        let weight = read_u8(reader)?;
        frame_len -= 5;

        (d & 0x7FFFFFFF, d & 0x80000000 != 0, weight)
    } else {
        (0x0, false, 16)
    };

    if pad_len as u32 > frame_len {
        Err(ErrorCode::ProtocolError)?
    }

    if dep == stream_id {
        Err(ErrorCode::ProtocolError)?
    }

    frame_len -= pad_len as u32;

    frame.exclusive = exclusive;
    frame.stream_dep = dep;
    frame.weight = weight;
    frame.end = flags & (FLAGS_END_STREAM | FLAGS_END_HEADERS);
    frame.data.reserve_len(frame_len as usize);
    frame.data.len = frame_len as usize;

    reader.read_exact(&mut frame.data)?;

    let mut pad_buf = [0u8; 256];

    reader.read_exact(&mut pad_buf[..pad_len as usize])?;

    Ok(())
}

const INDEXED_TAG: u8 = 0x80;
const INCREMENTAL_TAG: u8 = 0x40;
const SIZE_TAG: u8 = 0x20;

pub fn read_header(
    context: &mut Context,
    data: &[u8],
    name: &mut Buffer,
    value: &mut Buffer,
) -> Result<usize, DecodeError> {
    let Some(tag) = data.get(0).copied() else {
        return Err(DecodeError::TooSmall);
    };

    value.clear();
    name.clear();

    if tag & INDEXED_TAG != 0 {
        let (idx, read) = parse_integer(7, data)?;
        println!("Indexed: {idx}");

        context
            .header_table
            .get(idx as usize, name, Some(value))
            .map(|_| read)
            .map_err(|_| DecodeError::IndexNotFound)
    } else if tag & INCREMENTAL_TAG != 0 {
        let (idx, mut read) = parse_integer(6, data)?;

        if idx == 0 {
            read += parse_string(&data[read..], name)?;
        } else {
            context
                .header_table
                .get(idx as usize, name, None)
                .map_err(|_| DecodeError::IndexNotFound)?;
        }

        read += parse_string(&data[read..], value)?;

        context.header_table.insert(name, value);

        Ok(read)
    } else if tag & SIZE_TAG != 0 {
        let (size, read) = parse_integer(5, data)?;

        if size as usize > context.header_table.get_max_size() {
            return Err(DecodeError::TableTooSmall);
        }

        context.header_table.resize(size as usize);
        Ok(read)
    } else {
        let (idx, mut read) = parse_integer(4, data)?;

        if idx == 0 {
            read += parse_string(&data[read..], name)?;
        } else {
            context
                .header_table
                .get(idx as usize, name, None)
                .map_err(|_| DecodeError::IndexNotFound)?
        }

        read += parse_string(&data[read..], value)?;

        Ok(read)
    }
}

/// SAFETY:
///
/// 1) reader must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a HeaderFrame
///
/// On error, the contents of the data buffer are undefined
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_header_read_frame(
    reader: *mut Reader,
    header: FrameHeader,
    res: *mut HttpResult,
) -> ErrorCode {
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
    let reader = unsafe { reader.as_mut_unchecked() };
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    let ok = res.ok.cast::<HeaderFrame>();
    assert!(
        ok.is_sane(),
        "{}: Result ok is not convertible to a reference to a HeaderFrame",
        function!()
    );

    let frame = unsafe { ok.as_mut_unchecked() };

    match read_frame(header, frame, reader) {
        Ok(()) => {
            res.is_ok = true;

            ErrorCode::NoError
        }
        Err(e) => set_err!(res, e.get_error_code(), "{e}"),
    }
}

pub fn write_frame(
    stream_id: u32,
    pad: Option<u8>,
    priority: Option<(u32, bool, u8)>,
    end_flags: u8,
    data: &[u8],
    writer: &mut impl Write,
) -> std::io::Result<usize> {
    let mut flags = end_flags & (FLAGS_END_HEADERS | FLAGS_END_STREAM);

    let mut len = data.len() as u32;

    if let Some(p) = pad {
        flags |= FLAGS_PADDED;
        len += 1 + p as u32;
    }

    if let Some(_) = priority {
        flags |= FLAGS_PRIORITY;
        len += 5;
    }

    let mut written = write_header(
        FrameHeader {
            stream_id,
            frame_len: len,
            frame_type: TYPE_NUM,
            flags,
        },
        writer,
    )?;

    if let Some(p) = pad {
        write_u8(p, writer)?;
        written += 1;
    }

    if let Some((id, exclusive, weight)) = priority {
        let stream = id & 0x7FFFFFFF | if exclusive { 0x80000000 } else { 0 };

        write_u32(stream, writer)?;
        write_u8(weight, writer)?;
        written += 5;
    }

    writer.write_all(data)?;
    written += data.len();

    if let Some(p) = pad {
        writer.write_all(&PAD_BUF[..p as usize])?;
        written += p as usize;
    }

    Ok(written)
}

/// SAFETY:
///
/// 1) writer must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_header_write_frame(
    writer: *mut Writer,
    stream_id: u32,
    frame_props_packed: u64,
    data: ConstBuffer,
    res: *mut HttpResult,
) {
    assert!(
        writer.is_sane(),
        "{}: Writer is not convertible to a reference",
        function!()
    );
    assert!(
        res.is_sane(),
        "{}: Result is not convertible to a reference",
        function!(),
    );
    assert!(
        data.is_sane(),
        "{}: Header data buffer is not convertible to a slice",
        function!()
    );
    let writer = unsafe { writer.as_mut_unchecked() };
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    let (pad, priority, end_flags) = {
        let flags = (frame_props_packed & 0xFF) as u8;
        let weight = ((frame_props_packed >> 0x08) & 0xFF) as u8;
        let pad = ((frame_props_packed >> 0x10) & 0xFF) as u8;

        let stream_id = ((frame_props_packed >> 0x20) & 0x7FFFFFFF) as u32;

        (
            Some(pad).filter(|_| flags & FLAGS_PADDED != 0),
            Some((stream_id, flags & 0x2 != 0, weight)).filter(|_| flags & FLAGS_PRIORITY != 0),
            flags & (FLAGS_END_HEADERS | FLAGS_END_STREAM),
        )
    };

    match write_frame(stream_id, pad, priority, end_flags, &*data, writer) {
        Ok(l) => unsafe { set_ok(res, l, function!()) },
        Err(e) => set_err!(res, (), "{e}"),
    };
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;

    use bstr::{BStr, BString};

    use crate::{
        buffer::http_new_buffer,
        http2::{context::Context, frame::header::read_header},
        test_utils::parse_hex,
    };

    struct Test {
        wire: &'static str,
        headers: HashMap<BString, BString>,
    }

    fn run_test(test: Test, context: &mut Context) {
        let code = parse_hex(test.wire).expect("Failed to parse test wire format");

        let mut headers = HashMap::new();
        let mut name = http_new_buffer(64);
        let mut value = http_new_buffer(64);

        let mut data = &*code;

        while data.len() > 0 {
            let read =
                read_header(context, data, &mut name, &mut value).expect("Failed to decode header");

            println!(
                "Got {}, {} (read: {read})",
                BStr::new(&name),
                BStr::new(&value)
            );

            data = &data[read..];

            headers.insert(BString::from(&*name), BString::from(&*value));
        }

        assert_eq!(test.headers, headers);
    }

    #[test]
    fn basic_encoding() {
        // Test cases pulled from https://github.com/http2jp/hpack-test-case/blob/master/nghttp2/
        // Pre parsed so it is easier to use and run.
        // I only took a few just to make sure that everything is working correctly
        let tests = vec![
            vec![
                Test {
                    wire: "828641871d23f67a9721e9847abcd07f66a281b0dae053fad0321aa49d13fda992a49685340c8a6adca7e28102ef7da9677b8171707f6a62293a9d810020004015309ac2ca7f2c05c5c153b0497ca589d34d1f43aeba0c41a4c7a98f33a69a3fdf9a68fa1d75d0620d263d4c79a68fbed00177febe58f9fbed00177b518b2d4b70ddf45abefb4005db90408721eaa8a4498f5788ea52d6b0e83772ff",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "amazon.com"),
                      (":path", "/"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "82864191996293cae6a473150b0e91fb3d4b90f4ff04ab60d48e62a18c4c002c4d51d88ca321ea62e94643d5babb0c92adc372c00af17168017c0cb6cb712f5d537fc2539a352398ac5754df46a473158f9fbed00177bebe58f9fbed00176fc190c073919d29aee30c78f1e171d23f67a9721e963f",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "g-ecx.images-amazon.com"),
                      (":path", "/images/G/01/gno/beacon/BeaconSprite-US-01._V401903535_.png"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.amazon.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286c004ad60d48e62a18c4c002c795a83907415821e9a4f5309b07522b1d85a92b566f25a178b8b2f38fb4269c6a25e634bc4bfc290c1be",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "g-ecx.images-amazon.com"),
                      (":path", "/images/G/01/x-locale/common/transparent-pixel._V386942464_.gif"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.amazon.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286c004bf60d48e62a18c4c002c1a9982260e99cb63121903424b62d61683165619001621e8b69a9840ea93d2d61683165899003cbadaf171680071e7da7c312f5d537fc4bfc290c1be",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "g-ecx.images-amazon.com"),
                      (":path", "/images/G/01/img12/other/disaster-relief/300-column/sandy-relief_300x75._V400689491_.png"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.amazon.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286418bf1e3c2e3a47ecf52e43d3f84c5c4c390c2",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "www.amazon.com"),
                      (":path", "/"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286c104ad60d48e62a18c4c002c795a83907415821e9a4f5309b07522b1d85a92b566f25a178b885f109969c75b89798d2fc5c0c390c2bf",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "g-ecx.images-amazon.com"),
                      (":path", "/images/G/01/x-locale/common/transparent-pixel._V192234675_.gif"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.amazon.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286c104c160d48e62a18c4c002c1a9982261139ca86103a0a888bdcb5250c0431547eec040c82284842a107b0c546bdbab46a8b172b0d34e95e2e2d000e09c7db044bcc697fc5c0c390c2bf",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "g-ecx.images-amazon.com"),
                      (":path", "/images/G/01/img12/shoes/sales_events/11_nov/1030_AccessoriesPROMO_GWright._V400626950_.gif"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.amazon.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286c104ac60d48e62a18c4c002c436a4f49d26ee562c3a4e862fdb60c85a287000882202f1710be2101a75c6a25fa5737c5c0c390c2bf",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "g-ecx.images-amazon.com"),
                      (":path", "/images/G/01/Automotive/rotos/Duracell600_120._V192204764_.jpg"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.amazon.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286c104b060d48e62a18c4c002c5a662838e4c9548620d27b10c5071c992a90c41a4f62d40ec98abc5c42f882fb6d3c089798d2ffc5c0c390c2bf",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "g-ecx.images-amazon.com"),
                      (":path", "/images/G/01/ui/loadIndicators/loadIndicator-large._V192195480_.gif"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.amazon.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286418f293cae6a473150b0e91fb3d4b90f4f049a60d48e62a18c8c341c7fab69beb6ee19d78b7670b2dc4bf4ae6fc6c1c490c3c0",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "ecx.images-amazon.com"),
                      (":path", "/images/I/41HZ-ND-SUL._SL135_.jpg"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.amazon.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
            ],
            vec![
                Test {
                    wire: "828641878c6692d5c87a7f847abcd07f66a281b0dae053fad0321aa49d13fda992a49685340c8a6adca7e28102ef7da9677b8171707f6a62293a9d810020004015309ac2ca7f2c05c5c153b0497ca589d34d1f43aeba0c41a4c7a98f33a69a3fdf9a68fa1d75d0620d263d4c79a68fbed00177febe58f9fbed00177b518b2d4b70ddf45abefb4005db90408721eaa8a4498f5788ea52d6b0e83772ff",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "baidu.com"),
                      (":path", "/"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286c204896251f7310f52e621ffc1c0bf90be",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "baidu.com"),
                      (":path", "/favicon.ico"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286418af1e3c2f18cd25ab90f4f84c2c1c090bf",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "www.baidu.com"),
                      (":path", "/"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286be049060d4ccc4633496c48f541e6385798d2fc2539a352398ac5754df46a473158f9fbed00177bebe58f9fbed00176fc190c073909d29aee30c78f1e178c6692d5c87a58f60a4bb0e4bfc325f82eb8165c86f04182ee0042f61bd7c417305d71abcd5e0c2ddeb9871401f",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "www.baidu.com"),
                      (":path", "/img/baidu_sylogo1.gif"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.baidu.com/"),
                      ("cookie", "BAIDUID=B6136AC10EBE0A8FCD216EB64C4C1A5C:FG=1"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286c10491608324e5626a0f18e860d4ccc4c85e634bc5c0c390c2bfbe",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "www.baidu.com"),
                      (":path", "/cache/global/img/gs.gif"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.baidu.com/"),
                      ("cookie", "BAIDUID=B6136AC10EBE0A8FCD216EB64C4C1A5C:FG=1"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286418a40578e442469311721e9049f62c63c78f0c10649cac4d41e31d0c7443091d53583a560aecaed102b817e88c653032a2f2ac590c4c1",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "s1.bdstatic.com"),
                      (":path", "/r/www/cache/global/js/tangram-1.3.4c1.0.js"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "*/*"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.baidu.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286bf049962c63c78f0c10649cac4d41e31d0c7443139e92ac15de5fa23c7bec590c4c1",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "s1.bdstatic.com"),
                      (":path", "/r/www/cache/global/js/home-1.8.js"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "*/*"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.baidu.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286bf049762c63c78f0c10649cac5a82d8c744316ac15d95da5fa23c7bec590c4c1",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "s1.bdstatic.com"),
                      (":path", "/r/www/cache/user/js/u-1.3.4.js"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "*/*"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.baidu.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286bf049162c63c78f0c1a999832c15c0b817aea9bfc7c2c590c4c1",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "s1.bdstatic.com"),
                      (":path", "/r/www/img/i-1.0.0.png"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.baidu.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286c304896251f7310f52e621ffc7c6c590c4c0",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "www.baidu.com"),
                      (":path", "/favicon.ico"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("cookie", "BAIDUID=B6136AC10EBE0A8FCD216EB64C4C1A5C:FG=1"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
            ],
            vec![
                Test {
                    wire: "828641878c6692d5c87a7f847abcd07f66a281b0dae053fad0321aa49d13fda992a49685340c8a6adca7e28102ef7da9677b8171707f6a62293a9d810020004015309ac2ca7f2c05c5c153b0497ca589d34d1f43aeba0c41a4c7a98f33a69a3fdf9a68fa1d75d0620d263d4c79a68fbed00177febe58f9fbed00177b518b2d4b70ddf45abefb4005db90408721eaa8a4498f5788ea52d6b0e83772ff",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "baidu.com"),
                      (":path", "/"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286c204896251f7310f52e621ffc1c0bf90be",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "baidu.com"),
                      (":path", "/favicon.ico"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286418af1e3c2f18cd25ab90f4f84c2c1c090bf",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "www.baidu.com"),
                      (":path", "/"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286be049060d4ccc4633496c48f541e6385798d2fc2539a352398ac5754df46a473158f9fbed00177bebe58f9fbed00176fc190c073909d29aee30c78f1e178c6692d5c87a58f60a4bb0e4bfc325f82eb8165c86f04182ee0042f61bd7c417305d71abcd5e0c2ddeb9871401f",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "www.baidu.com"),
                      (":path", "/img/baidu_sylogo1.gif"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.baidu.com/"),
                      ("cookie", "BAIDUID=B6136AC10EBE0A8FCD216EB64C4C1A5C:FG=1"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286c10491608324e5626a0f18e860d4ccc4c85e634bc5c0c390c2bfbe",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "www.baidu.com"),
                      (":path", "/cache/global/img/gs.gif"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.baidu.com/"),
                      ("cookie", "BAIDUID=B6136AC10EBE0A8FCD216EB64C4C1A5C:FG=1"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286418a40578e442469311721e9049f62c63c78f0c10649cac4d41e31d0c7443091d53583a560aecaed102b817e88c653032a2f2ac590c4c1",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "s1.bdstatic.com"),
                      (":path", "/r/www/cache/global/js/tangram-1.3.4c1.0.js"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "*/*"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.baidu.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286bf049962c63c78f0c10649cac4d41e31d0c7443139e92ac15de5fa23c7bec590c4c1",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "s1.bdstatic.com"),
                      (":path", "/r/www/cache/global/js/home-1.8.js"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "*/*"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.baidu.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286bf049762c63c78f0c10649cac5a82d8c744316ac15d95da5fa23c7bec590c4c1",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "s1.bdstatic.com"),
                      (":path", "/r/www/cache/user/js/u-1.3.4.js"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "*/*"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.baidu.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286bf049162c63c78f0c1a999832c15c0b817aea9bfc7c2c590c4c1",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "s1.bdstatic.com"),
                      (":path", "/r/www/img/i-1.0.0.png"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "image/png,image/*;q=0.8,*/*;q=0.5"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("referer", "http://www.baidu.com/"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
                Test {
                    wire: "8286c304896251f7310f52e621ffc7c6c590c4c0",
                    headers: HashMap::from_iter([
                      (":method", "GET"),
                      (":scheme", "http"),
                      (":authority", "www.baidu.com"),
                      (":path", "/favicon.ico"),
                      ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.8; rv:16.0) Gecko/20100101 Firefox/16.0"),
                      ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
                      ("accept-language", "en-US,en;q=0.5"),
                      ("accept-encoding", "gzip, deflate"),
                      ("connection", "keep-alive"),
                      ("cookie", "BAIDUID=B6136AC10EBE0A8FCD216EB64C4C1A5C:FG=1"),
                  ]
                    .into_iter()
                    .map(|(k, v)| (BString::from(k), BString::from(v))))
                },
            ],
        ];

        for (i, t) in tests.into_iter().enumerate() {
            let mut context = Context::new_no_alloc();

            for (j, t) in t.into_iter().enumerate() {
                println!("Running Suite {i}, Test {j}");
                println!("-----------------------");
                run_test(t, &mut context);
            }
        }
    }
}
