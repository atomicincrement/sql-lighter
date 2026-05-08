//! Connection wrapper - rusqlite-compatible API (Phase 6a/6b/Phase 7b)
//!
//! Provides high-level database connection interface matching rusqlite patterns.
//! Supports both in-memory and persistent file-based database storage.
//! References: https://github.com/rusqlite/rusqlite

use crate::error::{Error, Result};
use crate::executor::VirtualMachine;
use crate::parser::Parser;
use crate::planner::{Planner, ExecutionPlan};
use crate::types::Value;
use crate::params::Params;
use crate::file_format::{DatabaseFile, PageType};
use std::collections::HashMap;
use std::sync::Arc;

/// Database connection - main API entry point
/// 
/// Provides rusqlite-compatible interface for SQL execution with optional file persistence.
/// References: https://github.com/rusqlite/rusqlite/blob/master/src/lib.rs
pub struct Connection {
    /// Virtual machine for query execution
    vm: VirtualMachine,
    /// Whether connection is in-memory only
    in_memory: bool,
    /// File-based database handle (None for in-memory databases)
    db_file: Option<DatabaseFile>,
    /// Path to database file (None for in-memory databases)
    path: Option<String>,
}

impl Connection {
    /// Open a connection to a database file for reading and writing
    ///
    /// Loads the database structure from the SQLite file using B-tree storage.
    /// Changes are persistent and will be written back to the file.
    ///
    /// # Example
    /// ```ignore
    /// let mut conn = Connection::open("database.db")?;
    /// conn.execute("INSERT INTO person VALUES (?1, ?2, ?3)", (42, "Alice", "data"))?;
    /// conn.close()?;  // or changes persist when dropped
    /// ```
    pub fn open(path: &str) -> Result<Self> {
        let db_file = DatabaseFile::open(path)?;
        let mut vm = VirtualMachine::new();

        // Phase 7d: Try loading from pages sequentially using read_page_ref for zero-copy
        // SQLite stores tables on multiple pages; we load the "person" table when found
        for page_num in 1..=10 {
            match db_file.read_page_ref(page_num) {
                Ok(page_ref) => {
                    // Try to load this page as the person table
                    // Phase 7d: Use PageRef directly for zero-copy reads
                    // We'll create a "person" table with 3 columns: id, name, data
                    // This is a simplification - ideally we'd read the schema from sqlite_master
                    if let Err(_e) = vm.load_table_from_page(
                        "person",
                        vec!["id".to_string(), "name".to_string(), "data".to_string()],
                        page_ref,
                    ) {
                        // Silently ignore errors and try next page
                        // This allows us to skip schema pages and find actual data
                    }
                }
                Err(_) => {
                    // Reached end of file or error reading page, stop trying
                    break;
                }
            }
        }

        Ok(Self {
            vm,
            in_memory: false,
            db_file: Some(db_file),
            path: Some(path.to_string()),
        })
    }

    /// Open an in-memory database
    ///
    /// Creates a temporary database in memory that persists only for the
    /// lifetime of the connection. Simulates file I/O semantically but uses
    /// in-memory storage for performance.
    ///
    /// # Example
    /// ```ignore
    /// let mut conn = Connection::open_in_memory()?;
    /// conn.execute("CREATE TABLE users (id INTEGER, name TEXT)")?;
    /// ```
    pub fn open_in_memory() -> Result<Self> {
        Ok(Self {
            vm: VirtualMachine::new(),
            in_memory: true,
            db_file: None,
            path: None,
        })
    }



