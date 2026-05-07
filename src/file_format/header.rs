//! SQLite file header parsing and writing

use crate::error::{Error, Result};
use std::io::{Read, Seek, SeekFrom, Write};

const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";
const HEADER_SIZE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8 = 1,
    Utf16LE = 2,
    Utf16BE = 3,
}

impl TextEncoding {
    pub fn from_u32(value: u32) -> Result<Self> {
        match value {
            1 => Ok(TextEncoding::Utf8),
            2 => Ok(TextEncoding::Utf16LE),
            3 => Ok(TextEncoding::Utf16BE),
            _ => Err(Error::ParseError(format!("Invalid text encoding: {}", value))),
        }
    }

    pub fn to_u32(self) -> u32 {
        self as u32
    }
}

/// SQLite database file header (100 bytes)
#[derive(Debug, Clone)]
pub struct FileHeader {
    pub magic: &'static [u8],
    pub page_size: u32,  // Changed from u16 to u32 to support 65536
    pub write_version: u8,
    pub read_version: u8,
    pub reserved_per_page: u8,
    pub max_payload_fraction: u8,
    pub min_payload_fraction: u8,
    pub leaf_payload_fraction: u8,
    pub change_counter: u32,
    pub page_count: u32,
    pub freelist_trunk: u32,
    pub freelist_pages: u32,
    pub schema_cookie: u32,
    pub schema_format: u32,
    pub cache_size: u32,
    pub largest_root: u32,
    pub text_encoding: TextEncoding,
    pub user_version: u32,
    pub incremental_vacuum: u32,
    pub app_id: u32,
    pub version_valid: u32,
    pub version_number: u32,
}

impl Default for FileHeader {
    fn default() -> Self {
        Self {
            magic: SQLITE_MAGIC,
            page_size: 4096,
            write_version: 1,
            read_version: 1,
            reserved_per_page: 0,
            max_payload_fraction: 64,
            min_payload_fraction: 32,
            leaf_payload_fraction: 32,
            change_counter: 0,
            page_count: 0,
            freelist_trunk: 0,
            freelist_pages: 0,
            schema_cookie: 0,
            schema_format: 4,
            cache_size: 0,
            largest_root: 0,
            text_encoding: TextEncoding::Utf8,
            user_version: 0,
            incremental_vacuum: 0,
            app_id: 0,
            version_valid: 0,
            version_number: 0,
        }
    }
}

impl FileHeader {
    /// Read header from file
    pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        reader
            .seek(SeekFrom::Start(0))
            .map_err(|e| Error::IoError(e.to_string()))?;

        let mut buffer = [0u8; HEADER_SIZE];
        reader
            .read_exact(&mut buffer)
            .map_err(|e| Error::IoError(e.to_string()))?;

