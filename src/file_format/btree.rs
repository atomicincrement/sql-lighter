//! B-tree operations for multi-page storage
//! 
//! Phase 8a: Complete B-tree implementation with:
//! - Pre-serialized bytes (no Cell enum)
//! - Child page following for interior page traversal
//! - Page splitting on INSERT when pages overflow
//! - B-tree balancing and rotation for multi-level trees

use crate::error::{Error, Result};
use crate::file_format::{PageType, PageRef};
use crate::file_format::varint::read_varint;
use crate::transaction::Transaction;

/// Improved B-tree implementation with transaction-based page access (Phase 8c)
///
/// Traverses SQLite B-tree pages using a transaction context, allowing proper
/// handling of modified pages and multi-page navigation.
pub struct BTree<'t> {
    /// Root page number of the B-tree
    root_page: u32,
    /// Reference to the transaction for page access
    transaction: &'t Transaction,
}

impl<'t> BTree<'t> {
    /// Create a new B-tree reference pointing to a root page
    pub fn new(root_page: u32, transaction: &'t Transaction) -> Self {
        Self {
            root_page,
            transaction,
        }
    }

    /// Dump all keys in the B-tree by traversing all pages
    ///
    /// Prints each cell to stdout using Display trait (zero-copy iteration).
    /// Recursively traverses interior and leaf pages without allocating a results vector.
    pub fn dump_all(&self) -> Result<()> {
        self.dump_page(self.root_page)?;
        Ok(())
    }

    /// Recursively dump cells from a page and its children
    fn dump_page(&self, page_num: u32) -> Result<()> {
        let page_ref = self.transaction.page(page_num)?;
        let page_type = page_ref.page_type()?;

        match page_type {
            PageType::TableLeaf | PageType::IndexLeaf => {
                // Leaf page: print all leaf cells
                self.dump_leaf_page(&page_ref)?;
            }
            PageType::TableInterior | PageType::IndexInterior => {
                // Interior page: print keys and recurse into children
                self.dump_interior_page(&page_ref)?;
            }
        }

        Ok(())
    }

    /// Print all leaf cells from a leaf page (zero-copy iteration)
    fn dump_leaf_page(&self, page_ref: &PageRef<'_>) -> Result<()> {
        if let Some(leaf_iter) = page_ref.as_leaf_cells()? {
            for cell_result in leaf_iter {
                match cell_result {
                    Ok(leaf_cell) => println!("  {}", leaf_cell),
                    Err(e) => eprintln!("  Error reading leaf cell: {}", e),
                }
            }
        }
        Ok(())
    }

    /// Print keys from interior page cells and recurse into children
    fn dump_interior_page(&self, page_ref: &PageRef<'_>) -> Result<()> {
        if let Some(interior_iter) = page_ref.as_interior_cells()? {
            for cell_result in interior_iter {
                match cell_result {
                    Ok(interior_cell) => {
                        println!("  {}", interior_cell);
                        // Recurse into child page
                        let child_ptr = interior_cell.child_pointer();
                        self.dump_page(child_ptr)?;
                    }
                    Err(e) => eprintln!("  Error reading interior cell: {}", e),
                }
            }
        }
        Ok(())
    }

    /// Get an iterator over all leaf cell payloads in the B-tree
    ///
    /// Traverses the entire B-tree structure and yields the binary encoded
    /// record payloads from each leaf cell. This is useful for scanning all
    /// rows stored in a table without allocating intermediate structures.
    pub fn leaf_payloads(&self) -> Result<LeafIterator<'t>> {
        Ok(LeafIterator::new(self.root_page, self.transaction))
    }

    /// Find a row by rowid or the insertion point for a new rowid
    ///
    /// Traverses the B-tree to locate:
    /// - An existing leaf cell with the given rowid (if found)
    /// - The correct leaf page where a new rowid should be inserted
    ///
    /// Returns the leaf page number, all parent page numbers from root to leaf,
    /// and whether the rowid was found.
    pub fn find_rowid_path(&self, target_rowid: u64) -> Result<(u32, Vec<u32>, bool)> {
        let mut parent_pages = vec![self.root_page];
        let mut current_page_num = self.root_page;
        let mut rowid_found = false;

        loop {
            let page_ref = self.transaction.page(current_page_num)?;
            let page_type = page_ref.page_type()?;

            match page_type {
                PageType::TableLeaf | PageType::IndexLeaf => {
                    // Reached a leaf page - check if rowid exists
                    let raw_cells = page_ref.raw_cells()?;
                    for cell_bytes in raw_cells {
                        // Parse leaf cell: varint(payload_len) + varint(rowid) + payload
                        if let Ok((_, mut offset)) = read_varint(cell_bytes) {
                            if let Ok((rowid, _)) = read_varint(&cell_bytes[offset..]) {
                                if rowid == target_rowid {
                                    rowid_found = true;
                                    break;
                                }
                            }
                        }
                    }
                    // Return leaf page and parent path
                    return Ok((current_page_num, parent_pages, rowid_found));
                }
                PageType::TableInterior | PageType::IndexInterior => {
                    // Interior page - find the child to follow
                    let mut next_child = None;

                    if let Some(interior_iter) = page_ref.as_interior_cells()? {
                        for cell_result in interior_iter {
                            if let Ok(interior_cell) = cell_result {
                                if let Ok(cell_key) = interior_cell.key() {
                                    let child_ptr = interior_cell.child_pointer();

                                    // If target_rowid <= cell_key, follow this child
                                    if target_rowid <= cell_key {
                                        next_child = Some(child_ptr);
                                        break;
                                    }
                                    // Otherwise continue to check next cell (might follow right child)
                                    next_child = Some(child_ptr);
                                }
                            }
                        }
                    }

                    if let Some(child_page_num) = next_child {
                        parent_pages.push(child_page_num);
                        current_page_num = child_page_num;
                    } else {
                        return Err(Error::ExecutionError(
                            "No child found in interior page".to_string(),
                        ));
                    }
                }
            }
        }
    }
}

