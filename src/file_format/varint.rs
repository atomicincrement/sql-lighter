//! Variable-length integer (varint) encoding/decoding
//!
//! SQLite uses varint encoding to efficiently store integers of varying sizes.

use crate::error::{Error, Result};

/// Encode an unsigned 64-bit integer as a varint
pub fn write_varint(value: u64) -> Vec<u8> {
    let mut result = Vec::new();
    let mut v = value;

    // SQLite varint encoding:
    // Each byte stores 7 bits of data. The high bit indicates if there are more bytes.
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;

        if v == 0 {
            result.push(byte);
            break;
        }

        result.push(byte | 0x80);
    }

    result
}

/// Decode a varint from a buffer
/// Returns (value, bytes_read)
pub fn read_varint(buffer: &[u8]) -> Result<(u64, usize)> {
    if buffer.is_empty() {
        return Err(Error::ParseError("Empty buffer for varint".into()));
    }

    let mut value: u64 = 0;
    let mut shift = 0;
    let mut bytes_read = 0;

    for (i, &byte) in buffer.iter().enumerate() {
        if i >= 9 {
            return Err(Error::ParseError("Varint too long".into()));
        }

        value |= ((byte & 0x7F) as u64) << shift;
        bytes_read += 1;

        if (byte & 0x80) == 0 {
            break;
        }

        shift += 7;
    }

    Ok((value, bytes_read))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_single_byte() {
        let encoded = write_varint(5);
        assert_eq!(encoded, vec![5]);

        let (decoded, len) = read_varint(&encoded).unwrap();
        assert_eq!(decoded, 5);
        assert_eq!(len, 1);
    }

    #[test]
    fn test_varint_two_bytes() {
        let encoded = write_varint(300);
        assert!(encoded.len() > 1);

        let (decoded, _) = read_varint(&encoded).unwrap();
        assert_eq!(decoded, 300);
    }

    #[test]
    fn test_varint_large() {
        // Maximum value that fits in 9 bytes: (1 << 63) - 1
        let max_varint = (1u64 << 63) - 1;
        let encoded = write_varint(max_varint);
        let (decoded, _) = read_varint(&encoded).unwrap();
        assert_eq!(decoded, max_varint);
    }

    #[test]
    fn test_varint_zero() {
        let encoded = write_varint(0);
        assert_eq!(encoded, vec![0]);

        let (decoded, _) = read_varint(&encoded).unwrap();
        assert_eq!(decoded, 0);
    }
}
