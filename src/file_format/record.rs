//! SQL record format

use crate::error::{Error, Result};
use crate::types::Value;
use super::varint::read_varint;

/// A SQL record
#[derive(Debug, Clone)]
pub struct Record {
    pub columns: Vec<Value>,
}

impl Record {
    /// Parse a record from a buffer
    pub fn parse(buffer: &[u8]) -> Result<Self> {
        if buffer.is_empty() {
            return Err(Error::ParseError("Empty buffer for record".into()));
        }

        // Read header length (varint)
        let (header_len, mut offset) = read_varint(buffer)?;
        let header_len = header_len as usize;

        if offset > buffer.len() {
            return Err(Error::ParseError("Record header out of bounds".into()));
        }

        eprintln!("len={header_len} offset={offset}");

        // Read type codes from header
        let mut type_codes = Vec::new();
        while offset < header_len {
            eprintln!("  read_varint offset={offset} {:02x}", buffer[offset]);
            let (type_code, len) = read_varint(&buffer[offset..])?;
            type_codes.push(type_code as u32);
            offset += len;
        }

        // Parse column values
        let mut columns = Vec::new();
        for type_code in type_codes {
            if offset < buffer.len() {
                eprintln!("  parse_value offset={offset} {:02x}", buffer[offset]);
            }
            let mut relative_offset = 0usize;
            let value = Self::parse_value(type_code, &buffer[offset..], &mut relative_offset)?;
            offset += relative_offset;
            columns.push(value);
        }

        Ok(Self { columns })
    }

    fn parse_value(type_code: u32, buffer: &[u8], offset: &mut usize) -> Result<Value> {
        let value = match type_code {
            0 => Value::Null,
            1 => {
                // 1-byte signed integer
                if buffer.is_empty() {
                    return Err(Error::ParseError("Not enough bytes for 1-byte integer".into()));
                }
                let int_val = buffer[0] as i8 as i64;
                *offset += 1;
                Value::Integer(int_val)
            }
            2 => {
                // 2-byte big-endian signed integer
                if buffer.len() < 2 {
                    return Err(Error::ParseError("Not enough bytes for 2-byte integer".into()));
                }
                let int_val = i16::from_be_bytes([buffer[0], buffer[1]]) as i64;
                *offset += 2;
                Value::Integer(int_val)
            }
            3 => {
                // 3-byte big-endian signed integer
                if buffer.len() < 3 {
                    return Err(Error::ParseError("Not enough bytes for 3-byte integer".into()));
                }
                // Sign-extend from 3 bytes
                let int_val = if buffer[0] & 0x80 != 0 {
                    // Negative: sign-extend with 0xFF bytes
                    i64::from_be_bytes([0xFF, 0xFF, 0xFF, buffer[0], buffer[1], buffer[2], 0, 0]) >> 16
                } else {
                    // Positive: sign-extend with 0x00 bytes
                    i64::from_be_bytes([0x00, 0x00, 0x00, buffer[0], buffer[1], buffer[2], 0, 0]) >> 16
                };
                *offset += 3;
                Value::Integer(int_val)
            }
            4 => {
                // 4-byte big-endian signed integer
                if buffer.len() < 4 {
                    return Err(Error::ParseError("Not enough bytes for 4-byte integer".into()));
                }
                let int_val = i32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as i64;
                *offset += 4;
                Value::Integer(int_val)
            }
            5 => {
                // 6-byte big-endian signed integer
                if buffer.len() < 6 {
                    return Err(Error::ParseError("Not enough bytes for 6-byte integer".into()));
                }
                // Sign-extend from 6 bytes
                let int_val = if buffer[0] & 0x80 != 0 {
                    // Negative: sign-extend with 0xFF bytes
                    i64::from_be_bytes([0xFF, 0xFF, buffer[0], buffer[1], buffer[2], buffer[3], buffer[4], buffer[5]])
                } else {
                    // Positive: sign-extend with 0x00 bytes
                    i64::from_be_bytes([0x00, 0x00, buffer[0], buffer[1], buffer[2], buffer[3], buffer[4], buffer[5]])
                };
                *offset += 6;
                Value::Integer(int_val)
            }
            6 => {
                // 8-byte big-endian signed integer
                if buffer.len() < 8 {
                    return Err(Error::ParseError("Not enough bytes for 8-byte integer".into()));
                }
                let int_val = i64::from_be_bytes([
                    buffer[0], buffer[1], buffer[2], buffer[3], buffer[4], buffer[5], buffer[6], buffer[7],
                ]);
                *offset += 8;
                Value::Integer(int_val)
            }
            7 => {
                // 8-byte IEEE floating point number
                if buffer.len() < 8 {
                    return Err(Error::ParseError("Not enough bytes for 8-byte real".into()));
                }
                let float_val = f64::from_be_bytes([
                    buffer[0], buffer[1], buffer[2], buffer[3], buffer[4], buffer[5], buffer[6], buffer[7],
                ]);
                *offset += 8;
                Value::Real(float_val)
            }
            8 | 9 => {
                // Reserved type codes 8 and 9 represent constants 0 and 1
                Value::Integer(type_code as i64 - 8)
            }
            code if code >= 13 && code % 2 == 1 => {
                // Text: (code - 13) / 2 bytes
                let len = ((code - 13) / 2) as usize;
                // Read as much as available, don't error if truncated
                let actual_len = len.min(buffer.len());
                let text = String::from_utf8(buffer[..actual_len].to_vec())
                    .map_err(|_| Error::ParseError("Invalid UTF-8 in text".into()))?;
                *offset += actual_len;
                Value::Text(text)
            }
            code if code >= 12 && code % 2 == 0 => {
                // Blob: (code - 12) / 2 bytes
                let len = ((code - 12) / 2) as usize;
                // Read as much as available, don't error if truncated
                let actual_len = len.min(buffer.len());
                let blob = buffer[..actual_len].to_vec();
                *offset += actual_len;
                Value::Blob(blob)
            }
            _ => return Err(Error::ParseError(format!("Unknown type code: {}", type_code))),
        };

        Ok(value)
    }

