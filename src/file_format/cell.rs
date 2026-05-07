//! B-tree cells

use crate::error::{Error, Result};
use super::varint::read_varint;
use super::page::{PageHeaderRef, PageType};

/// Zero-copy reference to a leaf cell
#[derive(Debug, Clone, Copy)]
pub struct LeafCellRef<'a> {
    buffer: &'a [u8],
}

impl<'a> LeafCellRef<'a> {
    /// Parse a leaf cell from a buffer
    pub fn new(buffer: &'a [u8]) -> Result<Self> {
        if buffer.is_empty() {
            return Err(Error::ParseError("Empty buffer for leaf cell".into()));
        }
        Ok(Self { buffer })
    }

    /// Get the rowid without copying
    pub fn rowid(&self) -> Result<u64> {
        let (_payload_len, offset) = read_varint(self.buffer)?;
        let (rowid, _) = read_varint(&self.buffer[offset..])?;
        Ok(rowid)
    }

    /// Get the payload as a slice (zero-copy)
    pub fn payload(&self) -> Result<&'a [u8]> {
        let (payload_len, mut offset) = read_varint(self.buffer)?;
        let (_, rowid_len) = read_varint(&self.buffer[offset..])?;
        offset += rowid_len;

        let payload_end = offset + payload_len as usize;
        if payload_end > self.buffer.len() {
            return Err(Error::ParseError("Leaf cell payload out of bounds".into()));
        }

        Ok(&self.buffer[offset..payload_end])
    }
}

/// Zero-copy reference to an interior cell
#[derive(Debug, Clone, Copy)]
pub struct InteriorCellRef<'a> {
    buffer: &'a [u8],
}

impl<'a> InteriorCellRef<'a> {
    /// Parse an interior cell from a buffer
    pub fn new(buffer: &'a [u8]) -> Result<Self> {
        if buffer.len() < 4 {
            return Err(Error::ParseError("Interior cell too short".into()));
        }
        Ok(Self { buffer })
    }

    /// Get the child page pointer without copying
    pub fn child_pointer(&self) -> u32 {
        u32::from_be_bytes([self.buffer[0], self.buffer[1], self.buffer[2], self.buffer[3]])
    }

    /// Get the key without copying
    pub fn key(&self) -> Result<u64> {
        let (key, _) = read_varint(&self.buffer[4..])?;
        Ok(key)
    }
}

/// Iterator over leaf cells in a page (zero-copy)
pub struct LeafCellIter<'a> {
    page_buffer: &'a [u8],
    cell_pointers_start: usize,
    cell_count: u16,
    current_index: u16,
}

impl<'a> LeafCellIter<'a> {
    pub(crate) fn new(
        page_buffer: &'a [u8],
        cell_pointers_start: usize,
        cell_count: u16,
    ) -> Self {
        Self {
            page_buffer,
            cell_pointers_start,
            cell_count,
            current_index: 0,
        }
    }
}

impl<'a> Iterator for LeafCellIter<'a> {
    type Item = Result<LeafCellRef<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.cell_count {
            return None;
        }

        let ptr_offset = self.cell_pointers_start + (self.current_index as usize * 2);
        if ptr_offset + 2 > self.page_buffer.len() {
            self.current_index = self.cell_count; // Skip remaining
            return Some(Err(Error::ParseError("Cell pointer out of bounds".into())));
        }

        let cell_offset = u16::from_be_bytes([
            self.page_buffer[ptr_offset],
            self.page_buffer[ptr_offset + 1],
        ]) as usize;

        if cell_offset >= self.page_buffer.len() {
            self.current_index = self.cell_count;
            return Some(Err(Error::ParseError("Cell offset out of bounds".into())));
        }

        self.current_index += 1;

        match LeafCellRef::new(&self.page_buffer[cell_offset..]) {
            Ok(cell_ref) => Some(Ok(cell_ref)),
            Err(e) => Some(Err(e)),
        }
    }
}

/// Iterator over interior cells in a page (zero-copy)
pub struct InteriorCellIter<'a> {
    page_buffer: &'a [u8],
    cell_pointers_start: usize,
    cell_count: u16,
    current_index: u16,
}

impl<'a> InteriorCellIter<'a> {
    pub(crate) fn new(
        page_buffer: &'a [u8],
        cell_pointers_start: usize,
        cell_count: u16,
    ) -> Self {
        Self {
            page_buffer,
            cell_pointers_start,
            cell_count,
            current_index: 0,
        }
    }
}

