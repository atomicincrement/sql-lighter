//! Error types for SQL Lighter - rusqlite-compatible
//! 
//! References: https://docs.rs/rusqlite/latest/rusqlite/enum.Error.html

use std::fmt;
use std::error::Error as StdError;

/// Result type for SQL Lighter operations - matches rusqlite::Result
pub type Result<T> = std::result::Result<T, Error>;

/// Error types - matches rusqlite::Error API
/// 
/// Simplified version with core variants needed for database operations.
#[derive(Debug, PartialEq)]
pub enum Error {
    /// Query that was expected to return at least one row (e.g., query_row) did not return any
    QueryReturnedNoRows,
    
    /// Query that was expected to return only one row did return more than one
    QueryReturnedMoreThanOneRow,
    
    /// Number of bound parameters does not match the number expected
    /// Format: (parameters_given, parameters_expected)
    InvalidParameterCount(usize, usize),
    
    /// execute() call returned rows (should have returned no rows)
    ExecuteReturnedResults,
    
    /// Requested column index is out of range
    InvalidColumnIndex(usize),
    
    /// Requested column by name, but no column with that name exists
    InvalidColumnName(String),
    
    /// Error converting column value to requested Rust type
    /// Format: (column_index, column_name, error_message)
    InvalidColumnType(usize, String, String),
    
    /// Internal execution error with description
    ExecutionError(String),
    
    /// Parse/SQL syntax error
    ParseError(String),
    
    /// File I/O error
    IoError(String),
    
    /// Other errors (catch-all)
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::QueryReturnedNoRows => {
                write!(f, "query returned no rows when at least one was expected")
            }
            Error::QueryReturnedMoreThanOneRow => {
                write!(f, "query returned more than one row when at most one was expected")
            }
            Error::InvalidParameterCount(given, expected) => {
                write!(f, "wrong number of parameters: expected {}, got {}", expected, given)
            }
            Error::ExecuteReturnedResults => {
                write!(f, "execute() returned results when no rows were expected")
            }
            Error::InvalidColumnIndex(idx) => {
                write!(f, "column index {} is out of range", idx)
            }
            Error::InvalidColumnName(name) => {
                write!(f, "no column named '{}' exists", name)
            }
            Error::InvalidColumnType(idx, name, msg) => {
                write!(f, "error converting column {} ({}) to requested type: {}", idx, name, msg)
            }
            Error::ExecutionError(msg) => write!(f, "execution error: {}", msg),
            Error::ParseError(msg) => write!(f, "parse error: {}", msg),
            Error::IoError(msg) => write!(f, "IO error: {}", msg),
            Error::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl StdError for Error {}
