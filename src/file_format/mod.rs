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
pub use page::{Page, PageHeaderRef, PageHeaderMut, PageType, PageRef, PageMut};
pub use btree::BTree;
pub use cell::{Cell, LeafCellRef, InteriorCellRef, LeafCellIter, InteriorCellIter};
pub use record::Record;
pub use varint::{read_varint, write_varint};

use crate::error::{Error, Result};
use std::fs::{File, OpenOptions};
use std::path::Path;
use memmap2::{Mmap, MmapMut};

/// Read-only database file handler using memory mapping
/// 
/// Uses zero-copy semantics with PageRef on demand from the memory map.
/// No caching - each page is parsed directly from the mmap for minimum memory overhead.
pub struct DatabaseFileRead {
    _file: File,  // Keep file handle alive for mmap lifetime
    mmap: Mmap,
}

impl DatabaseFileRead {
    /// Open an existing SQLite database file for reading
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|e| Error::IoError(e.to_string()))?;

        let mmap = unsafe {
            Mmap::map(&file).map_err(|e| Error::IoError(e.to_string()))?
        };

        if mmap.len() < HEADER_SIZE {
            return Err(Error::ParseError("File too small for SQLite header".into()));
        }

        FileHeaderRef::new(&mmap[0..HEADER_SIZE])?;  // Validate header

        Ok(Self {
            _file: file,
            mmap,
        })
    }

    /// Create a zero-copy page reference from the memory map
    /// 
    /// Returns a PageRef that borrows from the mmap, enabling zero-copy access
    /// to page data without allocating or caching.
    pub fn read_page_ref<'a>(&'a self, page_num: u32) -> Result<PageRef<'a>> {
        let header_ref = FileHeaderRef::new(&self.mmap[0..HEADER_SIZE])?;
        let page_size = header_ref.page_size() as usize;

        // For page 1, the B-tree page data starts at byte 100 (after file header)
        // For other pages, they start at their calculated offset
        let page_offset = if page_num == 1 {
            HEADER_SIZE
        } else {
            (page_num as usize - 1) * page_size
        };

        let page_end = page_offset + page_size;
        if page_end > self.mmap.len() {
            return Err(Error::ParseError("Page offset out of bounds".into()));
        }

        let page_data = &self.mmap[page_offset..page_end];
        PageRef::new(page_data, page_num)
    }

    /// Read a page from the database into an owned Page
    /// 
    /// For convenience, this parses the page data into an owned Page struct.
    /// For zero-copy access, use read_page_ref() instead.
    pub fn read_page(&self, page_num: u32) -> Result<Page> {
        let page_ref = self.read_page_ref(page_num)?;
        let page_type = page_ref.page_type()?;
        let cells = page_ref.cells()?;
        Ok(Page {
            page_num,
            page_type,
            cells,
        })
    }

    /// Get the file header as a reference
    pub fn header(&self) -> Result<FileHeaderRef<'_>> {
        FileHeaderRef::new(&self.mmap[0..HEADER_SIZE])
    }

    /// Get page count
    pub fn page_count(&self) -> Result<u32> {
        let header_ref = FileHeaderRef::new(&self.mmap[0..HEADER_SIZE])?;
        Ok(header_ref.page_count())
    }

    /// Get total file size in bytes
    pub fn file_size(&self) -> usize {
        self.mmap.len()
    }
}

/// Read-write database file handler using memory mapping
pub struct DatabaseFile {
    _file: File,  // Keep file handle alive for mmap lifetime
    mmap: MmapMut,
    page_cache: std::collections::HashMap<u32, Page>,
}

