//! Error types for SQL Lighter

use std::fmt;

/// Result type for SQL Lighter operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types
#[derive(Debug)]
pub enum Error {
    /// File I/O error
    IoError(String),
    
    /// Parse error
    ParseError(String),
    
    /// Query planning error
    PlanError(String),
    
    /// Query execution error
    ExecutionError(String),
    
    /// Storage error
    StorageError(String),
    
    /// Type error
    TypeError(String),
    
    /// Constraint violation
    ConstraintError(String),
    
    /// Not implemented
    NotImplemented(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::IoError(msg) => write!(f, "IO Error: {}", msg),
            Error::ParseError(msg) => write!(f, "Parse Error: {}", msg),
            Error::PlanError(msg) => write!(f, "Plan Error: {}", msg),
            Error::ExecutionError(msg) => write!(f, "Execution Error: {}", msg),
            Error::StorageError(msg) => write!(f, "Storage Error: {}", msg),
            Error::TypeError(msg) => write!(f, "Type Error: {}", msg),
            Error::ConstraintError(msg) => write!(f, "Constraint Error: {}", msg),
            Error::NotImplemented(msg) => write!(f, "Not Implemented: {}", msg),
        }
    }
}

impl std::error::Error for Error {}
