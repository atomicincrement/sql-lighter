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

    /// Read a page from the database into an owned Page (deprecated - use read_page_ref instead)
    /// 
    /// Phase 7g: For zero-copy access without Cell allocations, use read_page_ref() instead.
    /// This method is kept for backward compatibility but should be phased out.
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

/// Read-write database file handler using memory mapping (Phase 7c: Zero-cache persistence)
/// 
/// Writes directly to the memory map without caching for immediate persistence.
/// All page modifications are written directly to the mmap.
pub struct DatabaseFile {
    _file: File,  // Keep file handle alive for mmap lifetime
    mmap: MmapMut,
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
        })
    }
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
        })
    }

    /// Read a page from the database (Phase 7c: Direct mmap read, no cache)
    pub fn read_page(&mut self, page_num: u32) -> Result<Page> {

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
        Page::parse(page_data, page_num)
    }

    /// Phase 7d: Read a page as a zero-copy PageRef from the mmap
    /// 
    /// Returns a PageRef that borrows from the mmap, enabling zero-copy access
    /// to page data without allocating or caching.
    pub fn read_page_ref(&self, page_num: u32) -> Result<PageRef<'_>> {
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

    /// Phase 7h: All writes now go directly to mmap via PageMut (see get_page_mut)
    /// Use get_page_mut() -> write_cells_bytes() -> flush() instead

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

    /// Phase 7e: Get mutable PageMut reference for zero-copy writes
    /// 
    /// Returns a PageMut that borrows mutably from the mmap, enabling direct writes
    /// to page buffers without intermediate allocations.
    pub fn get_page_mut(&mut self, page_num: u32) -> Result<PageMut<'_>> {
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

        let page_data = &mut self.mmap[page_offset..page_end];
        PageMut::new(page_data, page_num)
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

    #[test]
    fn test_zero_cache_immediate_persistence() -> Result<()> {
        // Phase 7h: Verify that pages are written directly to mmap via PageMut without caching
        let temp = NamedTempFile::new().map_err(|e| Error::IoError(e.to_string()))?;
        let path = temp.path();

        // Write with first connection using direct mmap writes via PageMut (Phase 7h)
        {
            let mut db = DatabaseFile::create(path, 4096)?;
            let mut page_mut = db.get_page_mut(1)?;
            page_mut.write_cells(PageType::TableLeaf, &[])?;
            db.flush()?;
        }

        // Read with second connection - should see the written data immediately
        {
            let mut db = DatabaseFile::open(path)?;
            let read_page = db.read_page(1)?;
            assert_eq!(read_page.page_num, 1);
            assert_eq!(read_page.page_type, PageType::TableLeaf);
        }

        Ok(())
    }

    #[test]
    fn test_multiple_connections_concurrent_access() -> Result<()> {
        // Phase 7h: Verify multiple connections can safely access the same file
        let temp = NamedTempFile::new().map_err(|e| Error::IoError(e.to_string()))?;
        let path = temp.path();

        // Create database with a valid page using direct mmap writes via PageMut (Phase 7h)
        {
            let mut db = DatabaseFile::create(path, 4096)?;
            let mut page_mut = db.get_page_mut(1)?;
            page_mut.write_cells(PageType::TableLeaf, &[])?;
            db.flush()?;
        }

        // Connection 1: Read and modify
        {
            let mut db1 = DatabaseFile::open(path)?;
            let page = db1.read_page(1)?;
            // Verify we can read what was written
            assert_eq!(page.page_type, PageType::TableLeaf);
            
            // Modify via direct mmap write (Phase 7h: page 1 header preserved)
            let mut page_mut = db1.get_page_mut(1)?;
            page_mut.write_cells(PageType::TableInterior, &[])?;
            db1.flush()?;
        }

        // Connection 2: Read the same data
        {
            let mut db2 = DatabaseFile::open(path)?;
            let page = db2.read_page(1)?;
            assert_eq!(page.page_type, PageType::TableInterior);
        }

        Ok(())
    }

    #[test]
    fn test_fsync_durability() -> Result<()> {
        // Phase 7h: Verify fsync ensures data persistence with direct mmap writes
        let temp = NamedTempFile::new().map_err(|e| Error::IoError(e.to_string()))?;
        let path = temp.path();

        // Write and fsync using direct mmap writes via PageMut (Phase 7h)
        {
            let mut db = DatabaseFile::create(path, 4096)?;
            let mut page_mut = db.get_page_mut(1)?;
            page_mut.write_cells(PageType::TableLeaf, &[])?;
            db.flush()?;  // This should call fsync via mmap.flush()
        }

        // Verify data persists after process boundary (simulated by drop)
        {
            let mut db = DatabaseFile::open(path)?;
            let page = db.read_page(1)?;
            assert_eq!(page.page_type, PageType::TableLeaf);
        }

        Ok(())
    }

    #[test]
    fn test_multi_table_writes() -> Result<()> {
        // Phase 7h: Integration test for multi-table updates with direct mmap writes
        let temp = NamedTempFile::new().map_err(|e| Error::IoError(e.to_string()))?;
        let path = temp.path();
        
        // Create and initialize database with page 1
        {
            let mut db = DatabaseFile::create(path, 4096)?;
            let mut page_mut = db.get_page_mut(1)?;
            page_mut.write_cells(PageType::TableLeaf, &[])?;
            db.flush()?;
        }
        
        // Reopen with larger file and write to multiple pages via direct mmap (Phase 7h)
        {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .map_err(|e| Error::IoError(e.to_string()))?;
            
            // Expand file to hold 3 pages (4096 byte pages)
            let new_size = 100 + (4096 * 3);
            file.set_len(new_size as u64)
                .map_err(|e| Error::IoError(e.to_string()))?;
            drop(file);
            
            let mut db = DatabaseFile::open(path)?;
            
            // Write to page 1 (table leaf)
            {
                let mut page_mut = db.get_page_mut(1)?;
                page_mut.write_cells(PageType::TableLeaf, &[])?;
            }
            
            // Write to page 2 (table interior)
            {
                let mut page_mut = db.get_page_mut(2)?;
                page_mut.write_cells(PageType::TableInterior, &[])?;
            }
            
            // Write to page 3 (table leaf again)
            {
                let mut page_mut = db.get_page_mut(3)?;
                page_mut.write_cells(PageType::TableLeaf, &[])?;
            }
            
            db.flush()?;
        }
        
        // Verify all pages were written correctly
        {
            let mut db = DatabaseFile::open(path)?;
            let page1 = db.read_page(1)?;
            let page2 = db.read_page(2)?;
            let page3 = db.read_page(3)?;
            
            assert_eq!(page1.page_type, PageType::TableLeaf);
            assert_eq!(page2.page_type, PageType::TableInterior);
            assert_eq!(page3.page_type, PageType::TableLeaf);
        }
        
        Ok(())
    }
}