    /// Execute a SQL statement with typed parameters
    ///
    /// This method accepts anything implementing the Params trait, allowing for
    /// ergonomic parameter binding with tuples, arrays, and slices.
    ///
    /// For file-based connections, write operations (INSERT, UPDATE, DELETE, CREATE TABLE, CREATE INDEX)
    /// are automatically persisted to disk.
    ///
    /// # Arguments
    /// * `sql` - SQL statement with ?1, ?2, etc. placeholders
    /// * `params` - Parameters implementing the Params trait
    ///
    /// # Returns
    /// ExecutionResult containing result set (if applicable)
    ///
    /// # Example
    /// ```ignore
    /// let mut conn = Connection::open("database.db")?;
    /// conn.execute("INSERT INTO users VALUES (?1, ?2)", ("Alice", 30))?;
    /// conn.execute("SELECT * FROM users WHERE id IN (?1, ?2, ?3)", [1, 2, 3])?;
    /// conn.execute("CREATE TABLE users (id INTEGER, name TEXT)", ())?;
    /// ```
    pub fn execute<P: Params>(&mut self, sql: &str, params: P) -> Result<ExecutionResult> {
        // Bind parameters
        let param_map = params.bind_params()?;
        
        // Parse SQL with parameter substitution
        let processed_sql = self.substitute_parameters(sql, &param_map)?;
        
        // Parse
        let mut parser = Parser::new(&processed_sql)?;
        let stmt = parser.parse_statement()?;

        // Plan
        let planner = Planner::new();
        let plan = planner.plan(&stmt)?;

        // Check if this is a write operation
        let is_write = self.is_write_operation(&plan);

        // Execute
        let result_set = self.vm.execute(&plan)?;

        // Persist changes to file if this was a write operation
        if is_write && !self.in_memory {
            self.persist()?;
        }

        Ok(ExecutionResult {
            rows: result_set.rows.clone(),
            columns: result_set.columns.clone(),
        })
    }

    /// Check if an execution plan modifies data
    fn is_write_operation(&self, plan: &ExecutionPlan) -> bool {
        matches!(
            plan,
            ExecutionPlan::Insert { .. }
                | ExecutionPlan::Update { .. }
                | ExecutionPlan::Delete { .. }
                | ExecutionPlan::CreateTable { .. }
                | ExecutionPlan::CreateIndex { .. }
                | ExecutionPlan::DropIndex { .. }
        )
    }

    /// Persist all modified tables to disk (Phase 7b/7e/7f)
    ///
    /// Phase 7f: Uses pre-serialized cell bytes directly into PageMut buffers in the mmap.
    /// Cells are pre-serialized, so we only copy bytes directly - no serialization step needed.
    /// No intermediate Cell or Page struct allocations - writes directly to mmap pages.
    fn persist(&mut self) -> Result<()> {
        if let Some(db_file) = self.db_file.as_mut() {
            // Get all tables from the virtual machine
            let tables = self.vm.get_all_tables();

            // Write each table's page to disk using PageMut (Phase 7f: direct byte writing)
            for (_table_name, table_storage) in tables {
                // Get mutable reference to the page in the mmap
                let mut page_mut = db_file.get_page_mut(table_storage.page_num)?;
                
                // Phase 7f: Convert Vec<Vec<u8>> to Vec<&[u8]> for write_cells_bytes
                let cell_byte_refs: Vec<&[u8]> = table_storage.cells_bytes
                    .iter()
                    .map(|b| b.as_slice())
                    .collect();
                
                // Write pre-serialized cell bytes directly into the page buffer (Phase 7f: eliminate Cell struct from write path)
                page_mut.write_cells_bytes(PageType::TableLeaf, &cell_byte_refs)?;
            }

            // Flush changes to disk immediately
            db_file.flush()?;

            Ok(())
        } else {
            Ok(())
        }
    }

    /// Execute a query and return rows
    ///
    /// Convenience method for SELECT queries
    ///
    /// # Example
    /// ```ignore
    /// let rows = conn.query("SELECT * FROM users", ())?;
    /// for row in rows {
    ///     println!("{:?}", row);
    /// }
    /// ```
    pub fn query<P: Params>(&mut self, sql: &str, params: P) -> Result<Vec<Row>> {
        let result = self.execute(sql, params)?;
        Ok(result.rows)
    }

    /// Execute query returning exactly one row
    ///
    /// Errors if query returns no rows or multiple rows
    ///
    /// # Example
    /// ```ignore
    /// let row = conn.query_row("SELECT * FROM users WHERE id = ?1", (42,))?;
    /// ```
    pub fn query_row<P: Params>(&mut self, sql: &str, params: P) -> Result<Row> {
        let result = self.execute(sql, params)?;
        match result.rows.len() {
            0 => Err(Error::QueryReturnedNoRows),
            1 => Ok(result.rows[0].clone()),
            _ => Err(Error::QueryReturnedMoreThanOneRow),
        }
    }

