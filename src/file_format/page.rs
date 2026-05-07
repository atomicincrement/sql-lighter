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

/// B-tree page header (8 bytes)
#[derive(Debug, Clone)]
pub struct PageHeader {
    pub page_type: PageType,
    pub first_freeblock: u16,
    pub cell_count: u16,
    pub cell_start: u16,
    pub fragmented_free: u8,
    pub right_pointer: Option<u32>, // Only for interior pages
}

impl PageHeader {
    pub fn parse(buffer: &[u8]) -> Result<Self> {
        if buffer.len() < 8 {
            return Err(Error::ParseError("Buffer too small for page header".into()));
        }

        let page_type = PageType::from_u8(buffer[0])?;
        let first_freeblock = u16::from_be_bytes([buffer[1], buffer[2]]);
        let cell_count = u16::from_be_bytes([buffer[3], buffer[4]]);
        let cell_start = u16::from_be_bytes([buffer[5], buffer[6]]);
        let fragmented_free = buffer[7];

        let right_pointer = if page_type.is_interior() {
            if buffer.len() < 12 {
                return Err(Error::ParseError("Buffer too small for interior page header".into()));
            }
            Some(u32::from_be_bytes([buffer[8], buffer[9], buffer[10], buffer[11]]))
        } else {
            None
        };

        Ok(Self {
            page_type,
            first_freeblock,
            cell_count,
            cell_start,
            fragmented_free,
            right_pointer,
        })
    }

    pub fn serialize(&self, _is_first_page: bool) -> Vec<u8> {
        let mut buffer = Vec::new();
        let _header_size = if self.page_type.is_interior() { 12 } else { 8 };

        buffer.push(self.page_type.to_u8());
        buffer.extend_from_slice(&self.first_freeblock.to_be_bytes());
        buffer.extend_from_slice(&self.cell_count.to_be_bytes());
        buffer.extend_from_slice(&self.cell_start.to_be_bytes());
        buffer.push(self.fragmented_free);

        if let Some(right_ptr) = self.right_pointer {
            buffer.extend_from_slice(&right_ptr.to_be_bytes());
        }

        buffer
    }

    pub fn header_size(&self) -> usize {
        if self.page_type.is_interior() {
            12
        } else {
            8
        }
    }
}

/// B-tree page
#[derive(Debug, Clone)]
pub struct Page {
    pub page_num: u32,
    pub header: PageHeader,
    pub cells: Vec<Cell>,
    pub raw_data: Vec<u8>,
}

impl Page {
    pub fn parse(buffer: &[u8], page_num: u32) -> Result<Self> {
        if buffer.is_empty() {
            return Err(Error::ParseError("Empty buffer for page".into()));
        }

        let header = PageHeader::parse(buffer)?;
        let header_size = header.header_size();

        // Cell pointers come after the header
        let cell_pointer_start = if page_num == 1 { 100 } else { header_size };
        let _cell_pointer_end = cell_pointer_start + (header.cell_count as usize) * 2;

        let mut cells = Vec::new();

        // Read cell pointers and cells
        for i in 0..header.cell_count as usize {
            let ptr_offset = cell_pointer_start + (i * 2);
            if ptr_offset + 2 > buffer.len() {
                return Err(Error::ParseError("Cell pointer out of bounds".into()));
            }

            let cell_offset = u16::from_be_bytes([buffer[ptr_offset], buffer[ptr_offset + 1]]) as usize;

            if cell_offset >= buffer.len() {
                return Err(Error::ParseError("Cell offset out of bounds".into()));
            }

            let cell = Cell::parse(&buffer[cell_offset..], &header)?;
            cells.push(cell);
        }

        Ok(Self {
            page_num,
            header,
            cells,
            raw_data: buffer.to_vec(),
        })
    }

    pub fn serialize(&self, page_size: usize) -> Result<Vec<u8>> {
        let mut buffer = vec![0u8; page_size];

        // For first page, file header takes first 100 bytes
        let header_start = if self.page_num == 1 { 100 } else { 0 };

        // Write page header
        let page_header = self.header.serialize(self.page_num == 1);
        buffer[header_start..header_start + page_header.len()].copy_from_slice(&page_header);

        // Calculate cell pointer array start
        let cell_ptr_start = if self.header.page_type.is_interior() {
            if self.page_num == 1 { 100 + 12 } else { 12 }
        } else {
            if self.page_num == 1 { 100 + 8 } else { 8 }
        };

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

    pub fn add_cell(&mut self, cell: Cell) {
        self.cells.push(cell);
        self.header.cell_count += 1;
    }

    pub fn remove_cell(&mut self, index: usize) -> Option<Cell> {
        if index < self.cells.len() {
            self.header.cell_count -= 1;
            Some(self.cells.remove(index))
        } else {
            None
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.header.page_type.is_leaf()
    }

    pub fn is_interior(&self) -> bool {
        self.header.page_type.is_interior()
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

        let header = PageHeader::parse(&buffer)?;
        assert_eq!(header.page_type, PageType::TableLeaf);
        assert_eq!(header.cell_count, 10);

        Ok(())
    }
}
