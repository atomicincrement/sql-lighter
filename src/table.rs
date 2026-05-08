//! Table reference and schema management
//!
//! Contains TableRef struct that holds parsed table metadata from sqlite_master.
//! Provides methods for table operations using BTree iterators.

use crate::error::Result;
use crate::types::{Row, Value};
use crate::parser::Expression;

/// Reference to a table schema extracted from the sqlite_master table
///
/// Contains parsed information about a table including its name, column definitions,
/// the original CREATE TABLE statement, and the root page of its B-tree.
#[derive(Debug, Clone)]
pub struct TableRef {
    /// Table name
    pub name: String,
    /// Column names and types parsed from the CREATE TABLE statement
    pub columns: Vec<(String, String)>,
    /// Original CREATE TABLE SQL statement
    pub sql: String,
    /// Root page number of the table's B-tree (for BTree traversal)
    pub root_page: u32,
}

impl TableRef {
    /// Create a new table reference with schema
    pub fn new(
        name: String,
        columns: Vec<(String, String)>,
        sql: String,
        root_page: u32,
    ) -> Self {
        Self {
            name,
            columns,
            sql,
            root_page,
        }
    }

    /// Get column names in order
    pub fn column_names(&self) -> Vec<String> {
        self.columns.iter().map(|(name, _)| name.clone()).collect()
    }

    /// Get column types in order
    pub fn column_types(&self) -> Vec<String> {
        self.columns.iter().map(|(_, ty)| ty.clone()).collect()
    }

    /// Add a row to the table storage
    /// 
    /// TODO: Implement via Transaction.insert_into_btree()
    pub fn add_row(&mut self, _row: &Row) -> Result<()> {
        todo!("TableRef::add_row - will use Transaction to insert into B-tree")
    }

    /// Get all rows from the table storage using BTree leaf iterator
    /// 
    /// Iterates through the B-tree leaf cells and parses them into rows.
    /// This requires access to the transaction for B-tree traversal.
    pub fn get_all_rows(&self, transaction: &crate::transaction::Transaction) -> Result<Vec<Row>> {
        // Use BTree::leaf_payloads() to iterate over leaf cells
        let btree = crate::file_format::btree::BTree::new(self.root_page, transaction);
        let mut rows = Vec::new();

        for payload_result in btree.leaf_payloads()? {
            if let Ok(payload) = payload_result {
                if let Ok(record) = crate::file_format::Record::parse(&payload) {
                    let mut row = Row::new();
                    for (i, (col_name, _)) in self.columns.iter().enumerate() {
                        if i < record.columns.len() {
                            row.push((col_name.clone(), record.columns[i].clone()));
                        } else {
                            row.push((col_name.clone(), Value::Null));
                        }
                    }
                    rows.push(row);
                }
            }
        }

        Ok(rows)
    }

    /// Convert to ResultSet for compatibility
    pub fn to_result_set(&self, transaction: &crate::transaction::Transaction) -> Result<crate::executor::ResultSet> {
        let column_names = self.column_names();
        let mut result = crate::executor::ResultSet::new(column_names);
        
        let rows = self.get_all_rows(transaction)?;
        for row in rows {
            result.add_row(row);
        }
        
        Ok(result)
    }

    /// Delete rows matching a condition
    /// 
    /// TODO: Implement via Transaction.delete_from_btree()
    pub fn delete_matching(
        &mut self,
        _condition: &Option<Expression>,
        _columns: &[String],
    ) -> Result<usize> {
        todo!("TableRef::delete_matching - will use Transaction to delete from B-tree")
    }

    /// Update rows matching a condition
    /// 
    /// TODO: Implement via Transaction.update_in_btree()
    pub fn update_matching(
        &mut self,
        _assignments: &[(String, Value)],
        _condition: &Option<Expression>,
        _columns: &[String],
    ) -> Result<usize> {
        todo!("TableRef::update_matching - will use Transaction to update in B-tree")
    }
}
