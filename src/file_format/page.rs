//! B-tree page handling

use crate::error::{Error, Result};
use super::cell::Cell;

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

/// B-tree page
#[derive(Debug, Clone)]
pub struct Page {
    pub page_num: u32,
    pub header_buffer: Vec<u8>,  // 8 or 12 bytes depending on page type
    pub cells: Vec<Cell>,
    pub raw_data: Vec<u8>,
}

impl Page {
    pub fn parse(buffer: &[u8], page_num: u32) -> Result<Self> {
        if buffer.is_empty() {
            return Err(Error::ParseError("Empty buffer for page".into()));
        }

        let header_ref = PageHeaderRef::new(buffer)?;
        let header_size = header_ref.header_size()?;

        // Extract and store the header
        let header_buffer = buffer[0..header_size].to_vec();

        // Cell pointers come after the header
        let cell_pointer_start = if page_num == 1 { 100 } else { header_size };
        let cell_count = header_ref.cell_count();
        let _cell_pointer_end = cell_pointer_start + (cell_count as usize) * 2;

        let mut cells = Vec::new();

        // Read cell pointers and cells
        for i in 0..cell_count as usize {
            let ptr_offset = cell_pointer_start + (i * 2);
            if ptr_offset + 2 > buffer.len() {
                return Err(Error::ParseError("Cell pointer out of bounds".into()));
            }

            let cell_offset = u16::from_be_bytes([buffer[ptr_offset], buffer[ptr_offset + 1]]) as usize;

            if cell_offset >= buffer.len() {
                return Err(Error::ParseError("Cell offset out of bounds".into()));
            }

            let cell = Cell::parse(&buffer[cell_offset..], &header_ref)?;
            cells.push(cell);
        }

        Ok(Self {
            page_num,
            header_buffer,
            cells,
            raw_data: buffer.to_vec(),
        })
    }

    /// Get immutable reference to page header
    pub fn header(&self) -> Result<PageHeaderRef<'_>> {
        PageHeaderRef::new(&self.header_buffer)
    }

    /// Get mutable reference to page header
    pub fn header_mut(&mut self) -> Result<PageHeaderMut<'_>> {
        PageHeaderMut::new(&mut self.header_buffer)
    }

    pub fn serialize(&self, page_size: usize) -> Result<Vec<u8>> {
        let header_ref = self.header()?;
        let header_size = header_ref.header_size()?;
        let mut buffer = vec![0u8; page_size];

        // For first page, file header takes first 100 bytes
        let header_start = if self.page_num == 1 { 100 } else { 0 };

        // Write page header
        buffer[header_start..header_start + header_size].copy_from_slice(&self.header_buffer);

        // Calculate cell pointer array start
        let cell_ptr_start = header_start + header_size;

        // Write cells (from the end of the page, working backwards)
        let mut current_cell_offset = page_size;

        for (i, cell) in self.cells.iter().enumerate() {
            let cell_data = cell.serialize()?;
            current_cell_offset -= cell_data.len();

            buffer[current_cell_offset..current_cell_offset + cell_data.len()].copy_from_slice(&cell_data);

            // Write cell pointer
            let ptr_offset = cell_ptr_start + (i * 2);
            buffer[ptr_offset..ptr_offset + 2].copy_from_slice(&(current_cell_offset as u16).to_be_bytes());
        }

        Ok(buffer)
    }

    pub fn add_cell(&mut self, cell: Cell) -> Result<()> {
        self.cells.push(cell);
        let mut header_mut = self.header_mut()?;
        let count = header_mut.as_ref().cell_count();
        header_mut.set_cell_count(count + 1);
        Ok(())
    }

    pub fn remove_cell(&mut self, index: usize) -> Result<Option<Cell>> {
        if index < self.cells.len() {
            let mut header_mut = self.header_mut()?;
            let count = header_mut.as_ref().cell_count();
            header_mut.set_cell_count(count - 1);
            Ok(Some(self.cells.remove(index)))
        } else {
            Ok(None)
        }
    }

    pub fn is_leaf(&self) -> Result<bool> {
        let header = self.header()?;
        Ok(header.page_type()?.is_leaf())
    }

    pub fn is_interior(&self) -> Result<bool> {
        let header = self.header()?;
        Ok(header.page_type()?.is_interior())
    }
}

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
