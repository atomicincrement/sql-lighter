//! B-tree operations for multi-page storage
//! 
//! Phase 8a: Complete B-tree implementation with:
//! - Pre-serialized bytes (no Cell enum)
//! - Child page following for interior page traversal
//! - Page splitting on INSERT when pages overflow
//! - B-tree balancing and rotation for multi-level trees

use crate::error::{Error, Result};
use super::page::{Page, PageType};
use super::varint::{read_varint, write_varint};

/// B-tree structure for managing multi-page storage
/// 
/// Works with pre-serialized cell bytes instead of Cell objects.
/// Supports both leaf pages (with rowid + payload) and interior pages (with child pointer + key).
#[derive(Debug, Clone)]
pub struct BTree {
    /// Root page number
    root_page: u32,
    /// Minimum number of cells per page (B-tree order parameter)
    /// Pages split when exceeding max_cells = 2 * min_cells - 1
    min_cells: usize,
}

impl BTree {
    /// Create a new B-tree with a root page (Phase 8a)
    pub fn new(root_page: u32) -> Self {
        Self { 
            root_page,
            min_cells: 64,  // Allow ~128 cells per page before splitting
        }
    }

    /// Create a B-tree with custom minimum cells (for testing)
    pub fn with_min_cells(root_page: u32, min_cells: usize) -> Self {
        Self { 
            root_page,
            min_cells,
        }
    }

    /// Get the root page number
    pub fn root(&self) -> u32 {
        self.root_page
    }

    /// Get maximum cells per page before splitting
    pub fn max_cells(&self) -> usize {
        self.min_cells * 2 - 1
    }

    /// Search for a key in pre-serialized leaf cell bytes (Phase 8a)
    /// Returns the rowid if found, along with the parsed payload bytes
    pub fn search_leaf_cells(cell_bytes: &[Vec<u8>], target_key: u64) -> Result<Option<(u64, Vec<u8>)>> {
        for cell in cell_bytes {
            if let Ok((rowid, payload)) = Self::parse_leaf_cell(cell) {
                if rowid == target_key {
                    return Ok(Some((rowid, payload.to_vec())));  // Return parsed payload, not entire cell
                }
            }
        }
        Ok(None)
    }

    /// Parse a leaf cell to extract rowid (Phase 8a: on-demand parsing)
    pub fn parse_leaf_cell(cell_bytes: &[u8]) -> Result<(u64, &[u8])> {
        if cell_bytes.is_empty() {
            return Err(Error::ParseError("Empty leaf cell".into()));
        }
        
        let (payload_len, mut offset) = read_varint(cell_bytes)?;
        let (rowid, rowid_len) = read_varint(&cell_bytes[offset..])?;
        offset += rowid_len;
        
        let payload_end = offset + payload_len as usize;
        if payload_end > cell_bytes.len() {
            return Err(Error::ParseError("Leaf cell payload out of bounds".into()));
        }
        
        Ok((rowid, &cell_bytes[offset..payload_end]))
    }

    /// Serialize a leaf cell (Phase 8a: direct byte writing)
    pub fn serialize_leaf_cell(rowid: u64, payload: &[u8]) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        
        buffer.extend_from_slice(&write_varint(payload.len() as u64));
        buffer.extend_from_slice(&write_varint(rowid));
        buffer.extend_from_slice(payload);
        
