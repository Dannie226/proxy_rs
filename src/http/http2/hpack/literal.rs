use std::io::{self, Read, Write};

use crate::http::http2::hpack::huffman::decode_bytes;

pub fn parse_integer(bits: u8, data: &[u8]) -> io::Result<(u64, usize)> {
    let first_byte = data[0];
    let mask = (1 << bits) - 1;
    let first = first_byte & mask;
    let mut offset = 1;

    let mut output = first as u64;
    if first < mask {
        return Ok((output, 1));
    }

    let mut m = 0;

    while m < 64 {
        let byte = data[offset];
        output = output + ((byte & 0x7F) as u64) << m;
        m += 7;
        offset += 1;

        if byte & 0x80 == 0 {
            return Ok((output, offset - 1));
        }
    }

    Err(io::Error::other("Data overflowed a 64-bit integer"))
}

pub fn write_integer(
    prefix: u8,
    bits: u8,
    mut int: u64,
    writer: &mut impl Write,
) -> io::Result<()> {
    let mask = (1 << bits) - 1;

    if int < mask as u64 {
        let output = [(prefix << bits) | ((int as u8) & mask)];
        return writer.write_all(&output);
    }

    let mut output = [(prefix << bits) | mask];
    writer.write_all(&output)?;

    int = int - mask as u64;

    while int >= 128 {
        output[0] = ((int & 0x7F) as u8) | 0x80;
        writer.write_all(&output)?;
        int >>= 7;
    }

    output[0] = (int & 0x7F) as u8;
    writer.write_all(&output)
}

pub fn parse_string(data: &[u8]) -> io::Result<(Vec<u8>, usize)> {
    let (len, parsed) = parse_integer(7, data)?;
    let len = len as usize;

    if data[0] & 0x80 == 0x80 {
        decode_bytes(len, &data[parsed..]).map(|v| (v, parsed + len))
    } else {
        Ok((data[parsed..parsed + len].to_vec(), parsed + len))
    }
}
