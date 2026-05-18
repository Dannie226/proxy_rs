use std::io::{self, Write};

use crate::{
    buffer::{Buffer, http_clear_buffer, http_new_buffer},
    http2::hpack::huffman::{DecodeError, encode_bytes, get_size},
};

use super::huffman::decode_bytes;

pub fn parse_integer(bits: u8, data: &[u8]) -> Result<(u64, usize), DecodeError> {
    let Some(first_byte) = data.get(0).copied() else {
        return Err(DecodeError::TooSmall);
    };

    let mask = (1 << bits) - 1;
    let first = first_byte & mask;
    let mut offset = 1;

    let mut output = first as u64;
    if first < mask {
        return Ok((output, 1));
    }

    let mut m = 0;

    while m < 64 {
        let Some(byte) = data.get(offset).copied() else {
            return Err(DecodeError::TooSmall);
        };
        output += ((byte & 0x7F) as u64) << m;
        m += 7;
        offset += 1;

        if byte & 0x80 == 0 {
            return Ok((output, offset));
        }
    }

    Err(DecodeError::Overflow)
}

pub fn write_integer(
    prefix: u8,
    bits: u8,
    mut int: u64,
    writer: &mut impl Write,
) -> io::Result<usize> {
    let mask = (1 << bits) - 1;
    let mut len = 1;

    if int < mask as u64 {
        let output = [(prefix << bits) | ((int as u8) & mask)];
        return writer.write_all(&output).map(|_| len);
    }

    let mut output = [(prefix << bits) | mask];
    writer.write_all(&output)?;

    int = int - mask as u64;

    while int >= 128 {
        output[0] = ((int & 0x7F) as u8) | 0x80;
        writer.write_all(&output)?;
        int >>= 7;
        len += 1;
    }

    output[0] = (int & 0x7F) as u8;
    writer.write_all(&output).map(|_| len + 1)
}

pub fn parse_string(data: &[u8], out: &mut Buffer) -> Result<usize, DecodeError> {
    let (len, parsed) = parse_integer(7, data)?;
    let len = len as usize;

    if data[0] & 0x80 == 0x80 {
        let Some(d) = data.get(parsed..parsed + len) else {
            return Err(DecodeError::TooSmall);
        };

        decode_bytes(d, out)?;
    } else {
        let Some(d) = data.get(parsed..parsed + len) else {
            return Err(DecodeError::TooSmall);
        };

        out.clear();
        out.reserve_len(d.len());
        out.push_slice(d);
    }

    Ok(parsed + len)
}

pub fn write_string(s: &[u8], writer: &mut impl Write) -> io::Result<usize> {
    let size = get_size(s);

    if size < s.len() {
        let mut buffer = http_new_buffer(size);
        let i = write_integer(0x1, 1, size as u64, writer)?;
        encode_bytes(s, &mut buffer);
        writer.write_all(&buffer)?;

        // SAFETY: Quite obviously, buffer is a reference
        unsafe {
            http_clear_buffer(&mut buffer);
        }

        Ok(i + size)
    } else {
        let i = write_integer(0x0, 1, s.len() as u64, writer)?;
        writer.write_all(s)?;

        Ok(i + s.len())
    }
}
