//! SQLite file format implementation
//!
//! This module implements reading and writing of SQLite database files,
//! including file header parsing, page management, and B-tree operations.

pub mod header;
pub mod page;
pub mod btree;
pub mod cell;
pub mod record;
pub mod varint;

pub use header::{FileHeaderRef, FileHeaderMut, TextEncoding, HEADER_SIZE};
pub use page::{Page, PageHeader, PageType};
pub use btree::BTree;
pub use cell::Cell;
pub use record::Record;
pub use varint::{read_varint, write_varint};

use crate::error::{Error, Result};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Main database file handler
pub struct DatabaseFile {
    file: File,
    header_buffer: [u8; HEADER_SIZE],
    page_cache: std::collections::HashMap<u32, Page>,
}

impl DatabaseFile {
    /// Open an existing SQLite database file
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| Error::IoError(e.to_string()))?;

        let header_buffer = header::io::read_header(&mut file)?;
        
        Ok(Self {
            file,
            header_buffer,
            page_cache: std::collections::HashMap::new(),
        })
    }

    /// Create a new SQLite database file
    pub fn create<P: AsRef<Path>>(path: P, page_size: u16) -> Result<Self> {
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| Error::IoError(e.to_string()))?;

        let mut header_buffer = [0u8; HEADER_SIZE];
        let mut header_mut = FileHeaderMut::new(&mut header_buffer)?;
        header_mut.init();
        header_mut.set_page_size(page_size as u32);

        header::io::write_header(&mut file, &header_buffer)?;

        Ok(Self {
            file,
            header_buffer,
            page_cache: std::collections::HashMap::new(),
        })
    }

    /// Read a page from the database
    pub fn read_page(&mut self, page_num: u32) -> Result<Page> {
        if let Some(page) = self.page_cache.get(&page_num) {
            return Ok(page.clone());
        }

        let header_ref = FileHeaderRef::new(&self.header_buffer)?;
        let page_size = header_ref.page_size();
        let offset = (page_num as u64 - 1) * page_size as u64;
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| Error::IoError(e.to_string()))?;

        let mut buffer = vec![0u8; page_size as usize];
        self.file
            .read_exact(&mut buffer)
            .map_err(|e| Error::IoError(e.to_string()))?;

        let page = Page::parse(&buffer, page_num)?;
        self.page_cache.insert(page_num, page.clone());

        Ok(page)
    }

    /// Write a page to the database
    pub fn write_page(&mut self, page: &Page) -> Result<()> {
        let header_ref = FileHeaderRef::new(&self.header_buffer)?;
        let page_size = header_ref.page_size();
        let offset = (page.page_num as u64 - 1) * page_size as u64;
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| Error::IoError(e.to_string()))?;

        let buffer = page.serialize(page_size as usize)?;
        self.file
            .write_all(&buffer)
            .map_err(|e| Error::IoError(e.to_string()))?;

        self.page_cache.insert(page.page_num, page.clone());

        Ok(())
    }

    /// Flush all changes to disk
    pub fn flush(&mut self) -> Result<()> {
        header::io::write_header(&mut self.file, &self.header_buffer)?;
        self.file
            .flush()
            .map_err(|e| Error::IoError(e.to_string()))?;
        Ok(())
    }

    /// Get the file header as a reference
    pub fn header(&self) -> Result<FileHeaderRef<'_>> {
        FileHeaderRef::new(&self.header_buffer)
    }

    /// Get mutable reference to header
    pub fn header_mut(&mut self) -> Result<FileHeaderMut<'_>> {
        FileHeaderMut::new(&mut self.header_buffer)
    }

    /// Clear the page cache
    pub fn clear_cache(&mut self) {
        self.page_cache.clear();
    }

    /// Get page count
    pub fn page_count(&self) -> Result<u32> {
        let header_ref = FileHeaderRef::new(&self.header_buffer)?;
        Ok(header_ref.page_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_create_database() -> Result<()> {
        let temp = NamedTempFile::new().map_err(|e| Error::IoError(e.to_string()))?;
        let path = temp.path();

        let db = DatabaseFile::create(path, 4096)?;
        let header = db.header()?;
        assert_eq!(header.page_size(), 4096);
        assert_eq!(header.write_version(), 1);

        Ok(())
    }

    #[test]
    fn test_open_database() -> Result<()> {
        let temp = NamedTempFile::new().map_err(|e| Error::IoError(e.to_string()))?;
        let path = temp.path();

        let _db = DatabaseFile::create(path, 4096)?;
        let db2 = DatabaseFile::open(path)?;

        let header = db2.header()?;
        assert_eq!(header.page_size(), 4096);
        assert_eq!(header.magic(), b"SQLite format 3\0");

        Ok(())
    }
}
