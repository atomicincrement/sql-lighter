//! B-tree page handling

use crate::error::{Error, Result};
use super::cell::{LeafCellIter, InteriorCellIter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageType {
    IndexInterior = 0x02,
    TableInterior = 0x05,
    IndexLeaf = 0x0A,
    TableLeaf = 0x0D,
}

impl PageType {
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0x02 => Ok(PageType::IndexInterior),
            0x05 => Ok(PageType::TableInterior),
            0x0A => Ok(PageType::IndexLeaf),
            0x0D => Ok(PageType::TableLeaf),
            _ => Err(Error::ParseError(format!("Invalid page type: 0x{:02x}", value))),
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self, PageType::IndexLeaf | PageType::TableLeaf)
    }

    pub fn is_interior(&self) -> bool {
        !self.is_leaf()
    }
}

/// Reference-based B-tree page wrapper for read-only access
/// Provides zero-copy views into page header and cells
#[derive(Debug, Copy, Clone)]
pub struct PageRef<'a> {
    page_num: u32,
    buffer: &'a [u8],
}

// From the SQLite file format documentation:
//
// The format of a cell depends on which kind of b-tree page the cell appears on. The following table shows the elements of a cell, in order of appearance, for the various b-tree page types.

// Table B-Tree Leaf Cell (header 0x0d):

// A varint which is the total number of bytes of payload, including any overflow
// A varint which is the integer key, a.k.a. "rowid"
// The initial portion of the payload that does not spill to overflow pages.
// A 4-byte big-endian integer page number for the first page of the overflow page list - omitted if all payload fits on the b-tree page.
// Table B-Tree Interior Cell (header 0x05):

// A 4-byte big-endian page number which is the left child pointer.
// A varint which is the integer key
// Index B-Tree Leaf Cell (header 0x0a):

// A varint which is the total number of bytes of key payload, including any overflow
// The initial portion of the payload that does not spill to overflow pages.
// A 4-byte big-endian integer page number for the first page of the overflow page list - omitted if all payload fits on the b-tree page.
// Index B-Tree Interior Cell (header 0x02):

// A 4-byte big-endian page number which is the left child pointer.
// A varint which is the total number of bytes of key payload, including any overflow
// The initial portion of the payload that does not spill to overflow pages.
// A 4-byte big-endian integer page number for the first page of the overflow page list - omitted if all payload fits on the b-tree page.

impl<'a> PageRef<'a> {
    /// Create a new reference to a page buffer
    pub fn new(buffer: &'a [u8], page_num: u32) -> Result<Self> {
        if buffer.is_empty() {
            return Err(Error::ParseError("Empty buffer for page".into()));
        }
        Ok(Self { page_num, buffer })
    }

    pub fn page_num(&self) -> u32 {
        self.page_num
    }