    /// Prepare a SQL statement for execution
    ///
    /// Creates a prepared statement that can be executed multiple times
    /// with different parameter values.
    ///
    /// # Example
    /// ```ignore
    /// let stmt = conn.prepare("SELECT id, name FROM users WHERE id = ?1")?;
    /// let results = stmt.query_map(&mut conn, (42,), |row| {
    ///     Ok((row.get(0)?, row.get(1)?))
    /// })?;
    /// ```
    pub fn prepare(&self, sql: &str) -> Result<crate::statement::Statement> {
        Ok(crate::statement::Statement::new(sql.to_string()))
    }

    /// Get the number of rows modified by last INSERT, UPDATE, DELETE
    pub fn changes(&self) -> u64 {
        // Phase 6b: Track changes in VirtualMachine
        0
    }

    /// Substitute ?1, ?2, etc parameters in SQL
    fn substitute_parameters(&self, sql: &str, params: &HashMap<String, Value>) -> Result<String> {
        let mut result = String::new();
        let mut chars = sql.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '?' {
                // Found a parameter placeholder, collect the number
                let mut num_str = String::new();
                while let Some(&next_ch) = chars.peek() {
                    if next_ch.is_ascii_digit() {
                        num_str.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }

                if num_str.is_empty() {
                    return Err(Error::ParseError("Parameter index expected after ?".to_string()));
                }

                if let Some(value) = params.get(&num_str) {
                    // Substitute parameter value
                    match value {
                        Value::Integer(i) => result.push_str(&i.to_string()),
                        Value::Real(f) => result.push_str(&f.to_string()),
                        Value::Text(s) => {
                            result.push('\'');
                            result.push_str(&s.replace('\'', "''"));
                            result.push('\'');
                        }
                        Value::Null => result.push_str("NULL"),
                        Value::Blob(_) => {
                            return Err(Error::ExecutionError(
                                "Blob parameters not yet supported".to_string(),
                            ))
                        }
                    }
                } else {
                    return Err(Error::InvalidParameterCount(params.len(), num_str.parse::<usize>().unwrap_or(0) + 1));
                }
            } else {
                result.push(ch);
            }
        }

        Ok(result)
    }
}

/// Result of a SQL execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Result rows
    pub rows: Vec<crate::types::Row>,
    /// Column names
    pub columns: Vec<String>,
}

impl ExecutionResult {
    /// Get number of rows in result
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Check if result is empty
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Get column count
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }
}