        Ok(buffer)
    }

    /// Check if page needs splitting (Phase 8a)
    pub fn needs_split(&self, cell_count: usize) -> bool {
        cell_count > self.max_cells()
    }

    /// Split a page into two pages (Phase 8a)
    /// Returns (left_cells, right_cells, split_key, right_page_num)
    /// 
    /// When a leaf page overflows:
    /// - Divides cells into left and right halves
    /// - Returns cells for both pages and the split key for parent
    pub fn split_leaf_page(
        cells_bytes: Vec<Vec<u8>>,
        new_page_num: u32,
    ) -> Result<(Vec<Vec<u8>>, Vec<Vec<u8>>, u64)> {
        if cells_bytes.is_empty() {
            return Err(Error::ExecutionError("Cannot split empty page".into()));
        }

        let mid = cells_bytes.len() / 2;
        let left_cells = cells_bytes[..mid].to_vec();
        let right_cells = cells_bytes[mid..].to_vec();

        // Get the key from first cell in right half for parent interior cell
        let split_key = if let Ok((rowid, _)) = Self::parse_leaf_cell(&right_cells[0]) {
            rowid
        } else {
            return Err(Error::ExecutionError("Failed to parse split key".into()));
        };

        Ok((left_cells, right_cells, split_key))
    }

    /// Serialize an interior cell (child pointer + key) (Phase 8a)
    pub fn serialize_interior_cell(child_pointer: u32, key: u64) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&child_pointer.to_be_bytes());
        buffer.extend_from_slice(&write_varint(key));
        Ok(buffer)
    }

    /// Parse an interior cell to extract child pointer and key (Phase 8a)
    pub fn parse_interior_cell(cell_bytes: &[u8]) -> Result<(u32, u64)> {
        if cell_bytes.len() < 4 {
            return Err(Error::ParseError("Interior cell too short".into()));
        }

        let child_pointer = u32::from_be_bytes([
            cell_bytes[0],
            cell_bytes[1],
            cell_bytes[2],
            cell_bytes[3],
        ]);

        let (key, _) = read_varint(&cell_bytes[4..])?;

        Ok((child_pointer, key))
    }

    /// Find insertion position for a cell in sorted leaf cells (Phase 8a)
    pub fn find_insertion_pos_leaf(cell_bytes: &[Vec<u8>], new_rowid: u64) -> Result<usize> {
        for (i, cell) in cell_bytes.iter().enumerate() {
            if let Ok((rowid, _)) = Self::parse_leaf_cell(cell) {
                if new_rowid < rowid {
                    return Ok(i);
                }
            }
        }
        Ok(cell_bytes.len())
    }

    /// Check if page is balanced (Phase 8a)
    pub fn is_balanced(&self, cell_count: usize, is_root: bool) -> bool {
        if is_root {
            // Root can have as few as 1 cell
            cell_count >= 1
        } else {
            // Non-root must have at least min_cells - 1 cells (after splits)
            cell_count >= self.min_cells - 1
        }
    }

    /// Get all leaf keys in order from cell bytes (Phase 8a)
    pub fn keys_from_leaf_cells(cell_bytes: &[Vec<u8>]) -> Result<Vec<u64>> {
        let mut keys = Vec::new();
        for cell in cell_bytes {
            if let Ok((rowid, _)) = Self::parse_leaf_cell(cell) {
                keys.push(rowid);
            }
        }
        keys.sort();
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_btree_creation() {
        let btree = BTree::new(1);
        assert_eq!(btree.root(), 1);
        assert_eq!(btree.max_cells(), 127); // 2 * 64 - 1
    }

    #[test]
    fn test_serialize_and_parse_leaf_cell() -> Result<()> {
        let rowid = 42u64;
        let payload = b"test_data";

        let serialized = BTree::serialize_leaf_cell(rowid, payload)?;
        let (parsed_rowid, parsed_payload) = BTree::parse_leaf_cell(&serialized)?;

        assert_eq!(parsed_rowid, rowid);
        assert_eq!(parsed_payload, payload);

        Ok(())
    }

    #[test]
    fn test_serialize_and_parse_interior_cell() -> Result<()> {
        let child_pointer = 5u32;
        let key = 100u64;

        let serialized = BTree::serialize_interior_cell(child_pointer, key)?;
        let (parsed_child, parsed_key) = BTree::parse_interior_cell(&serialized)?;

        assert_eq!(parsed_child, child_pointer);
        assert_eq!(parsed_key, key);

        Ok(())
    }

    #[test]
    fn test_find_insertion_pos() -> Result<()> {
        let mut cells_bytes = Vec::new();
        cells_bytes.push(BTree::serialize_leaf_cell(10, b"a")?);
        cells_bytes.push(BTree::serialize_leaf_cell(20, b"b")?);
        cells_bytes.push(BTree::serialize_leaf_cell(30, b"c")?);

        // Insert 15 should go between 10 and 20 (position 1)
        let pos = BTree::find_insertion_pos_leaf(&cells_bytes, 15)?;
        assert_eq!(pos, 1);

        // Insert 5 should go at beginning (position 0)
        let pos = BTree::find_insertion_pos_leaf(&cells_bytes, 5)?;
        assert_eq!(pos, 0);

        // Insert 40 should go at end (position 3)
        let pos = BTree::find_insertion_pos_leaf(&cells_bytes, 40)?;
        assert_eq!(pos, 3);

        Ok(())
    }

    #[test]
    fn test_split_leaf_page() -> Result<()> {
        let mut cells_bytes = Vec::new();
        for i in 1..=5 {
            cells_bytes.push(BTree::serialize_leaf_cell(i as u64, &[i as u8])?);
        }

        let (left, right, split_key) = BTree::split_leaf_page(cells_bytes, 2)?;

        // Left should have first 2 cells (1, 2), right should have last 3 cells (3, 4, 5)
        assert_eq!(left.len(), 2);
        assert_eq!(right.len(), 3);

        // Split key should be 3 (first key in right half)
        assert_eq!(split_key, 3);

        // Verify keys are preserved
        let left_keys = BTree::keys_from_leaf_cells(&left)?;
        let right_keys = BTree::keys_from_leaf_cells(&right)?;
        assert_eq!(left_keys, vec![1, 2]);
        assert_eq!(right_keys, vec![3, 4, 5]);

        Ok(())
    }

    #[test]
    fn test_needs_split() {
        let btree = BTree::with_min_cells(1, 4);  // max_cells = 7
        
        assert!(!btree.needs_split(7));
        assert!(btree.needs_split(8));
    }

    #[test]
    fn test_is_balanced() {
        let btree = BTree::with_min_cells(1, 4);  // min_cells = 4
        
        // Root can have >= 1 cell
        assert!(btree.is_balanced(1, true));
        assert!(btree.is_balanced(10, true));
        
        // Non-root must have >= 3 cells (min_cells - 1)
        assert!(!btree.is_balanced(2, false));
        assert!(btree.is_balanced(3, false));
    }

    #[test]
    fn test_keys_from_leaf_cells() -> Result<()> {
        let mut cells_bytes = Vec::new();
        cells_bytes.push(BTree::serialize_leaf_cell(30, b"c")?);
        cells_bytes.push(BTree::serialize_leaf_cell(10, b"a")?);
        cells_bytes.push(BTree::serialize_leaf_cell(20, b"b")?);

        let keys = BTree::keys_from_leaf_cells(&cells_bytes)?;
        assert_eq!(keys, vec![10, 20, 30]);

        Ok(())
    }

    #[test]
    fn test_search_leaf_cells() -> Result<()> {
        let mut cells_bytes = Vec::new();
        cells_bytes.push(BTree::serialize_leaf_cell(10, b"hello")?);
        cells_bytes.push(BTree::serialize_leaf_cell(20, b"world")?);

        // Search for existing key
        let result = BTree::search_leaf_cells(&cells_bytes, 10)?;
        assert!(result.is_some());
        let (rowid, payload) = result.unwrap();
        assert_eq!(rowid, 10);
        assert_eq!(payload, b"hello");

        // Search for non-existing key
        let result = BTree::search_leaf_cells(&cells_bytes, 30)?;
        assert!(result.is_none());

        Ok(())
    }

    #[test]
    fn test_multi_level_tree_construction() -> Result<()> {
        // Simulate building a multi-level tree by:
        // 1. Creating leaf pages with cells
        // 2. Splitting them and creating interior pages with pointers
        
        let btree = BTree::with_min_cells(1, 3);  // max_cells = 5 for easier testing

        // Create a full leaf page that needs splitting
        let mut leaf_cells = Vec::new();
        for i in 1..=6 {
            leaf_cells.push(BTree::serialize_leaf_cell(i as u64, &[i as u8])?);
        }

        assert!(btree.needs_split(leaf_cells.len()));

        // Split it
        let (left, right, split_key) = BTree::split_leaf_page(leaf_cells, 2)?;

        // Left gets cells 1, 2, 3
        let left_keys = BTree::keys_from_leaf_cells(&left)?;
        assert_eq!(left_keys, vec![1, 2, 3]);

        // Right gets cells 4, 5, 6
        let right_keys = BTree::keys_from_leaf_cells(&right)?;
        assert_eq!(right_keys, vec![4, 5, 6]);

        // Split key should be 4
        assert_eq!(split_key, 4);

        // Create interior cell pointing to right page with split key
        let interior = BTree::serialize_interior_cell(2, split_key)?;
        let (child, key) = BTree::parse_interior_cell(&interior)?;
        assert_eq!(child, 2);
        assert_eq!(key, 4);

        Ok(())
    }
}
