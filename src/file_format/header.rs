//! SQLite file header parsing and writing

use crate::error::{Error, Result};
use std::io::{Read, Seek, SeekFrom, Write};

const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";
pub const HEADER_SIZE: usize = 100;

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

/// Reference-based SQLite file header wrapper (100 bytes)
/// Reads all fields directly from the byte slice without copying.
#[derive(Debug, Clone, Copy)]
pub struct FileHeaderRef<'a> {
    buffer: &'a [u8],
}

impl<'a> FileHeaderRef<'a> {
    /// Create a new reference to a file header buffer
    pub fn new(buffer: &'a [u8]) -> Result<Self> {
        if buffer.len() < HEADER_SIZE {
            return Err(Error::ParseError("Buffer too small for file header".into()));
        }
        if &buffer[0..16] != SQLITE_MAGIC {
            return Err(Error::ParseError("Invalid SQLite magic number".into()));
        }
        Ok(Self { buffer })
    }

    pub fn magic(&self) -> &[u8] {
        &self.buffer[0..16]
    }

    pub fn page_size(&self) -> u32 {
        let raw = u16::from_be_bytes([self.buffer[16], self.buffer[17]]);
        if raw == 1 { 65536 } else { raw as u32 }
    }

    pub fn write_version(&self) -> u8 {
        self.buffer[18]
    }

    pub fn read_version(&self) -> u8 {
        self.buffer[19]
    }

    pub fn reserved_per_page(&self) -> u8 {
        self.buffer[20]
    }

    pub fn max_payload_fraction(&self) -> u8 {
        self.buffer[21]
    }

    pub fn min_payload_fraction(&self) -> u8 {
        self.buffer[22]
    }

    pub fn leaf_payload_fraction(&self) -> u8 {
        self.buffer[23]
    }

    pub fn change_counter(&self) -> u32 {
        u32::from_be_bytes([self.buffer[24], self.buffer[25], self.buffer[26], self.buffer[27]])
    }

    pub fn page_count(&self) -> u32 {
        u32::from_be_bytes([self.buffer[28], self.buffer[29], self.buffer[30], self.buffer[31]])
    }

    pub fn freelist_trunk(&self) -> u32 {
        u32::from_be_bytes([self.buffer[32], self.buffer[33], self.buffer[34], self.buffer[35]])
    }

    pub fn freelist_pages(&self) -> u32 {
        u32::from_be_bytes([self.buffer[36], self.buffer[37], self.buffer[38], self.buffer[39]])
    }

    pub fn schema_cookie(&self) -> u32 {
        u32::from_be_bytes([self.buffer[40], self.buffer[41], self.buffer[42], self.buffer[43]])
    }

    pub fn schema_format(&self) -> u32 {
        u32::from_be_bytes([self.buffer[44], self.buffer[45], self.buffer[46], self.buffer[47]])
    }

    pub fn cache_size(&self) -> u32 {
        u32::from_be_bytes([self.buffer[48], self.buffer[49], self.buffer[50], self.buffer[51]])
    }

    pub fn largest_root(&self) -> u32 {
        u32::from_be_bytes([self.buffer[52], self.buffer[53], self.buffer[54], self.buffer[55]])
    }

    pub fn text_encoding(&self) -> Result<TextEncoding> {
        let raw = u32::from_be_bytes([self.buffer[56], self.buffer[57], self.buffer[58], self.buffer[59]]);
        TextEncoding::from_u32(raw)
    }

    pub fn user_version(&self) -> u32 {
        u32::from_be_bytes([self.buffer[60], self.buffer[61], self.buffer[62], self.buffer[63]])
    }

    pub fn incremental_vacuum(&self) -> u32 {
        u32::from_be_bytes([self.buffer[64], self.buffer[65], self.buffer[66], self.buffer[67]])
    }

    pub fn app_id(&self) -> u32 {
        u32::from_be_bytes([self.buffer[68], self.buffer[69], self.buffer[70], self.buffer[71]])
    }

    pub fn version_valid(&self) -> u32 {
        u32::from_be_bytes([self.buffer[92], self.buffer[93], self.buffer[94], self.buffer[95]])
    }

    pub fn version_number(&self) -> u32 {
        u32::from_be_bytes([self.buffer[96], self.buffer[97], self.buffer[98], self.buffer[99]])
    }
}

/// Mutable reference-based SQLite file header wrapper
/// Writes all fields directly to the byte slice without copying.
pub struct FileHeaderMut<'a> {
    buffer: &'a mut [u8],
}

impl<'a> FileHeaderMut<'a> {
    /// Create a new mutable reference to a file header buffer
    pub fn new(buffer: &'a mut [u8]) -> Result<Self> {
        if buffer.len() < HEADER_SIZE {
            return Err(Error::ParseError("Buffer too small for file header".into()));
        }
        Ok(Self { buffer })
    }

    /// Initialize with SQLite magic number and defaults
    pub fn init(&mut self) {
        self.buffer[0..16].copy_from_slice(SQLITE_MAGIC);
        self.set_write_version(1);
        self.set_read_version(1);
        self.set_page_size(4096);
        self.set_schema_format(4);
        self.set_text_encoding(TextEncoding::Utf8);
    }

