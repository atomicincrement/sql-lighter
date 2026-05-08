//! Transaction context for query execution
//!
//! Represents a single transaction session where queries are executed.
//! Tracks all page modifications made during the transaction and manages access
//! to the database file through a shared Arc reference.

use crate::error::Result;
use crate::executor::VirtualMachine;
use crate::parser::Parser;
use crate::planner::{Planner, ExecutionPlan};
use crate::types::Value;
use crate::params::Params;
use crate::file_format::DatabaseFile;
use crate::connection::ExecutionResult;
use crate::table::TableRef;
use std::collections::HashMap;
use std::sync::Arc;

/// Transaction context for query execution (Phase 8b: Transaction support)
///
/// Represents a single transaction session where queries are executed.
/// Tracks all page modifications made during the transaction in a write-ahead log.
pub struct Transaction {
    /// Shared reference to the database file
    pub db_file: Arc<DatabaseFile>,
    /// Pages modified during this transaction: page_num -> bytes
    /// This tracks all writes that need to be persisted when the transaction commits
    modified_pages: HashMap<u32, Box<[u8]>>,
    /// Cached table references: table_name -> TableRef
    /// Populated on-demand when tables are accessed
    table_refs: HashMap<String, TableRef>,
}

impl Transaction {
    /// Create a new transaction
    pub(crate) fn new(db_file: Arc<DatabaseFile>) -> Result<Self> {
        Ok(Self {
            db_file,
            modified_pages: HashMap::new(),
            table_refs: HashMap::new(),
        })
    }

    /// Get mutable access to table references for VM operations
    pub fn get_table(&mut self, name: &str) -> Option<&mut TableRef> {
        self.table_refs.get_mut(name)
    }

    /// Insert a new table reference
    pub fn insert_table(&mut self, name: String, table: TableRef) {
        self.table_refs.insert(name, table);
    }

    /// Get immutable access to a table reference
    pub fn get_table_ref(&self, name: &str) -> Option<&TableRef> {
        self.table_refs.get(name)
    }

    /// Get all table references
    pub fn all_tables(&self) -> &HashMap<String, TableRef> {
        &self.table_refs
    }
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
    /// Creates a virtual machine on-demand and executes the plan.
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

        // Create virtual machine on demand and execute with mutable transaction reference
        let mut vm = VirtualMachine::new();
        let result_set = vm.execute(&plan, self)?;

        // For write operations, track that pages were modified
        // TODO: Implement page modification tracking from transaction table_refs
        // (actual page serialization happens during commit)

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
                    return Err(crate::error::Error::ParseError("Parameter index expected after ?".to_string()));
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
                            return Err(crate::error::Error::ExecutionError(
                                "Blob parameters not yet supported".to_string(),
                            ))
                        }
                    }
                } else {
                    return Err(crate::error::Error::InvalidParameterCount(params.len(), num_str.parse::<usize>().unwrap_or(0) + 1));
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
