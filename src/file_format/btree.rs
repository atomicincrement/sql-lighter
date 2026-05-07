//! B-tree operations

use crate::error::Result;
use super::page::Page;
use super::cell::Cell;

/// B-tree structure for managing pages
pub struct BTree {
    root_page: u32,
}

impl BTree {
    /// Create a new B-tree with a root page
    pub fn new(root_page: u32) -> Self {
        Self { root_page }
    }

    /// Get the root page number
    pub fn root(&self) -> u32 {
        self.root_page
    }

    /// Search for a key in the tree
    pub fn search(&self, page: &Page, key: u64) -> Result<Option<Cell>> {
        // Find the appropriate cell
        for cell in &page.cells {
            if cell.get_key() == key {
                return Ok(Some(cell.clone()));
            }
        }

        // If not found on this page
        if page.is_interior() {
            // In a real implementation, we would follow the child pointer
            // For now, return not found
            Ok(None)
        } else {
            Ok(None)
        }
    }

    /// Insert a key-value pair
    pub fn insert(&self, page: &mut Page, cell: Cell) -> Result<()> {
        // Find insertion point
        let key = cell.get_key();
        let mut insert_pos = page.cells.len();

        for (i, existing_cell) in page.cells.iter().enumerate() {
            if key < existing_cell.get_key() {
                insert_pos = i;
                break;
            }
        }

        page.cells.insert(insert_pos, cell);
        page.header.cell_count += 1;

        Ok(())
    }

    /// Delete a key from the tree
    pub fn delete(&self, page: &mut Page, key: u64) -> Result<bool> {
        for (i, cell) in page.cells.iter().enumerate() {
            if cell.get_key() == key {
                page.cells.remove(i);
                page.header.cell_count -= 1;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Check if page is balanced
    pub fn is_balanced(page: &Page) -> bool {
        // Simple check: leaf pages should have cells
        page.cells.len() > 0
    }

    /// Rebalance pages if needed
    pub fn rebalance_if_needed(page: &mut Page, min_cells: usize) -> Result<bool> {
        if page.cells.len() < min_cells && !page.cells.is_empty() {
            // In a real implementation, would merge or redistribute with siblings
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get all keys in order
    pub fn keys_in_order(page: &Page) -> Vec<u64> {
        let mut keys: Vec<_> = page.cells.iter().map(|c| c.get_key()).collect();
        keys.sort();
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::page::{PageHeader, PageType};

    fn create_test_page() -> Page {
        Page {
            page_num: 1,
            header: PageHeader {
                page_type: PageType::TableLeaf,
                first_freeblock: 0,
                cell_count: 0,
                cell_start: 0,
                fragmented_free: 0,
                right_pointer: None,
            },
            cells: Vec::new(),
            raw_data: Vec::new(),
        }
    }

    #[test]
    fn test_btree_creation() {
        let btree = BTree::new(1);
        assert_eq!(btree.root(), 1);
    }

    #[test]
    fn test_insert_and_search() -> Result<()> {
        let btree = BTree::new(1);
        let mut page = create_test_page();

        let cell = Cell::Leaf {
            rowid: 42,
            payload: vec![1, 2, 3],
        };

        btree.insert(&mut page, cell)?;
        assert_eq!(page.cells.len(), 1);

        let found = btree.search(&page, 42)?;
        assert!(found.is_some());

        Ok(())
    }

    #[test]
    fn test_delete() -> Result<()> {
        let btree = BTree::new(1);
        let mut page = create_test_page();

        let cell = Cell::Leaf {
            rowid: 99,
            payload: vec![],
        };

        btree.insert(&mut page, cell)?;
        assert_eq!(page.cells.len(), 1);

        let deleted = btree.delete(&mut page, 99)?;
        assert!(deleted);
        assert_eq!(page.cells.len(), 0);

        Ok(())
    }

    #[test]
    fn test_keys_in_order() {
        let _btree = BTree::new(1);
        let mut page = create_test_page();

        page.cells.push(Cell::Leaf {
            rowid: 30,
            payload: vec![],
        });
        page.cells.push(Cell::Leaf {
            rowid: 10,
            payload: vec![],
        });
        page.cells.push(Cell::Leaf {
            rowid: 20,
            payload: vec![],
        });

        let keys = BTree::keys_in_order(&page);
        assert_eq!(keys, vec![10, 20, 30]);
    }
}