impl<'a> Iterator for InteriorCellIter<'a> {
    type Item = Result<InteriorCellRef<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.cell_count {
            return None;
        }

        let ptr_offset = self.cell_pointers_start + (self.current_index as usize * 2);
        if ptr_offset + 2 > self.page_buffer.len() {
            self.current_index = self.cell_count; // Skip remaining
            return Some(Err(Error::ParseError("Cell pointer out of bounds".into())));
        }

        let cell_offset = u16::from_be_bytes([
            self.page_buffer[ptr_offset],
            self.page_buffer[ptr_offset + 1],
        ]) as usize;

        if cell_offset >= self.page_buffer.len() {
            self.current_index = self.cell_count;
            return Some(Err(Error::ParseError("Cell offset out of bounds".into())));
        }

        self.current_index += 1;

        match InteriorCellRef::new(&self.page_buffer[cell_offset..]) {
            Ok(cell_ref) => Some(Ok(cell_ref)),
            Err(e) => Some(Err(e)),
        }
    }
}

/// A B-tree cell (either interior or leaf)
#[derive(Debug, Clone)]
pub enum Cell {
    /// Interior cell with a child page pointer
    Interior {
        key: u64,
        child_pointer: u32,
    },
    /// Leaf cell with a payload
    Leaf {
        rowid: u64,
        payload: Vec<u8>,
    },
}

impl Cell {
    pub fn parse(buffer: &[u8], header: &PageHeaderRef) -> Result<Self> {
        if buffer.is_empty() {
            return Err(Error::ParseError("Empty buffer for cell".into()));
        }

        let page_type = header.page_type()?;
        match page_type {
            PageType::IndexInterior | PageType::TableInterior => {
                // Interior cell: 4-byte child pointer + key
                if buffer.len() < 4 {
                    return Err(Error::ParseError("Interior cell too short".into()));
                }

                let child_pointer = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
                let (key, _) = read_varint(&buffer[4..])?;

                Ok(Cell::Interior {
                    key,
                    child_pointer,
                })
            }
            PageType::IndexLeaf | PageType::TableLeaf => {
                // Leaf cell: varint payload length + varint rowid + payload
                let (payload_len, mut offset) = read_varint(buffer)?;
                let (rowid, rowid_len) = read_varint(&buffer[offset..])?;
                offset += rowid_len;

                let payload_end = offset + payload_len as usize;
                if payload_end > buffer.len() {
                    return Err(Error::ParseError("Leaf cell payload out of bounds".into()));
                }

                let payload = buffer[offset..payload_end].to_vec();

                Ok(Cell::Leaf { rowid, payload })
            }
        }
    }

    pub fn serialize(&self) -> Result<Vec<u8>> {
        match self {
            Cell::Interior {
                key,
                child_pointer,
            } => {
                let mut buffer = Vec::new();
                buffer.extend_from_slice(&child_pointer.to_be_bytes());

                let key_varint = super::varint::write_varint(*key);
                buffer.extend_from_slice(&key_varint);

                Ok(buffer)
            }
            Cell::Leaf { rowid, payload } => {
                let mut buffer = Vec::new();

                let payload_varint = super::varint::write_varint(payload.len() as u64);
                buffer.extend_from_slice(&payload_varint);

                let rowid_varint = super::varint::write_varint(*rowid);
                buffer.extend_from_slice(&rowid_varint);

                buffer.extend_from_slice(payload);

                Ok(buffer)
            }
        }
    }

    pub fn get_key(&self) -> u64 {
        match self {
            Cell::Interior { key, .. } => *key,
            Cell::Leaf { rowid, .. } => *rowid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_interior_cell() -> Result<()> {
        let cell = Cell::Interior {
            key: 123,
            child_pointer: 1,
        };

        let serialized = cell.serialize()?;
        assert!(serialized.len() > 4);

        Ok(())
    }

    #[test]
    fn test_serialize_leaf_cell() -> Result<()> {
        let payload = vec![1, 2, 3, 4];
        let cell = Cell::Leaf {
            rowid: 42,
            payload: payload.clone(),
        };

        let serialized = cell.serialize()?;
        assert!(serialized.len() > 0);

        Ok(())
    }

    #[test]
    fn test_interior_cell_get_key() {
        let cell = Cell::Interior {
            key: 999,
            child_pointer: 2,
        };
        assert_eq!(cell.get_key(), 999);
    }

    #[test]
    fn test_leaf_cell_get_key() {
        let cell = Cell::Leaf {
            rowid: 555,
            payload: vec![],
        };
        assert_eq!(cell.get_key(), 555);
    }
}