    /// Get immutable reference to page header
    /// For page 1, the page header starts at offset 100 (after the file header)
    /// For other pages, it starts at offset 0
    pub fn header(&self) -> Result<PageHeaderRef<'_>> {
        let header_offset = if self.page_num == 1 { 100 } else { 0 };
        PageHeaderRef::new(&self.buffer[header_offset..])
    }

    /// Get the page type
    pub fn page_type(&self) -> Result<PageType> {
        self.header()?.page_type()
    }

    /// Parse all cells from this page (deprecated - use raw_cells() instead)
    pub fn cells(&self) -> Result<Vec<()>> {
        todo!("Use raw_cells() or as_leaf_cells()/as_interior_cells() iterators for zero-copy access")
    }

    /// Phase 7g: Get raw cell byte slices without parsing into Cell objects
    /// Returns Vec of byte slices, one for each cell in the page
    pub fn raw_cells(&self) -> Result<Vec<&'a [u8]>> {
        let header = self.header()?;
        let header_size = header.header_size()?;

        // Cell pointers come after the header
        let cell_pointer_start = header_size;
        let cell_count = header.cell_count();

        let mut cells = Vec::new();

        // Read cell pointers and cells
        for i in 0..cell_count as usize {
            let ptr_offset = cell_pointer_start + (i * 2);
            if ptr_offset + 2 > self.buffer.len() {
                return Err(Error::ParseError("Cell pointer out of bounds".into()));
            }

            let cell_offset = u16::from_be_bytes([
                self.buffer[ptr_offset],
                self.buffer[ptr_offset + 1],
            ]) as usize;

            if cell_offset >= self.buffer.len() {
                return Err(Error::ParseError("Cell offset out of bounds".into()));
            }

            // Find the end of this cell by looking at the next cell pointer or end of free space
            let cell_end = if i + 1 < cell_count as usize {
                // Look at next cell pointer to find end
                let next_ptr_offset = cell_pointer_start + ((i + 1) * 2);
                let next_cell_offset = u16::from_be_bytes([
                    self.buffer[next_ptr_offset],
                    self.buffer[next_ptr_offset + 1],
                ]) as usize;
                next_cell_offset
            } else {
                // For last cell, go until end of buffer or fragmented free bytes marker
                self.buffer.len()
            };

            if cell_offset < cell_end {
                cells.push(&self.buffer[cell_offset..cell_end]);
            }
        }

        Ok(cells)
    }

    /// Get an iterator over leaf cells in this page (zero-copy)
    /// Returns None if this page is not a leaf page
    pub fn as_leaf_cells(&self) -> Result<Option<LeafCellIter<'_>>> {
        let header = self.header()?;
        
        if !header.page_type()?.is_leaf() {
            return Ok(None);
        }

        // For page 1: file header (100 bytes) + leaf page header (8 bytes) = 108
        // For other pages: page header (8 bytes) = 8
        let cell_pointer_start = if self.page_num == 1 { 108 } else { 8 };
        let cell_count = header.cell_count();

        Ok(Some(LeafCellIter::new(self.buffer, cell_pointer_start, cell_count)))
    }

    /// Get an iterator over interior cells in this page (zero-copy)
    /// Returns None if this page is not an interior page
    pub fn as_interior_cells(&self) -> Result<Option<InteriorCellIter<'_>>> {
        let header = self.header()?;
        
        if !header.page_type()?.is_interior() {
            return Ok(None);
        }

        // For page 1: file header (100 bytes) + interior page header (12 bytes) = 112
        // For other pages: page header (12 bytes) = 12
        let cell_pointer_start = if self.page_num == 1 { 112 } else { 12 };
        let cell_count = header.cell_count();

        Ok(Some(InteriorCellIter::new(self.buffer, cell_pointer_start, cell_count)))
    }

    pub fn is_leaf(&self) -> Result<bool> {
        Ok(self.page_type()?.is_leaf())
    }

    pub fn is_interior(&self) -> Result<bool> {
        Ok(self.page_type()?.is_interior())
    }
}

/// Mutable reference-based B-tree page wrapper for read-write access
pub struct PageMut<'a> {
    page_num: u32,
    buffer: &'a mut [u8],
}

impl<'a> PageMut<'a> {
    /// Create a new mutable reference to a page buffer
    pub fn new(buffer: &'a mut [u8], page_num: u32) -> Result<Self> {
        if buffer.is_empty() {
            return Err(Error::ParseError("Empty buffer for page".into()));
        }
        Ok(Self { page_num, buffer })
    }

    pub fn page_num(&self) -> u32 {
        self.page_num
    }