impl DatabaseFile {
    /// Open an existing SQLite database file
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| Error::IoError(e.to_string()))?;

        let mmap = unsafe {
            MmapMut::map_mut(&file).map_err(|e| Error::IoError(e.to_string()))?
        };

        if mmap.len() < HEADER_SIZE {
            return Err(Error::ParseError("File too small for SQLite header".into()));
        }

        FileHeaderRef::new(&mmap[0..HEADER_SIZE])?;  // Validate header

        Ok(Self {
            _file: file,
            mmap,
            page_cache: std::collections::HashMap::new(),
        })
    }

    /// Create a new SQLite database file
    pub fn create<P: AsRef<Path>>(path: P, page_size: u16) -> Result<Self> {
        // Create file and allocate space for at least one page (100 byte header + page)
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|e| Error::IoError(e.to_string()))?;

        let initial_size = HEADER_SIZE + page_size as usize;
        file.set_len(initial_size as u64)
            .map_err(|e| Error::IoError(e.to_string()))?;

        let mut mmap = unsafe {
            MmapMut::map_mut(&file).map_err(|e| Error::IoError(e.to_string()))?
        };

        // Initialize header directly in mmap
        let mut header_mut = FileHeaderMut::new(&mut mmap[0..HEADER_SIZE])?;
        header_mut.init();
        header_mut.set_page_size(page_size as u32);

        Ok(Self {
            _file: file,
            mmap,
            page_cache: std::collections::HashMap::new(),
        })
    }

    /// Read a page from the database
    pub fn read_page(&mut self, page_num: u32) -> Result<Page> {
        if let Some(page) = self.page_cache.get(&page_num) {
            return Ok(page.clone());
        }

        let header_ref = FileHeaderRef::new(&self.mmap[0..HEADER_SIZE])?;
        let page_size = header_ref.page_size() as usize;

        // For page 1, the B-tree page data starts at byte 100 (after file header)
        // For other pages, they start at their calculated offset
        let page_offset = if page_num == 1 {
            HEADER_SIZE
        } else {
            (page_num as usize - 1) * page_size
        };

        let page_end = page_offset + page_size;
        if page_end > self.mmap.len() {
            return Err(Error::ParseError("Page offset out of bounds".into()));
        }

        let page_data = &self.mmap[page_offset..page_end];
        let page = Page::parse(page_data, page_num)?;
        self.page_cache.insert(page_num, page.clone());

        Ok(page)
    }

    /// Write a page to the database
    pub fn write_page(&mut self, page: &Page) -> Result<()> {
        let header_ref = FileHeaderRef::new(&self.mmap[0..HEADER_SIZE])?;
        let page_size = header_ref.page_size() as usize;

        // For page 1, the B-tree page data starts at byte 100 (after file header)
        // For other pages, they start at their calculated offset
        let (page_offset, write_size) = if page.page_num == 1 {
            // Page 1: starts at offset 100, and only the page portion (not file header)
            (HEADER_SIZE, page_size - HEADER_SIZE)
        } else {
            // Other pages: start at standard offset, full page size
            ((page.page_num as usize - 1) * page_size, page_size)
        };

        // Ensure mmap is large enough
        let page_end = page_offset + write_size;
        if page_end > self.mmap.len() {
            return Err(Error::ParseError("Page offset out of bounds".into()));
        }

        let buffer = page.serialize(page_size)?;
        
        // For page 1, serialize includes a file header area (bytes 0-100) that we should skip
        // since the real file header is at mmap[0..100]
        if page.page_num == 1 {
            // Copy only the page data portion (bytes 100 onwards from the serialized buffer)
            self.mmap[page_offset..page_end].copy_from_slice(&buffer[HEADER_SIZE..HEADER_SIZE + write_size]);
        } else {
            // For other pages, copy the entire serialized buffer
            self.mmap[page_offset..page_end].copy_from_slice(&buffer);
        }

        self.page_cache.insert(page.page_num, page.clone());

        Ok(())
    }

    /// Flush all changes to disk
    pub fn flush(&mut self) -> Result<()> {
        self.mmap
            .flush()
            .map_err(|e| Error::IoError(e.to_string()))?;
        Ok(())
    }

    /// Get the file header as a reference
    pub fn header(&self) -> Result<FileHeaderRef<'_>> {
        FileHeaderRef::new(&self.mmap[0..HEADER_SIZE])
    }

    /// Get mutable reference to header
    pub fn header_mut(&mut self) -> Result<FileHeaderMut<'_>> {
        FileHeaderMut::new(&mut self.mmap[0..HEADER_SIZE])
    }

    /// Clear the page cache
    pub fn clear_cache(&mut self) {
        self.page_cache.clear();
    }

    /// Get page count
    pub fn page_count(&self) -> Result<u32> {
        let header_ref = FileHeaderRef::new(&self.mmap[0..HEADER_SIZE])?;
        Ok(header_ref.page_count())
    }

    /// Get total file size in bytes
    pub fn file_size(&self) -> usize {
        self.mmap.len()
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