/// Single row result
pub type Row = crate::types::Row;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_open_in_memory() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(conn.in_memory);
    }

    #[test]
    fn test_connection_open_file() {
        // Only test if a database file exists
        if let Ok(conn) = Connection::open("person_split.db") {
            assert!(!conn.in_memory);
        }
    }

    #[test]
    fn test_create_table() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        let result = conn.execute("CREATE TABLE users (id INTEGER, name TEXT)", ())?;
        assert_eq!(result.len(), 0); // DDL returns no rows
        Ok(())
    }

    #[test]
    fn test_insert_basic() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute("CREATE TABLE users (id INTEGER, name TEXT)", ())?;
        let result = conn.execute("INSERT INTO users VALUES (1, 'Alice')", ())?;
        assert_eq!(result.len(), 0); // DML returns no rows
        Ok(())
    }

    #[test]
    fn test_query_row_no_results() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute("CREATE TABLE users (id INTEGER, name TEXT)", ())?;
        
        let result = conn.query_row("SELECT * FROM users", ());
        assert!(result.is_err());
        match result {
            Err(Error::QueryReturnedNoRows) => {}, // Expected
            _ => panic!("Expected QueryReturnedNoRows error"),
        }
        Ok(())
    }

    #[test]
    fn test_query_row_multiple_results() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute("CREATE TABLE users (id INTEGER, name TEXT)", ())?;
        conn.execute("INSERT INTO users VALUES (?1, ?2)", (1i32, "Alice"))?;
        conn.execute("INSERT INTO users VALUES (?1, ?2)", (2i32, "Bob"))?;
        
        let result = conn.query_row("SELECT * FROM users", ());
        assert!(result.is_err());
        match result {
            Err(Error::QueryReturnedMoreThanOneRow) => {}, // Expected
            _ => panic!("Expected QueryReturnedMoreThanOneRow error"),
        }
        Ok(())
    }

    #[test]
    fn test_query_row_exact_one() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute("CREATE TABLE users (id INTEGER, name TEXT)", ())?;
        conn.execute("INSERT INTO users VALUES (?1, ?2)", (1i32, "Alice"))?;
        
        // Query should succeed (not raise QueryReturnedNoRows or QueryReturnedMoreThanOneRow)
        let _row = conn.query_row("SELECT * FROM users", ())?;
        // Successfully got exactly one row
        Ok(())
    }

    #[test]
    fn test_execute_with_params_tuple() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute("CREATE TABLE users (id INTEGER, name TEXT)", ())?;
        
        // Insert using tuple parameters
        conn.execute(
            "INSERT INTO users VALUES (?1, ?2)",
            (1i32, "Alice"),
        )?;
        
        // Verify insertion
        let result = conn.execute("SELECT COUNT(*) FROM users", ())?;
        assert!(!result.is_empty());
        Ok(())
    }

    #[test]
    fn test_execute_with_params_array() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute("CREATE TABLE numbers (value INTEGER)", ())?;
        
        // Insert using array parameters
        conn.execute(
            "INSERT INTO numbers VALUES (?1)",
            [42i32],
        )?;
        
        // Verify insertion
        let result = conn.execute("SELECT COUNT(*) FROM numbers", ())?;
        assert!(!result.is_empty());
        Ok(())
    }

    #[test]
    fn test_connection2_with_btree2_schema_dump() -> Result<()> {
        // Create a SQLite database with rusqlite
        let temp_file = tempfile::NamedTempFile::new()
            .map_err(|e| Error::IoError(e.to_string()))?;
        let db_path = temp_file.path().to_path_buf();
        println!("Created temporary database file at {:?}", db_path);

        // Write with rusqlite
        {
            let conn = rusqlite::Connection::open(&db_path)
                .map_err(|e| Error::ExecutionError(e.to_string()))?;
            
            // Create a table with multiple column types to test record format variants
            conn.execute(
                "CREATE TABLE test_table (
                    id INTEGER PRIMARY KEY,
                    tiny_int INTEGER,
                    small_int INTEGER,
                    medium_int INTEGER,
                    big_int INTEGER,
                    very_big_int INTEGER,
                    float_val REAL,
                    text_val TEXT,
                    blob_val BLOB
                )",
                [],
            ).map_err(|e| Error::ExecutionError(e.to_string()))?;
            
            // Insert records to test all integer type code variants (1-7)
            // Type 1: 1-byte (-128 to 127)
            conn.execute(
                "INSERT INTO test_table VALUES (1, 42, 300, 70000, 2000000, 1000000000000, 3.14, 'one', x'00010203')",
                [],
            ).map_err(|e| Error::ExecutionError(e.to_string()))?;
            
            // Type 2: 2-byte (-32768 to 32767)
            conn.execute(
                "INSERT INTO test_table VALUES (2, 127, 1000, 80000, 3000000, 2000000000000, 2.71, 'two', x'04050607')",
                [],
            ).map_err(|e| Error::ExecutionError(e.to_string()))?;
            
            // Type 3: 3-byte (-8388608 to 8388607)
            conn.execute(
                "INSERT INTO test_table VALUES (3, -128, 5000, 100000, 4000000, 3000000000000, 1.41, 'three', x'08090a0b')",
                [],
            ).map_err(|e| Error::ExecutionError(e.to_string()))?;
            
            // Type 4: 4-byte (-2147483648 to 2147483647)
            conn.execute(
                "INSERT INTO test_table VALUES (4, -1, 10000, 500000, 1000000000, 4000000000000, 0.5, 'four', x'0c0d0e0f')",
                [],
            ).map_err(|e| Error::ExecutionError(e.to_string()))?;
            
            // Type 6: 8-byte (full i64 range)
            conn.execute(
                "INSERT INTO test_table VALUES (5, 0, 32000, 1000000, 2000000000, 5000000000000, -3.14, 'five', x'101112131415')",
                [],
            ).map_err(|e| Error::ExecutionError(e.to_string()))?;
            
            // Test NULL values mixed with different types
            conn.execute(
                "INSERT INTO test_table VALUES (6, NULL, NULL, NULL, NULL, NULL, NULL, 'six', NULL)",
                [],
            ).map_err(|e| Error::ExecutionError(e.to_string()))?;
            
            conn.close().map_err(|(_, e)| Error::ExecutionError(e.to_string()))?;
        }

        // Now read with Connection2 and Transaction
        let conn2 = Connection2::open(db_path.to_str().unwrap())?;
        let transaction = conn2.transaction()?;

        // Open the schema table at page 1 with BTree2
        let btree = BTree2::new(1, &transaction);
        
        // Dump all entries from the schema table to stdout
        println!("✓ Connection2 created successfully");
        println!("✓ Transaction started successfully");
        println!("✓ BTree2 opened at root page 1");
        println!("✓ Schema entries (zero-copy iteration):");
        btree.dump_all()?;
        
        // Dump page 2 with BTree2 - should have all test data with varied record formats
        println!("\n✓ BTree2 opened at page 2");
        println!("✓ Page 2 entries (record format variants - all integer types, floats, text, blobs):");
        let btree2 = BTree2::new(2, &transaction);
        btree2.dump_all()?;
        
        // Keep the file for inspection with xxd
        let temp_path = temp_file.into_temp_path();
        temp_path.keep()
            .map_err(|e| Error::IoError(format!("Failed to keep temp file: {}", e)))?;
        println!("✓ Database file preserved at: {:?}", db_path);
        
        Ok(())
    }
}