    /// Get immutable reference to page header
    /// For page 1, the page header starts at offset 100 (after the file header)
    /// For other pages, it starts at offset 0
    pub fn header(&self) -> Result<PageHeaderRef<'_>> {
        let header_offset = if self.page_num == 1 { 100 } else { 0 };
        PageHeaderRef::new(&self.buffer[header_offset..])
    }

    /// Get mutable reference to page header
    /// For page 1, the page header starts at offset 100 (after the file header)
    /// For other pages, it starts at offset 0
    pub fn header_mut(&mut self) -> Result<PageHeaderMut<'_>> {
        let header_offset = if self.page_num == 1 { 100 } else { 0 };
        PageHeaderMut::new(&mut self.buffer[header_offset..])
    }

    /// Get page as immutable reference
    pub fn as_ref(&self) -> PageRef<'_> {
        PageRef {
            page_num: self.page_num,
            buffer: self.buffer,
        }
    }

    /// Parse all cells from this page (deprecated - use raw_cells() instead)
    pub fn cells(&self) -> Result<Vec<()>> {
        todo!("Use raw_cells() or as_leaf_cells()/as_interior_cells() iterators for zero-copy access")
    }

    /// Get an iterator over leaf cells in this page (zero-copy)
    /// Returns None if this page is not a leaf page
    pub fn as_leaf_cells(&mut self) -> Result<Option<LeafCellIter<'_>>> {
        let header = PageHeaderRef::new(self.buffer)?;
        
        if !header.page_type()?.is_leaf() {
            return Ok(None);
        }

        let header_size = header.header_size()?;
        let cell_pointer_start = if self.page_num == 1 { 100 } else { header_size };
        let cell_count = header.cell_count();

        Ok(Some(LeafCellIter::new(self.buffer, cell_pointer_start, cell_count)))
    }

    /// Get an iterator over interior cells in this page (zero-copy)
    /// Returns None if this page is not an interior page
    pub fn as_interior_cells(&mut self) -> Result<Option<InteriorCellIter<'_>>> {
        let header = PageHeaderRef::new(self.buffer)?;
        
        if !header.page_type()?.is_interior() {
            return Ok(None);
        }

        let header_size = header.header_size()?;
        let cell_pointer_start = if self.page_num == 1 { 100 } else { header_size };
        let cell_count = header.cell_count();

        Ok(Some(InteriorCellIter::new(self.buffer, cell_pointer_start, cell_count)))
    }

    pub fn is_leaf(&self) -> Result<bool> {
        self.as_ref().is_leaf()
    }

    pub fn is_interior(&self) -> Result<bool> {
        self.as_ref().is_interior()
    }

    /// Phase 7f: Write pre-serialized cell bytes directly into this page buffer
    /// 
    /// Rebuilds the page header and cell structure in-place without intermediate allocations.
    /// Cells must be pre-serialized; this method writes them directly to mmap.
    /// Eliminates Cell enum from the write path by working only with raw bytes.
    pub fn write_cells_bytes(&mut self, page_type: PageType, cell_bytes: &[&[u8]]) -> Result<()> {
        // Initialize page header (8 bytes for leaf, 12 for interior)
        let header_size = if page_type.is_leaf() { 8 } else { 12 };
        
        if self.buffer.len() < header_size {
            return Err(Error::ParseError("Buffer too small for page header".into()));
        }

        // Write page type
        self.buffer[0] = page_type as u8;
        
        // Write first freeblock (0 = no free blocks for now)
        self.buffer[1..3].copy_from_slice(&0u16.to_be_bytes());
        
        // Write cell count
        let cell_count = cell_bytes.len() as u16;
        self.buffer[3..5].copy_from_slice(&cell_count.to_be_bytes());
        
        // Write start of content area (initially just after header + cell pointers)
        let content_start = header_size as u16 + (cell_count * 2);
        self.buffer[5..7].copy_from_slice(&content_start.to_be_bytes());
        
        // Write fragmented free bytes (0 for now)
        self.buffer[7] = 0;
        
        // For interior pages, write right child pointer
        if !page_type.is_leaf() {
            if self.buffer.len() < 12 {
                return Err(Error::ParseError("Buffer too small for interior page header".into()));
            }
            // Right child pointer (0 for now - can be updated later)
            self.buffer[8..12].copy_from_slice(&0u32.to_be_bytes());
        }

        // Write cell pointers and pre-serialized cell bytes (Phase 7f: direct byte writing)
        let mut current_offset = content_start as usize;
        
        for (i, cell_data) in cell_bytes.iter().enumerate() {
            // Check we have space for this cell
            if current_offset + cell_data.len() > self.buffer.len() {
                return Err(Error::ParseError("Insufficient space for cell".into()));
            }
            
            // Write cell pointer (offset of this cell)
            let ptr_offset = header_size + (i * 2);
            self.buffer[ptr_offset..ptr_offset + 2]
                .copy_from_slice(&(current_offset as u16).to_be_bytes());
            
            // Write pre-serialized cell data directly (Phase 7f: eliminate serialization step)
            self.buffer[current_offset..current_offset + cell_data.len()]
                .copy_from_slice(cell_data);
            current_offset += cell_data.len();
        }

        Ok(())
    }

    /// Phase 7e compatibility: Write Cell objects (deprecated - use write_cells_bytes() instead)
    pub fn write_cells(&mut self, page_type: PageType, _cells: &[()]) -> Result<()> {
        todo!("Use write_cells_bytes() with pre-serialized cell bytes instead")
    }
}

/// Reference-based B-tree page header wrapper
/// Leaf header: 8 bytes, Interior header: 12 bytes
#[derive(Debug, Clone, Copy)]
pub struct PageHeaderRef<'a> {
    buffer: &'a [u8],
}

impl<'a> PageHeaderRef<'a> {
    /// Create a new reference to a page header buffer
    pub fn new(buffer: &'a [u8]) -> Result<Self> {
        if buffer.len() < 8 {
            return Err(Error::ParseError("Buffer too small for page header".into()));
        }
        Ok(Self { buffer })
    }

    pub fn page_type(&self) -> Result<PageType> {
        PageType::from_u8(self.buffer[0])
    }

    pub fn first_freeblock(&self) -> u16 {
        u16::from_be_bytes([self.buffer[1], self.buffer[2]])
    }

    pub fn cell_count(&self) -> u16 {
        u16::from_be_bytes([self.buffer[3], self.buffer[4]])
    }

