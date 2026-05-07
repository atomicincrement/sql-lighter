//! SQL data types

use serde::{Deserialize, Serialize};

/// SQL data types supported by SQL Lighter
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SqlType {
    /// NULL type
    Null,
    
    /// Boolean (true/false)
    Boolean,
    
    /// Integer (8, 16, 32, or 64 bits)
    Integer,
    
    /// Real (floating point)
    Real,
    
    /// Text/String
    Text,
    
    /// Binary blob
    Blob,
}

/// SQL value
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SqlValue {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl SqlValue {
    /// Get the type of this value
    pub fn sql_type(&self) -> SqlType {
        match self {
            SqlValue::Null => SqlType::Null,
            SqlValue::Boolean(_) => SqlType::Boolean,
            SqlValue::Integer(_) => SqlType::Integer,
            SqlValue::Real(_) => SqlType::Real,
            SqlValue::Text(_) => SqlType::Text,
            SqlValue::Blob(_) => SqlType::Blob,
        }
    }
}

/// A row of data
pub type Row = Vec<(String, SqlValue)>;