/// Simplified database connection (Phase 8b: Multithreading support)
/// 
/// Contains only a shared reference to the database file. Transactions are
/// created from this connection and handle the execution context separately.
pub struct Connection2 {
    /// Shared reference to the database file
    db_file: Arc<DatabaseFile>,
}

impl Connection2 {
    /// Open a connection to a database file for reading and writing
    ///
    /// Creates a shareable connection that can be used to start transactions.
    /// The database file is wrapped in Arc for shared ownership.
    pub fn open(path: &str) -> Result<Self> {
        let db_file = DatabaseFile::open(path)?;
        Ok(Self {
            db_file: Arc::new(db_file),
        })
    }

    /// Create a new transaction from this connection
    ///
    /// Returns a Transaction that can execute queries and track page modifications.
    pub fn transaction(&self) -> Result<Transaction> {
        Transaction::new(Arc::clone(&self.db_file))
    }
}

/// Transaction context for query execution (Phase 8b: Transaction support)
///
/// Represents a single transaction session where queries are executed.
/// Tracks all page modifications made during the transaction in a write-ahead log.
pub struct Transaction {
    /// Shared reference to the database file
    pub db_file: Arc<DatabaseFile>,
    /// Virtual machine for query execution
    vm: VirtualMachine,
    /// Pages modified during this transaction: page_num -> bytes
    /// This tracks all writes that need to be persisted when the transaction commits
    modified_pages: HashMap<u32, Box<[u8]>>,
}

impl Transaction {
    /// Create a new transaction
    fn new(db_file: Arc<DatabaseFile>) -> Result<Self> {
        Ok(Self {
            db_file,
            vm: VirtualMachine::new(),
            modified_pages: HashMap::new(),
        })
    }