    pub fn cell_start(&self) -> u16 {
        u16::from_be_bytes([self.buffer[5], self.buffer[6]])
    }

    pub fn fragmented_free(&self) -> u8 {
        self.buffer[7]
    }

    pub fn right_pointer(&self) -> Result<Option<u32>> {
        let page_type = self.page_type()?;
        if page_type.is_interior() {
            if self.buffer.len() < 12 {
                return Err(Error::ParseError("Buffer too small for interior page header".into()));
            }
            Ok(Some(u32::from_be_bytes([self.buffer[8], self.buffer[9], self.buffer[10], self.buffer[11]])))
        } else {
            Ok(None)
        }
    }

    pub fn header_size(&self) -> Result<usize> {
        let page_type = self.page_type()?;
        Ok(if page_type.is_interior() { 12 } else { 8 })
    }
}

/// Mutable reference-based B-tree page header wrapper
pub struct PageHeaderMut<'a> {
    buffer: &'a mut [u8],
}

impl<'a> PageHeaderMut<'a> {
    /// Create a new mutable reference to a page header buffer
    pub fn new(buffer: &'a mut [u8]) -> Result<Self> {
        if buffer.len() < 8 {
            return Err(Error::ParseError("Buffer too small for page header".into()));
        }
        Ok(Self { buffer })
    }

    /// Initialize header with defaults
    pub fn init(&mut self, page_type: PageType) {
        self.set_page_type(page_type);
        self.set_first_freeblock(0);
        self.set_cell_count(0);
        self.set_cell_start(0);
        self.set_fragmented_free(0);
    }

    pub fn as_ref(&self) -> PageHeaderRef<'_> {
        PageHeaderRef { buffer: self.buffer }
    }

    pub fn set_page_type(&mut self, value: PageType) {
        self.buffer[0] = value.to_u8();
    }

    pub fn set_first_freeblock(&mut self, value: u16) {
        self.buffer[1..3].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_cell_count(&mut self, value: u16) {
        self.buffer[3..5].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_cell_start(&mut self, value: u16) {
        self.buffer[5..7].copy_from_slice(&value.to_be_bytes());
    }

    pub fn set_fragmented_free(&mut self, value: u8) {
        self.buffer[7] = value;
    }

    pub fn set_right_pointer(&mut self, value: Option<u32>) -> Result<()> {
        if let Some(ptr) = value {
            if self.buffer.len() < 12 {
                return Err(Error::ParseError("Buffer too small for interior page header".into()));
            }
            self.buffer[8..12].copy_from_slice(&ptr.to_be_bytes());
        }
        Ok(())
    }

    pub fn header_size(&self) -> Result<usize> {
        self.as_ref().header_size()
    }
}

/// Page struct removed - use PageRef/PageMut for zero-copy access instead

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_type_conversions() {
        assert_eq!(PageType::from_u8(0x02).unwrap(), PageType::IndexInterior);
        assert_eq!(PageType::from_u8(0x05).unwrap(), PageType::TableInterior);
        assert_eq!(PageType::from_u8(0x0A).unwrap(), PageType::IndexLeaf);
        assert_eq!(PageType::from_u8(0x0D).unwrap(), PageType::TableLeaf);
    }

    #[test]
    fn test_page_type_properties() {
        assert!(PageType::TableLeaf.is_leaf());
        assert!(!PageType::TableLeaf.is_interior());
        assert!(PageType::TableInterior.is_interior());
        assert!(!PageType::TableInterior.is_leaf());
    }

    #[test]
    fn test_page_header_parse() -> Result<()> {
        let mut buffer = vec![0u8; 100];
        buffer[0] = 0x0D; // Table leaf
        buffer[3..5].copy_from_slice(&10u16.to_be_bytes()); // cell_count

        let header = PageHeaderRef::new(&buffer)?;
        assert_eq!(header.page_type()?, PageType::TableLeaf);
        assert_eq!(header.cell_count(), 10);

        Ok(())
    }

    #[test]
    fn test_page_header_set_get() -> Result<()> {
        let mut buffer = vec![0u8; 12];
        let mut header_mut = PageHeaderMut::new(&mut buffer)?;
        header_mut.init(PageType::TableLeaf);
        header_mut.set_cell_count(5);
        header_mut.set_first_freeblock(42);

        let header_ref = header_mut.as_ref();
        assert_eq!(header_ref.page_type()?, PageType::TableLeaf);
        assert_eq!(header_ref.cell_count(), 5);
        assert_eq!(header_ref.first_freeblock(), 42);

        Ok(())
    }
}
