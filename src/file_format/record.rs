//! SQL record format

use crate::error::{Error, Result};
use crate::types::SqlValue;
use super::varint::read_varint;

/// A SQL record
#[derive(Debug, Clone)]
pub struct Record {
    pub columns: Vec<SqlValue>,
}

impl Record {
    /// Parse a record from a buffer
    pub fn parse(buffer: &[u8]) -> Result<Self> {
        if buffer.is_empty() {
            return Err(Error::ParseError("Empty buffer for record".into()));
        }

        // Read header length (varint)
        let (header_len, offset) = read_varint(buffer)?;
        let header_len = header_len as usize;

        if offset + header_len > buffer.len() {
            return Err(Error::ParseError("Record header out of bounds".into()));
        }

        // Read type codes from header
        let mut type_codes = Vec::new();
        let mut header_offset = offset;

        while header_offset < offset + header_len {
            let (type_code, len) = read_varint(&buffer[header_offset..])?;
            type_codes.push(type_code as u32);
            header_offset += len;
        }

        // Parse column values
        let mut columns = Vec::new();
        let mut data_offset = offset + header_len;

        for type_code in type_codes {
            let value = Self::parse_value(type_code, &buffer[data_offset..], &mut data_offset)?;
            columns.push(value);
        }

        Ok(Self { columns })
    }

    fn parse_value(type_code: u32, buffer: &[u8], offset: &mut usize) -> Result<SqlValue> {
        let value = match type_code {
            0 => SqlValue::Null,
            1 => {
                if buffer.len() < 8 {
                    return Err(Error::ParseError("Not enough bytes for integer".into()));
                }
                let int_val =
                    i64::from_be_bytes([
                        buffer[0], buffer[1], buffer[2], buffer[3], buffer[4], buffer[5], buffer[6],
                        buffer[7],
                    ]);
                *offset += 8;
                SqlValue::Integer(int_val)
            }
            2 => {
                if buffer.len() < 8 {
                    return Err(Error::ParseError("Not enough bytes for real".into()));
                }
                let float_val = f64::from_be_bytes([
                    buffer[0], buffer[1], buffer[2], buffer[3], buffer[4], buffer[5], buffer[6],
                    buffer[7],
                ]);
                *offset += 8;
                SqlValue::Real(float_val)
            }
            code if code >= 13 && code % 2 == 1 => {
                // Text: (code - 13) / 2 bytes
                let len = ((code - 13) / 2) as usize;
                if buffer.len() < len {
                    return Err(Error::ParseError("Not enough bytes for text".into()));
                }
                let text = String::from_utf8(buffer[..len].to_vec())
                    .map_err(|_| Error::ParseError("Invalid UTF-8 in text".into()))?;
                *offset += len;
                SqlValue::Text(text)
            }
            code if code >= 12 && code % 2 == 0 => {
                // Blob: (code - 12) / 2 bytes
                let len = ((code - 12) / 2) as usize;
                if buffer.len() < len {
                    return Err(Error::ParseError("Not enough bytes for blob".into()));
                }
                let blob = buffer[..len].to_vec();
                *offset += len;
                SqlValue::Blob(blob)
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

    fn serialize_value(value: &SqlValue) -> Result<(u32, Vec<u8>)> {
        match value {
            SqlValue::Null => Ok((0, Vec::new())),
            SqlValue::Boolean(b) => {
                let byte = if *b { 1u8 } else { 0u8 };
                Ok((1, vec![byte]))
            }
            SqlValue::Integer(i) => {
                Ok((1, i.to_be_bytes().to_vec()))
            }
            SqlValue::Real(f) => {
                Ok((2, f.to_be_bytes().to_vec()))
            }
            SqlValue::Text(s) => {
                let bytes = s.as_bytes();
                let type_code = 13 + (bytes.len() * 2) as u32;
                Ok((type_code, bytes.to_vec()))
            }
            SqlValue::Blob(b) => {
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
                SqlValue::Integer(42),
                SqlValue::Text("hello".to_string()),
                SqlValue::Null,
            ],
        };

        let serialized = record.serialize()?;
        assert!(!serialized.is_empty());

        Ok(())
    }

    #[test]
    fn test_roundtrip_record() -> Result<()> {
        let original = Record {
            columns: vec![
                SqlValue::Integer(123),
                SqlValue::Real(3.14),
                SqlValue::Text("test".to_string()),
            ],
        };

        let serialized = original.serialize()?;
        let parsed = Record::parse(&serialized)?;

        assert_eq!(original.columns.len(), parsed.columns.len());

        Ok(())
    }
}
