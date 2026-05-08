//! B-tree cells

use crate::error::{Error, Result};
use super::varint::read_varint;
use super::page::{PageHeaderRef, PageType};
use super::record::Record;
use std::fmt;
use crate::types::Value;

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

    /// Get the total number of bytes of payload (including any overflow)
    /// According to SQLite spec: first varint in leaf cell
    pub fn payload_len(&self) -> Result<u64> {
        let (payload_len, _) = read_varint(self.buffer)?;
        Ok(payload_len)
    }

    /// Get the rowid without copying
    /// According to SQLite spec: second varint in leaf cell (after payload_len)
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

impl<'a> fmt::Display for LeafCellRef<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display in order per SQLite spec: payload_len, rowid, then decoded record values
        match self.payload_len() {
            Ok(payload_len) => {
                write!(f, "LeafCell {{ payload_len: {}, rowid: ", payload_len)?;
                match self.rowid() {
                    Ok(rowid) => {
                        write!(f, "{}, values: ", rowid)?;
                        match self.payload() {
                            Ok(payload) => {
                                // Decode the record format per SQLite spec section 2.1
                                match Record::parse(payload) {
                                    Ok(record) => {
                                        write!(f, "[")?;
                                        for (i, value) in record.columns.iter().enumerate() {
                                            if i > 0 {
                                                write!(f, ", ")?;
                                            }
                                            // Format each value according to its type
                                            match value {
                                                Value::Null => write!(f, "NULL")?,
                                                Value::Integer(i) => write!(f, "{}", i)?,
                                                Value::Real(r) => write!(f, "{}", r)?,
                                                Value::Text(s) => write!(f, "'{}'", s)?,
                                                Value::Blob(b) => {
                                                    write!(f, "Blob(")?;
                                                    // Show first 20 bytes of blob as hex
                                                    let to_show = b.len().min(20);
                                                    for byte in &b[..to_show] {
                                                        write!(f, "{:02x}", byte)?;
                                                    }
                                                    if b.len() > 20 {
                                                        write!(f, "...")?;
                                                    }
                                                    write!(f, ")")?;
                                                }
                                            }
                                        }
                                        write!(f, "] }}")
                                    }
                                    Err(_) => {
                                        // Fallback to hex dump if record parsing fails
                                        write!(f, "[")?;
                                        let max_bytes = 80;
                                        let to_show = payload.len().min(max_bytes);
                                        for byte in &payload[..to_show] {
                                            write!(f, "{:02x}", byte)?;
                                        }
                                        if payload.len() > max_bytes {
                                            write!(f, "... ({} bytes total)", payload.len())?;
                                        }
                                        write!(f, "] }}")
                                    }
                                }
                            }
                            Err(_) => write!(f, "[error reading payload] }}"),
                        }
                    }
                    Err(_) => write!(f, "[error reading rowid] }}"),
                }
            }
            Err(_) => write!(f, "LeafCell {{ [error reading payload_len] }}"),
        }
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
    /// According to SQLite spec: first 4 bytes of interior cell
    pub fn child_pointer(&self) -> u32 {
        u32::from_be_bytes([self.buffer[0], self.buffer[1], self.buffer[2], self.buffer[3]])
    }

    /// Get the key (or payload_len for index cells) without copying
    /// According to SQLite spec: varint after child_pointer in interior cell
    pub fn key(&self) -> Result<u64> {
        let (key, _) = read_varint(&self.buffer[4..])?;
        Ok(key)
    }
}

impl<'a> fmt::Display for InteriorCellRef<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display in order per SQLite spec: child_pointer, then key
        let child = self.child_pointer();
        write!(f, "InteriorCell {{ child_pointer: {}, key: ", child)?;
        match self.key() {
            Ok(key) => write!(f, "{} }}", key),
            Err(_) => write!(f, "[error reading key] }}"),
        }
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

/// A B-tree cell (either interior or leaf) - REMOVED
/// Use raw byte slices with zero-copy iterators instead
/// Use LeafCellRef/InteriorCellRef for zero-copy parsing or BTree::parse_leaf_cell/parse_interior_cell for direct byte parsing
#[derive(Debug, Clone)]
pub enum Cell {}

// Cell impl methods removed - use raw bytes instead

#[cfg(test)]
mod tests {
    // Cell tests removed - functionality migrated to zero-copy iterators
}