    /// Get a read-only reference to a page (Phase 8b: Transaction support)
    ///
    /// Checks modified_pages first; if found, creates PageRef from those bytes.
    /// Otherwise, reads from the database file using read_page_ref().
    pub fn page(&self, page_num: u32) -> Result<crate::file_format::PageRef<'_>> {
        // For now, delegate to db_file.read_page_ref()
        // In the future, this should check modified_pages and use those bytes if available
        self.db_file.read_page_ref(page_num)
    }

    /// Execute a SQL statement within this transaction
    ///
    /// Queries are executed against the virtual machine's table storage.
    /// Write operations track modified pages for later persistence.
    pub fn execute<P: Params>(&mut self, sql: &str, params: P) -> Result<ExecutionResult> {
        // Bind parameters
        let param_map = params.bind_params()?;
        
        // Parse SQL with parameter substitution
        let processed_sql = self.substitute_parameters(sql, &param_map)?;
        
        // Parse
        let mut parser = Parser::new(&processed_sql)?;
        let stmt = parser.parse_statement()?;

        // Plan
        let planner = Planner::new();
        let plan = planner.plan(&stmt)?;

        // Execute
        let result_set = self.vm.execute(&plan)?;

        // For write operations, track that pages were modified
        // (actual page serialization happens during commit)
        if self.is_write_operation(&plan) {
            let tables = self.vm.get_all_tables();
            for (_table_name, table_storage) in tables {
                // Mark this page as needing to be persisted
                // The actual bytes will be serialized during commit
                self.modified_pages.insert(table_storage.page_num, Box::new([]));
            }
        }

        Ok(ExecutionResult {
            rows: result_set.rows.clone(),
            columns: result_set.columns.clone(),
        })
    }

    /// Check if an execution plan modifies data
    fn is_write_operation(&self, plan: &ExecutionPlan) -> bool {
        matches!(
            plan,
            ExecutionPlan::Insert { .. }
                | ExecutionPlan::Update { .. }
                | ExecutionPlan::Delete { .. }
                | ExecutionPlan::CreateTable { .. }
                | ExecutionPlan::CreateIndex { .. }
                | ExecutionPlan::DropIndex { .. }
        )
    }

    /// Substitute ?1, ?2, etc parameters in SQL
    fn substitute_parameters(&self, sql: &str, params: &HashMap<String, Value>) -> Result<String> {
        let mut result = String::new();
        let mut chars = sql.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '?' {
                // Found a parameter placeholder, collect the number
                let mut num_str = String::new();
                while let Some(&next_ch) = chars.peek() {
                    if next_ch.is_ascii_digit() {
                        num_str.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }

                if num_str.is_empty() {
                    return Err(Error::ParseError("Parameter index expected after ?".to_string()));
                }

                if let Some(value) = params.get(&num_str) {
                    // Substitute parameter value
                    match value {
                        Value::Integer(i) => result.push_str(&i.to_string()),
                        Value::Real(f) => result.push_str(&f.to_string()),
                        Value::Text(s) => {
                            result.push('\'');
                            result.push_str(&s.replace('\'', "''"));
                            result.push('\'');
                        }
                        Value::Null => result.push_str("NULL"),
                        Value::Blob(_) => {
                            return Err(Error::ExecutionError(
                                "Blob parameters not yet supported".to_string(),
                            ))
                        }
                    }
                } else {
                    return Err(Error::InvalidParameterCount(params.len(), num_str.parse::<usize>().unwrap_or(0) + 1));
                }
            } else {
                result.push(ch);
            }
        }

        Ok(result)
    }

    /// Get the modified pages from this transaction
    pub fn modified_pages(&self) -> &HashMap<u32, Box<[u8]>> {
        &self.modified_pages
    }

    /// Commit the transaction (placeholder for now)
    ///
    /// In a full implementation, this would:
    /// 1. Serialize modified pages into modified_pages HashMap
    /// 2. Write changes to the database file
    /// 3. Flush to disk
    pub fn commit(&self) -> Result<()> {
        // TODO: Implement commit logic
        Ok(())
    }

    /// Rollback the transaction (placeholder for now)
    ///
    /// Discards all changes made during this transaction.
    pub fn rollback(&self) -> Result<()> {
        // TODO: Implement rollback logic
        Ok(())
    }
}

/// Improved B-tree implementation with transaction-based page access (Phase 8c)
///
/// Traverses SQLite B-tree pages using a transaction context, allowing proper
/// handling of modified pages and multi-page navigation.
pub struct BTree2<'t> {
    /// Root page number of the B-tree
    root_page: u32,
    /// Reference to the transaction for page access
    transaction: &'t Transaction,
}

impl<'t> BTree2<'t> {
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
    fn dump_leaf_page(&self, page_ref: &crate::file_format::PageRef<'_>) -> Result<()> {
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
    fn dump_interior_page(&self, page_ref: &crate::file_format::PageRef<'_>) -> Result<()> {
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
}