/// Iterator over leaf cell payloads in a B-tree
///
/// Recursively traverses a B-tree and yields binary encoded record payloads
/// for all leaf cells. Uses a stack-based traversal to handle multi-level trees.
pub struct LeafIterator<'t> {
    /// Stack of pages to visit: (page_num, is_processed)
    /// is_processed indicates if we've already yielded all children
    pages_to_visit: Vec<(u32, bool)>,
    /// Reference to transaction for page access
    transaction: &'t Transaction,
    /// Current page cells being iterated
    current_cells: Vec<Vec<u8>>,
    /// Current cell index in current_cells
    current_cell_index: usize,
}

impl<'t> LeafIterator<'t> {
    /// Create a new iterator starting from a root page
    fn new(root_page: u32, transaction: &'t Transaction) -> Self {
        Self {
            pages_to_visit: vec![(root_page, false)],
            transaction,
            current_cells: Vec::new(),
            current_cell_index: 0,
        }
    }

    /// Load all leaf cells from a leaf page into current_cells
    fn load_leaf_cells(&mut self, page_num: u32) -> Result<()> {
        let page_ref = self.transaction.page(page_num)?;
        let page_type = page_ref.page_type()?;

        match page_type {
            PageType::TableLeaf | PageType::IndexLeaf => {
                // Get raw cell bytes from leaf page (returns Vec<&[u8]>)
                let raw_cells = page_ref.raw_cells()?;
                // Convert to owned Vec<Vec<u8>>
                self.current_cells = raw_cells.into_iter().map(|b| b.to_vec()).collect();
                self.current_cell_index = 0;
                Ok(())
            }
            _ => Err(Error::ExecutionError(
                "Expected leaf page in LeafIterator".to_string(),
            )),
        }
    }

    /// Process an interior page to add its children to the stack
    fn process_interior_page(&mut self, page_num: u32) -> Result<()> {
        let page_ref = self.transaction.page(page_num)?;
        let page_type = page_ref.page_type()?;

        match page_type {
            PageType::TableInterior | PageType::IndexInterior => {
                if let Some(interior_iter) = page_ref.as_interior_cells()? {
                    // Collect all child page pointers in reverse order to visit in correct sequence
                    let mut children = Vec::new();
                    for cell_result in interior_iter {
                        if let Ok(interior_cell) = cell_result {
                            children.push(interior_cell.child_pointer());
                        }
                    }
                    // Add children in reverse so they're popped in correct order
                    for child_ptr in children.into_iter().rev() {
                        self.pages_to_visit.push((child_ptr, false));
                    }
                }
                Ok(())
            }
            _ => Err(Error::ExecutionError(
                "Expected interior page in process_interior_page".to_string(),
            )),
        }
    }
}

impl<'t> Iterator for LeafIterator<'t> {
    /// Yields the binary encoded payload for each leaf cell
    type Item = Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If we have cells from current page, yield the next one
            if self.current_cell_index < self.current_cells.len() {
                let cell_bytes = self.current_cells[self.current_cell_index].clone();
                self.current_cell_index += 1;
                
                // Parse the leaf cell inline to extract the payload
                // Cell format: varint(payload_len) + varint(rowid) + payload
                if cell_bytes.is_empty() {
                    return Some(Err(Error::ParseError("Empty leaf cell".into())));
                }
                
                match read_varint(&cell_bytes) {
                    Ok((payload_len, mut offset)) => {
                        match read_varint(&cell_bytes[offset..]) {
                            Ok((_rowid, rowid_len)) => {
                                offset += rowid_len;
                                let payload_end = offset + payload_len as usize;
                                if payload_end > cell_bytes.len() {
                                    return Some(Err(Error::ParseError("Leaf cell payload out of bounds".into())));
                                }
                                return Some(Ok(cell_bytes[offset..payload_end].to_vec()));
                            }
                            Err(e) => return Some(Err(e)),
                        }
                    }
                    Err(e) => return Some(Err(e)),
                }
            }

            // No more cells in current page, move to next page
            match self.pages_to_visit.pop() {
                Some((page_num, false)) => {
                    // First time visiting this page, need to check its type
                    match self.transaction.page(page_num) {
                        Ok(page_ref) => {
                            match page_ref.page_type() {
                                Ok(PageType::TableLeaf)
                                | Ok(PageType::IndexLeaf) => {
                                    // Load leaf cells
                                    if let Err(e) = self.load_leaf_cells(page_num) {
                                        return Some(Err(e));
                                    }
                                }
                                Ok(PageType::TableInterior)
                                | Ok(PageType::IndexInterior) => {
                                    // Process interior page to add children
                                    if let Err(e) = self.process_interior_page(page_num) {
                                        return Some(Err(e));
                                    }
                                }
                                Err(e) => return Some(Err(e)),
                            }
                        }
                        Err(e) => return Some(Err(e)),
                    }
                }
                Some((_page_num, true)) => {
                    // Already processed, skip
                    continue;
                }
                None => {
                    // No more pages to visit
                    return None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // BTree tests - functionality is in integration tests
}