        Self::parse(&buffer)
    }

    /// Parse header from buffer
    pub fn parse(buffer: &[u8]) -> Result<Self> {
        if buffer.len() < HEADER_SIZE {
            return Err(Error::ParseError("Buffer too small for file header".into()));
        }

        // Verify magic number
        if &buffer[0..16] != SQLITE_MAGIC {
            return Err(Error::ParseError("Invalid SQLite magic number".into()));
        }

        let page_size_raw = u16::from_be_bytes([buffer[16], buffer[17]]);
        let page_size = if page_size_raw == 1 { 65536u32 } else { page_size_raw as u32 };

        let write_version = buffer[18];
        let read_version = buffer[19];
        let reserved_per_page = buffer[20];
        let max_payload_fraction = buffer[21];
        let min_payload_fraction = buffer[22];
        let leaf_payload_fraction = buffer[23];

        let change_counter = u32::from_be_bytes([buffer[24], buffer[25], buffer[26], buffer[27]]);
        let page_count = u32::from_be_bytes([buffer[28], buffer[29], buffer[30], buffer[31]]);
        let freelist_trunk = u32::from_be_bytes([buffer[32], buffer[33], buffer[34], buffer[35]]);
        let freelist_pages = u32::from_be_bytes([buffer[36], buffer[37], buffer[38], buffer[39]]);
        let schema_cookie = u32::from_be_bytes([buffer[40], buffer[41], buffer[42], buffer[43]]);
        let schema_format = u32::from_be_bytes([buffer[44], buffer[45], buffer[46], buffer[47]]);
        let cache_size = u32::from_be_bytes([buffer[48], buffer[49], buffer[50], buffer[51]]);
        let largest_root = u32::from_be_bytes([buffer[52], buffer[53], buffer[54], buffer[55]]);

        let text_encoding_raw = u32::from_be_bytes([buffer[56], buffer[57], buffer[58], buffer[59]]);
        let text_encoding = TextEncoding::from_u32(text_encoding_raw)?;

        let user_version = u32::from_be_bytes([buffer[60], buffer[61], buffer[62], buffer[63]]);
        let incremental_vacuum = u32::from_be_bytes([buffer[64], buffer[65], buffer[66], buffer[67]]);
        let app_id = u32::from_be_bytes([buffer[68], buffer[69], buffer[70], buffer[71]]);
        let version_valid = u32::from_be_bytes([buffer[92], buffer[93], buffer[94], buffer[95]]);
        let version_number = u32::from_be_bytes([buffer[96], buffer[97], buffer[98], buffer[99]]);

        Ok(Self {
            magic: SQLITE_MAGIC,
            page_size,
            write_version,
            read_version,
            reserved_per_page,
            max_payload_fraction,
            min_payload_fraction,
            leaf_payload_fraction,
            change_counter,
            page_count,
            freelist_trunk,
            freelist_pages,
            schema_cookie,
            schema_format,
            cache_size,
            largest_root,
            text_encoding,
            user_version,
            incremental_vacuum,
            app_id,
            version_valid,
            version_number,
        })
    }

    /// Write header to file
    pub fn write<W: Write + Seek>(&self, writer: &mut W) -> Result<()> {
        writer
            .seek(SeekFrom::Start(0))
            .map_err(|e| Error::IoError(e.to_string()))?;

        let mut buffer = vec![0u8; HEADER_SIZE];

        // Magic number
        buffer[0..16].copy_from_slice(SQLITE_MAGIC);

        // Page size (special: 1 means 65536)
        let page_size_field = if self.page_size == 65536 { 1u16 } else { self.page_size as u16 };
        buffer[16..18].copy_from_slice(&page_size_field.to_be_bytes());

        // Versions
        buffer[18] = self.write_version;
        buffer[19] = self.read_version;

        // Payload fractions
        buffer[20] = self.reserved_per_page;
        buffer[21] = self.max_payload_fraction;
        buffer[22] = self.min_payload_fraction;
        buffer[23] = self.leaf_payload_fraction;

        // Counters and sizes
        buffer[24..28].copy_from_slice(&self.change_counter.to_be_bytes());
        buffer[28..32].copy_from_slice(&self.page_count.to_be_bytes());
        buffer[32..36].copy_from_slice(&self.freelist_trunk.to_be_bytes());
        buffer[36..40].copy_from_slice(&self.freelist_pages.to_be_bytes());
        buffer[40..44].copy_from_slice(&self.schema_cookie.to_be_bytes());
        buffer[44..48].copy_from_slice(&self.schema_format.to_be_bytes());
        buffer[48..52].copy_from_slice(&self.cache_size.to_be_bytes());
        buffer[52..56].copy_from_slice(&self.largest_root.to_be_bytes());

        // Text encoding
        buffer[56..60].copy_from_slice(&self.text_encoding.to_u32().to_be_bytes());

        // Version info
        buffer[60..64].copy_from_slice(&self.user_version.to_be_bytes());
        buffer[64..68].copy_from_slice(&self.incremental_vacuum.to_be_bytes());
        buffer[68..72].copy_from_slice(&self.app_id.to_be_bytes());
        buffer[92..96].copy_from_slice(&self.version_valid.to_be_bytes());
        buffer[96..100].copy_from_slice(&self.version_number.to_be_bytes());

        writer
            .write_all(&buffer)
            .map_err(|e| Error::IoError(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_header() {
        let header = FileHeader::default();
        assert_eq!(header.page_size, 4096);
        assert_eq!(header.write_version, 1);
        assert_eq!(header.text_encoding, TextEncoding::Utf8);
    }

    #[test]
    fn test_parse_header() -> Result<()> {
        let mut buffer = vec![0u8; 100];
        buffer[0..16].copy_from_slice(SQLITE_MAGIC);
        buffer[16..18].copy_from_slice(&4096u16.to_be_bytes());
        buffer[18] = 1;
        buffer[19] = 1;
        buffer[56..60].copy_from_slice(&1u32.to_be_bytes());

        let header = FileHeader::parse(&buffer)?;
        assert_eq!(header.page_size, 4096);
        assert_eq!(header.text_encoding, TextEncoding::Utf8);

        Ok(())
    }

    #[test]
    fn test_invalid_magic() {
        let mut buffer = vec![0u8; 100];
        buffer[0] = 0xFF;

        assert!(FileHeader::parse(&buffer).is_err());
    }
}