    /// Serialize a record to bytes
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();

        // Build header with type codes
        let mut header = Vec::new();
        let mut data = Vec::new();

        for column in &self.columns {
            let (type_code, value_bytes) = Self::serialize_value(column)?;
            header.push(type_code);
            data.extend_from_slice(&value_bytes);
        }

        // Write header length as varint
        let header_varint = super::varint::write_varint(header.len() as u64);
        buffer.extend_from_slice(&header_varint);

        // Write type codes
        for type_code in header {
            let code_varint = super::varint::write_varint(type_code as u64);
            buffer.extend_from_slice(&code_varint);
        }

        // Write data
        buffer.extend_from_slice(&data);

        Ok(buffer)
    }

    fn serialize_value(value: &Value) -> Result<(u32, Vec<u8>)> {
        match value {
            Value::Null => Ok((0, Vec::new())),
            Value::Integer(i) => {
                // Choose the smallest type code that can represent this integer
                if *i >= -128 && *i <= 127 {
                    // Fits in 1 byte
                    Ok((1, vec![*i as u8]))
                } else if *i >= -32768 && *i <= 32767 {
                    // Fits in 2 bytes
                    Ok((2, i.to_be_bytes()[6..8].to_vec()))
                } else if *i >= -8388608 && *i <= 8388607 {
                    // Fits in 3 bytes (need to extract middle 3 bytes)
                    let bytes = i.to_be_bytes();
                    Ok((3, bytes[5..8].to_vec()))
                } else if *i >= -2147483648 && *i <= 2147483647 {
                    // Fits in 4 bytes
                    Ok((4, (*i as i32).to_be_bytes().to_vec()))
                } else if *i >= -140737488355328 && *i <= 140737488355327 {
                    // Fits in 6 bytes (need to extract middle 6 bytes)
                    let bytes = i.to_be_bytes();
                    Ok((5, bytes[2..8].to_vec()))
                } else {
                    // Needs full 8 bytes
                    Ok((6, i.to_be_bytes().to_vec()))
                }
            }
            Value::Real(f) => {
                // Use type code 7 for 8-byte IEEE floating point
                Ok((7, f.to_be_bytes().to_vec()))
            }
            Value::Text(s) => {
                let bytes = s.as_bytes();
                let type_code = 13 + (bytes.len() * 2) as u32;
                Ok((type_code, bytes.to_vec()))
            }
            Value::Blob(b) => {
                let type_code = 12 + (b.len() * 2) as u32;
                Ok((type_code, b.clone()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_record() -> Result<()> {
        let record = Record {
            columns: vec![
                Value::Integer(42),
                Value::Text("hello".to_string()),
                Value::Null,
            ],
        };

        let serialized = record.serialize()?;
        assert!(!serialized.is_empty());

        Ok(())
    }

}