    pub fn as_ref(&self) -> FileHeaderRef<'_> {
        FileHeaderRef { buffer: self.buffer }
    }

    pub fn set_page_size(&mut self, value: u32) {
        let page_size_field = if value == 65536 { 1u16 } else { value as u16 };
        self.buffer[16..18].copy_from_slice(&page_size_field.to_be_bytes());
    }

    pub fn set_write_version(&mut self, value: u8) {
        self.buffer[18] = value;
    }

    pub fn set_read_version(&mut self, value: u8) {
        self.buffer[19] = value;
    }

    pub fn set_reserved_per_page(&mut self, value: u8) {
        self.buffer[20] = value;
    }

    pub fn set_max_payload_fraction(&mut self, value: u8) {
        self.buffer[21] = value;
    }

    pub fn set_min_payload_fraction(&mut self, value: u8) {
        self.buffer[22] = value;
    }

    pub fn set_leaf_payload_fraction(&mut self, value: u8) {
        self.buffer[23] = value;
    }

    pub fn set_change_counter(&mut self, value: u32) {
        self.buffer[24..28].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_page_count(&mut self, value: u32) {
        self.buffer[28..32].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_freelist_trunk(&mut self, value: u32) {
        self.buffer[32..36].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_freelist_pages(&mut self, value: u32) {
        self.buffer[36..40].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_schema_cookie(&mut self, value: u32) {
        self.buffer[40..44].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_schema_format(&mut self, value: u32) {
        self.buffer[44..48].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_cache_size(&mut self, value: u32) {
        self.buffer[48..52].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_largest_root(&mut self, value: u32) {
        self.buffer[52..56].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_text_encoding(&mut self, value: TextEncoding) {
        self.buffer[56..60].copy_from_slice(&value.to_u32().to_be_bytes());
    }

    pub fn set_user_version(&mut self, value: u32) {
        self.buffer[60..64].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_incremental_vacuum(&mut self, value: u32) {
        self.buffer[64..68].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_app_id(&mut self, value: u32) {
        self.buffer[68..72].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_version_valid(&mut self, value: u32) {
        self.buffer[92..96].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_version_number(&mut self, value: u32) {
        self.buffer[96..100].copy_from_slice(&value.to_be_bytes());
    }
}


/// Helper functions for file header I/O
pub mod io {
    use super::*;

    /// Read header from file into a buffer
    pub fn read_header<R: Read + Seek>(reader: &mut R) -> Result<[u8; HEADER_SIZE]> {
        reader
            .seek(SeekFrom::Start(0))
            .map_err(|e| Error::IoError(e.to_string()))?;

        let mut buffer = [0u8; HEADER_SIZE];
        reader
            .read_exact(&mut buffer)
            .map_err(|e| Error::IoError(e.to_string()))?;

        Ok(buffer)
    }

    /// Write header buffer to file
    pub fn write_header<W: Write + Seek>(writer: &mut W, buffer: &[u8; HEADER_SIZE]) -> Result<()> {
        writer
            .seek(SeekFrom::Start(0))
            .map_err(|e| Error::IoError(e.to_string()))?;
        writer
            .write_all(buffer)
            .map_err(|e| Error::IoError(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_header() -> Result<()> {
        let mut buffer = [0u8; HEADER_SIZE];
        let mut header = FileHeaderMut::new(&mut buffer)?;
        header.init();

        let header_ref = header.as_ref();
        assert_eq!(header_ref.page_size(), 4096);
        assert_eq!(header_ref.write_version(), 1);
        assert_eq!(header_ref.text_encoding()?, TextEncoding::Utf8);

        Ok(())
    }

    #[test]
    fn test_parse_header_ref() -> Result<()> {
        let mut buffer = vec![0u8; HEADER_SIZE];
        buffer[0..16].copy_from_slice(SQLITE_MAGIC);
        buffer[16..18].copy_from_slice(&4096u16.to_be_bytes());
        buffer[18] = 1;
        buffer[19] = 1;
        buffer[56..60].copy_from_slice(&1u32.to_be_bytes());

        let header = FileHeaderRef::new(&buffer)?;
        assert_eq!(header.page_size(), 4096);
        assert_eq!(header.text_encoding()?, TextEncoding::Utf8);

        Ok(())
    }

    #[test]
    fn test_invalid_magic() {
        let mut buffer = [0u8; HEADER_SIZE];
        buffer[0] = 0xFF;

        assert!(FileHeaderRef::new(&buffer).is_err());
    }

    #[test]
    fn test_set_and_get_fields() -> Result<()> {
        let mut buffer = [0u8; HEADER_SIZE];
        let mut header_mut = FileHeaderMut::new(&mut buffer)?;
        header_mut.init();
        header_mut.set_page_count(42);
        header_mut.set_change_counter(100);

        let header_ref = header_mut.as_ref();
        assert_eq!(header_ref.page_count(), 42);
        assert_eq!(header_ref.change_counter(), 100);

        Ok(())
    }

    #[test]
    fn test_large_page_size() -> Result<()> {
        let mut buffer = [0u8; HEADER_SIZE];
        let mut header_mut = FileHeaderMut::new(&mut buffer)?;
        header_mut.init();
        header_mut.set_page_size(65536);

        let header_ref = header_mut.as_ref();
        assert_eq!(header_ref.page_size(), 65536);

        Ok(())
    }
}
